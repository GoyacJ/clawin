use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::bridge::{BridgeCommandAction, BridgeController};
use crate::{ClawinResult, ConversationMessage, SessionId, SessionRuntime};

/// Persisted snapshot of a session-owned worktree.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PersistedWorktreeSession {
    pub canonical_project_root: PathBuf,
    pub worktree_path: PathBuf,
    pub branch: Option<String>,
    pub session_owned: bool,
}

impl PersistedWorktreeSession {
    /// Create a persisted worktree snapshot.
    pub fn new(
        canonical_project_root: PathBuf,
        worktree_path: PathBuf,
        branch: Option<String>,
        session_owned: bool,
    ) -> Self {
        Self {
            canonical_project_root,
            worktree_path,
            branch,
            session_owned,
        }
    }
}

/// Lightweight session list item surfaced by resume flows.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SessionPreview {
    pub session_id: SessionId,
    pub transcript_path: PathBuf,
    pub last_prompt: Option<String>,
    pub active_project_root: PathBuf,
}

/// Supported resume query shapes for CLI and REPL commands.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum ResumeQuery {
    Continue,
    Exact(String),
    Search(String),
    Path(PathBuf),
}

/// Stable interruption marker detected while restoring a session transcript.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResumeInterruptionState {
    #[default]
    None,
    InterruptedPrompt,
}

/// Restored session payload returned from session persistence.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RestoredSession {
    pub session_id: SessionId,
    pub transcript_path: PathBuf,
    pub canonical_project_root: PathBuf,
    pub active_project_root: PathBuf,
    pub transcript: Vec<ConversationMessage>,
    pub last_prompt: Option<String>,
    pub worktree_state: Option<PersistedWorktreeSession>,
    pub interruption_state: ResumeInterruptionState,
}

/// Local session persistence interface injected into the runtime.
pub trait SessionStore: Send + Sync {
    fn initialize_session(&self, runtime: &SessionRuntime) -> ClawinResult<()>;
    fn save_last_prompt(&self, runtime: &SessionRuntime, prompt: &str) -> ClawinResult<()>;
    fn append_message(
        &self,
        runtime: &SessionRuntime,
        message: &ConversationMessage,
    ) -> ClawinResult<()>;
    fn save_worktree_state(
        &self,
        runtime: &SessionRuntime,
        worktree: Option<&PersistedWorktreeSession>,
    ) -> ClawinResult<()>;
    fn list_recent_sessions(&self, runtime: &SessionRuntime) -> ClawinResult<Vec<SessionPreview>>;
    fn resolve_resume(
        &self,
        runtime: &SessionRuntime,
        query: ResumeQuery,
    ) -> ClawinResult<Option<RestoredSession>>;
}

/// Return whether the token should be treated as a transcript path.
pub fn looks_like_transcript_path(value: &str) -> bool {
    value.ends_with(".jsonl") || value.contains('/') || value.contains('\\')
}

/// Resolve a resume token using the shared exact-then-search semantics.
pub fn resolve_resume_target<S>(
    runtime: &SessionRuntime,
    store: &S,
    token: &str,
) -> ClawinResult<Option<RestoredSession>>
where
    S: SessionStore + ?Sized,
{
    if looks_like_transcript_path(token) {
        return store.resolve_resume(runtime, ResumeQuery::Path(PathBuf::from(token)));
    }

    if let Some(session) = store.resolve_resume(runtime, ResumeQuery::Exact(token.to_owned()))? {
        return Ok(Some(session));
    }

    store.resolve_resume(runtime, ResumeQuery::Search(token.to_owned()))
}

/// Stable exit actions for the `ExitWorktree` tool.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorktreeExitAction {
    Keep,
    Remove,
}

/// Session-scoped worktree manager interface injected into the runtime.
pub trait WorktreeManager: Send + Sync {
    fn enter_worktree(
        &self,
        runtime: &SessionRuntime,
        name: Option<&str>,
    ) -> ClawinResult<PersistedWorktreeSession>;
    fn exit_worktree(
        &self,
        runtime: &SessionRuntime,
        action: WorktreeExitAction,
        discard_changes: bool,
    ) -> ClawinResult<Option<PersistedWorktreeSession>>;
}

/// Structured effect returned by a slash command.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CommandEffect {
    ResumeSession { session: RestoredSession },
    BridgeControl { action: BridgeCommandAction },
}

/// Shared runtime services exposed to upper-layer commands and tools.
#[derive(Clone, Default)]
pub struct SessionServices {
    pub session_store: Option<Arc<dyn SessionStore>>,
    pub worktree_manager: Option<Arc<dyn WorktreeManager>>,
    pub bridge_controller: Option<Arc<dyn BridgeController>>,
}
