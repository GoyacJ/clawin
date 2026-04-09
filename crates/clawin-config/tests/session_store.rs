// Phase 7A tests continue under DIFF-2026-001: session persistence and resume stay in the Clawin namespace.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use clawin_config::{JsonlSessionStore, load_startup_config};
use clawin_core::{
    ConversationMessage, PermissionMode, PersistedWorktreeSession, ResumeInterruptionState,
    ResumeQuery, RuntimeCapabilities, SessionId, SessionRuntime,
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

#[test]
fn keeps_single_transcript_file_when_session_enters_worktree() {
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

    let runtime = SessionRuntime::new(
        SessionId::from_owned("session-c"),
        RuntimeCapabilities::new(false, false),
        harness.project_dir(),
        config.paths().project_root().to_path_buf(),
        PermissionMode::Default,
    );

    let root_transcript = config
        .paths()
        .projects_root()
        .join(policy.sanitize_for_session_dir(config.paths().project_root()))
        .join("session-c.jsonl");
    let worktree_transcript = config
        .paths()
        .projects_root()
        .join(policy.sanitize_for_session_dir(&worktree_dir))
        .join("session-c.jsonl");

    store
        .initialize_session(&runtime)
        .expect("root transcript should initialize");
    store
        .append_message(
            &runtime,
            &ConversationMessage::User {
                content: "hello".to_owned(),
            },
        )
        .expect("root message should persist");

    runtime.set_active_worktree(Some(PersistedWorktreeSession::new(
        config.paths().project_root().to_path_buf(),
        worktree_dir.clone(),
        Some("worktree-feature-a".to_owned()),
        true,
    )));
    runtime.set_active_project_root(worktree_dir.clone());
    store
        .save_worktree_state(&runtime, runtime.active_worktree().as_ref())
        .expect("worktree state should persist");
    store
        .append_message(
            &runtime,
            &ConversationMessage::Assistant {
                content: "continued".to_owned(),
            },
        )
        .expect("continued message should persist");

    assert!(root_transcript.exists());
    assert!(
        !worktree_transcript.exists(),
        "same session should not create a second transcript inside the worktree directory"
    );

    let previews = store
        .list_recent_sessions(&runtime)
        .expect("recent sessions should load");
    assert_eq!(previews.len(), 1);
    assert_eq!(previews[0].transcript_path, root_transcript);

    let restored = store
        .resolve_resume(&runtime, ResumeQuery::Exact("session-c".to_owned()))
        .expect("resume query should succeed")
        .expect("session should be found");
    assert_eq!(restored.transcript_path, root_transcript);
    assert_eq!(restored.active_project_root, worktree_dir);
}

#[test]
fn restores_fixture_session_with_worktree_state_and_ignores_unknown_entries() {
    let harness = SessionHarness::new();
    let policy = TestPathPolicy::new(harness.home_dir());
    let config = load_startup_config(harness.project_dir(), &policy).expect("config should load");
    let store = JsonlSessionStore::new(
        config.paths().clone(),
        policy,
        Arc::new(FakeGitWorktreeAdapter::new()),
    );
    let runtime = SessionRuntime::new(
        SessionId::from_owned("fixture-runtime"),
        RuntimeCapabilities::new(false, false),
        harness.project_dir(),
        config.paths().project_root().to_path_buf(),
        PermissionMode::Default,
    );

    let restored = store
        .resolve_resume(
            &runtime,
            ResumeQuery::Path(fixture_path(
                "tests/fixtures/restored_session_with_worktree.jsonl",
            )),
        )
        .expect("fixture restore should succeed")
        .expect("fixture session should load");

    assert_eq!(restored.session_id.as_str(), "fixture-session");
    assert_eq!(restored.last_prompt.as_deref(), Some("inspect worktree"));
    assert_eq!(restored.transcript.len(), 2);
    assert_eq!(
        restored.active_project_root,
        PathBuf::from("/repo/.clawin/worktrees/feature-a")
    );
    assert_eq!(
        restored.worktree_state,
        Some(PersistedWorktreeSession::new(
            PathBuf::from("/repo"),
            PathBuf::from("/repo/.clawin/worktrees/feature-a"),
            Some("worktree-feature-a".to_owned()),
            true,
        ))
    );
}

#[test]
fn detects_interrupted_prompt_from_fixture() {
    let harness = SessionHarness::new();
    let policy = TestPathPolicy::new(harness.home_dir());
    let config = load_startup_config(harness.project_dir(), &policy).expect("config should load");
    let store = JsonlSessionStore::new(
        config.paths().clone(),
        policy,
        Arc::new(FakeGitWorktreeAdapter::new()),
    );
    let runtime = SessionRuntime::new(
        SessionId::from_owned("fixture-runtime"),
        RuntimeCapabilities::new(false, false),
        harness.project_dir(),
        config.paths().project_root().to_path_buf(),
        PermissionMode::Default,
    );

    let restored = store
        .resolve_resume(
            &runtime,
            ResumeQuery::Path(fixture_path(
                "tests/fixtures/interrupted_prompt_session.jsonl",
            )),
        )
        .expect("fixture restore should succeed")
        .expect("fixture session should load");

    assert_eq!(
        restored.interruption_state,
        ResumeInterruptionState::InterruptedPrompt
    );
    assert_eq!(restored.transcript.len(), 1);
}

#[test]
fn rejects_invalid_known_entries_from_fixture() {
    let harness = SessionHarness::new();
    let policy = TestPathPolicy::new(harness.home_dir());
    let config = load_startup_config(harness.project_dir(), &policy).expect("config should load");
    let store = JsonlSessionStore::new(
        config.paths().clone(),
        policy,
        Arc::new(FakeGitWorktreeAdapter::new()),
    );
    let runtime = SessionRuntime::new(
        SessionId::from_owned("fixture-runtime"),
        RuntimeCapabilities::new(false, false),
        harness.project_dir(),
        config.paths().project_root().to_path_buf(),
        PermissionMode::Default,
    );

    let error = store
        .resolve_resume(
            &runtime,
            ResumeQuery::Path(fixture_path("tests/fixtures/invalid_message_entry.jsonl")),
        )
        .expect_err("invalid session entry should fail");

    assert!(matches!(
        error,
        clawin_core::ClawinError::InvalidConfiguration { message }
            if message.contains("invalid session message entry")
    ));
}

#[test]
fn rejects_unsupported_schema_fixture() {
    let harness = SessionHarness::new();
    let policy = TestPathPolicy::new(harness.home_dir());
    let config = load_startup_config(harness.project_dir(), &policy).expect("config should load");
    let store = JsonlSessionStore::new(
        config.paths().clone(),
        policy,
        Arc::new(FakeGitWorktreeAdapter::new()),
    );
    let runtime = SessionRuntime::new(
        SessionId::from_owned("fixture-runtime"),
        RuntimeCapabilities::new(false, false),
        harness.project_dir(),
        config.paths().project_root().to_path_buf(),
        PermissionMode::Default,
    );

    let error = store
        .resolve_resume(
            &runtime,
            ResumeQuery::Path(fixture_path(
                "tests/fixtures/unsupported_schema_session.jsonl",
            )),
        )
        .expect_err("unsupported schema should fail");

    assert!(matches!(
        error,
        clawin_core::ClawinError::InvalidConfiguration { message }
            if message.contains("unsupported session schema version")
    ));
}

#[test]
fn deduplicates_same_repo_session_scope_after_path_normalization() {
    let harness = SessionHarness::new();
    let policy = WindowsLikePathPolicy::new(harness.home_dir());
    let config = load_startup_config(harness.project_dir(), &policy).expect("config should load");
    let git = Arc::new(FakeGitWorktreeAdapter::new());
    git.register_repository(
        config.paths().project_root().to_path_buf(),
        vec![
            config.paths().project_root().to_path_buf(),
            PathBuf::from("/REPO"),
        ],
    );
    let store = JsonlSessionStore::new(config.paths().clone(), policy.clone(), git);

    let runtime = SessionRuntime::new(
        SessionId::from_owned("session-win"),
        RuntimeCapabilities::new(false, false),
        harness.project_dir(),
        config.paths().project_root().to_path_buf(),
        PermissionMode::Default,
    )
    .with_active_project_root(PathBuf::from("/REPO"));
    store
        .initialize_session(&runtime)
        .expect("session header should persist");

    let previews = store
        .list_recent_sessions(&runtime)
        .expect("recent sessions should load");
    assert_eq!(previews.len(), 1);
    assert_eq!(previews[0].session_id.as_str(), "session-win");
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

#[derive(Clone, Debug)]
struct WindowsLikePathPolicy {
    home_dir: PathBuf,
}

impl WindowsLikePathPolicy {
    fn new(home_dir: PathBuf) -> Self {
        Self { home_dir }
    }
}

impl PathPolicy for WindowsLikePathPolicy {
    fn home_dir(&self) -> Option<PathBuf> {
        Some(self.home_dir.clone())
    }

    fn normalize_for_config_key(&self, path: &Path) -> String {
        path.to_string_lossy()
            .replace('\\', "/")
            .to_ascii_lowercase()
    }

    fn project_directory_name(&self) -> &'static str {
        ".clawin"
    }

    fn project_manifest_name(&self) -> &'static str {
        "CLAWIN.md"
    }
}

fn fixture_path(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(path)
}
