// Phase 7A tests continue under DIFF-2026-001: session persistence and resume stay in the Clawin namespace.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use clawin_config::{JsonlSessionStore, load_startup_config};
use clawin_core::{
    ConversationMessage, PermissionMode, ResumeInterruptionState, ResumeQuery, RuntimeCapabilities,
    SessionId, SessionRuntime,
};
use clawin_platform::{FakeGitWorktreeAdapter, PathPolicy};
use tempfile::TempDir;

#[test]
fn persists_and_restores_session_jsonl_roundtrip() {
    let harness = SessionHarness::new();
    let policy = TestPathPolicy::new(harness.home_dir());
    let config = load_startup_config(harness.project_dir(), &policy).expect("config should load");
    let git = Arc::new(FakeGitWorktreeAdapter::new());
    git.register_repository(
        config.paths().project_root().to_path_buf(),
        vec![config.paths().project_root().to_path_buf()],
    );
    let store = JsonlSessionStore::new(config.paths().clone(), policy.clone(), git);

    let runtime = SessionRuntime::new(
        SessionId::from_owned("session-a"),
        RuntimeCapabilities::new(false, false),
        harness.project_dir(),
        config.paths().project_root().to_path_buf(),
        PermissionMode::Default,
    );

    store
        .initialize_session(&runtime)
        .expect("header should persist");
    store
        .save_last_prompt(&runtime, "resume me")
        .expect("last prompt should persist");
    store
        .append_message(
            &runtime,
            &ConversationMessage::User {
                content: "resume me".to_owned(),
            },
        )
        .expect("user message should persist");
    store
        .append_message(
            &runtime,
            &ConversationMessage::Assistant {
                content: "not yet".to_owned(),
            },
        )
        .expect("assistant message should persist");

    let restored = store
        .resolve_resume(&runtime, ResumeQuery::Exact("session-a".to_owned()))
        .expect("resume query should succeed")
        .expect("session should be found");

    assert_eq!(restored.session_id.as_str(), "session-a");
    assert_eq!(restored.transcript.len(), 2);
    assert_eq!(restored.last_prompt.as_deref(), Some("resume me"));
    assert_eq!(restored.interruption_state, ResumeInterruptionState::None);
    assert!(restored.transcript_path.ends_with("session-a.jsonl"));
}

#[test]
fn detects_interrupted_prompt_when_last_message_is_user() {
    let harness = SessionHarness::new();
    let policy = TestPathPolicy::new(harness.home_dir());
    let config = load_startup_config(harness.project_dir(), &policy).expect("config should load");
    let git = Arc::new(FakeGitWorktreeAdapter::new());
    git.register_repository(
        config.paths().project_root().to_path_buf(),
        vec![config.paths().project_root().to_path_buf()],
    );
    let store = JsonlSessionStore::new(config.paths().clone(), policy.clone(), git);

    let runtime = SessionRuntime::new(
        SessionId::from_owned("session-b"),
        RuntimeCapabilities::new(false, false),
        harness.project_dir(),
        config.paths().project_root().to_path_buf(),
        PermissionMode::Default,
    );

    store
        .initialize_session(&runtime)
        .expect("header should persist");
    store
        .save_last_prompt(&runtime, "unfinished")
        .expect("last prompt should persist");
    store
        .append_message(
            &runtime,
            &ConversationMessage::User {
                content: "unfinished".to_owned(),
            },
        )
        .expect("user message should persist");

    let restored = store
        .resolve_resume(&runtime, ResumeQuery::Continue)
        .expect("continue query should succeed")
        .expect("session should be found");

    assert_eq!(
        restored.interruption_state,
        ResumeInterruptionState::InterruptedPrompt
    );
}

#[test]
fn lists_same_repo_worktree_sessions_in_recent_order() {
    let harness = SessionHarness::new();
    let worktree_dir = harness.project_dir().join(".clawin/worktrees/feature-a");
    fs::create_dir_all(&worktree_dir).expect("worktree dir should exist");

    let policy = TestPathPolicy::new(harness.home_dir());
    let config = load_startup_config(harness.project_dir(), &policy).expect("config should load");
    let git = Arc::new(FakeGitWorktreeAdapter::new());
    git.register_repository(
        config.paths().project_root().to_path_buf(),
        vec![
            config.paths().project_root().to_path_buf(),
            worktree_dir.clone(),
        ],
    );
    let store = JsonlSessionStore::new(config.paths().clone(), policy.clone(), git);

    let base_runtime = SessionRuntime::new(
        SessionId::from_owned("root-session"),
        RuntimeCapabilities::new(false, false),
        harness.project_dir(),
        config.paths().project_root().to_path_buf(),
        PermissionMode::Default,
    );
    store
        .initialize_session(&base_runtime)
        .expect("root session should persist");
    store
        .save_last_prompt(&base_runtime, "root prompt")
        .expect("root last prompt should persist");

    let worktree_runtime = SessionRuntime::new(
        SessionId::from_owned("worktree-session"),
        RuntimeCapabilities::new(false, false),
        worktree_dir.clone(),
        config.paths().project_root().to_path_buf(),
        PermissionMode::Default,
    )
    .with_active_project_root(worktree_dir.clone());
    store
        .initialize_session(&worktree_runtime)
        .expect("worktree session should persist");
    store
        .save_last_prompt(&worktree_runtime, "worktree prompt")
        .expect("worktree last prompt should persist");

    let previews = store
        .list_recent_sessions(&worktree_runtime)
        .expect("recent sessions should load");

    assert_eq!(previews.len(), 2);
    assert_eq!(previews[0].session_id.as_str(), "worktree-session");
    assert_eq!(previews[0].last_prompt.as_deref(), Some("worktree prompt"));
    assert_eq!(previews[1].session_id.as_str(), "root-session");
}

struct SessionHarness {
    _tempdir: TempDir,
    home_dir: PathBuf,
    project_dir: PathBuf,
}

impl SessionHarness {
    fn new() -> Self {
        let tempdir = tempfile::tempdir().expect("tempdir should exist");
        let home_dir = tempdir.path().join("home");
        let project_dir = tempdir.path().join("workspace").join("app");
        fs::create_dir_all(&home_dir).expect("home dir should exist");
        fs::create_dir_all(&project_dir).expect("project dir should exist");
        Self {
            _tempdir: tempdir,
            home_dir,
            project_dir,
        }
    }

    fn home_dir(&self) -> PathBuf {
        self.home_dir.clone()
    }

    fn project_dir(&self) -> PathBuf {
        self.project_dir.clone()
    }
}

#[derive(Clone, Debug)]
struct TestPathPolicy {
    home_dir: PathBuf,
}

impl TestPathPolicy {
    fn new(home_dir: PathBuf) -> Self {
        Self { home_dir }
    }
}

impl PathPolicy for TestPathPolicy {
    fn home_dir(&self) -> Option<PathBuf> {
        Some(self.home_dir.clone())
    }

    fn normalize_for_config_key(&self, path: &Path) -> String {
        path.to_string_lossy().replace('\\', "/")
    }

    fn sanitize_for_session_dir(&self, path: &Path) -> String {
        path.to_string_lossy()
            .replace(':', "")
            .replace(['\\', '/'], "-")
    }

    fn project_directory_name(&self) -> &'static str {
        ".clawin"
    }

    fn project_manifest_name(&self) -> &'static str {
        "CLAWIN.md"
    }
}
