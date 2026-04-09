#![forbid(unsafe_code)]

//! Conversation engine with a typed turn loop, streaming events, and deterministic compaction/budget hooks.

use clawin_commands::CommandRegistry;
use clawin_core::{
    BudgetTracker, CancellationFlag, ClawinError, ClawinResult, CommandEffect, CompactionDecision,
    CompactionPolicy, ConversationMessage, ConversationRequest, EngineEvent, EngineOutcome,
    ModelDriver, ModelFinishReason, ModelRequest, ModelStreamEvent, PermissionResolver,
    QueryConfigSnapshot, StopReason, TokenBudgetDecision, ToolCall, TurnId, TurnLoopConfig,
};
use clawin_core::{SessionId, SessionRuntime};
use clawin_tools::ToolRegistry;

const BUDGET_COMPLETION_THRESHOLD_NUMERATOR: u64 = 9;
const BUDGET_COMPLETION_THRESHOLD_DENOMINATOR: u64 = 10;
const DIMINISHING_THRESHOLD: u64 = 500;

/// Borrowed service bundle passed into a single engine submit.
pub struct EngineServices<'a> {
    runtime: &'a SessionRuntime,
    commands: &'a CommandRegistry,
    tools: &'a ToolRegistry,
    model: &'a dyn ModelDriver,
    permission_resolver: &'a dyn PermissionResolver,
    cancellation: CancellationFlag,
}

impl<'a> EngineServices<'a> {
    /// Create a new borrowed service bundle.
    pub fn new(
        runtime: &'a SessionRuntime,
        commands: &'a CommandRegistry,
        tools: &'a ToolRegistry,
        model: &'a dyn ModelDriver,
        permission_resolver: &'a dyn PermissionResolver,
        cancellation: CancellationFlag,
    ) -> Self {
        Self {
            runtime,
            commands,
            tools,
            model,
            permission_resolver,
            cancellation,
        }
    }

    fn runtime(&self) -> &SessionRuntime {
        self.runtime
    }

    fn commands(&self) -> &CommandRegistry {
        self.commands
    }

    fn tools(&self) -> &ToolRegistry {
        self.tools
    }

    fn model(&self) -> &dyn ModelDriver {
        self.model
    }

    fn permission_resolver(&self) -> &dyn PermissionResolver {
        self.permission_resolver
    }

    fn cancellation(&self) -> &CancellationFlag {
        &self.cancellation
    }
}

/// Conversation-scoped engine state retained across submits.
#[derive(Clone, Debug)]
pub struct ConversationEngine {
    session_id: SessionId,
    turn_count: u64,
    transcript: Vec<ConversationMessage>,
    budget_tracker: BudgetTracker,
    pending_tool_call: Option<ToolCall>,
    last_assistant_message: Option<String>,
}

impl ConversationEngine {
    /// Create a new engine bound to a session identifier.
    pub fn new(session_id: SessionId) -> Self {
        Self {
            session_id,
            turn_count: 0,
            transcript: Vec::new(),
            budget_tracker: BudgetTracker::default(),
            pending_tool_call: None,
            last_assistant_message: None,
        }
    }

    /// Restore an engine from a persisted session identifier and transcript snapshot.
    pub fn restore(session_id: SessionId, transcript: Vec<ConversationMessage>) -> Self {
        let last_assistant_message = transcript.iter().rev().find_map(|message| match message {
            ConversationMessage::Assistant { content } => Some(content.clone()),
            _ => None,
        });

        Self {
            session_id,
            turn_count: 0,
            transcript,
            budget_tracker: BudgetTracker::default(),
            pending_tool_call: None,
            last_assistant_message,
        }
    }

    /// Borrow the current session identifier.
    pub fn session_id(&self) -> &SessionId {
        &self.session_id
    }

    /// Return the number of submits started for this conversation.
    pub fn turn_count(&self) -> u64 {
        self.turn_count
    }

    /// Borrow the retained transcript.
    pub fn transcript(&self) -> &[ConversationMessage] {
        &self.transcript
    }

    /// Borrow the most recent assistant output, if any.
    pub fn last_assistant_message(&self) -> Option<&str> {
        self.last_assistant_message.as_deref()
    }

    /// Borrow the active budget tracker state.
    pub fn budget_tracker(&self) -> &BudgetTracker {
        &self.budget_tracker
    }

    /// Submit a new prompt or slash command into the conversation engine.
    pub async fn submit_message<F>(
        &mut self,
        services: &EngineServices<'_>,
        request: ConversationRequest,
        config: TurnLoopConfig,
        mut emit: F,
    ) -> ClawinResult<EngineOutcome>
    where
        F: FnMut(EngineEvent),
    {
        if config.max_turns == 0 {
            return Err(ClawinError::InvalidConfiguration {
                message: "turn loop max_turns must be greater than 0".to_owned(),
            });
        }

        self.budget_tracker = BudgetTracker::default();
        self.pending_tool_call = None;
        self.last_assistant_message = None;

        let turn_id = self.begin_turn();
        emit(EngineEvent::SessionStarted {
            session_id: services.runtime().session_id().as_str().to_owned(),
        });
        emit(EngineEvent::TurnStarted {
            turn_id: turn_id.get(),
        });

        if services.cancellation().is_cancelled() {
            return Ok(self.finish_success(
                turn_id,
                StopReason::Cancelled,
                None,
                None,
                None,
                &mut emit,
            ));
        }

        let result = match request {
            ConversationRequest::SlashCommand(raw) => {
                self.handle_slash_command(turn_id, services, raw, &mut emit)
            }
            ConversationRequest::Prompt(prompt) => {
                self.handle_prompt(turn_id, services, prompt, config, &mut emit)
                    .await
            }
        };

        match result {
            Ok(outcome) => Ok(outcome),
            Err(error) => {
                emit(EngineEvent::EngineFailed {
                    turn_id: Some(turn_id.get()),
                    message: error_message(&error),
                });
                emit(EngineEvent::TurnFinished {
                    turn_id: turn_id.get(),
                    stop_reason: StopReason::Failed,
                });
                emit(EngineEvent::SessionFinished {
                    session_id: services.runtime().session_id().as_str().to_owned(),
                    stop_reason: StopReason::Failed,
                });
                Err(error)
            }
        }
    }

    fn begin_turn(&mut self) -> TurnId {
        self.turn_count += 1;
        TurnId::new(self.turn_count)
    }

    fn handle_slash_command<F>(
        &mut self,
        turn_id: TurnId,
        services: &EngineServices<'_>,
        raw: String,
        emit: &mut F,
    ) -> ClawinResult<EngineOutcome>
    where
        F: FnMut(EngineEvent),
    {
        let invocation = services.commands().parse(&raw)?;
        emit(EngineEvent::CommandParsed {
            raw_name: invocation.raw_name.clone(),
            command_name: invocation.command_name.clone(),
        });

        let result = services.commands().execute(&raw, services.runtime())?;
        emit(EngineEvent::CommandExecuted {
            command_name: result.command_name.clone(),
            output: result.output.clone(),
        });

        Ok(self.finish_success(
            turn_id,
            StopReason::CommandHandled,
            None,
            None,
            result.effect,
            emit,
        ))
    }

    async fn handle_prompt<F>(
        &mut self,
        turn_id: TurnId,
        services: &EngineServices<'_>,
        prompt: String,
        config: TurnLoopConfig,
        emit: &mut F,
    ) -> ClawinResult<EngineOutcome>
    where
        F: FnMut(EngineEvent),
    {
        let mut last_budget_decision = None;
        let mut last_compaction = None;
        let mut model_passes = 0_u64;

        self.append_message_and_maybe_compact(
            turn_id,
            ConversationMessage::User { content: prompt },
            &config.compaction_policy,
            &mut last_compaction,
            emit,
        );

        loop {
            if services.cancellation().is_cancelled() {
                return Ok(self.finish_success(
                    turn_id,
                    StopReason::Cancelled,
                    last_budget_decision,
                    last_compaction,
                    None,
                    emit,
                ));
            }

            if model_passes >= config.max_turns {
                return Ok(self.finish_success(
                    turn_id,
                    StopReason::MaxTurnsReached,
                    last_budget_decision,
                    last_compaction,
                    None,
                    emit,
                ));
            }
            model_passes += 1;

            let request = self.build_model_request(turn_id, services);
            let stream = services.model().stream(request).await?;
            let mut assistant_buffer = String::new();
            let mut assistant_emitted = false;
            let mut pending_calls = Vec::new();
            let mut finish_reason = None;
            let mut usage_total = None;

            for event in stream {
                if services.cancellation().is_cancelled() {
                    return Ok(self.finish_success(
                        turn_id,
                        StopReason::Cancelled,
                        last_budget_decision,
                        last_compaction,
                        None,
                        emit,
                    ));
                }

                match event {
                    ModelStreamEvent::TextDelta { delta } => {
                        assistant_buffer.push_str(&delta);
                        emit(EngineEvent::AssistantTextDelta {
                            turn_id: turn_id.get(),
                            delta,
                        });
                    }
                    ModelStreamEvent::ToolCallRequested { call } => {
                        emit(EngineEvent::ToolRequested {
                            turn_id: turn_id.get(),
                            call_id: call.call_id.clone(),
                            tool_name: call.tool_name.clone(),
                        });
                        pending_calls.push(call);
                    }
                    ModelStreamEvent::UsageUpdated { total_tokens } => {
                        usage_total = Some(total_tokens);
                    }
                    ModelStreamEvent::AssistantMessageFinished => {
                        if !assistant_buffer.is_empty() {
                            emit(EngineEvent::AssistantMessageCompleted {
                                turn_id: turn_id.get(),
                                content: assistant_buffer.clone(),
                            });
                            assistant_emitted = true;
                        }
                    }
                    ModelStreamEvent::ModelFinished {
                        finish_reason: reason,
                    } => {
                        finish_reason = Some(reason);
                    }
                    ModelStreamEvent::ModelError { message } => {
                        return Err(ClawinError::ModelDriver { message });
                    }
                }
            }

            let finish_reason = finish_reason.ok_or_else(|| ClawinError::EngineProtocol {
                message: "model stream finished without a final ModelFinished event".to_owned(),
            })?;

            if !assistant_buffer.is_empty() {
                if !assistant_emitted {
                    emit(EngineEvent::AssistantMessageCompleted {
                        turn_id: turn_id.get(),
                        content: assistant_buffer.clone(),
                    });
                }
                self.last_assistant_message = Some(assistant_buffer.clone());
                self.append_message_and_maybe_compact(
                    turn_id,
                    ConversationMessage::Assistant {
                        content: assistant_buffer,
                    },
                    &config.compaction_policy,
                    &mut last_compaction,
                    emit,
                );
            }

            if !pending_calls.is_empty() {
                for call in pending_calls {
                    self.pending_tool_call = Some(call.clone());
                    self.append_message_and_maybe_compact(
                        turn_id,
                        ConversationMessage::ToolUse {
                            call_id: call.call_id.clone(),
                            tool_name: call.tool_name.clone(),
                            input: call.input.clone(),
                        },
                        &config.compaction_policy,
                        &mut last_compaction,
                        emit,
                    );

                    let execution = services
                        .tools()
                        .execute_with_resolver(
                            call.clone(),
                            services.runtime(),
                            services.permission_resolver(),
                        )
                        .await?;
                    emit(EngineEvent::ToolPermissionResolved {
                        turn_id: turn_id.get(),
                        call_id: execution.result.call_id.clone(),
                        tool_name: execution.result.tool_name.clone(),
                        behavior: execution.permission_behavior,
                    });
                    emit(EngineEvent::ToolCompleted {
                        turn_id: turn_id.get(),
                        call_id: execution.result.call_id.clone(),
                        tool_name: execution.result.tool_name.clone(),
                        is_error: execution.result.is_error,
                    });

                    self.append_message_and_maybe_compact(
                        turn_id,
                        ConversationMessage::ToolResult {
                            call_id: execution.result.call_id.clone(),
                            tool_name: execution.result.tool_name.clone(),
                            is_error: execution.result.is_error,
                            content: execution.result.content.clone(),
                        },
                        &config.compaction_policy,
                        &mut last_compaction,
                        emit,
                    );
                }

                self.pending_tool_call = None;
                continue;
            }

            match finish_reason {
                ModelFinishReason::Cancelled => {
                    return Ok(self.finish_success(
                        turn_id,
                        StopReason::Cancelled,
                        last_budget_decision,
                        last_compaction,
                        None,
                        emit,
                    ));
                }
                ModelFinishReason::ToolUse => {
                    return Err(ClawinError::EngineProtocol {
                        message: "model requested tool use without emitting a tool call".to_owned(),
                    });
                }
                ModelFinishReason::Completed => {}
            }

            if let Some(decision) = evaluate_token_budget(
                &mut self.budget_tracker,
                config.token_budget,
                usage_total,
                config.allow_budget_continuation,
            ) {
                match decision.clone() {
                    TokenBudgetDecision::Continue {
                        continuation_count,
                        consumed_tokens,
                        budget_tokens,
                    } => {
                        emit(EngineEvent::BudgetContinuationSuggested {
                            turn_id: turn_id.get(),
                            continuation_count,
                            budget_tokens,
                            consumed_tokens,
                        });
                        self.append_message_and_maybe_compact(
                            turn_id,
                            ConversationMessage::System {
                                content: continuation_message(consumed_tokens, budget_tokens),
                            },
                            &config.compaction_policy,
                            &mut last_compaction,
                            emit,
                        );
                        last_budget_decision = Some(decision);
                        continue;
                    }
                    TokenBudgetDecision::Stop { .. } => {
                        last_budget_decision = Some(decision);
                        return Ok(self.finish_success(
                            turn_id,
                            StopReason::BudgetStopped,
                            last_budget_decision,
                            last_compaction,
                            None,
                            emit,
                        ));
                    }
                }
            }

            return Ok(self.finish_success(
                turn_id,
                StopReason::Completed,
                last_budget_decision,
                last_compaction,
                None,
                emit,
            ));
        }
    }

    fn build_model_request(&self, turn_id: TurnId, services: &EngineServices<'_>) -> ModelRequest {
        ModelRequest {
            turn_id,
            transcript: self.transcript.clone(),
            available_tools: services.tools().tool_specs().collect(),
            query_config: QueryConfigSnapshot {
                session_id: services.runtime().session_id().as_str().to_owned(),
                interactive_terminal: services.runtime().capabilities().interactive_terminal(),
                permission_mode: services.runtime().permission_mode(),
            },
        }
    }

    fn append_message_and_maybe_compact<F>(
        &mut self,
        turn_id: TurnId,
        message: ConversationMessage,
        policy: &CompactionPolicy,
        last_compaction: &mut Option<CompactionDecision>,
        emit: &mut F,
    ) where
        F: FnMut(EngineEvent),
    {
        self.transcript.push(message);

        if let Some(decision) = maybe_compact_transcript(&mut self.transcript, policy) {
            let preview = match &decision {
                CompactionDecision::Applied { summary, .. } => summary.clone(),
            };
            let replaced_message_count = match &decision {
                CompactionDecision::Applied {
                    replaced_message_count,
                    ..
                } => *replaced_message_count,
            };

            emit(EngineEvent::CompactionApplied {
                turn_id: turn_id.get(),
                replaced_message_count,
                summary_preview: preview,
            });
            *last_compaction = Some(decision);
        }
    }

    fn finish_success<F>(
        &self,
        turn_id: TurnId,
        stop_reason: StopReason,
        budget_decision: Option<TokenBudgetDecision>,
        compaction: Option<CompactionDecision>,
        command_effect: Option<CommandEffect>,
        emit: &mut F,
    ) -> EngineOutcome
    where
        F: FnMut(EngineEvent),
    {
        emit(EngineEvent::TurnFinished {
            turn_id: turn_id.get(),
            stop_reason,
        });
        emit(EngineEvent::SessionFinished {
            session_id: self.session_id.as_str().to_owned(),
            stop_reason,
        });

        EngineOutcome {
            stop_reason,
            final_assistant_message: self.last_assistant_message.clone(),
            turn_count: self.turn_count,
            last_turn_id: turn_id,
            transcript: self.transcript.clone(),
            budget_tracker: self.budget_tracker.clone(),
            budget_decision,
            compaction,
            command_effect,
        }
    }
}

fn error_message(error: &ClawinError) -> String {
    match error {
        ClawinError::ModelDriver { message }
        | ClawinError::EngineProtocol { message }
        | ClawinError::InvalidConfiguration { message } => message.clone(),
        ClawinError::UnknownCommand { name } => format!("unknown command: {name}"),
        ClawinError::InvalidCommandInvocation { message } => message.clone(),
        ClawinError::UnknownTool { name } => format!("unknown tool: {name}"),
        ClawinError::ToolInputInvalid { message, .. } => message.clone(),
        ClawinError::ToolExecution { message, .. } => message.clone(),
        ClawinError::NotImplemented { subsystem } => format!("{subsystem} is not implemented yet"),
    }
}

fn evaluate_token_budget(
    tracker: &mut BudgetTracker,
    budget: Option<u64>,
    total_tokens: Option<u64>,
    allow_budget_continuation: bool,
) -> Option<TokenBudgetDecision> {
    let budget = budget?;
    let total_tokens = total_tokens?;
    if budget == 0 {
        return None;
    }

    let delta_tokens = total_tokens.saturating_sub(tracker.last_total_tokens);
    let diminishing_returns = tracker.continuation_count >= 3
        && delta_tokens < DIMINISHING_THRESHOLD
        && tracker.last_delta_tokens < DIMINISHING_THRESHOLD;

    let below_completion_threshold = total_tokens
        .saturating_mul(BUDGET_COMPLETION_THRESHOLD_DENOMINATOR)
        < budget.saturating_mul(BUDGET_COMPLETION_THRESHOLD_NUMERATOR);

    if allow_budget_continuation && below_completion_threshold && !diminishing_returns {
        tracker.continuation_count += 1;
        tracker.last_delta_tokens = delta_tokens;
        tracker.last_total_tokens = total_tokens;
        return Some(TokenBudgetDecision::Continue {
            continuation_count: tracker.continuation_count,
            consumed_tokens: total_tokens,
            budget_tokens: budget,
        });
    }

    if total_tokens >= budget
        || tracker.continuation_count > 0
        || diminishing_returns
        || !below_completion_threshold
    {
        tracker.last_delta_tokens = delta_tokens;
        tracker.last_total_tokens = total_tokens;
        return Some(TokenBudgetDecision::Stop {
            continuation_count: tracker.continuation_count,
            consumed_tokens: total_tokens,
            budget_tokens: budget,
            diminishing_returns,
        });
    }

    tracker.last_delta_tokens = delta_tokens;
    tracker.last_total_tokens = total_tokens;
    None
}

fn continuation_message(consumed_tokens: u64, budget_tokens: u64) -> String {
    format!(
        "Please continue within the remaining token budget. Consumed {consumed_tokens} of {budget_tokens} tokens."
    )
}

fn maybe_compact_transcript(
    transcript: &mut Vec<ConversationMessage>,
    policy: &CompactionPolicy,
) -> Option<CompactionDecision> {
    match policy {
        CompactionPolicy::Disabled => None,
        CompactionPolicy::MessageCount {
            trigger_message_count,
            keep_recent_messages,
        } => {
            if transcript.len() <= *trigger_message_count {
                return None;
            }

            let keep_recent_messages = (*keep_recent_messages).min(transcript.len());
            let replaced_message_count = transcript.len().saturating_sub(keep_recent_messages);
            if replaced_message_count == 0 {
                return None;
            }

            let summary = summarize_messages(&transcript[..replaced_message_count]);
            let recent_messages = transcript[replaced_message_count..].to_vec();
            transcript.clear();
            transcript.push(ConversationMessage::CompactSummary {
                content: summary.clone(),
                replaced_message_count,
            });
            transcript.extend(recent_messages);

            Some(CompactionDecision::Applied {
                replaced_message_count,
                summary,
            })
        }
    }
}

fn summarize_messages(messages: &[ConversationMessage]) -> String {
    let mut summary = String::new();

    for (index, message) in messages.iter().enumerate() {
        if index > 0 {
            summary.push('\n');
        }

        match message {
            ConversationMessage::System { content } => {
                summary.push_str("system: ");
                summary.push_str(content);
            }
            ConversationMessage::User { content } => {
                summary.push_str("user: ");
                summary.push_str(content);
            }
            ConversationMessage::Assistant { content } => {
                summary.push_str("assistant: ");
                summary.push_str(content);
            }
            ConversationMessage::ToolUse {
                tool_name,
                call_id,
                input,
            } => {
                summary.push_str("tool_use ");
                summary.push_str(tool_name);
                summary.push('#');
                summary.push_str(call_id);
                summary.push_str(": ");
                summary.push_str(&input.to_string());
            }
            ConversationMessage::ToolResult {
                tool_name,
                call_id,
                is_error,
                content,
            } => {
                summary.push_str("tool_result ");
                summary.push_str(tool_name);
                summary.push('#');
                summary.push_str(call_id);
                summary.push_str(" error=");
                summary.push_str(if *is_error { "true" } else { "false" });
                summary.push_str(": ");
                summary.push_str(&content.to_string());
            }
            ConversationMessage::CompactSummary { content, .. } => {
                summary.push_str("compact_summary: ");
                summary.push_str(content);
            }
        }
    }

    summary
}
