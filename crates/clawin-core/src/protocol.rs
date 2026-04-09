use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{ClawinResult, CommandEffect, TurnId};

/// Shared command shape used by registry, engine, and bootstrap layers.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CommandSpec {
    pub name: String,
    pub description: String,
    pub aliases: Vec<String>,
    pub kind: CommandKind,
    pub source: CommandSource,
    pub origin_label: Option<String>,
}

/// Minimal command categories supported in Phase 3.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandKind {
    Prompt,
    Local,
}

/// Origin marker for slash commands.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandSource {
    Builtin,
    Dynamic,
}

/// Parsed slash command invocation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ParsedCommandInvocation {
    pub raw_name: String,
    pub command_name: String,
    pub args: String,
}

/// Stable command execution result passed back to bootstrap or engine.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CommandExecutionResult {
    pub command_name: String,
    pub output: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effect: Option<CommandEffect>,
}

/// Shared tool shape used by registry and engine.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub input_schema_json: Value,
    pub kind: ToolKind,
    pub source: ToolSource,
}

/// Minimal tool categories supported in Phase 3.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolKind {
    ReadOnly,
    Unknown,
}

/// Origin marker for tools.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolSource {
    Builtin,
    Dynamic,
    Mcp,
}

/// Structured tool invocation payload.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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
#[derive(Clone, Copy, Debug, Eq, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionMode {
    AcceptEdits,
    BypassPermissions,
    #[default]
    Default,
    DontAsk,
    Plan,
}

/// Minimal permission outcomes surfaced during Phase 3 tool execution.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
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
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
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

/// Typed async return used by injected permission resolvers.
pub type PermissionResolverFuture<'a> =
    Pin<Box<dyn Future<Output = ClawinResult<PermissionDecision>> + Send + 'a>>;

/// Async permission hook used by structured/headless callers to resolve `ask` decisions.
pub trait PermissionResolver: Send + Sync {
    fn resolve(
        &self,
        call: &ToolCall,
        decision: PermissionDecision,
    ) -> PermissionResolverFuture<'_>;
}

/// Default resolver used by REPL and existing non-structured paths.
#[derive(Clone, Copy, Debug, Default)]
pub struct PassthroughPermissionResolver;

impl PermissionResolver for PassthroughPermissionResolver {
    fn resolve(
        &self,
        _call: &ToolCall,
        decision: PermissionDecision,
    ) -> PermissionResolverFuture<'_> {
        Box::pin(async move { Ok(decision) })
    }
}

/// Top-level request routed through the conversation engine.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ConversationRequest {
    Prompt(String),
    SlashCommand(String),
}

/// Transcript messages retained across submits.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ConversationMessage {
    System {
        content: String,
    },
    User {
        content: String,
    },
    Assistant {
        content: String,
    },
    ToolUse {
        call_id: String,
        tool_name: String,
        input: Value,
    },
    ToolResult {
        call_id: String,
        tool_name: String,
        is_error: bool,
        content: Value,
    },
    CompactSummary {
        content: String,
        replaced_message_count: usize,
    },
}

/// Structured reason that ended a single `submit_message` run.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    Completed,
    MaxTurnsReached,
    BudgetStopped,
    Cancelled,
    CommandHandled,
    Failed,
}

/// Per-submit loop configuration for the conversation engine.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TurnLoopConfig {
    pub max_turns: u64,
    pub token_budget: Option<u64>,
    pub compaction_policy: CompactionPolicy,
    pub allow_budget_continuation: bool,
}

/// Deterministic compaction policy used before real semantic compact lands.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CompactionPolicy {
    Disabled,
    MessageCount {
        trigger_message_count: usize,
        keep_recent_messages: usize,
    },
}

/// Structured compact result surfaced by the engine.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CompactionDecision {
    Applied {
        replaced_message_count: usize,
        summary: String,
    },
}

/// Mutable token budget tracker kept by the engine for the active submit.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct BudgetTracker {
    pub continuation_count: u32,
    pub last_delta_tokens: u64,
    pub last_total_tokens: u64,
}

/// Structured budget decisions taken after a model pass completes.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TokenBudgetDecision {
    Continue {
        continuation_count: u32,
        consumed_tokens: u64,
        budget_tokens: u64,
    },
    Stop {
        continuation_count: u32,
        consumed_tokens: u64,
        budget_tokens: u64,
        diminishing_returns: bool,
    },
}

/// Immutable query snapshot bundled into every model request.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct QueryConfigSnapshot {
    pub session_id: String,
    pub interactive_terminal: bool,
    pub permission_mode: PermissionMode,
}

/// Structured model request produced by the engine.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ModelRequest {
    pub turn_id: TurnId,
    pub transcript: Vec<ConversationMessage>,
    pub available_tools: Vec<ToolSpec>,
    pub query_config: QueryConfigSnapshot,
}

/// Model stream finish marker used by the deterministic fake driver.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelFinishReason {
    Completed,
    ToolUse,
    Cancelled,
}

/// Incremental events emitted from a model stream.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ModelStreamEvent {
    TextDelta { delta: String },
    ToolCallRequested { call: ToolCall },
    UsageUpdated { total_tokens: u64 },
    AssistantMessageFinished,
    ModelFinished { finish_reason: ModelFinishReason },
    ModelError { message: String },
}

/// Typed stream emitted by a model driver.
pub type ModelDriverFuture<'a> =
    Pin<Box<dyn Future<Output = ClawinResult<Vec<ModelStreamEvent>>> + Send + 'a>>;

/// Driver abstraction for model-backed streaming.
pub trait ModelDriver: Send + Sync {
    fn stream(&self, request: ModelRequest) -> ModelDriverFuture<'_>;
}

/// Explicit cancellation handle threaded through engine services.
#[derive(Clone, Debug, Default)]
pub struct CancellationFlag {
    cancelled: Arc<AtomicBool>,
}

impl CancellationFlag {
    /// Create a new cancellation flag in the non-cancelled state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Mark the current operation as cancelled.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    /// Whether cancellation has been requested.
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
}

/// Typed engine events emitted incrementally during a submit.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EngineEvent {
    SessionStarted {
        session_id: String,
    },
    TurnStarted {
        turn_id: u64,
    },
    CommandParsed {
        raw_name: String,
        command_name: String,
    },
    CommandExecuted {
        command_name: String,
        output: String,
    },
    AssistantTextDelta {
        turn_id: u64,
        delta: String,
    },
    AssistantMessageCompleted {
        turn_id: u64,
        content: String,
    },
    ToolRequested {
        turn_id: u64,
        call_id: String,
        tool_name: String,
    },
    ToolPermissionResolved {
        turn_id: u64,
        call_id: String,
        tool_name: String,
        behavior: PermissionBehavior,
    },
    ToolCompleted {
        turn_id: u64,
        call_id: String,
        tool_name: String,
        is_error: bool,
    },
    BudgetContinuationSuggested {
        turn_id: u64,
        continuation_count: u32,
        budget_tokens: u64,
        consumed_tokens: u64,
    },
    CompactionApplied {
        turn_id: u64,
        replaced_message_count: usize,
        summary_preview: String,
    },
    TurnFinished {
        turn_id: u64,
        stop_reason: StopReason,
    },
    SessionFinished {
        session_id: String,
        stop_reason: StopReason,
    },
    EngineFailed {
        turn_id: Option<u64>,
        message: String,
    },
}

/// Final engine outcome for a single `submit_message` invocation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EngineOutcome {
    pub stop_reason: StopReason,
    pub final_assistant_message: Option<String>,
    pub turn_count: u64,
    pub last_turn_id: TurnId,
    pub transcript: Vec<ConversationMessage>,
    pub budget_tracker: BudgetTracker,
    pub budget_decision: Option<TokenBudgetDecision>,
    pub compaction: Option<CompactionDecision>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command_effect: Option<CommandEffect>,
}

/// Structured control requests emitted by the headless stdout protocol.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StructuredControlRequest {
    CanUseTool {
        request_id: String,
        call_id: String,
        tool_name: String,
        input: Value,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },
}

/// Structured control responses accepted from the headless stdin protocol.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StructuredControlResponse {
    CanUseTool {
        request_id: String,
        behavior: PermissionBehavior,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },
}

/// Control requests accepted from the headless stdin protocol.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StructuredInputControlRequest {
    Interrupt,
}

/// Structured line-delimited stdin protocol for headless mode.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StructuredInputMessage {
    User {
        content: String,
    },
    ControlRequest {
        request: StructuredInputControlRequest,
    },
    ControlResponse {
        response: StructuredControlResponse,
    },
    KeepAlive,
}

/// Stable result payload emitted by headless mode after a submit completes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StructuredRunResult {
    pub outcome: EngineOutcome,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command_output: Option<String>,
}

/// Structured line-delimited stdout protocol for headless mode.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StructuredOutputMessage {
    SessionStarted { session_id: String },
    StreamEvent { event: EngineEvent },
    ControlRequest { request: StructuredControlRequest },
    ControlCancelRequest { request_id: String, reason: String },
    Result { result: Box<StructuredRunResult> },
    Error { code: String, message: String },
    KeepAlive,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn structured_input_messages_round_trip() {
        let original = StructuredInputMessage::ControlResponse {
            response: StructuredControlResponse::CanUseTool {
                request_id: "perm-1".to_owned(),
                behavior: PermissionBehavior::Allow,
                message: None,
            },
        };

        let encoded = serde_json::to_string(&original).expect("input should serialize");
        let decoded: StructuredInputMessage =
            serde_json::from_str(&encoded).expect("input should deserialize");

        assert_eq!(decoded, original);
    }

    #[test]
    fn structured_output_messages_round_trip() {
        let original = StructuredOutputMessage::ControlRequest {
            request: StructuredControlRequest::CanUseTool {
                request_id: "perm-1".to_owned(),
                call_id: "toolu_1".to_owned(),
                tool_name: "file_read".to_owned(),
                input: serde_json::json!({ "file_path": "notes.txt" }),
                message: Some("requested path is outside the project root".to_owned()),
            },
        };

        let encoded = serde_json::to_string(&original).expect("output should serialize");
        let decoded: StructuredOutputMessage =
            serde_json::from_str(&encoded).expect("output should deserialize");

        assert_eq!(decoded, original);
    }
}
