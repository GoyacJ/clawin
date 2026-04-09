// Phase 7A tests continue under DIFF-2026-001: worktree enter/exit flows stay in Clawin-owned tool/runtime boundaries.

use std::sync::{Arc, Mutex};

use clawin_core::{
    ClawinResult, ConversationMessage, PersistedWorktreeSession, RestoredSession, ResumeQuery,
    RuntimeCapabilities, SessionId, SessionPreview, SessionRuntime, SessionStore,
    WorktreeExitAction, WorktreeManager,
};
use clawin_tools::builtin_tool_registry;
use serde_json::json;

#[test]
fn enter_and_exit_worktree_updates_runtime_state() {
    let project_root = std::env::temp_dir().join("clawin-worktree-test");
    let worktree = PersistedWorktreeSession::new(
        project_root.clone(),
        project_root.join(".clawin/worktrees/feature-a"),
        Some("worktree-feature-a".to_owned()),
        true,
    );
    let runtime = SessionRuntime::new(
        SessionId::from_static("tools-worktree"),
        RuntimeCapabilities::new(false, false),
        project_root.clone(),
        project_root.clone(),
        clawin_core::PermissionMode::Default,
    )
    .with_session_store(Arc::new(NoopSessionStore))
    .with_worktree_manager(Arc::new(FakeWorktreeManager::new(worktree.clone())));

    let registry = builtin_tool_registry();

    let enter = registry
        .execute(
            clawin_core::ToolCall::new(
                "toolu_enter",
                "EnterWorktree",
                json!({ "name": "feature-a" }),
            ),
            &runtime,
        )
        .expect("enter worktree should execute");
    assert!(!enter.result.is_error);
    assert_eq!(runtime.active_project_root(), worktree.worktree_path);
    assert_eq!(
        runtime
            .active_worktree()
            .expect("active worktree should be set"),
        worktree
    );

    let exit = registry
        .execute(
            clawin_core::ToolCall::new(
                "toolu_exit",
                "ExitWorktree",
                json!({ "action": "remove", "discard_changes": true }),
            ),
            &runtime,
        )
        .expect("exit worktree should execute");
    assert!(!exit.result.is_error);
    assert!(runtime.active_worktree().is_none());
    assert_eq!(runtime.active_project_root(), project_root);
}

#[derive(Debug)]
struct NoopSessionStore;

impl SessionStore for NoopSessionStore {
    fn initialize_session(&self, _runtime: &SessionRuntime) -> ClawinResult<()> {
        Ok(())
    }

    fn save_last_prompt(&self, _runtime: &SessionRuntime, _prompt: &str) -> ClawinResult<()> {
        Ok(())
    }

    fn append_message(
        &self,
        _runtime: &SessionRuntime,
        _message: &ConversationMessage,
    ) -> ClawinResult<()> {
        Ok(())
    }

    fn save_worktree_state(
        &self,
        _runtime: &SessionRuntime,
        _worktree: Option<&PersistedWorktreeSession>,
    ) -> ClawinResult<()> {
        Ok(())
    }

    fn list_recent_sessions(&self, _runtime: &SessionRuntime) -> ClawinResult<Vec<SessionPreview>> {
        Ok(Vec::new())
    }

    fn resolve_resume(
        &self,
        _runtime: &SessionRuntime,
        _query: ResumeQuery,
    ) -> ClawinResult<Option<RestoredSession>> {
        Ok(None)
    }
}

#[derive(Debug)]
struct FakeWorktreeManager {
    worktree: PersistedWorktreeSession,
    exit_calls: Mutex<Vec<WorktreeExitAction>>,
}

impl FakeWorktreeManager {
    fn new(worktree: PersistedWorktreeSession) -> Self {
        Self {
            worktree,
            exit_calls: Mutex::new(Vec::new()),
        }
    }
}

impl WorktreeManager for FakeWorktreeManager {
    fn enter_worktree(
        &self,
        runtime: &SessionRuntime,
        _name: Option<&str>,
    ) -> ClawinResult<PersistedWorktreeSession> {
        runtime.set_active_project_root(self.worktree.worktree_path.clone());
        runtime.set_active_worktree(Some(self.worktree.clone()));
        Ok(self.worktree.clone())
    }

    fn exit_worktree(
        &self,
        runtime: &SessionRuntime,
        action: WorktreeExitAction,
        _discard_changes: bool,
    ) -> ClawinResult<Option<PersistedWorktreeSession>> {
        self.exit_calls
            .lock()
            .expect("exit calls lock should be available")
            .push(action);
        let previous = runtime.active_worktree();
        runtime.set_active_project_root(runtime.canonical_project_root().to_path_buf());
        runtime.set_active_worktree(None);
        Ok(previous)
    }
}
