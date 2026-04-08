#![forbid(unsafe_code)]

//! Shared types, errors, and runtime models used across the Clawin workspace.

mod error;
mod ids;
mod protocol;
mod runtime;

pub use error::{ClawinError, ClawinResult};
pub use ids::{ConversationId, SessionId, TurnId};
pub use protocol::{
    CommandExecutionResult, CommandKind, CommandSource, CommandSpec, MinimalSessionOutcome,
    MinimalSessionRequest, MinimalSessionResponse, ParsedCommandInvocation, PermissionBehavior,
    PermissionDecision, PermissionMode, SessionEvent, ToolCall, ToolKind, ToolResult, ToolSource,
    ToolSpec,
};
pub use runtime::{RuntimeCapabilities, SessionRuntime};
