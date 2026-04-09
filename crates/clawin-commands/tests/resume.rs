// Phase 7A tests continue under DIFF-2026-001: resume flows stay inside Clawin-owned command/runtime surfaces.

use std::path::PathBuf;
use std::sync::Arc;

use clawin_commands::builtin_command_registry;
use clawin_core::{
    ClawinError, ClawinResult, CommandEffect, ConversationMessage, PersistedWorktreeSession,
    RestoredSession, ResumeInterruptionState, ResumeQuery, RuntimeCapabilities, SessionId,
    SessionPreview, SessionRuntime, SessionStore, WorktreeExitAction, WorktreeManager,
};

#[test]
fn resume_without_args_lists_recent_sessions() {
    let registry = builtin_command_registry();
    let runtime = runtime(Arc::new(FakeSessionStore::with_recent(vec![
        SessionPreview {
            session_id: SessionId::from_owned("session-123"),
            transcript_path: PathBuf::from("/tmp/session-123.jsonl"),
            last_prompt: Some("fix parser".to_owned()),
            active_project_root: PathBuf::from("/repo"),
        },
    ])));

    let result = registry
        .execute("/resume", &runtime)
        .expect("resume list command should succeed");

    assert!(result.effect.is_none());
    assert!(result.output.contains("session-123"));
    assert!(result.output.contains("fix parser"));
}

#[test]
fn continue_alias_returns_resume_effect_for_exact_match() {
    let registry = builtin_command_registry();
    let restored = RestoredSession {
        session_id: SessionId::from_owned("session-456"),
        transcript_path: PathBuf::from("/tmp/session-456.jsonl"),
        canonical_project_root: PathBuf::from("/repo"),
        active_project_root: PathBuf::from("/repo"),
        transcript: vec![
            ConversationMessage::User {
                content: "hello".to_owned(),
            },
            ConversationMessage::Assistant {
                content: "world".to_owned(),
            },
        ],
        last_prompt: Some("hello".to_owned()),
        worktree_state: Some(PersistedWorktreeSession::new(
            PathBuf::from("/repo"),
            PathBuf::from("/repo/.clawin/worktrees/feature-a"),
            Some("worktree-feature-a".to_owned()),
            true,
        )),
        interruption_state: ResumeInterruptionState::InterruptedPrompt,
    };
    let runtime = runtime(Arc::new(FakeSessionStore::with_restored(restored.clone())));

    let result = registry
        .execute("/continue session-456", &runtime)
        .expect("continue alias should resolve restored session");

    match result.effect {
        Some(CommandEffect::ResumeSession { session }) => {
            assert_eq!(session, restored);
        }
        other => panic!("unexpected command effect: {other:?}"),
    }
    assert!(result.output.contains("session-456"));
}

#[test]
fn resume_surfaces_ambiguous_search_errors() {
    let registry = builtin_command_registry();
    let runtime = runtime(Arc::new(FakeSessionStore::with_query_results(
        QueryResult::ok(None),
        QueryResult::err("resume query matched multiple sessions"),
    )));

    let error = registry
        .execute("/resume session-ambiguous", &runtime)
        .expect_err("ambiguous search should surface an error");

    assert!(matches!(
        error,
        ClawinError::InvalidConfiguration { message }
        if message.contains("multiple sessions")
    ));
}

#[test]
fn resume_surfaces_invalid_transcript_errors() {
    let registry = builtin_command_registry();
    let runtime = runtime(Arc::new(FakeSessionStore::with_query_results(
        QueryResult::ok(None),
        QueryResult::err("failed to read session transcript /tmp/bad.jsonl"),
    )));

    let error = registry
        .execute("/resume session-bad", &runtime)
        .expect_err("invalid transcript should surface an error");

    assert!(matches!(
        error,
        ClawinError::InvalidConfiguration { message }
        if message.contains("failed to read session transcript")
    ));
}

fn runtime(store: Arc<dyn SessionStore>) -> SessionRuntime {
    SessionRuntime::new(
        SessionId::from_static("commands-resume"),
        RuntimeCapabilities::new(false, false),
        std::env::temp_dir(),
        std::env::temp_dir(),
        clawin_core::PermissionMode::Default,
    )
    .with_session_store(store)
    .with_worktree_manager(Arc::new(NoopWorktreeManager))
}

#[derive(Debug)]
struct FakeSessionStore {
    recent: Vec<SessionPreview>,
    exact: QueryResult<Option<RestoredSession>>,
    search: QueryResult<Option<RestoredSession>>,
    path: QueryResult<Option<RestoredSession>>,
}

impl FakeSessionStore {
    fn with_recent(recent: Vec<SessionPreview>) -> Self {
        Self {
            recent,
            exact: QueryResult::ok(None),
            search: QueryResult::ok(None),
            path: QueryResult::ok(None),
        }
    }

    fn with_restored(restored: RestoredSession) -> Self {
        Self {
            recent: Vec::new(),
            exact: QueryResult::ok(Some(restored)),
            search: QueryResult::ok(None),
            path: QueryResult::ok(None),
        }
    }

    fn with_query_results(
        exact: QueryResult<Option<RestoredSession>>,
        search: QueryResult<Option<RestoredSession>>,
    ) -> Self {
        Self {
            recent: Vec::new(),
            exact,
            search,
            path: QueryResult::ok(None),
        }
    }
}

impl SessionStore for FakeSessionStore {
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
        Ok(self.recent.clone())
    }

    fn resolve_resume(
        &self,
        _runtime: &SessionRuntime,
        query: ResumeQuery,
    ) -> ClawinResult<Option<RestoredSession>> {
        match query {
            ResumeQuery::Continue => Ok(None),
            ResumeQuery::Exact(_) => self.exact.resolve(),
            ResumeQuery::Search(_) => self.search.resolve(),
            ResumeQuery::Path(_) => self.path.resolve(),
        }
    }
}

#[derive(Debug)]
struct QueryResult<T> {
    value: Result<T, String>,
}

impl<T> QueryResult<T> {
    fn ok(value: T) -> Self {
        Self { value: Ok(value) }
    }

    fn err(message: impl Into<String>) -> Self {
        Self {
            value: Err(message.into()),
        }
    }
}

impl<T: Clone> QueryResult<T> {
    fn resolve(&self) -> ClawinResult<T> {
        match &self.value {
            Ok(value) => Ok(value.clone()),
            Err(message) => Err(ClawinError::InvalidConfiguration {
                message: message.clone(),
            }),
        }
    }
}

#[derive(Debug)]
struct NoopWorktreeManager;

impl WorktreeManager for NoopWorktreeManager {
    fn enter_worktree(
        &self,
        _runtime: &SessionRuntime,
        _name: Option<&str>,
    ) -> ClawinResult<PersistedWorktreeSession> {
        unreachable!("worktree manager should not be used in resume command tests")
    }

    fn exit_worktree(
        &self,
        _runtime: &SessionRuntime,
        _action: WorktreeExitAction,
        _discard_changes: bool,
    ) -> ClawinResult<Option<PersistedWorktreeSession>> {
        unreachable!("worktree manager should not be used in resume command tests")
    }
}
