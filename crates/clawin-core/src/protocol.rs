use serde_json::Value;

use crate::TurnId;

/// Shared command shape used by registry, engine, and bootstrap layers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandSpec {
    pub name: String,
    pub description: String,
    pub aliases: Vec<String>,
    pub kind: CommandKind,
    pub source: CommandSource,
}

/// Minimal command categories supported in Phase 3.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommandKind {
    Prompt,
    Local,
}

/// Origin marker for slash commands.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommandSource {
    Builtin,
    Dynamic,
}

/// Parsed slash command invocation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParsedCommandInvocation {
    pub raw_name: String,
    pub command_name: String,
    pub args: String,
}

/// Stable command execution result passed back to bootstrap or engine.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandExecutionResult {
    pub command_name: String,
    pub output: String,
}

/// Shared tool shape used by registry and engine.
#[derive(Clone, Debug, PartialEq)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub input_schema_json: Value,
    pub kind: ToolKind,
    pub source: ToolSource,
}

/// Minimal tool categories supported in Phase 3.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ToolKind {
    ReadOnly,
}

/// Origin marker for tools.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ToolSource {
    Builtin,
    Dynamic,
}

/// Structured tool invocation payload.
#[derive(Clone, Debug, PartialEq)]
pub struct ToolCall {
    pub call_id: String,
    pub tool_name: String,
    pub input: Value,
}

impl ToolCall {
    /// Create a new structured tool invocation.
    pub fn new(call_id: impl Into<String>, tool_name: impl Into<String>, input: Value) -> Self {
        Self {
            call_id: call_id.into(),
            tool_name: tool_name.into(),
            input,
        }
    }
}

/// Structured tool result, including tool-originated errors.
#[derive(Clone, Debug, PartialEq)]
pub struct ToolResult {
    pub call_id: String,
    pub tool_name: String,
    pub is_error: bool,
    pub content: Value,
}

impl ToolResult {
    /// Create a successful tool result.
    pub fn success(call: &ToolCall, content: Value) -> Self {
        Self {
            call_id: call.call_id.clone(),
            tool_name: call.tool_name.clone(),
            is_error: false,
            content,
        }
    }

    /// Create an error tool result.
    pub fn error(call: &ToolCall, content: Value) -> Self {
        Self {
            call_id: call.call_id.clone(),
            tool_name: call.tool_name.clone(),
            is_error: true,
            content,
        }
    }
}

/// Permission modes aligned with upstream naming for future config wiring.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Default)]
pub enum PermissionMode {
    AcceptEdits,
    BypassPermissions,
    #[default]
    Default,
    DontAsk,
    Plan,
}

/// Minimal permission outcomes surfaced during Phase 3 tool execution.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PermissionBehavior {
    Allow,
    Ask,
    Deny,
}

impl PermissionBehavior {
    /// Return a stable lowercase label for result payloads.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Ask => "ask",
            Self::Deny => "deny",
        }
    }
}

/// Structured permission decision produced by tools.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PermissionDecision {
    pub behavior: PermissionBehavior,
    pub message: Option<String>,
}

impl PermissionDecision {
    /// Create a permission decision with an optional operator-facing message.
    pub fn new(behavior: PermissionBehavior, message: impl Into<Option<String>>) -> Self {
        Self {
            behavior,
            message: message.into(),
        }
    }
}

/// Minimal events emitted by the Phase 3 session runner.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SessionEvent {
    SessionStarted {
        session_id: String,
    },
    TurnStarted {
        turn_id: TurnId,
    },
    CommandParsed {
        raw_name: String,
        command_name: String,
    },
    CommandExecuted {
        command_name: String,
    },
    ToolRequested {
        call_id: String,
        tool_name: String,
    },
    ToolPermissionResolved {
        call_id: String,
        tool_name: String,
        behavior: PermissionBehavior,
    },
    ToolCompleted {
        call_id: String,
        tool_name: String,
        is_error: bool,
    },
    SessionFinished {
        turn_id: TurnId,
    },
}

/// Minimal requests supported by the Phase 3 engine runner.
#[derive(Clone, Debug, PartialEq)]
pub enum MinimalSessionRequest {
    SlashCommand(String),
    ToolCall(ToolCall),
}

/// Minimal responses supported by the Phase 3 engine runner.
#[derive(Clone, Debug, PartialEq)]
pub enum MinimalSessionResponse {
    Command(CommandExecutionResult),
    Tool(ToolResult),
}

/// Minimal engine run result used by integration tests and bootstrap assembly.
#[derive(Clone, Debug, PartialEq)]
pub struct MinimalSessionOutcome {
    pub events: Vec<SessionEvent>,
    pub response: MinimalSessionResponse,
}
