#![forbid(unsafe_code)]

//! Shared types, errors, and runtime models used across the Clawin workspace.

mod bridge;
mod error;
mod ids;
mod protocol;
mod runtime;
mod session;

pub use bridge::{
    BridgeCommandAction, BridgeController, BridgeMode, BridgePointer, BridgePointerSource,
    BridgeSessionHost, BridgeState, BridgeStatusSnapshot,
};
pub use error::{ClawinError, ClawinResult};
pub use ids::{ConversationId, SessionId, TurnId};
pub use protocol::{
    BudgetTracker, CancellationFlag, CommandExecutionResult, CommandKind, CommandSource,
    CommandSpec, CompactionDecision, CompactionPolicy, ConversationMessage, ConversationRequest,
    EngineEvent, EngineOutcome, ModelDriver, ModelDriverFuture, ModelFinishReason, ModelRequest,
    ModelStreamEvent, ParsedCommandInvocation, PassthroughPermissionResolver, PermissionBehavior,
    PermissionDecision, PermissionMode, PermissionResolver, PermissionResolverFuture,
    QueryConfigSnapshot, StopReason, StructuredControlRequest, StructuredControlResponse,
    StructuredInputControlRequest, StructuredInputMessage, StructuredOutputMessage,
    StructuredRunResult, TokenBudgetDecision, ToolCall, ToolKind, ToolResult, ToolSource, ToolSpec,
    TurnLoopConfig,
};
pub use runtime::{RuntimeCapabilities, SessionRuntime};
pub use session::{
    CommandEffect, PersistedWorktreeSession, RestoredSession, ResumeInterruptionState, ResumeQuery,
    SessionPreview, SessionServices, SessionStore, WorktreeExitAction, WorktreeManager,
    looks_like_transcript_path, resolve_resume_target,
};
