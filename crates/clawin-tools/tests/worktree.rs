// Phase 7A tests continue under DIFF-2026-001: worktree enter/exit flows stay in Clawin-owned tool/runtime boundaries.

use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use clawin_core::{
    ClawinError, ClawinResult, ConversationMessage, PersistedWorktreeSession, RestoredSession,
    ResumeQuery, RuntimeCapabilities, SessionId, SessionPreview, SessionRuntime, SessionStore,
    ToolCall, WorktreeExitAction, WorktreeManager,
};
use clawin_tools::builtin_tool_registry;
use serde_json::{Value, json};

#[test]
fn enter_worktree_returns_stable_result_and_persists_state() {
    let project_root = fixture_project_root();
    let worktree = PersistedWorktreeSession::new(
        project_root.clone(),
        project_root.join(".clawin/worktrees/feature-a"),
        Some("worktree-feature-a".to_owned()),
        true,
    );
    let store = Arc::new(RecordingSessionStore::default());
    let runtime = SessionRuntime::new(
        SessionId::from_static("tools-worktree"),
        RuntimeCapabilities::new(false, false),
        project_root.clone(),
        project_root.clone(),
        clawin_core::PermissionMode::Default,
    )
    .with_session_store(store.clone())
    .with_worktree_manager(Arc::new(FakeWorktreeManager::new(worktree.clone())));

    let enter = builtin_tool_registry()
        .execute(
            ToolCall::new(
                "toolu_enter",
                "EnterWorktree",
                json!({ "name": "feature-a" }),
            ),
            &runtime,
        )
        .expect("enter worktree should execute");

    assert!(!enter.result.is_error);
    assert_eq!(
        enter.result.content,
        fixture_json(
            "tests/fixtures/enter_worktree_result.json",
            &[(
                "__WORKTREE_PATH__",
                worktree.worktree_path.display().to_string(),
            )],
        )
    );
    assert_eq!(runtime.active_project_root(), worktree.worktree_path);
    assert_eq!(
        runtime
            .active_worktree()
            .expect("active worktree should be set"),
        worktree.clone()
    );
    assert_eq!(store.saved_worktree_states(), vec![Some(worktree)]);
}

#[test]
fn exit_worktree_keep_returns_stable_result_and_clears_runtime_state() {
    let project_root = fixture_project_root();
    let worktree = PersistedWorktreeSession::new(
        project_root.clone(),
        project_root.join(".clawin/worktrees/feature-a"),
        Some("worktree-feature-a".to_owned()),
        true,
    );
    let worktree_path = worktree.worktree_path.clone();
    let store = Arc::new(RecordingSessionStore::default());
    let runtime = SessionRuntime::new(
        SessionId::from_static("tools-worktree"),
        RuntimeCapabilities::new(false, false),
        project_root.clone(),
        project_root.clone(),
        clawin_core::PermissionMode::Default,
    )
    .with_session_store(store.clone())
    .with_worktree_manager(Arc::new(FakeWorktreeManager::new(worktree.clone())));
    runtime.set_active_project_root(worktree.worktree_path.clone());
    runtime.set_active_worktree(Some(worktree));

    let exit = builtin_tool_registry()
        .execute(
            ToolCall::new("toolu_exit", "ExitWorktree", json!({ "action": "keep" })),
            &runtime,
        )
        .expect("exit keep should execute");

    assert!(!exit.result.is_error);
    assert_eq!(
        exit.result.content,
        fixture_json(
            "tests/fixtures/exit_worktree_keep_result.json",
            &[("__WORKTREE_PATH__", worktree_path.display().to_string())],
        )
    );
    assert!(runtime.active_worktree().is_none());
    assert_eq!(runtime.active_project_root(), project_root);
    assert_eq!(store.saved_worktree_states(), vec![None]);
}

#[test]
fn exit_worktree_remove_returns_stable_result() {
    let project_root = fixture_project_root();
    let worktree = PersistedWorktreeSession::new(
        project_root.clone(),
        project_root.join(".clawin/worktrees/feature-a"),
        Some("worktree-feature-a".to_owned()),
        true,
    );
    let worktree_path = worktree.worktree_path.clone();
    let runtime = SessionRuntime::new(
        SessionId::from_static("tools-worktree"),
        RuntimeCapabilities::new(false, false),
        project_root.clone(),
        project_root.clone(),
        clawin_core::PermissionMode::Default,
    )
    .with_session_store(Arc::new(RecordingSessionStore::default()))
    .with_worktree_manager(Arc::new(FakeWorktreeManager::new(worktree.clone())));
    runtime.set_active_project_root(worktree.worktree_path.clone());
    runtime.set_active_worktree(Some(worktree));

    let exit = builtin_tool_registry()
        .execute(
            ToolCall::new(
                "toolu_exit",
                "ExitWorktree",
                json!({ "action": "remove", "discard_changes": true }),
            ),
            &runtime,
        )
        .expect("exit remove should execute");

    assert!(!exit.result.is_error);
    assert_eq!(
        exit.result.content,
        fixture_json(
            "tests/fixtures/exit_worktree_remove_result.json",
            &[("__WORKTREE_PATH__", worktree_path.display().to_string())],
        )
    );
}

#[test]
fn exit_worktree_without_active_worktree_returns_stable_noop_result() {
    let project_root = fixture_project_root();
    let runtime = SessionRuntime::new(
        SessionId::from_static("tools-worktree"),
        RuntimeCapabilities::new(false, false),
        project_root.clone(),
        project_root.clone(),
        clawin_core::PermissionMode::Default,
    )
    .with_session_store(Arc::new(RecordingSessionStore::default()))
    .with_worktree_manager(Arc::new(FakeWorktreeManager::without_worktree()));

    let exit = builtin_tool_registry()
        .execute(
            ToolCall::new("toolu_exit", "ExitWorktree", json!({ "action": "keep" })),
            &runtime,
        )
        .expect("noop exit should execute");

    assert!(!exit.result.is_error);
    assert_eq!(
        exit.result.content,
        fixture_json("tests/fixtures/exit_worktree_noop_result.json", &[])
    );
}

#[test]
fn exit_worktree_surfaces_dirty_refusal_from_manager() {
    let project_root = fixture_project_root();
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
    .with_session_store(Arc::new(RecordingSessionStore::default()))
    .with_worktree_manager(Arc::new(FakeWorktreeManager::dirty(worktree.clone())));
    runtime.set_active_project_root(worktree.worktree_path.clone());
    runtime.set_active_worktree(Some(worktree));

    let error = builtin_tool_registry()
        .execute(
            ToolCall::new("toolu_exit", "ExitWorktree", json!({ "action": "remove" })),
            &runtime,
        )
        .expect_err("dirty removal should surface an error");

    assert!(matches!(
        error,
        ClawinError::InvalidConfiguration { message }
            if message
                == "cannot remove a dirty session-owned worktree without discard_changes = true"
    ));
}

#[derive(Debug, Default)]
struct RecordingSessionStore {
    saved_worktree_states: Mutex<Vec<Option<PersistedWorktreeSession>>>,
}

impl RecordingSessionStore {
    fn saved_worktree_states(&self) -> Vec<Option<PersistedWorktreeSession>> {
        self.saved_worktree_states
            .lock()
            .expect("saved worktree states lock should be available")
            .clone()
    }
}

impl SessionStore for RecordingSessionStore {
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
        worktree: Option<&PersistedWorktreeSession>,
    ) -> ClawinResult<()> {
        self.saved_worktree_states
            .lock()
            .expect("saved worktree states lock should be available")
            .push(worktree.cloned());
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
    worktree: Option<PersistedWorktreeSession>,
    exit_calls: Mutex<Vec<WorktreeExitAction>>,
    dirty_remove_error: bool,
}

impl FakeWorktreeManager {
    fn new(worktree: PersistedWorktreeSession) -> Self {
        Self {
            worktree: Some(worktree),
            exit_calls: Mutex::new(Vec::new()),
            dirty_remove_error: false,
        }
    }

    fn without_worktree() -> Self {
        Self {
            worktree: None,
            exit_calls: Mutex::new(Vec::new()),
            dirty_remove_error: false,
        }
    }

    fn dirty(worktree: PersistedWorktreeSession) -> Self {
        Self {
            worktree: Some(worktree),
            exit_calls: Mutex::new(Vec::new()),
            dirty_remove_error: true,
        }
    }
}

impl WorktreeManager for FakeWorktreeManager {
    fn enter_worktree(
        &self,
        runtime: &SessionRuntime,
        _name: Option<&str>,
    ) -> ClawinResult<PersistedWorktreeSession> {
        let worktree = self
            .worktree
            .clone()
            .expect("enter test should provide a worktree");
        runtime.set_active_project_root(worktree.worktree_path.clone());
        runtime.set_active_worktree(Some(worktree.clone()));
        Ok(worktree)
    }

    fn exit_worktree(
        &self,
        runtime: &SessionRuntime,
        action: WorktreeExitAction,
        discard_changes: bool,
    ) -> ClawinResult<Option<PersistedWorktreeSession>> {
        self.exit_calls
            .lock()
            .expect("exit calls lock should be available")
            .push(action);
        if self.dirty_remove_error && action == WorktreeExitAction::Remove && !discard_changes {
            return Err(ClawinError::InvalidConfiguration {
                message:
                    "cannot remove a dirty session-owned worktree without discard_changes = true"
                        .to_owned(),
            });
        }
        let previous = runtime.active_worktree();
        runtime.set_active_project_root(runtime.canonical_project_root().to_path_buf());
        runtime.set_active_worktree(None);
        Ok(previous)
    }
}

fn fixture_project_root() -> PathBuf {
    std::env::temp_dir().join("clawin-worktree-test")
}

fn fixture_json(path: &str, replacements: &[(&str, String)]) -> Value {
    let fixture_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(path);
    let mut contents = fs::read_to_string(fixture_path).expect("fixture should exist");
    for (placeholder, replacement) in replacements {
        contents = contents.replace(placeholder, replacement);
    }
    serde_json::from_str(&contents).expect("fixture should be valid json")
}
