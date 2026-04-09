#![forbid(unsafe_code)]

//! Minimal Phase 5 REPL built on top of the conversation engine event stream.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use anyhow::Context;
use clawin_commands::CommandRegistry;
use clawin_core::{
    BridgeCommandAction, BridgeMode, BridgePointerSource, BridgeSessionHost, BridgeState,
    BridgeStatusSnapshot, CancellationFlag, ClawinError, CommandEffect, ConversationMessage,
    ConversationRequest, EngineEvent, EngineOutcome, ModelDriver, PassthroughPermissionResolver,
    PermissionBehavior, PermissionDecision, PermissionResolver, PermissionResolverFuture,
    RestoredSession, SessionRuntime, StopReason, StructuredControlRequest,
    StructuredControlResponse, StructuredInputControlRequest, StructuredInputMessage,
    StructuredOutputMessage, TurnLoopConfig,
};
use clawin_engine::{ConversationEngine, EngineServices};
use clawin_platform::{
    TerminalEvent, TerminalKeyCode, TerminalKeyEvent, TerminalKeyModifiers, TerminalSession,
    TerminalSize,
};
use clawin_tools::ToolRegistry;
use ratatui::Terminal;
use ratatui::backend::{CrosstermBackend, TestBackend};
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::text::{Line, Text};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use tokio::sync::{mpsc as tokio_mpsc, oneshot};

/// Fixed TUI renderer direction from ADR-0003.
pub const TUI_RENDERER: &str = "ratatui";

/// Fixed terminal event backend from ADR-0003.
pub const TUI_EVENT_BACKEND: &str = "crossterm";

const DEFAULT_POLL_INTERVAL_MS: u64 = 10;

/// Runtime configuration for the minimal REPL loop.
#[derive(Clone, Debug)]
pub struct ReplConfig {
    pub submit: TurnLoopConfig,
    pub poll_interval: Duration,
}

impl Default for ReplConfig {
    fn default() -> Self {
        Self {
            submit: TurnLoopConfig {
                max_turns: 4,
                token_budget: None,
                compaction_policy: clawin_core::CompactionPolicy::Disabled,
                allow_budget_continuation: false,
            },
            poll_interval: Duration::from_millis(DEFAULT_POLL_INTERVAL_MS),
        }
    }
}

/// Stable REPL exit payload.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReplExit {
    reason: ReplExitReason,
}

impl ReplExit {
    fn user_exit() -> Self {
        Self {
            reason: ReplExitReason::UserExit,
        }
    }

    /// Stable label used by tests and docs.
    pub fn reason_label(&self) -> &'static str {
        match self.reason {
            ReplExitReason::UserExit => "user_exit",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum ReplExitReason {
    UserExit,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum DisplayEntry {
    User(String),
    Assistant(String),
    CommandOutput(String),
    Notice(String),
}

impl DisplayEntry {
    fn line_prefix(&self) -> &'static str {
        match self {
            Self::User(_) => "You",
            Self::Assistant(_) => "Assistant",
            Self::CommandOutput(_) => "Command",
            Self::Notice(_) => "Status",
        }
    }

    fn content(&self) -> &str {
        match self {
            Self::User(content)
            | Self::Assistant(content)
            | Self::CommandOutput(content)
            | Self::Notice(content) => content,
        }
    }
}

/// Rendered REPL state kept on the UI thread.
#[derive(Clone, Debug)]
pub struct ReplViewState {
    entries: Vec<DisplayEntry>,
    input: String,
    cursor: usize,
    pending_assistant: String,
    status: String,
    busy: bool,
    size: TerminalSize,
}

impl Default for ReplViewState {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            input: String::new(),
            cursor: 0,
            pending_assistant: String::new(),
            status: "Ready".to_owned(),
            busy: false,
            size: TerminalSize::new(80, 24),
        }
    }
}

impl ReplViewState {
    fn from_transcript(transcript: &[ConversationMessage]) -> Self {
        Self {
            entries: transcript_entries(transcript),
            ..Self::default()
        }
    }

    fn push_notice(&mut self, message: impl Into<String>) {
        self.entries.push(DisplayEntry::Notice(message.into()));
    }

    fn set_size(&mut self, size: TerminalSize) {
        self.size = size;
    }

    fn insert_char(&mut self, ch: char) {
        let byte_index = byte_index(&self.input, self.cursor);
        self.input.insert(byte_index, ch);
        self.cursor += 1;
    }

    fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }

        let current = byte_index(&self.input, self.cursor);
        let previous = byte_index(&self.input, self.cursor - 1);
        self.input.replace_range(previous..current, "");
        self.cursor -= 1;
    }

    fn delete(&mut self) {
        let total = self.input.chars().count();
        if self.cursor >= total {
            return;
        }

        let current = byte_index(&self.input, self.cursor);
        let next = byte_index(&self.input, self.cursor + 1);
        self.input.replace_range(current..next, "");
    }

    fn move_left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    fn move_right(&mut self) {
        let total = self.input.chars().count();
        if self.cursor < total {
            self.cursor += 1;
        }
    }

    fn move_home(&mut self) {
        self.cursor = 0;
    }

    fn move_end(&mut self) {
        self.cursor = self.input.chars().count();
    }

    fn take_submittable_input(&mut self) -> Option<String> {
        let trimmed = self.input.trim();
        if trimmed.is_empty() {
            self.status = "Input is empty.".to_owned();
            return None;
        }

        let input = self.input.clone();
        self.entries.push(DisplayEntry::User(input.clone()));
        self.input.clear();
        self.cursor = 0;
        self.pending_assistant.clear();
        self.busy = true;
        self.status = if input.starts_with('/') {
            "Running slash command...".to_owned()
        } else {
            "Submitting prompt...".to_owned()
        };
        Some(input)
    }

    fn begin_external_submit(&mut self, input: &str) {
        self.entries.push(DisplayEntry::User(input.to_owned()));
        self.pending_assistant.clear();
        self.busy = true;
        self.status = if input.starts_with('/') {
            "Running remote slash command...".to_owned()
        } else {
            "Running remote prompt...".to_owned()
        };
    }

    fn apply_engine_event(&mut self, event: &EngineEvent) {
        match event {
            EngineEvent::CommandExecuted { output, .. } => {
                self.entries
                    .push(DisplayEntry::CommandOutput(output.clone()));
                self.status = "Slash command completed.".to_owned();
            }
            EngineEvent::AssistantTextDelta { delta, .. } => {
                self.pending_assistant.push_str(delta);
                self.status = "Streaming assistant response...".to_owned();
            }
            EngineEvent::AssistantMessageCompleted { content, .. } => {
                self.entries.push(DisplayEntry::Assistant(content.clone()));
                self.pending_assistant.clear();
                self.status = "Assistant response completed.".to_owned();
            }
            EngineEvent::ToolRequested { tool_name, .. } => {
                self.push_notice(format!("Tool `{tool_name}` requested."));
                self.status = format!("Running tool `{tool_name}`...");
            }
            EngineEvent::ToolPermissionResolved {
                tool_name,
                behavior,
                ..
            } => {
                let message = format!(
                    "Tool `{tool_name}` permission resolved: {}.",
                    permission_behavior_label(*behavior)
                );
                self.push_notice(message.clone());
                self.status = message;
            }
            EngineEvent::ToolCompleted {
                tool_name,
                is_error,
                ..
            } => {
                let message = if *is_error {
                    format!("Tool `{tool_name}` completed with an error.")
                } else {
                    format!("Tool `{tool_name}` completed.")
                };
                self.push_notice(message.clone());
                self.status = message;
            }
            EngineEvent::BudgetContinuationSuggested {
                continuation_count,
                consumed_tokens,
                budget_tokens,
                ..
            } => {
                let message = format!(
                    "Budget continuation suggested ({continuation_count}, {consumed_tokens}/{budget_tokens} tokens)."
                );
                self.push_notice(message.clone());
                self.status = message;
            }
            EngineEvent::CompactionApplied {
                replaced_message_count,
                summary_preview,
                ..
            } => {
                let message = format!(
                    "Transcript compaction applied ({replaced_message_count} messages): {summary_preview}"
                );
                self.push_notice(message.clone());
                self.status = message;
            }
            EngineEvent::EngineFailed { message, .. } => {
                self.push_notice(message.clone());
                self.pending_assistant.clear();
                self.status = message.clone();
            }
            EngineEvent::TurnFinished { stop_reason, .. } => {
                if *stop_reason == StopReason::Cancelled {
                    self.push_notice("Cancelled current request.");
                    self.pending_assistant.clear();
                    self.status = "Cancelled current request.".to_owned();
                }
            }
            EngineEvent::SessionStarted { .. }
            | EngineEvent::TurnStarted { .. }
            | EngineEvent::CommandParsed { .. }
            | EngineEvent::SessionFinished { .. } => {}
        }
    }

    fn complete(&mut self, result: &Result<EngineOutcome, ClawinError>) {
        self.busy = false;
        match result {
            Ok(outcome) => {
                self.status = match outcome.stop_reason {
                    StopReason::Completed => "Ready".to_owned(),
                    StopReason::CommandHandled => "Ready".to_owned(),
                    StopReason::Cancelled => "Cancelled current request.".to_owned(),
                    StopReason::MaxTurnsReached => "Turn limit reached.".to_owned(),
                    StopReason::BudgetStopped => "Budget stop reached.".to_owned(),
                    StopReason::Failed => "Request failed.".to_owned(),
                };
            }
            Err(error) => {
                self.pending_assistant.clear();
                self.entries.push(DisplayEntry::Notice(error.to_string()));
                self.status = error.to_string();
            }
        }
    }

    fn reset_for_restored_session(&mut self, session: &RestoredSession) {
        self.entries = transcript_entries(&session.transcript);
        if session.interruption_state == clawin_core::ResumeInterruptionState::InterruptedPrompt {
            self.entries.push(DisplayEntry::Notice(
                "Restored session with an interrupted prompt notice.".to_owned(),
            ));
            self.status = format!("Resumed interrupted session `{}`.", session.session_id);
        } else {
            self.status = format!("Resumed session `{}`.", session.session_id);
        }
        self.input.clear();
        self.cursor = 0;
        self.pending_assistant.clear();
        self.busy = false;
    }
}

enum WorkerCommand {
    LocalSubmit {
        request: ConversationRequest,
        config: TurnLoopConfig,
    },
    RemoteInput(StructuredInputMessage),
    TransportClosed(String),
    Shutdown,
}

enum WorkerEvent {
    Engine(EngineEvent),
    RemoteTurnStarted { input: String },
    Finished(Box<WorkerFinished>),
}

struct WorkerFinished {
    result: Result<EngineOutcome, ClawinError>,
    transcript: Vec<ConversationMessage>,
    runtime: SessionRuntime,
    restored_session: Option<RestoredSession>,
    bridge_action: Option<BridgeCommandAction>,
}

struct ReplWorker {
    runtime: SessionRuntime,
    commands: CommandRegistry,
    tools: ToolRegistry,
    engine: ConversationEngine,
    driver: Arc<dyn ModelDriver>,
    cancellation_slot: Arc<Mutex<Option<CancellationFlag>>>,
    bridge_output_tx: Sender<StructuredOutputMessage>,
}

/// Conversation-backed REPL controller with a worker thread that owns the engine session.
pub struct ReplController {
    runtime: SessionRuntime,
    transcript: Vec<ConversationMessage>,
    view_state: ReplViewState,
    submit_tx: tokio_mpsc::UnboundedSender<WorkerCommand>,
    event_rx: Receiver<WorkerEvent>,
    cancellation_slot: Arc<Mutex<Option<CancellationFlag>>>,
    bridge_host: Arc<ReplBridgeHost>,
    bridge_status: Option<BridgeStatusSnapshot>,
    worker_handle: Option<JoinHandle<()>>,
}

impl ReplController {
    /// Create a REPL controller from bootstrapped runtime, registries, engine, and model driver.
    pub fn new(
        runtime: SessionRuntime,
        commands: CommandRegistry,
        tools: ToolRegistry,
        engine: ConversationEngine,
        driver: Arc<dyn ModelDriver>,
        cancellation_slot: Arc<Mutex<Option<CancellationFlag>>>,
    ) -> Self {
        let controller_runtime = runtime.clone();
        let transcript = engine.transcript().to_vec();
        let view_state = ReplViewState::from_transcript(&transcript);
        let (submit_tx, submit_rx) = tokio_mpsc::unbounded_channel();
        let (event_tx, event_rx) = mpsc::channel();
        let (bridge_output_tx, bridge_output_rx) = mpsc::channel();
        let bridge_host = Arc::new(ReplBridgeHost::new(submit_tx.clone(), bridge_output_rx));
        let bridge_status = controller_runtime
            .bridge_controller()
            .and_then(|controller| controller.status().ok());
        let worker = ReplWorker {
            runtime,
            commands,
            tools,
            engine,
            driver,
            cancellation_slot: cancellation_slot.clone(),
            bridge_output_tx,
        };
        let worker_handle = thread::spawn(move || {
            worker_loop(worker, submit_rx, event_tx);
        });

        Self {
            runtime: controller_runtime,
            transcript,
            view_state,
            submit_tx,
            event_rx,
            cancellation_slot,
            bridge_host,
            bridge_status,
            worker_handle: Some(worker_handle),
        }
    }

    /// Borrow the runtime captured for this REPL.
    pub fn runtime(&self) -> &SessionRuntime {
        &self.runtime
    }

    /// Borrow the retained engine transcript mirror.
    pub fn transcript(&self) -> &[ConversationMessage] {
        &self.transcript
    }

    /// Whether a submit is currently running.
    pub fn is_busy(&self) -> bool {
        self.view_state.busy
    }

    /// Borrow the current UI view state for tests and snapshots.
    pub fn view_state(&self) -> &ReplViewState {
        &self.view_state
    }

    fn submit_input(&mut self, config: TurnLoopConfig) -> Result<(), ClawinError> {
        let Some(input) = self.view_state.take_submittable_input() else {
            return Ok(());
        };
        let request = if input.starts_with('/') {
            ConversationRequest::SlashCommand(input)
        } else {
            ConversationRequest::Prompt(input)
        };
        self.submit_tx
            .send(WorkerCommand::LocalSubmit { request, config })
            .map_err(|error| ClawinError::EngineProtocol {
                message: format!("repl worker is unavailable: {error}"),
            })
    }

    fn cancel_active(&mut self) {
        let active = self
            .cancellation_slot
            .lock()
            .expect("cancellation slot should be available")
            .clone();

        if let Some(flag) = active {
            flag.cancel();
            self.view_state.status = "Cancelling current request...".to_owned();
        }
    }

    fn drain_worker_events(&mut self) {
        loop {
            match self.event_rx.try_recv() {
                Ok(WorkerEvent::Engine(event)) => self.view_state.apply_engine_event(&event),
                Ok(WorkerEvent::RemoteTurnStarted { input }) => {
                    self.view_state.begin_external_submit(&input);
                }
                Ok(WorkerEvent::Finished(finished)) => {
                    let WorkerFinished {
                        result,
                        transcript,
                        runtime,
                        restored_session,
                        bridge_action,
                    } = *finished;
                    self.runtime = runtime;
                    self.transcript = transcript;
                    if let Some(session) = restored_session {
                        self.view_state.reset_for_restored_session(&session);
                    } else {
                        self.view_state.complete(&result);
                    }
                    if let Some(action) = bridge_action {
                        self.apply_bridge_action(action);
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.view_state.busy = false;
                    self.view_state.status = "REPL worker disconnected.".to_owned();
                    break;
                }
            }
        }

        self.refresh_bridge_status();
    }

    fn apply_bridge_action(&mut self, action: BridgeCommandAction) {
        let Some(controller) = self.runtime.bridge_controller() else {
            let message = "Remote control bridge is unavailable.".to_owned();
            self.view_state.push_notice(message.clone());
            self.view_state.status = message;
            return;
        };

        let status = match action {
            BridgeCommandAction::Start { name } => controller.start(
                &self.runtime,
                Arc::clone(&self.bridge_host) as Arc<dyn BridgeSessionHost>,
                BridgeMode::ReplAttached,
                BridgePointerSource::Repl,
                name,
                None,
            ),
            BridgeCommandAction::Stop => controller.stop(),
        };

        match status {
            Ok(status) => {
                self.bridge_status = Some(status.clone());
                let message = format!("Remote control bridge {}.", status.state.as_str());
                self.view_state.push_notice(message.clone());
                self.view_state.status = message;
            }
            Err(error) => {
                let message = error.to_string();
                self.view_state.push_notice(message.clone());
                self.view_state.status = message;
            }
        }
    }

    fn refresh_bridge_status(&mut self) {
        let Some(controller) = self.runtime.bridge_controller() else {
            self.bridge_status = None;
            return;
        };

        let Ok(status) = controller.status() else {
            return;
        };
        if self.bridge_status.as_ref() == Some(&status) {
            return;
        }

        let previous = self.bridge_status.replace(status.clone());
        if previous.as_ref().is_some_and(|old| {
            old == &status
                || (old.state == BridgeState::Ready && status.state == BridgeState::Ready)
        }) {
            return;
        }

        if status.state != BridgeState::Ready {
            let message = format!("Remote control bridge {}.", status.state.as_str());
            self.view_state.push_notice(message.clone());
            self.view_state.status = message;
        }
    }
}

impl Drop for ReplController {
    fn drop(&mut self) {
        let _ = self.submit_tx.send(WorkerCommand::Shutdown);
        if let Some(handle) = self.worker_handle.take() {
            let _ = handle.join();
        }
    }
}

/// Run the Phase 5 REPL loop on the provided terminal session.
pub fn run_repl_session(
    controller: &mut ReplController,
    terminal_session: &mut dyn TerminalSession,
    config: ReplConfig,
) -> anyhow::Result<ReplExit> {
    terminal_session
        .enter()
        .context("failed to enter terminal session")?;
    controller.view_state.set_size(terminal_session.size());
    draw_view(terminal_session, controller.view_state())
        .context("failed to draw initial repl frame")?;

    let outcome = (|| -> anyhow::Result<ReplExit> {
        loop {
            controller.drain_worker_events();
            draw_view(terminal_session, controller.view_state())
                .context("failed to draw repl frame")?;

            let maybe_event = terminal_session
                .poll_event(config.poll_interval)
                .context("failed to poll terminal event")?;
            if let Some(event) = maybe_event {
                match event {
                    TerminalEvent::Resize(size) => {
                        controller.view_state.set_size(size);
                    }
                    TerminalEvent::Key(key) => {
                        if let Some(exit) = handle_key_event(controller, key, &config.submit)? {
                            return Ok(exit);
                        }
                    }
                }
            }
        }
    })();

    let leave_result = terminal_session.leave();
    match (outcome, leave_result) {
        (Ok(exit), Ok(())) => Ok(exit),
        (Ok(_), Err(error)) => Err(error).context("failed to restore terminal session"),
        (Err(error), Ok(())) => Err(error),
        (Err(error), Err(leave_error)) => {
            Err(error).context(format!("failed to restore terminal session: {leave_error}"))
        }
    }
}

/// Render the current REPL view state into a string snapshot using `ratatui::TestBackend`.
pub fn render_repl_snapshot(state: &ReplViewState, size: TerminalSize) -> String {
    let backend = TestBackend::new(size.width(), size.height());
    let mut terminal = Terminal::new(backend).expect("test backend terminal should initialize");
    terminal
        .draw(|frame| render_repl(frame, state))
        .expect("snapshot rendering should succeed");
    format!("{}", terminal.backend())
}

fn worker_loop(
    mut worker: ReplWorker,
    mut submit_rx: tokio_mpsc::UnboundedReceiver<WorkerCommand>,
    event_tx: Sender<WorkerEvent>,
) {
    let runtime_builder = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build();
    let Ok(runtime_handle) = runtime_builder else {
        let _ = event_tx.send(WorkerEvent::Finished(Box::new(WorkerFinished {
            result: Err(ClawinError::EngineProtocol {
                message: "failed to create repl runtime".to_owned(),
            }),
            transcript: worker.engine.transcript().to_vec(),
            runtime: worker.runtime.clone(),
            restored_session: None,
            bridge_action: None,
        })));
        return;
    };

    runtime_handle.block_on(async move {
        let resolver = Arc::new(BridgePermissionResolver::new(
            worker.bridge_output_tx.clone(),
        ));

        while let Some(command) = submit_rx.recv().await {
            match command {
                WorkerCommand::Shutdown => {
                    let _ = resolver.cancel_all("repl_shutdown", false);
                    break;
                }
                WorkerCommand::LocalSubmit { request, config } => {
                    if run_local_submit(
                        &mut worker,
                        &mut submit_rx,
                        &event_tx,
                        &resolver,
                        request,
                        config,
                    )
                    .await
                    {
                        break;
                    }
                }
                WorkerCommand::RemoteInput(message) => {
                    if run_remote_input(&mut worker, &mut submit_rx, &event_tx, &resolver, message)
                        .await
                    {
                        break;
                    }
                }
                WorkerCommand::TransportClosed(reason) => {
                    let _ = resolver.cancel_all(&reason, false);
                }
            }
        }
    });
}

async fn run_local_submit(
    worker: &mut ReplWorker,
    submit_rx: &mut tokio_mpsc::UnboundedReceiver<WorkerCommand>,
    event_tx: &Sender<WorkerEvent>,
    _resolver: &BridgePermissionResolver,
    request: ConversationRequest,
    config: TurnLoopConfig,
) -> bool {
    let transcript_len_before = worker.engine.transcript().len();
    if let Some(prompt) = request_prompt(&request) {
        if let Err(error) = persist_last_prompt(&worker.runtime, prompt) {
            let _ = event_tx.send(WorkerEvent::Finished(Box::new(WorkerFinished {
                result: Err(error),
                transcript: worker.engine.transcript().to_vec(),
                runtime: worker.runtime.clone(),
                restored_session: None,
                bridge_action: None,
            })));
            return false;
        }
    }

    let cancellation = CancellationFlag::new();
    set_active_cancellation(&worker.cancellation_slot, Some(cancellation.clone()));

    let services = EngineServices::new(
        &worker.runtime,
        &worker.commands,
        &worker.tools,
        worker.driver.as_ref(),
        &PassthroughPermissionResolver,
        cancellation.clone(),
    );
    let (result, should_shutdown) = {
        let submit = worker
            .engine
            .submit_message(&services, request, config, |event| {
                let _ = event_tx.send(WorkerEvent::Engine(event));
            });
        tokio::pin!(submit);

        let mut should_shutdown = false;
        let result = loop {
            tokio::select! {
                result = &mut submit => break result,
                input = submit_rx.recv() => {
                    match input {
                        Some(WorkerCommand::RemoteInput(message)) => {
                            handle_remote_input_during_local_turn(
                                message,
                                &worker.bridge_output_tx,
                                &cancellation,
                            );
                        }
                        Some(WorkerCommand::TransportClosed(_)) => {}
                        Some(WorkerCommand::Shutdown) | None => {
                            cancellation.cancel();
                            should_shutdown = true;
                        }
                        Some(WorkerCommand::LocalSubmit { .. }) => {}
                    }
                }
            }
        };
        (result, should_shutdown)
    };

    set_active_cancellation(&worker.cancellation_slot, None);
    let final_result = finalize_worker_result(worker, transcript_len_before, result);
    emit_finished(worker, event_tx, final_result);
    should_shutdown
}

async fn run_remote_input(
    worker: &mut ReplWorker,
    submit_rx: &mut tokio_mpsc::UnboundedReceiver<WorkerCommand>,
    event_tx: &Sender<WorkerEvent>,
    resolver: &BridgePermissionResolver,
    message: StructuredInputMessage,
) -> bool {
    match message {
        StructuredInputMessage::User { content } => {
            let request = request_from_text(content.clone());
            let transcript_len_before = worker.engine.transcript().len();
            if let Some(prompt) = request_prompt(&request) {
                if let Err(error) = persist_last_prompt(&worker.runtime, prompt) {
                    let _ = worker
                        .bridge_output_tx
                        .send(StructuredOutputMessage::Error {
                            code: structured_error_code(&error).to_owned(),
                            message: error.to_string(),
                        });
                    let _ = event_tx.send(WorkerEvent::Finished(Box::new(WorkerFinished {
                        result: Err(error),
                        transcript: worker.engine.transcript().to_vec(),
                        runtime: worker.runtime.clone(),
                        restored_session: None,
                        bridge_action: None,
                    })));
                    return false;
                }
            }

            let _ = event_tx.send(WorkerEvent::RemoteTurnStarted {
                input: content.clone(),
            });

            let cancellation = CancellationFlag::new();
            set_active_cancellation(&worker.cancellation_slot, Some(cancellation.clone()));

            let command_output = Arc::new(Mutex::new(None::<String>));
            let bridge_output_error = Arc::new(Mutex::new(None::<String>));
            let services = EngineServices::new(
                &worker.runtime,
                &worker.commands,
                &worker.tools,
                worker.driver.as_ref(),
                resolver,
                cancellation.clone(),
            );
            let (result, should_shutdown) = {
                let command_output_sink = Arc::clone(&command_output);
                let bridge_output_error_sink = Arc::clone(&bridge_output_error);
                let submit = worker.engine.submit_message(
                    &services,
                    request,
                    remote_turn_config(),
                    |event| {
                        if let EngineEvent::CommandExecuted { output, .. } = &event {
                            *command_output_sink
                                .lock()
                                .expect("remote command output lock should be available") =
                                Some(output.clone());
                        }
                        let _ = event_tx.send(WorkerEvent::Engine(event.clone()));
                        let mut error_guard = bridge_output_error_sink
                            .lock()
                            .expect("remote bridge output error lock should be available");
                        if error_guard.is_none() {
                            if let Err(error) = worker
                                .bridge_output_tx
                                .send(StructuredOutputMessage::StreamEvent { event })
                            {
                                *error_guard = Some(error.to_string());
                            }
                        }
                    },
                );
                tokio::pin!(submit);

                let mut should_shutdown = false;
                let result = loop {
                    tokio::select! {
                        result = &mut submit => break result,
                        input = submit_rx.recv() => {
                            match input {
                                Some(WorkerCommand::RemoteInput(message)) => {
                                    handle_remote_input_during_remote_turn(
                                        message,
                                        resolver,
                                        &worker.bridge_output_tx,
                                        &cancellation,
                                    );
                                }
                                Some(WorkerCommand::LocalSubmit { .. }) => {
                                    // The UI thread should already be marked busy by `RemoteTurnStarted`.
                                    // Ignore raced local submits instead of perturbing the active remote turn.
                                }
                                Some(WorkerCommand::TransportClosed(reason)) => {
                                    cancellation.cancel();
                                    let _ = resolver.cancel_all(&reason, true);
                                }
                                Some(WorkerCommand::Shutdown) | None => {
                                    cancellation.cancel();
                                    let _ = resolver.cancel_all("repl_shutdown", true);
                                    should_shutdown = true;
                                }
                            }
                        }
                    }
                };
                (result, should_shutdown)
            };

            set_active_cancellation(&worker.cancellation_slot, None);

            let mut final_result = finalize_worker_result(worker, transcript_len_before, result);
            if let Some(message) = bridge_output_error
                .lock()
                .expect("remote bridge output error lock should be available")
                .take()
            {
                if final_result.is_ok() {
                    final_result = Err(ClawinError::EngineProtocol {
                        message: format!("failed to emit remote bridge output: {message}"),
                    });
                }
            }

            match &final_result {
                Ok(outcome) => {
                    let _ = worker
                        .bridge_output_tx
                        .send(StructuredOutputMessage::Result {
                            result: Box::new(clawin_core::StructuredRunResult {
                                outcome: outcome.clone(),
                                command_output: command_output
                                    .lock()
                                    .expect("remote command output lock should be available")
                                    .clone(),
                            }),
                        });
                }
                Err(error) => {
                    let _ = worker
                        .bridge_output_tx
                        .send(StructuredOutputMessage::Error {
                            code: structured_error_code(error).to_owned(),
                            message: error.to_string(),
                        });
                }
            }

            emit_finished(worker, event_tx, final_result);
            should_shutdown
        }
        StructuredInputMessage::ControlRequest { .. } => {
            let _ = worker
                .bridge_output_tx
                .send(StructuredOutputMessage::Error {
                    code: "no_active_turn".to_owned(),
                    message: "interrupt control request requires an active turn".to_owned(),
                });
            false
        }
        StructuredInputMessage::ControlResponse { response } => {
            match resolver.apply_response(response) {
                Ok(true) => {}
                Ok(false) => {
                    let _ = worker
                        .bridge_output_tx
                        .send(StructuredOutputMessage::Error {
                            code: "unexpected_control_response".to_owned(),
                            message: "received a control response without an active turn"
                                .to_owned(),
                        });
                }
                Err(error) => {
                    let _ = worker
                        .bridge_output_tx
                        .send(StructuredOutputMessage::Error {
                            code: "invalid_input_message".to_owned(),
                            message: error.to_string(),
                        });
                }
            }
            false
        }
        StructuredInputMessage::KeepAlive => {
            let _ = worker
                .bridge_output_tx
                .send(StructuredOutputMessage::KeepAlive);
            false
        }
    }
}

fn finalize_worker_result(
    worker: &mut ReplWorker,
    transcript_len_before: usize,
    result: Result<EngineOutcome, ClawinError>,
) -> Result<EngineOutcome, ClawinError> {
    let restored_session = result
        .as_ref()
        .ok()
        .and_then(|outcome| outcome.command_effect.as_ref())
        .and_then(|effect| match effect {
            CommandEffect::ResumeSession { session } => Some(session.clone()),
            CommandEffect::BridgeControl { .. } => None,
        });
    let mut final_result = result;

    if let Some(session) = restored_session.as_ref() {
        worker.runtime = restore_runtime(&worker.runtime, session);
        worker.engine =
            ConversationEngine::restore(session.session_id.clone(), session.transcript.clone());
    } else if let Err(error) = persist_transcript_delta(
        &worker.runtime,
        transcript_len_before,
        worker.engine.transcript(),
    ) {
        if final_result.is_ok() {
            final_result = Err(error);
        }
    }

    final_result
}

fn emit_finished(
    worker: &ReplWorker,
    event_tx: &Sender<WorkerEvent>,
    result: Result<EngineOutcome, ClawinError>,
) {
    let restored_session = result
        .as_ref()
        .ok()
        .and_then(|outcome| outcome.command_effect.as_ref())
        .and_then(|effect| match effect {
            CommandEffect::ResumeSession { session } => Some(session.clone()),
            CommandEffect::BridgeControl { .. } => None,
        });
    let bridge_action = result
        .as_ref()
        .ok()
        .and_then(|outcome| outcome.command_effect.as_ref())
        .and_then(|effect| match effect {
            CommandEffect::ResumeSession { .. } => None,
            CommandEffect::BridgeControl { action } => Some(action.clone()),
        });

    let _ = event_tx.send(WorkerEvent::Finished(Box::new(WorkerFinished {
        result,
        transcript: worker.engine.transcript().to_vec(),
        runtime: worker.runtime.clone(),
        restored_session,
        bridge_action,
    })));
}

fn handle_remote_input_during_local_turn(
    message: StructuredInputMessage,
    output_tx: &Sender<StructuredOutputMessage>,
    cancellation: &CancellationFlag,
) {
    match message {
        StructuredInputMessage::User { .. } => {
            let _ = output_tx.send(StructuredOutputMessage::Error {
                code: "busy".to_owned(),
                message: "a local repl turn is already running".to_owned(),
            });
        }
        StructuredInputMessage::ControlRequest {
            request: StructuredInputControlRequest::Interrupt,
        } => {
            cancellation.cancel();
        }
        StructuredInputMessage::ControlResponse { .. } => {
            let _ = output_tx.send(StructuredOutputMessage::Error {
                code: "unexpected_control_response".to_owned(),
                message: "received a control response without an active bridge turn".to_owned(),
            });
        }
        StructuredInputMessage::KeepAlive => {
            let _ = output_tx.send(StructuredOutputMessage::KeepAlive);
        }
    }
}

fn handle_remote_input_during_remote_turn(
    message: StructuredInputMessage,
    resolver: &BridgePermissionResolver,
    output_tx: &Sender<StructuredOutputMessage>,
    cancellation: &CancellationFlag,
) {
    match message {
        StructuredInputMessage::User { .. } => {
            let _ = output_tx.send(StructuredOutputMessage::Error {
                code: "busy".to_owned(),
                message: "a remote-control turn is already running".to_owned(),
            });
        }
        StructuredInputMessage::ControlRequest {
            request: StructuredInputControlRequest::Interrupt,
        } => {
            cancellation.cancel();
            let _ = resolver.cancel_all("interrupt", true);
        }
        StructuredInputMessage::ControlResponse { response } => {
            match resolver.apply_response(response) {
                Ok(true) => {}
                Ok(false) => {
                    let _ = output_tx.send(StructuredOutputMessage::Error {
                        code: "unexpected_control_response".to_owned(),
                        message: "received a control response without an active turn".to_owned(),
                    });
                }
                Err(error) => {
                    let _ = output_tx.send(StructuredOutputMessage::Error {
                        code: "invalid_input_message".to_owned(),
                        message: error.to_string(),
                    });
                }
            }
        }
        StructuredInputMessage::KeepAlive => {
            let _ = output_tx.send(StructuredOutputMessage::KeepAlive);
        }
    }
}

fn set_active_cancellation(
    slot: &Arc<Mutex<Option<CancellationFlag>>>,
    value: Option<CancellationFlag>,
) {
    let mut guard = slot.lock().expect("cancellation slot should be available");
    *guard = value;
}

fn request_from_text(content: String) -> ConversationRequest {
    let normalized = content.trim_end_matches(['\r', '\n']).to_owned();
    if normalized.trim_start().starts_with('/') {
        ConversationRequest::SlashCommand(normalized)
    } else {
        ConversationRequest::Prompt(normalized)
    }
}

fn remote_turn_config() -> TurnLoopConfig {
    TurnLoopConfig {
        max_turns: 4,
        token_budget: None,
        compaction_policy: clawin_core::CompactionPolicy::Disabled,
        allow_budget_continuation: false,
    }
}

fn structured_error_code(error: &ClawinError) -> &'static str {
    match error {
        ClawinError::NotImplemented { .. } => "not_implemented",
        ClawinError::InvalidConfiguration { .. } => "invalid_configuration",
        ClawinError::UnknownCommand { .. } => "unknown_command",
        ClawinError::InvalidCommandInvocation { .. } => "invalid_command_invocation",
        ClawinError::UnknownTool { .. } => "unknown_tool",
        ClawinError::ToolInputInvalid { .. } => "tool_input_invalid",
        ClawinError::ToolExecution { .. } => "tool_execution_failed",
        ClawinError::ModelDriver { .. } => "model_driver_failed",
        ClawinError::EngineProtocol { .. } => "engine_protocol_failed",
    }
}

fn handle_key_event(
    controller: &mut ReplController,
    key: TerminalKeyEvent,
    config: &TurnLoopConfig,
) -> Result<Option<ReplExit>, ClawinError> {
    if key.modifiers() == TerminalKeyModifiers::CONTROL && key.code() == TerminalKeyCode::Char('c')
    {
        if controller.view_state.busy {
            controller.cancel_active();
            return Ok(None);
        }

        return Ok(Some(ReplExit::user_exit()));
    }

    if controller.view_state.busy {
        return Ok(None);
    }

    match key.code() {
        TerminalKeyCode::Char(ch) => controller.view_state.insert_char(ch),
        TerminalKeyCode::Backspace => controller.view_state.backspace(),
        TerminalKeyCode::Delete => controller.view_state.delete(),
        TerminalKeyCode::Left => controller.view_state.move_left(),
        TerminalKeyCode::Right => controller.view_state.move_right(),
        TerminalKeyCode::Home => controller.view_state.move_home(),
        TerminalKeyCode::End => controller.view_state.move_end(),
        TerminalKeyCode::Enter => controller.submit_input(config.clone())?,
        TerminalKeyCode::Esc => controller.view_state.status = "Ready".to_owned(),
    }

    Ok(None)
}

fn draw_view(
    terminal_session: &mut dyn TerminalSession,
    state: &ReplViewState,
) -> anyhow::Result<()> {
    let backend = CrosstermBackend::new(terminal_session.writer());
    let mut terminal = Terminal::new(backend).context("failed to create crossterm backend")?;
    terminal
        .draw(|frame| render_repl(frame, state))
        .context("failed to draw crossterm frame")?;
    Ok(())
}

fn render_repl(frame: &mut ratatui::Frame<'_>, state: &ReplViewState) {
    let areas = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(5),
            Constraint::Length(1),
            Constraint::Length(3),
        ])
        .split(frame.area());

    let transcript = Paragraph::new(build_transcript_text(state))
        .block(Block::default().borders(Borders::ALL).title("Transcript"))
        .wrap(Wrap { trim: false });
    frame.render_widget(transcript, areas[0]);

    let status =
        Paragraph::new(Line::from(state.status.clone())).block(Block::default().title("Status"));
    frame.render_widget(status, areas[1]);

    let composer = Paragraph::new(Line::from(state.input.clone()))
        .block(Block::default().borders(Borders::ALL).title("Composer"));
    frame.render_widget(composer, areas[2]);
}

fn build_transcript_text(state: &ReplViewState) -> Text<'static> {
    let mut lines = Vec::new();

    for entry in &state.entries {
        let content = entry.content();
        for (index, line) in content.lines().enumerate() {
            if index == 0 {
                lines.push(Line::from(format!("{}: {line}", entry.line_prefix())));
            } else {
                lines.push(Line::from(format!("  {line}")));
            }
        }

        if content.is_empty() {
            lines.push(Line::from(format!("{}:", entry.line_prefix())));
        }
    }

    if !state.pending_assistant.is_empty() {
        let mut pending = String::new();
        let _ = write!(pending, "Assistant: {}", state.pending_assistant);
        lines.push(Line::from(pending));
    }

    if lines.is_empty() {
        lines.push(Line::from("Status: REPL is ready."));
    }

    Text::from(lines)
}

fn byte_index(input: &str, char_index: usize) -> usize {
    input
        .char_indices()
        .nth(char_index)
        .map(|(index, _)| index)
        .unwrap_or_else(|| input.len())
}

fn permission_behavior_label(behavior: PermissionBehavior) -> &'static str {
    match behavior {
        PermissionBehavior::Allow => "allow",
        PermissionBehavior::Ask => "ask",
        PermissionBehavior::Deny => "deny",
    }
}

fn transcript_entries(transcript: &[ConversationMessage]) -> Vec<DisplayEntry> {
    transcript
        .iter()
        .map(|message| match message {
            ConversationMessage::System { content } => DisplayEntry::Notice(content.clone()),
            ConversationMessage::User { content } => DisplayEntry::User(content.clone()),
            ConversationMessage::Assistant { content } => DisplayEntry::Assistant(content.clone()),
            ConversationMessage::ToolUse { tool_name, .. } => {
                DisplayEntry::Notice(format!("Tool `{tool_name}` requested."))
            }
            ConversationMessage::ToolResult {
                tool_name,
                is_error,
                ..
            } => {
                let label = if *is_error { "failed" } else { "completed" };
                DisplayEntry::Notice(format!("Tool `{tool_name}` {label}."))
            }
            ConversationMessage::CompactSummary { content, .. } => {
                DisplayEntry::Notice(format!("Compact summary: {content}"))
            }
        })
        .collect()
}

fn request_prompt(request: &ConversationRequest) -> Option<&str> {
    match request {
        ConversationRequest::Prompt(prompt) => Some(prompt.as_str()),
        ConversationRequest::SlashCommand(_) => None,
    }
}

fn persist_last_prompt(runtime: &SessionRuntime, prompt: &str) -> Result<(), ClawinError> {
    if let Some(store) = runtime.session_store() {
        store.save_last_prompt(runtime, prompt)?;
    }
    Ok(())
}

fn persist_transcript_delta(
    runtime: &SessionRuntime,
    previous_len: usize,
    transcript: &[ConversationMessage],
) -> Result<(), ClawinError> {
    let Some(store) = runtime.session_store() else {
        return Ok(());
    };

    for message in transcript.iter().skip(previous_len) {
        store.append_message(runtime, message)?;
    }
    Ok(())
}

fn restore_runtime(current: &SessionRuntime, session: &RestoredSession) -> SessionRuntime {
    let runtime = SessionRuntime::new(
        session.session_id.clone(),
        current.capabilities(),
        current.launch_cwd().to_path_buf(),
        session.canonical_project_root.clone(),
        current.permission_mode(),
    );
    runtime.set_active_project_root(session.active_project_root.clone());
    runtime.set_current_cwd(session.active_project_root.clone());
    runtime.set_active_worktree(session.worktree_state.clone());
    if let Some(store) = current.session_store() {
        runtime.set_session_store(store);
    }
    if let Some(manager) = current.worktree_manager() {
        runtime.set_worktree_manager(manager);
    }
    if let Some(controller) = current.bridge_controller() {
        runtime.set_bridge_controller(controller);
    }
    runtime
}

#[derive(Debug)]
struct ReplBridgeHost {
    command_tx: tokio_mpsc::UnboundedSender<WorkerCommand>,
    output_rx: Mutex<Receiver<StructuredOutputMessage>>,
}

impl ReplBridgeHost {
    fn new(
        command_tx: tokio_mpsc::UnboundedSender<WorkerCommand>,
        output_rx: Receiver<StructuredOutputMessage>,
    ) -> Self {
        Self {
            command_tx,
            output_rx: Mutex::new(output_rx),
        }
    }
}

impl BridgeSessionHost for ReplBridgeHost {
    fn send_input(&self, message: StructuredInputMessage) -> Result<(), ClawinError> {
        self.command_tx
            .send(WorkerCommand::RemoteInput(message))
            .map_err(|error| ClawinError::EngineProtocol {
                message: format!("repl bridge host is unavailable: {error}"),
            })
    }

    fn recv_output(
        &self,
        timeout: Duration,
    ) -> Result<Option<StructuredOutputMessage>, ClawinError> {
        let receiver = self
            .output_rx
            .lock()
            .expect("repl bridge output lock should be available");
        match receiver.recv_timeout(timeout) {
            Ok(message) => Ok(Some(message)),
            Err(RecvTimeoutError::Timeout) => Ok(None),
            Err(RecvTimeoutError::Disconnected) => Ok(None),
        }
    }

    fn notify_transport_closed(&self, reason: &str) -> Result<(), ClawinError> {
        self.command_tx
            .send(WorkerCommand::TransportClosed(reason.to_owned()))
            .map_err(|error| ClawinError::EngineProtocol {
                message: format!(
                    "failed to notify repl bridge host about transport close: {error}"
                ),
            })
    }
}

struct BridgePermissionResolver {
    output_tx: Sender<StructuredOutputMessage>,
    next_request_id: AtomicU64,
    pending: Mutex<BTreeMap<String, oneshot::Sender<PermissionDecision>>>,
}

impl BridgePermissionResolver {
    fn new(output_tx: Sender<StructuredOutputMessage>) -> Self {
        Self {
            output_tx,
            next_request_id: AtomicU64::new(1),
            pending: Mutex::new(BTreeMap::new()),
        }
    }

    fn apply_response(&self, response: StructuredControlResponse) -> Result<bool, ClawinError> {
        let StructuredControlResponse::CanUseTool {
            request_id,
            behavior,
            message,
        } = response;
        if behavior == PermissionBehavior::Ask {
            return Err(ClawinError::EngineProtocol {
                message: "control_response can_use_tool must resolve to allow or deny".to_owned(),
            });
        }

        let sender = self
            .pending
            .lock()
            .map_err(|_| ClawinError::EngineProtocol {
                message: "permission resolver state lock should be available".to_owned(),
            })?
            .remove(&request_id);

        if let Some(sender) = sender {
            let _ = sender.send(PermissionDecision::new(behavior, message));
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn cancel_all(&self, reason: &str, emit_cancel_request: bool) -> Result<(), ClawinError> {
        let mut pending = self
            .pending
            .lock()
            .map_err(|_| ClawinError::EngineProtocol {
                message: "permission resolver state lock should be available".to_owned(),
            })?;
        let entries = std::mem::take(&mut *pending);
        drop(pending);

        for (request_id, sender) in entries {
            if emit_cancel_request {
                self.output_tx
                    .send(StructuredOutputMessage::ControlCancelRequest {
                        request_id: request_id.clone(),
                        reason: reason.to_owned(),
                    })
                    .map_err(|error| ClawinError::EngineProtocol {
                        message: format!("failed to emit control cancel request: {error}"),
                    })?;
            }
            let _ = sender.send(PermissionDecision::new(
                PermissionBehavior::Deny,
                Some(reason.to_owned()),
            ));
        }

        Ok(())
    }
}

impl PermissionResolver for BridgePermissionResolver {
    fn resolve(
        &self,
        call: &clawin_core::ToolCall,
        decision: PermissionDecision,
    ) -> PermissionResolverFuture<'_> {
        let request_id = format!(
            "perm-{}",
            self.next_request_id.fetch_add(1, Ordering::SeqCst)
        );
        let (tx, rx) = oneshot::channel();
        let call_id = call.call_id.clone();
        let tool_name = call.tool_name.clone();
        let input = call.input.clone();
        let message = decision.message.clone();

        Box::pin(async move {
            self.pending
                .lock()
                .map_err(|_| ClawinError::EngineProtocol {
                    message: "permission resolver state lock should be available".to_owned(),
                })?
                .insert(request_id.clone(), tx);

            if let Err(error) = self
                .output_tx
                .send(StructuredOutputMessage::ControlRequest {
                    request: StructuredControlRequest::CanUseTool {
                        request_id: request_id.clone(),
                        call_id,
                        tool_name,
                        input,
                        message,
                    },
                })
            {
                let _ = self
                    .pending
                    .lock()
                    .map_err(|_| ClawinError::EngineProtocol {
                        message: "permission resolver state lock should be available".to_owned(),
                    })?
                    .remove(&request_id);
                return Err(ClawinError::EngineProtocol {
                    message: format!("failed to emit control request: {error}"),
                });
            }

            match rx.await {
                Ok(decision) => Ok(decision),
                Err(_) => Ok(PermissionDecision::new(
                    PermissionBehavior::Deny,
                    Some("permission request was cancelled".to_owned()),
                )),
            }
        })
    }
}
