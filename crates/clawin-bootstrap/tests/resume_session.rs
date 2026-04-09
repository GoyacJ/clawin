// Phase 7A tests continue under DIFF-2026-001: bootstrap restores Clawin-owned sessions from local JSONL transcripts.

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use clawin_bootstrap::{SessionBootstrapMode, bootstrap_session_from_request};
use clawin_config::{JsonlSessionStore, load_startup_config};
use clawin_core::{
    ConversationMessage, PermissionMode, RuntimeCapabilities, SessionId, SessionRuntime,
};
use clawin_platform::{FakeGitWorktreeAdapter, PathPolicy, StaticTerminalCapabilities};
use tempfile::TempDir;

#[test]
fn bootstrap_continue_restores_runtime_and_engine_transcript() {
    let harness = BootstrapHarness::new();
    let policy = TestPathPolicy {
        home_dir: harness.home_dir.clone(),
    };
    seed_session(&harness, &policy, "session-restore");

    let session = bootstrap_session_from_request(
        harness.project_dir.clone(),
        StaticTerminalCapabilities::new(false, false),
        policy,
        SessionBootstrapMode::Continue,
    )
    .expect("bootstrap continue should restore session");

    assert_eq!(session.runtime().session_id().as_str(), "session-restore");
    assert_eq!(
        session.engine().transcript(),
        &[
            ConversationMessage::User {
                content: "hello".to_owned(),
            },
            ConversationMessage::Assistant {
                content: "world".to_owned(),
            },
        ]
    );
}

#[test]
fn bootstrap_resume_by_session_id_restores_runtime_and_engine_transcript() {
    let harness = BootstrapHarness::new();
    let policy = TestPathPolicy {
        home_dir: harness.home_dir.clone(),
    };
    seed_session(&harness, &policy, "session-by-id");

    let session = bootstrap_session_from_request(
        harness.project_dir.clone(),
        StaticTerminalCapabilities::new(false, false),
        policy,
        SessionBootstrapMode::Resume("session-by-id".to_owned()),
    )
    .expect("bootstrap resume by id should restore session");

    assert_restored_session(&session, "session-by-id");
}

#[test]
fn bootstrap_resume_by_jsonl_path_restores_runtime_and_engine_transcript() {
    let harness = BootstrapHarness::new();
    let policy = TestPathPolicy {
        home_dir: harness.home_dir.clone(),
    };
    let transcript_path = seed_session(&harness, &policy, "session-by-path");

    let session = bootstrap_session_from_request(
        harness.project_dir.clone(),
        StaticTerminalCapabilities::new(false, false),
        policy,
        SessionBootstrapMode::Resume(transcript_path.display().to_string()),
    )
    .expect("bootstrap resume by path should restore session");

    assert_restored_session(&session, "session-by-path");
}

fn seed_session(harness: &BootstrapHarness, policy: &TestPathPolicy, session_id: &str) -> PathBuf {
    let snapshot = load_startup_config(harness.project_dir.clone(), policy)
        .expect("startup config should load");
    let store = JsonlSessionStore::new(
        snapshot.paths().clone(),
        policy.clone(),
        Arc::new(FakeGitWorktreeAdapter::new()),
    );
    let persisted_runtime = SessionRuntime::new(
        SessionId::from_owned(session_id),
        RuntimeCapabilities::new(false, false),
        harness.project_dir.clone(),
        snapshot.paths().project_root().to_path_buf(),
        PermissionMode::Default,
    );
    store
        .initialize_session(&persisted_runtime)
        .expect("session header should persist");
    store
        .save_last_prompt(&persisted_runtime, "hello")
        .expect("last prompt should persist");
    store
        .append_message(
            &persisted_runtime,
            &ConversationMessage::User {
                content: "hello".to_owned(),
            },
        )
        .expect("user message should persist");
    store
        .append_message(
            &persisted_runtime,
            &ConversationMessage::Assistant {
                content: "world".to_owned(),
            },
        )
        .expect("assistant message should persist");

    snapshot
        .paths()
        .projects_root()
        .join(policy.sanitize_for_session_dir(snapshot.paths().project_root()))
        .join(format!("{session_id}.jsonl"))
}

fn assert_restored_session(session: &clawin_bootstrap::BootstrappedSession, session_id: &str) {
    assert_eq!(session.runtime().session_id().as_str(), session_id);
    assert_eq!(
        session.engine().transcript(),
        &[
            ConversationMessage::User {
                content: "hello".to_owned(),
            },
            ConversationMessage::Assistant {
                content: "world".to_owned(),
            },
        ]
    );
}

struct BootstrapHarness {
    _tempdir: TempDir,
    home_dir: PathBuf,
    project_dir: PathBuf,
}

impl BootstrapHarness {
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
