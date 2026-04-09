use std::collections::VecDeque;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime};

use clawin_config::ClawinPaths;
use clawin_core::{
    BridgeController, BridgeMode, BridgePointer, BridgePointerSource, BridgeSessionHost,
    BridgeState, BridgeStatusSnapshot, ClawinError, ClawinResult, SessionRuntime,
    StructuredInputMessage, StructuredOutputMessage,
};
use clawin_platform::{GitWorktreeAdapter, PathPolicy};

pub const BRIDGE_POINTER_FILE_NAME: &str = "bridge-pointer.json";
pub const BRIDGE_POINTER_TTL: Duration = Duration::from_secs(4 * 60 * 60);
const DEFAULT_POLL_INTERVAL: Duration = Duration::from_millis(25);
const DEFAULT_RECONNECT_INITIAL_DELAY: Duration = Duration::from_secs(2);
const DEFAULT_RECONNECT_MAX_DELAY: Duration = Duration::from_secs(30);
const DEFAULT_RECONNECT_GIVE_UP: Duration = Duration::from_secs(10 * 60);
const DEFAULT_POINTER_FANOUT_LIMIT: usize = 50;

/// Stable reconnect policy used by the bridge worker.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReconnectPolicy {
    pub initial_delay: Duration,
    pub max_delay: Duration,
    pub give_up_after: Duration,
    pub poll_interval: Duration,
}

impl Default for ReconnectPolicy {
    fn default() -> Self {
        Self {
            initial_delay: DEFAULT_RECONNECT_INITIAL_DELAY,
            max_delay: DEFAULT_RECONNECT_MAX_DELAY,
            give_up_after: DEFAULT_RECONNECT_GIVE_UP,
            poll_interval: DEFAULT_POLL_INTERVAL,
        }
    }
}

/// Inputs used when opening a transport-backed bridge session.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BridgeConnectRequest {
    pub mode: BridgeMode,
    pub source: BridgePointerSource,
    pub name: Option<String>,
    pub local_session_id: String,
    pub transcript_path: PathBuf,
    pub continue_pointer: Option<BridgePointer>,
}

/// Polled input from the remote transport.
#[derive(Clone, Debug, PartialEq)]
pub enum BridgeTransportPoll {
    Message(StructuredInputMessage),
    Idle,
    Disconnected,
}

/// Live transport session created by a connector.
pub trait BridgeTransportSession: Send {
    fn bridge_session_id(&self) -> &str;
    fn environment_id(&self) -> &str;
    fn poll_input(&mut self, timeout: Duration) -> ClawinResult<BridgeTransportPoll>;
    fn send_output(&mut self, message: &StructuredOutputMessage) -> ClawinResult<()>;
    fn close(&mut self) -> ClawinResult<()>;
}

/// Transport connector used by the bridge manager.
pub trait BridgeTransportConnector: Send + Sync {
    fn connect(
        &self,
        request: &BridgeConnectRequest,
    ) -> ClawinResult<Box<dyn BridgeTransportSession>>;
}

/// Default unavailable connector used in production until a real backend lands.
#[derive(Clone, Copy, Debug, Default)]
pub struct UnavailableBridgeConnector;

impl BridgeTransportConnector for UnavailableBridgeConnector {
    fn connect(
        &self,
        _request: &BridgeConnectRequest,
    ) -> ClawinResult<Box<dyn BridgeTransportSession>> {
        Err(ClawinError::InvalidConfiguration {
            message:
                "remote control bridge is unavailable because no bridge connector is configured"
                    .to_owned(),
        })
    }
}

/// Filesystem-backed bridge pointer store scoped to the current project and same-repo worktrees.
#[derive(Debug)]
pub struct BridgePointerStore<P, G> {
    paths: ClawinPaths,
    path_policy: P,
    git: Arc<G>,
    ttl: Duration,
    fanout_limit: usize,
}

impl<P, G> Clone for BridgePointerStore<P, G>
where
    P: Clone,
{
    fn clone(&self) -> Self {
        Self {
            paths: self.paths.clone(),
            path_policy: self.path_policy.clone(),
            git: Arc::clone(&self.git),
            ttl: self.ttl,
            fanout_limit: self.fanout_limit,
        }
    }
}

impl<P, G> BridgePointerStore<P, G>
where
    P: PathPolicy + Clone,
    G: GitWorktreeAdapter,
{
    pub fn new(paths: ClawinPaths, path_policy: P, git: Arc<G>) -> Self {
        Self {
            paths,
            path_policy,
            git,
            ttl: BRIDGE_POINTER_TTL,
            fanout_limit: DEFAULT_POINTER_FANOUT_LIMIT,
        }
    }

    pub fn with_policy(
        paths: ClawinPaths,
        path_policy: P,
        git: Arc<G>,
        ttl: Duration,
        fanout_limit: usize,
    ) -> Self {
        Self {
            paths,
            path_policy,
            git,
            ttl,
            fanout_limit,
        }
    }

    pub fn transcript_path(&self, runtime: &SessionRuntime) -> PathBuf {
        if let Some(path) = runtime.session_transcript_path() {
            return path;
        }
        self.session_project_directory(runtime.active_project_root())
            .join(format!("{}.jsonl", runtime.session_id().as_str()))
    }

    pub fn pointer_path(&self, runtime: &SessionRuntime) -> PathBuf {
        self.session_project_directory(runtime.active_project_root())
            .join(BRIDGE_POINTER_FILE_NAME)
    }

    pub fn save(&self, runtime: &SessionRuntime, pointer: &BridgePointer) -> ClawinResult<PathBuf> {
        let path = self.pointer_path(runtime);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| ClawinError::InvalidConfiguration {
                message: format!(
                    "failed to create bridge pointer directory {}: {error}",
                    parent.display()
                ),
            })?;
        }
        let payload = serde_json::to_vec_pretty(pointer).map_err(|error| {
            ClawinError::InvalidConfiguration {
                message: format!(
                    "failed to serialize bridge pointer {}: {error}",
                    path.display()
                ),
            }
        })?;
        fs::write(&path, payload).map_err(|error| ClawinError::InvalidConfiguration {
            message: format!("failed to write bridge pointer {}: {error}", path.display()),
        })?;
        Ok(path)
    }

    pub fn remove_for_runtime(&self, runtime: &SessionRuntime) -> ClawinResult<()> {
        let path = self.pointer_path(runtime);
        self.remove_path(&path)
    }

    pub fn remove_path(&self, path: &Path) -> ClawinResult<()> {
        if !path.exists() {
            return Ok(());
        }
        fs::remove_file(path).map_err(|error| ClawinError::InvalidConfiguration {
            message: format!(
                "failed to remove bridge pointer {}: {error}",
                path.display()
            ),
        })
    }

    pub fn load(&self, path: &Path) -> ClawinResult<Option<BridgePointer>> {
        self.load_checked(path)
    }

    pub fn resolve_continue(
        &self,
        runtime: &SessionRuntime,
    ) -> ClawinResult<Option<BridgePointer>> {
        let candidates = self.pointer_files_in_scope(runtime)?;
        let mut saw_invalid = false;

        for path in candidates {
            match self.load_checked(&path) {
                Ok(Some(pointer)) => return Ok(Some(pointer)),
                Ok(None) => saw_invalid = true,
                Err(_) => saw_invalid = true,
            }
        }

        if saw_invalid {
            return Err(ClawinError::InvalidConfiguration {
                message: "no valid bridge pointer found in the current project scope".to_owned(),
            });
        }

        Ok(None)
    }

    fn load_checked(&self, path: &Path) -> ClawinResult<Option<BridgePointer>> {
        let metadata = match fs::metadata(path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                return Err(ClawinError::InvalidConfiguration {
                    message: format!("failed to stat bridge pointer {}: {error}", path.display()),
                });
            }
        };

        if is_stale(&metadata, self.ttl) {
            self.remove_path(path)?;
            return Ok(None);
        }

        let contents = match fs::read_to_string(path) {
            Ok(contents) => contents,
            Err(error) => {
                self.remove_path(path)?;
                return Err(ClawinError::InvalidConfiguration {
                    message: format!("failed to read bridge pointer {}: {error}", path.display()),
                });
            }
        };

        match serde_json::from_str::<BridgePointer>(&contents) {
            Ok(pointer) => Ok(Some(pointer)),
            Err(error) => {
                self.remove_path(path)?;
                Err(ClawinError::InvalidConfiguration {
                    message: format!("invalid bridge pointer {}: {error}", path.display()),
                })
            }
        }
    }

    fn pointer_files_in_scope(&self, runtime: &SessionRuntime) -> ClawinResult<Vec<PathBuf>> {
        let mut directories = vec![self.session_project_directory(runtime.active_project_root())];
        if let Some(repo_root) = self
            .git
            .canonical_git_root(runtime.canonical_project_root())
            .map_err(map_git_error)?
        {
            for worktree in self.git.list_worktrees(&repo_root).map_err(map_git_error)? {
                let directory = self.session_project_directory(worktree.path().to_path_buf());
                if !directories.iter().any(|existing| existing == &directory) {
                    directories.push(directory);
                }
            }
        }

        let mut files = directories
            .into_iter()
            .map(|directory| directory.join(BRIDGE_POINTER_FILE_NAME))
            .filter(|path| path.exists())
            .collect::<Vec<_>>();

        files.sort_by(|left, right| {
            let left_modified = fs::metadata(left).and_then(|meta| meta.modified()).ok();
            let right_modified = fs::metadata(right).and_then(|meta| meta.modified()).ok();
            right_modified.cmp(&left_modified)
        });
        files.truncate(self.fanout_limit);
        Ok(files)
    }

    fn session_project_directory(&self, active_project_root: PathBuf) -> PathBuf {
        self.paths.projects_root().join(
            self.path_policy
                .sanitize_for_session_dir(&active_project_root),
        )
    }
}

struct ActiveWorker {
    stop_tx: Sender<()>,
    handle: JoinHandle<()>,
    pointer_path: PathBuf,
}

struct BridgeManagerState {
    status: BridgeStatusSnapshot,
    worker: Option<ActiveWorker>,
}

/// Remote control bridge lifecycle owner injected into bootstrap/runtime services.
#[derive(Clone)]
pub struct BridgeManager<P, G> {
    connector: Arc<dyn BridgeTransportConnector>,
    pointer_store: BridgePointerStore<P, G>,
    reconnect_policy: ReconnectPolicy,
    state: Arc<Mutex<BridgeManagerState>>,
}

impl<P, G> std::fmt::Debug for BridgeManager<P, G> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let state = self
            .state
            .lock()
            .expect("bridge manager state lock should be available");
        formatter
            .debug_struct("BridgeManager")
            .field("status", &state.status)
            .finish()
    }
}

impl<P, G> BridgeManager<P, G>
where
    P: PathPolicy + Clone + Send + Sync + 'static,
    G: GitWorktreeAdapter + Send + Sync + 'static,
{
    pub fn new(
        paths: ClawinPaths,
        path_policy: P,
        git: Arc<G>,
        connector: Arc<dyn BridgeTransportConnector>,
    ) -> Self {
        Self::with_policy(
            paths,
            path_policy,
            git,
            connector,
            ReconnectPolicy::default(),
        )
    }

    pub fn with_policy(
        paths: ClawinPaths,
        path_policy: P,
        git: Arc<G>,
        connector: Arc<dyn BridgeTransportConnector>,
        reconnect_policy: ReconnectPolicy,
    ) -> Self {
        Self {
            connector,
            pointer_store: BridgePointerStore::new(paths, path_policy, git),
            reconnect_policy,
            state: Arc::new(Mutex::new(BridgeManagerState {
                status: BridgeStatusSnapshot::ready(),
                worker: None,
            })),
        }
    }

    pub fn pointer_store(&self) -> &BridgePointerStore<P, G> {
        &self.pointer_store
    }

    fn cleanup_finished_worker(&self) -> ClawinResult<()> {
        let finished = {
            let state = self
                .state
                .lock()
                .expect("bridge manager state lock should be available");
            state
                .worker
                .as_ref()
                .is_some_and(|worker| worker.handle.is_finished())
        };

        if finished {
            let worker = self
                .state
                .lock()
                .expect("bridge manager state lock should be available")
                .worker
                .take();
            if let Some(worker) = worker {
                let _ = worker.handle.join();
            }
        }

        Ok(())
    }

    fn connect_once(
        &self,
        request: &BridgeConnectRequest,
    ) -> ClawinResult<Box<dyn BridgeTransportSession>> {
        self.connector.connect(request)
    }
}

impl<P, G> BridgeController for BridgeManager<P, G>
where
    P: PathPolicy + Clone + Send + Sync + 'static,
    G: GitWorktreeAdapter + Send + Sync + 'static,
{
    fn status(&self) -> ClawinResult<BridgeStatusSnapshot> {
        self.cleanup_finished_worker()?;
        Ok(self
            .state
            .lock()
            .expect("bridge manager state lock should be available")
            .status
            .clone())
    }

    fn start(
        &self,
        runtime: &SessionRuntime,
        host: Arc<dyn BridgeSessionHost>,
        mode: BridgeMode,
        source: BridgePointerSource,
        name: Option<String>,
        pointer: Option<BridgePointer>,
    ) -> ClawinResult<BridgeStatusSnapshot> {
        self.cleanup_finished_worker()?;
        {
            let state = self
                .state
                .lock()
                .expect("bridge manager state lock should be available");
            if state.worker.is_some() {
                return Ok(state.status.clone());
            }
        }

        let transcript_path = pointer
            .as_ref()
            .map(|pointer| pointer.transcript_path.clone())
            .unwrap_or_else(|| self.pointer_store.transcript_path(runtime));
        let connect_request = BridgeConnectRequest {
            mode,
            source,
            name: name.clone(),
            local_session_id: runtime.session_id().as_str().to_owned(),
            transcript_path: transcript_path.clone(),
            continue_pointer: pointer.clone(),
        };
        let mut session = self.connect_once(&connect_request)?;
        let saved_pointer = BridgePointer {
            bridge_session_id: session.bridge_session_id().to_owned(),
            environment_id: session.environment_id().to_owned(),
            source,
            local_session_id: runtime.session_id().clone(),
            transcript_path: transcript_path.clone(),
        };
        let pointer_path = self.pointer_store.save(runtime, &saved_pointer)?;

        let initial_status = BridgeStatusSnapshot {
            state: BridgeState::Connected,
            mode: Some(mode),
            source: Some(source),
            name: name.clone(),
            bridge_session_id: Some(saved_pointer.bridge_session_id.clone()),
            environment_id: Some(saved_pointer.environment_id.clone()),
            local_session_id: Some(saved_pointer.local_session_id.clone()),
            transcript_path: Some(saved_pointer.transcript_path.clone()),
            last_error: None,
        };
        {
            let mut state = self
                .state
                .lock()
                .expect("bridge manager state lock should be available");
            state.status = initial_status.clone();
        }

        let reconnect_policy = self.reconnect_policy;
        let state = Arc::clone(&self.state);
        let connector = Arc::clone(&self.connector);
        let pointer_store = self.pointer_store.clone();
        let runtime = runtime.clone();
        let stop_status = initial_status.clone();
        let stop_name = name.clone();
        let stop_tx = {
            let (tx, rx) = mpsc::channel();
            let handle = thread::spawn(move || {
                let _ = session.send_output(&StructuredOutputMessage::SessionStarted {
                    session_id: runtime.session_id().as_str().to_owned(),
                });

                let mut active_session = session;
                loop {
                    match pump_transport(
                        &mut *active_session,
                        host.as_ref(),
                        &rx,
                        reconnect_policy.poll_interval,
                    ) {
                        WorkerNext::Stopped => {
                            let _ = host.notify_transport_closed("stopped");
                            let _ = active_session.close();
                            let mut state = state
                                .lock()
                                .expect("bridge manager state lock should be available");
                            state.status = BridgeStatusSnapshot {
                                state: BridgeState::Stopped,
                                mode: stop_status.mode,
                                source: stop_status.source,
                                name: stop_name.clone(),
                                bridge_session_id: stop_status.bridge_session_id.clone(),
                                environment_id: stop_status.environment_id.clone(),
                                local_session_id: stop_status.local_session_id.clone(),
                                transcript_path: stop_status.transcript_path.clone(),
                                last_error: None,
                            };
                            break;
                        }
                        WorkerNext::Disconnected(reason) => {
                            let _ = host.notify_transport_closed(&reason);
                            let _ = active_session.close();
                            let started = Instant::now();
                            let mut delay = reconnect_policy.initial_delay;
                            let mut reconnected = None;

                            while started.elapsed() < reconnect_policy.give_up_after {
                                {
                                    let mut state = state
                                        .lock()
                                        .expect("bridge manager state lock should be available");
                                    state.status = BridgeStatusSnapshot {
                                        state: BridgeState::Reconnecting,
                                        mode: stop_status.mode,
                                        source: stop_status.source,
                                        name: stop_name.clone(),
                                        bridge_session_id: stop_status.bridge_session_id.clone(),
                                        environment_id: stop_status.environment_id.clone(),
                                        local_session_id: stop_status.local_session_id.clone(),
                                        transcript_path: stop_status.transcript_path.clone(),
                                        last_error: Some(reason.clone()),
                                    };
                                }

                                if rx.recv_timeout(delay).is_ok() {
                                    let mut state = state
                                        .lock()
                                        .expect("bridge manager state lock should be available");
                                    state.status = BridgeStatusSnapshot {
                                        state: BridgeState::Stopped,
                                        mode: stop_status.mode,
                                        source: stop_status.source,
                                        name: stop_name.clone(),
                                        bridge_session_id: stop_status.bridge_session_id.clone(),
                                        environment_id: stop_status.environment_id.clone(),
                                        local_session_id: stop_status.local_session_id.clone(),
                                        transcript_path: stop_status.transcript_path.clone(),
                                        last_error: None,
                                    };
                                    return;
                                }

                                match connector.connect(&connect_request) {
                                    Ok(mut replacement) => {
                                        let replacement_pointer = BridgePointer {
                                            bridge_session_id: replacement
                                                .bridge_session_id()
                                                .to_owned(),
                                            environment_id: replacement.environment_id().to_owned(),
                                            source,
                                            local_session_id: runtime.session_id().clone(),
                                            transcript_path: transcript_path.clone(),
                                        };
                                        let _ = pointer_store.save(&runtime, &replacement_pointer);
                                        let _ = replacement.send_output(
                                            &StructuredOutputMessage::SessionStarted {
                                                session_id: runtime
                                                    .session_id()
                                                    .as_str()
                                                    .to_owned(),
                                            },
                                        );
                                        {
                                            let mut state = state.lock().expect(
                                                "bridge manager state lock should be available",
                                            );
                                            state.status = BridgeStatusSnapshot {
                                                state: BridgeState::Connected,
                                                mode: Some(mode),
                                                source: Some(source),
                                                name: stop_name.clone(),
                                                bridge_session_id: Some(
                                                    replacement_pointer.bridge_session_id,
                                                ),
                                                environment_id: Some(
                                                    replacement_pointer.environment_id,
                                                ),
                                                local_session_id: Some(
                                                    replacement_pointer.local_session_id,
                                                ),
                                                transcript_path: Some(
                                                    replacement_pointer.transcript_path,
                                                ),
                                                last_error: None,
                                            };
                                        }
                                        reconnected = Some(replacement);
                                        break;
                                    }
                                    Err(error) => {
                                        delay = std::cmp::min(
                                            delay.saturating_mul(2),
                                            reconnect_policy.max_delay,
                                        );
                                        {
                                            let mut state = state.lock().expect(
                                                "bridge manager state lock should be available",
                                            );
                                            state.status.last_error = Some(error.to_string());
                                        }
                                    }
                                }
                            }

                            if let Some(replacement) = reconnected {
                                active_session = replacement;
                                continue;
                            }

                            let mut state = state
                                .lock()
                                .expect("bridge manager state lock should be available");
                            state.status = BridgeStatusSnapshot {
                                state: BridgeState::Failed,
                                mode: stop_status.mode,
                                source: stop_status.source,
                                name: stop_name.clone(),
                                bridge_session_id: stop_status.bridge_session_id.clone(),
                                environment_id: stop_status.environment_id.clone(),
                                local_session_id: stop_status.local_session_id.clone(),
                                transcript_path: stop_status.transcript_path.clone(),
                                last_error: Some(reason),
                            };
                            break;
                        }
                        WorkerNext::Failed(message) => {
                            let mut state = state
                                .lock()
                                .expect("bridge manager state lock should be available");
                            state.status = BridgeStatusSnapshot {
                                state: BridgeState::Failed,
                                mode: stop_status.mode,
                                source: stop_status.source,
                                name: stop_name.clone(),
                                bridge_session_id: stop_status.bridge_session_id.clone(),
                                environment_id: stop_status.environment_id.clone(),
                                local_session_id: stop_status.local_session_id.clone(),
                                transcript_path: stop_status.transcript_path.clone(),
                                last_error: Some(message),
                            };
                            break;
                        }
                    }
                }
            });
            let mut state = self
                .state
                .lock()
                .expect("bridge manager state lock should be available");
            state.worker = Some(ActiveWorker {
                stop_tx: tx.clone(),
                handle,
                pointer_path,
            });
            tx
        };
        drop(stop_tx);

        Ok(initial_status)
    }

    fn stop(&self) -> ClawinResult<BridgeStatusSnapshot> {
        self.cleanup_finished_worker()?;
        let worker = self
            .state
            .lock()
            .expect("bridge manager state lock should be available")
            .worker
            .take();

        let Some(worker) = worker else {
            let mut status = self.status()?;
            status.state = BridgeState::Stopped;
            {
                let mut state = self
                    .state
                    .lock()
                    .expect("bridge manager state lock should be available");
                state.status = status.clone();
            }
            return Ok(status);
        };

        let _ = worker.stop_tx.send(());
        let _ = worker.handle.join();
        let _ = self.pointer_store.remove_path(&worker.pointer_path);
        self.status()
    }

    fn wait_for_terminal_state(&self) -> ClawinResult<BridgeStatusSnapshot> {
        loop {
            self.cleanup_finished_worker()?;
            let status = self
                .state
                .lock()
                .expect("bridge manager state lock should be available")
                .status
                .clone();

            if matches!(
                status.state,
                BridgeState::Failed | BridgeState::Stopped | BridgeState::Ready
            ) {
                return Ok(status);
            }

            thread::sleep(self.reconnect_policy.poll_interval);
        }
    }
}

enum WorkerNext {
    Stopped,
    Disconnected(String),
    Failed(String),
}

fn pump_transport(
    session: &mut dyn BridgeTransportSession,
    host: &dyn BridgeSessionHost,
    stop_rx: &Receiver<()>,
    poll_interval: Duration,
) -> WorkerNext {
    loop {
        if stop_rx.try_recv().is_ok() {
            return WorkerNext::Stopped;
        }

        match session.poll_input(poll_interval) {
            Ok(BridgeTransportPoll::Message(message)) => {
                if let Err(error) = host.send_input(message) {
                    return WorkerNext::Failed(error.to_string());
                }
            }
            Ok(BridgeTransportPoll::Idle) => {}
            Ok(BridgeTransportPoll::Disconnected) => {
                return WorkerNext::Disconnected("transport_disconnected".to_owned());
            }
            Err(error) => return WorkerNext::Disconnected(error.to_string()),
        }

        loop {
            match host.recv_output(Duration::from_millis(1)) {
                Ok(Some(message)) => {
                    if let Err(error) = session.send_output(&message) {
                        return WorkerNext::Disconnected(error.to_string());
                    }
                }
                Ok(None) => break,
                Err(error) => return WorkerNext::Failed(error.to_string()),
            }
        }
    }
}

fn is_stale(metadata: &fs::Metadata, ttl: Duration) -> bool {
    let Ok(modified) = metadata.modified() else {
        return false;
    };
    SystemTime::now()
        .duration_since(modified)
        .map(|age| age > ttl)
        .unwrap_or(false)
}

fn map_git_error(error: std::io::Error) -> ClawinError {
    ClawinError::InvalidConfiguration {
        message: format!("git worktree operation failed: {error}"),
    }
}

/// Fake remote handle used by integration tests to drive bridge traffic.
#[derive(Debug)]
pub struct FakeBridgeRemote {
    input_tx: Mutex<Option<Sender<StructuredInputMessage>>>,
    output_rx: Mutex<Option<Receiver<StructuredOutputMessage>>>,
}

impl FakeBridgeRemote {
    pub fn send(&self, message: StructuredInputMessage) -> ClawinResult<()> {
        let Some(sender) = self
            .input_tx
            .lock()
            .expect("fake bridge remote input lock should be available")
            .as_ref()
            .cloned()
        else {
            return Err(ClawinError::InvalidConfiguration {
                message: "fake remote input channel is closed".to_owned(),
            });
        };
        sender
            .send(message)
            .map_err(|error| ClawinError::InvalidConfiguration {
                message: format!("failed to send fake bridge input: {error}"),
            })
    }

    pub fn recv_timeout(&self, timeout: Duration) -> Option<StructuredOutputMessage> {
        self.output_rx
            .lock()
            .expect("fake bridge remote output lock should be available")
            .as_ref()
            .and_then(|receiver| receiver.recv_timeout(timeout).ok())
    }

    pub fn disconnect(&self) {
        self.input_tx
            .lock()
            .expect("fake bridge remote input lock should be available")
            .take();
        self.output_rx
            .lock()
            .expect("fake bridge remote output lock should be available")
            .take();
    }
}

#[derive(Debug)]
struct FakeBridgeSessionPlan {
    bridge_session_id: String,
    environment_id: String,
    input_rx: Receiver<StructuredInputMessage>,
    output_tx: Sender<StructuredOutputMessage>,
}

/// Fake transport connector used by bridge integration tests.
#[derive(Clone, Debug, Default)]
pub struct FakeBridgeConnector {
    sessions: Arc<Mutex<VecDeque<FakeBridgeSessionPlan>>>,
}

impl FakeBridgeConnector {
    pub fn with_sessions(
        sessions: Vec<(String, String, FakeBridgeRemote)>,
    ) -> (Arc<Self>, Vec<Arc<FakeBridgeRemote>>) {
        let mut plans = VecDeque::new();
        let mut remotes = Vec::new();

        for (bridge_session_id, environment_id, remote) in sessions {
            let remote = Arc::new(remote);
            let (input_tx, input_rx) = mpsc::channel();
            let (output_tx, output_rx) = mpsc::channel();
            *remote
                .input_tx
                .lock()
                .expect("fake bridge remote input lock should be available") = Some(input_tx);
            *remote
                .output_rx
                .lock()
                .expect("fake bridge remote output lock should be available") = Some(output_rx);
            plans.push_back(FakeBridgeSessionPlan {
                bridge_session_id,
                environment_id,
                input_rx,
                output_tx,
            });
            remotes.push(remote);
        }

        (
            Arc::new(Self {
                sessions: Arc::new(Mutex::new(plans)),
            }),
            remotes,
        )
    }

    pub fn empty_remote() -> FakeBridgeRemote {
        FakeBridgeRemote {
            input_tx: Mutex::new(None),
            output_rx: Mutex::new(None),
        }
    }
}

impl BridgeTransportConnector for FakeBridgeConnector {
    fn connect(
        &self,
        _request: &BridgeConnectRequest,
    ) -> ClawinResult<Box<dyn BridgeTransportSession>> {
        let plan = self
            .sessions
            .lock()
            .expect("fake bridge connector lock should be available")
            .pop_front()
            .ok_or_else(|| ClawinError::InvalidConfiguration {
                message: "no fake bridge session plan remains".to_owned(),
            })?;

        Ok(Box::new(FakeBridgeTransportSession {
            bridge_session_id: plan.bridge_session_id,
            environment_id: plan.environment_id,
            input_rx: plan.input_rx,
            output_tx: plan.output_tx,
            closed: AtomicBool::new(false),
        }))
    }
}

struct FakeBridgeTransportSession {
    bridge_session_id: String,
    environment_id: String,
    input_rx: Receiver<StructuredInputMessage>,
    output_tx: Sender<StructuredOutputMessage>,
    closed: AtomicBool,
}

impl BridgeTransportSession for FakeBridgeTransportSession {
    fn bridge_session_id(&self) -> &str {
        &self.bridge_session_id
    }

    fn environment_id(&self) -> &str {
        &self.environment_id
    }

    fn poll_input(&mut self, timeout: Duration) -> ClawinResult<BridgeTransportPoll> {
        if self.closed.load(Ordering::SeqCst) {
            return Ok(BridgeTransportPoll::Disconnected);
        }

        match self.input_rx.recv_timeout(timeout) {
            Ok(message) => Ok(BridgeTransportPoll::Message(message)),
            Err(RecvTimeoutError::Timeout) => Ok(BridgeTransportPoll::Idle),
            Err(RecvTimeoutError::Disconnected) => Ok(BridgeTransportPoll::Disconnected),
        }
    }

    fn send_output(&mut self, message: &StructuredOutputMessage) -> ClawinResult<()> {
        self.output_tx
            .send(message.clone())
            .map_err(|error| ClawinError::InvalidConfiguration {
                message: format!("failed to send fake bridge output: {error}"),
            })
    }

    fn close(&mut self) -> ClawinResult<()> {
        self.closed.store(true, Ordering::SeqCst);
        Ok(())
    }
}
