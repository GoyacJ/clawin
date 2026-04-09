use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::SystemTime;

use crate::{
    BridgeController, PermissionMode, PersistedWorktreeSession, SessionId, SessionServices,
    SessionStore, WorktreeManager,
};

/// Minimal process/session-scoped capabilities exposed during Phase 1.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RuntimeCapabilities {
    interactive_terminal: bool,
    mcp_available: bool,
}

impl RuntimeCapabilities {
    /// Create a new runtime capability snapshot.
    pub fn new(interactive_terminal: bool, mcp_available: bool) -> Self {
        Self {
            interactive_terminal,
            mcp_available,
        }
    }

    /// Whether the current process can drive an interactive terminal UI.
    pub fn interactive_terminal(self) -> bool {
        self.interactive_terminal
    }

    /// Whether MCP transports are currently wired.
    pub fn mcp_available(self) -> bool {
        self.mcp_available
    }
}

/// Minimal session-scoped runtime state placeholder.
#[derive(Clone)]
pub struct SessionRuntime {
    session_id: SessionId,
    launched_at: SystemTime,
    capabilities: RuntimeCapabilities,
    launch_cwd: PathBuf,
    canonical_project_root: PathBuf,
    permission_mode: PermissionMode,
    state: Arc<RwLock<SessionRuntimeState>>,
    services: Arc<RwLock<SessionServices>>,
}

#[derive(Clone, Debug)]
struct SessionRuntimeState {
    active_project_root: PathBuf,
    current_cwd: PathBuf,
    active_worktree: Option<PersistedWorktreeSession>,
}

impl SessionRuntime {
    /// Create a new runtime container for the current process/session.
    pub fn new(
        session_id: SessionId,
        capabilities: RuntimeCapabilities,
        original_cwd: PathBuf,
        project_root: PathBuf,
        permission_mode: PermissionMode,
    ) -> Self {
        Self {
            session_id,
            launched_at: SystemTime::now(),
            capabilities,
            launch_cwd: original_cwd.clone(),
            canonical_project_root: project_root.clone(),
            permission_mode,
            state: Arc::new(RwLock::new(SessionRuntimeState {
                active_project_root: project_root,
                current_cwd: original_cwd,
                active_worktree: None,
            })),
            services: Arc::new(RwLock::new(SessionServices::default())),
        }
    }

    /// Borrow the current session identifier.
    pub fn session_id(&self) -> &SessionId {
        &self.session_id
    }

    /// Read the launch timestamp.
    pub fn launched_at(&self) -> SystemTime {
        self.launched_at
    }

    /// Read the currently known runtime capabilities.
    pub fn capabilities(&self) -> RuntimeCapabilities {
        self.capabilities
    }

    /// Borrow the cwd that Clawin started from.
    pub fn original_cwd(&self) -> &Path {
        &self.launch_cwd
    }

    /// Borrow the cwd that Clawin launched from.
    pub fn launch_cwd(&self) -> &Path {
        &self.launch_cwd
    }

    /// Borrow the resolved project root for the session.
    pub fn project_root(&self) -> &Path {
        &self.canonical_project_root
    }

    /// Borrow the canonical project root for the session.
    pub fn canonical_project_root(&self) -> &Path {
        &self.canonical_project_root
    }

    /// Read the current active project root, which may point at a session-owned worktree.
    pub fn active_project_root(&self) -> PathBuf {
        self.state
            .read()
            .expect("session runtime state lock should be available")
            .active_project_root
            .clone()
    }

    /// Read the current cwd inside the active project root.
    pub fn current_cwd(&self) -> PathBuf {
        self.state
            .read()
            .expect("session runtime state lock should be available")
            .current_cwd
            .clone()
    }

    /// Read the current active worktree snapshot, if any.
    pub fn active_worktree(&self) -> Option<PersistedWorktreeSession> {
        self.state
            .read()
            .expect("session runtime state lock should be available")
            .active_worktree
            .clone()
    }

    /// Borrow the current permission mode.
    pub fn permission_mode(&self) -> PermissionMode {
        self.permission_mode
    }

    /// Return a cloned runtime whose active project root has been moved to the provided path.
    pub fn with_active_project_root(self, path: PathBuf) -> Self {
        self.set_active_project_root(path);
        self
    }

    /// Return a cloned runtime whose cwd has been moved to the provided path.
    pub fn with_current_cwd(self, path: PathBuf) -> Self {
        self.set_current_cwd(path);
        self
    }

    /// Return a cloned runtime with an active worktree snapshot attached.
    pub fn with_active_worktree(self, worktree: PersistedWorktreeSession) -> Self {
        self.set_active_worktree(Some(worktree));
        self
    }

    /// Return a cloned runtime with a session store service attached.
    pub fn with_session_store(self, store: Arc<dyn SessionStore>) -> Self {
        self.set_session_store(store);
        self
    }

    /// Return a cloned runtime with a worktree manager service attached.
    pub fn with_worktree_manager(self, manager: Arc<dyn WorktreeManager>) -> Self {
        self.set_worktree_manager(manager);
        self
    }

    /// Return a cloned runtime with a bridge controller service attached.
    pub fn with_bridge_controller(self, controller: Arc<dyn BridgeController>) -> Self {
        self.set_bridge_controller(controller);
        self
    }

    /// Update the current active project root.
    pub fn set_active_project_root(&self, path: PathBuf) {
        let mut state = self
            .state
            .write()
            .expect("session runtime state lock should be available");
        state.active_project_root = path.clone();
        state.current_cwd = path;
    }

    /// Update the current cwd.
    pub fn set_current_cwd(&self, path: PathBuf) {
        self.state
            .write()
            .expect("session runtime state lock should be available")
            .current_cwd = path;
    }

    /// Update the current active worktree snapshot.
    pub fn set_active_worktree(&self, worktree: Option<PersistedWorktreeSession>) {
        self.state
            .write()
            .expect("session runtime state lock should be available")
            .active_worktree = worktree;
    }

    /// Replace the session store service for this runtime.
    pub fn set_session_store(&self, store: Arc<dyn SessionStore>) {
        self.services
            .write()
            .expect("session runtime services lock should be available")
            .session_store = Some(store);
    }

    /// Replace the worktree manager service for this runtime.
    pub fn set_worktree_manager(&self, manager: Arc<dyn WorktreeManager>) {
        self.services
            .write()
            .expect("session runtime services lock should be available")
            .worktree_manager = Some(manager);
    }

    /// Replace the bridge controller service for this runtime.
    pub fn set_bridge_controller(&self, controller: Arc<dyn BridgeController>) {
        self.services
            .write()
            .expect("session runtime services lock should be available")
            .bridge_controller = Some(controller);
    }

    /// Borrow the currently attached session store service, if one exists.
    pub fn session_store(&self) -> Option<Arc<dyn SessionStore>> {
        self.services
            .read()
            .expect("session runtime services lock should be available")
            .session_store
            .clone()
    }

    /// Borrow the currently attached worktree manager service, if one exists.
    pub fn worktree_manager(&self) -> Option<Arc<dyn WorktreeManager>> {
        self.services
            .read()
            .expect("session runtime services lock should be available")
            .worktree_manager
            .clone()
    }

    /// Borrow the currently attached bridge controller service, if one exists.
    pub fn bridge_controller(&self) -> Option<Arc<dyn BridgeController>> {
        self.services
            .read()
            .expect("session runtime services lock should be available")
            .bridge_controller
            .clone()
    }
}

impl std::fmt::Debug for SessionRuntime {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let state = self
            .state
            .read()
            .expect("session runtime state lock should be available");
        formatter
            .debug_struct("SessionRuntime")
            .field("session_id", &self.session_id)
            .field("launched_at", &self.launched_at)
            .field("capabilities", &self.capabilities)
            .field("launch_cwd", &self.launch_cwd)
            .field("canonical_project_root", &self.canonical_project_root)
            .field("permission_mode", &self.permission_mode)
            .field("active_project_root", &state.active_project_root)
            .field("current_cwd", &state.current_cwd)
            .field("active_worktree", &state.active_worktree)
            .field("has_session_store", &self.session_store().is_some())
            .field("has_worktree_manager", &self.worktree_manager().is_some())
            .field("has_bridge_controller", &self.bridge_controller().is_some())
            .finish()
    }
}
