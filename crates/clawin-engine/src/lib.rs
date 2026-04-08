#![forbid(unsafe_code)]

//! Minimal conversation engine used to exercise commands and tools during Phase 3.

use clawin_commands::CommandRegistry;
use clawin_core::{
    ClawinResult, MinimalSessionOutcome, MinimalSessionRequest, MinimalSessionResponse,
    SessionEvent, SessionId, SessionRuntime, TurnId,
};
use clawin_tools::ToolRegistry;

/// Minimal conversation engine shell used to freeze crate boundaries.
#[derive(Clone, Debug)]
pub struct ConversationEngine {
    session_id: SessionId,
    turn_count: u64,
}

impl ConversationEngine {
    /// Create a new placeholder engine bound to a session.
    pub fn new(session_id: SessionId) -> Self {
        Self {
            session_id,
            turn_count: 0,
        }
    }

    /// Borrow the current session identifier.
    pub fn session_id(&self) -> &SessionId {
        &self.session_id
    }

    /// Return the number of turns started in this placeholder engine.
    pub fn turn_count(&self) -> u64 {
        self.turn_count
    }

    /// Advance the placeholder engine by one turn.
    pub fn begin_turn(&mut self) -> TurnId {
        self.turn_count += 1;
        TurnId::new(self.turn_count)
    }

    /// Execute the minimal Phase 3 session loop for a single slash command or tool call.
    pub fn run_minimal_session(
        &mut self,
        runtime: &SessionRuntime,
        commands: &CommandRegistry,
        tools: &ToolRegistry,
        request: MinimalSessionRequest,
    ) -> ClawinResult<MinimalSessionOutcome> {
        let turn_id = self.begin_turn();
        let mut events = vec![
            SessionEvent::SessionStarted {
                session_id: runtime.session_id().as_str().to_owned(),
            },
            SessionEvent::TurnStarted { turn_id },
        ];

        let response = match request {
            MinimalSessionRequest::SlashCommand(raw) => {
                let invocation = commands.parse(&raw)?;
                events.push(SessionEvent::CommandParsed {
                    raw_name: invocation.raw_name.clone(),
                    command_name: invocation.command_name.clone(),
                });

                let result = commands.execute(&raw, runtime)?;
                events.push(SessionEvent::CommandExecuted {
                    command_name: result.command_name.clone(),
                });
                MinimalSessionResponse::Command(result)
            }
            MinimalSessionRequest::ToolCall(call) => {
                events.push(SessionEvent::ToolRequested {
                    call_id: call.call_id.clone(),
                    tool_name: call.tool_name.clone(),
                });

                let execution = tools.execute(call, runtime)?;
                events.push(SessionEvent::ToolPermissionResolved {
                    call_id: execution.result.call_id.clone(),
                    tool_name: execution.result.tool_name.clone(),
                    behavior: execution.permission_behavior,
                });
                events.push(SessionEvent::ToolCompleted {
                    call_id: execution.result.call_id.clone(),
                    tool_name: execution.result.tool_name.clone(),
                    is_error: execution.result.is_error,
                });
                MinimalSessionResponse::Tool(execution.result)
            }
        };

        events.push(SessionEvent::SessionFinished { turn_id });

        Ok(MinimalSessionOutcome { events, response })
    }
}
