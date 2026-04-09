use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

use clawin_config::load_startup_config;
use clawin_core::{
    BridgeController, BridgeMode, BridgePointer, BridgePointerSource, BridgeSessionHost,
    BridgeState, PermissionMode, RuntimeCapabilities, SessionId, SessionRuntime,
    StructuredInputMessage, StructuredOutputMessage,
};
use clawin_integrations::{
    BridgeManager, BridgePointerStore, FakeBridgeConnector, ReconnectPolicy,
};
use clawin_platform::{FakeGitWorktreeAdapter, PathPolicy};
use tempfile::TempDir;

#[test]
fn bridge_pointer_store_prefers_freshest_same_repo_pointer() {
    let harness = Harness::new();
    let snapshot = harness.load_config();
    let git = Arc::new(FakeGitWorktreeAdapter::new());
    git.register_repository(
        harness.project_dir.clone(),
        vec![harness.project_dir.clone(), harness.worktree_dir.clone()],
    );
    let store = BridgePointerStore::new(
        snapshot.paths().clone(),
        harness.path_policy(),
        Arc::clone(&git),
    );

    let main_runtime = harness.runtime("bridge-main", harness.project_dir.clone());
    let worktree_runtime = harness.runtime("bridge-worktree", harness.worktree_dir.clone());

    store
        .save(
            &main_runtime,
            &BridgePointer {
                bridge_session_id: "bridge-main".to_owned(),
                environment_id: "env-main".to_owned(),
                source: BridgePointerSource::Standalone,
                local_session_id: main_runtime.session_id().clone(),
                transcript_path: store.transcript_path(&main_runtime),
            },
        )
        .expect("main bridge pointer should save");
    thread::sleep(Duration::from_millis(10));
    store
        .save(
            &worktree_runtime,
            &BridgePointer {
                bridge_session_id: "bridge-worktree".to_owned(),
                environment_id: "env-worktree".to_owned(),
                source: BridgePointerSource::Repl,
                local_session_id: worktree_runtime.session_id().clone(),
                transcript_path: store.transcript_path(&worktree_runtime),
            },
        )
        .expect("worktree bridge pointer should save");

    let resolved = store
        .resolve_continue(&main_runtime)
        .expect("continue pointer should resolve")
        .expect("freshest pointer should exist");

    assert_eq!(resolved.bridge_session_id, "bridge-worktree");
    assert_eq!(resolved.environment_id, "env-worktree");
    assert_eq!(resolved.source, BridgePointerSource::Repl);
}

#[test]
fn stale_bridge_pointer_is_cleaned_when_loaded() {
    let harness = Harness::new();
    let snapshot = harness.load_config();
    let git = Arc::new(FakeGitWorktreeAdapter::new());
    let store = BridgePointerStore::with_policy(
        snapshot.paths().clone(),
        harness.path_policy(),
        git,
        Duration::from_millis(1),
        50,
    );
    let runtime = harness.runtime("bridge-stale", harness.project_dir.clone());
    let path = store
        .save(
            &runtime,
            &BridgePointer {
                bridge_session_id: "bridge-stale".to_owned(),
                environment_id: "env-stale".to_owned(),
                source: BridgePointerSource::Standalone,
                local_session_id: runtime.session_id().clone(),
                transcript_path: store.transcript_path(&runtime),
            },
        )
        .expect("stale pointer should save");

    thread::sleep(Duration::from_millis(5));

    assert!(
        store
            .load(&path)
            .expect("stale pointer load should succeed")
            .is_none(),
        "stale pointer should be treated as missing"
    );
    assert!(!path.exists(), "stale pointer should be removed");
}

#[test]
fn bridge_manager_transitions_from_connected_to_failed_after_disconnect_give_up() {
    let harness = Harness::new();
    let snapshot = harness.load_config();
    let git = Arc::new(FakeGitWorktreeAdapter::new());
    let (connector, remotes) = FakeBridgeConnector::with_sessions(vec![(
        "bridge-session-1".to_owned(),
        "env-1".to_owned(),
        FakeBridgeConnector::empty_remote(),
    )]);
    let manager = BridgeManager::with_policy(
        snapshot.paths().clone(),
        harness.path_policy(),
        git,
        connector,
        ReconnectPolicy {
            initial_delay: Duration::from_millis(5),
            max_delay: Duration::from_millis(10),
            give_up_after: Duration::from_millis(25),
            poll_interval: Duration::from_millis(5),
        },
    );
    let runtime = harness.runtime("bridge-runtime", harness.project_dir.clone());
    let host = Arc::new(RecordingHost::default());

    let status = manager
        .start(
            &runtime,
            host.clone(),
            BridgeMode::Standalone,
            BridgePointerSource::Standalone,
            Some("demo".to_owned()),
            None,
        )
        .expect("bridge manager should start");

    assert_eq!(status.state, BridgeState::Connected);
    assert!(matches!(
        remotes[0].recv_timeout(Duration::from_millis(50)),
        Some(StructuredOutputMessage::SessionStarted { session_id })
            if session_id == runtime.session_id().as_str()
    ));

    remotes[0].disconnect();
    let final_status = manager
        .wait_for_terminal_state()
        .expect("bridge manager should stop waiting");

    assert_eq!(final_status.state, BridgeState::Failed);
    assert!(
        host.closed_reasons()
            .iter()
            .any(|reason| reason == "transport_disconnected")
    );
}

#[derive(Default)]
struct RecordingHost {
    closed: Mutex<Vec<String>>,
}

impl RecordingHost {
    fn closed_reasons(&self) -> Vec<String> {
        self.closed
            .lock()
            .expect("closed reasons lock should be available")
            .clone()
    }
}

impl BridgeSessionHost for RecordingHost {
    fn send_input(&self, _message: StructuredInputMessage) -> clawin_core::ClawinResult<()> {
        Ok(())
    }

    fn recv_output(
        &self,
        _timeout: Duration,
    ) -> clawin_core::ClawinResult<Option<StructuredOutputMessage>> {
        Ok(None)
    }

    fn notify_transport_closed(&self, reason: &str) -> clawin_core::ClawinResult<()> {
        self.closed
            .lock()
            .expect("closed reasons lock should be available")
            .push(reason.to_owned());
        Ok(())
    }
}

struct Harness {
    _tempdir: TempDir,
    home_dir: PathBuf,
    project_dir: PathBuf,
    worktree_dir: PathBuf,
}

impl Harness {
    fn new() -> Self {
        let tempdir = tempfile::tempdir().expect("tempdir should exist");
        let home_dir = tempdir.path().join("home");
        let project_dir = tempdir.path().join("workspace").join("app");
        let worktree_dir = tempdir.path().join("workspace").join("app-worktree");
        fs::create_dir_all(home_dir.join(".clawin")).expect("global dir should exist");
        fs::create_dir_all(&project_dir).expect("project dir should exist");
        fs::create_dir_all(&worktree_dir).expect("worktree dir should exist");

        Self {
            _tempdir: tempdir,
            home_dir,
            project_dir,
            worktree_dir,
        }
    }

    fn load_config(&self) -> clawin_config::LoadedConfigSnapshot {
        load_startup_config(self.project_dir.clone(), &self.path_policy())
            .expect("startup config should load")
    }

    fn path_policy(&self) -> TestPathPolicy {
        TestPathPolicy {
            home_dir: self.home_dir.clone(),
        }
    }

    fn runtime(&self, session_id: &str, active_project_root: PathBuf) -> SessionRuntime {
        SessionRuntime::new(
            SessionId::from_owned(session_id),
            RuntimeCapabilities::new(false, false),
            active_project_root.clone(),
            self.project_dir.clone(),
            PermissionMode::Default,
        )
        .with_active_project_root(active_project_root)
    }
}

#[derive(Clone, Debug)]
struct TestPathPolicy {
    home_dir: PathBuf,
}

impl PathPolicy for TestPathPolicy {
    fn home_dir(&self) -> Option<PathBuf> {
        Some(self.home_dir.clone())
    }

    fn normalize_for_config_key(&self, path: &std::path::Path) -> String {
        path.to_string_lossy().replace('\\', "/")
    }

    fn project_directory_name(&self) -> &'static str {
        ".clawin"
    }

    fn project_manifest_name(&self) -> &'static str {
        "CLAWIN.md"
    }
}
