use std::collections::BTreeMap;
use std::process::ExitCode;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use anyhow::{Result, anyhow};
use clawin_core::{
    BridgeMode, BridgePointer, BridgePointerSource, BridgeSessionHost, CancellationFlag,
    ClawinError, ClawinResult, ConversationRequest, EngineEvent, ModelDriver, PermissionBehavior,
    PermissionDecision, PermissionResolver, PermissionResolverFuture, StructuredControlRequest,
    StructuredControlResponse, StructuredInputControlRequest, StructuredInputMessage,
    StructuredOutputMessage, StructuredRunResult, TurnLoopConfig,
};
use tokio::sync::{mpsc as tokio_mpsc, oneshot};

use crate::run::BootstrappedSession;

enum HostInput {
    Message(StructuredInputMessage),
    TransportClosed(String),
    Shutdown,
}

/// Run the standalone `clawin remote-control` bridge worker against the already-bootstrapped session.
pub fn run_remote_control_session(
    session: BootstrappedSession,
    driver: Arc<dyn ModelDriver>,
    name: Option<String>,
    pointer: Option<BridgePointer>,
) -> Result<ExitCode> {
    let controller = session
        .runtime()
        .bridge_controller()
        .ok_or_else(|| anyhow!("remote control bridge is unavailable"))?;
    let host = Arc::new(StandaloneBridgeHost::new(session, driver));
    let source = pointer
        .as_ref()
        .map(|pointer| pointer.source)
        .unwrap_or(BridgePointerSource::Standalone);
    let status = controller
        .start(
            host.runtime(),
            host.clone(),
            BridgeMode::Standalone,
            source,
            name,
            pointer,
        )
        .map_err(|error| anyhow!(error.to_string()))?;

    println!("{}", render_remote_control_status(&status));

    let final_status = controller
        .wait_for_terminal_state()
        .map_err(|error| anyhow!(error.to_string()))?;
    if final_status != status {
        println!("{}", render_remote_control_status(&final_status));
    }

    Ok(match final_status.state {
        clawin_core::BridgeState::Failed => ExitCode::from(1),
        _ => ExitCode::SUCCESS,
    })
}

struct StandaloneBridgeHost {
    runtime: clawin_core::SessionRuntime,
    input_tx: tokio_mpsc::UnboundedSender<HostInput>,
    output_rx: Mutex<Receiver<StructuredOutputMessage>>,
    worker: Mutex<Option<JoinHandle<()>>>,
}

impl StandaloneBridgeHost {
    fn new(session: BootstrappedSession, driver: Arc<dyn ModelDriver>) -> Self {
        let runtime = session.runtime().clone();
        let (input_tx, input_rx) = tokio_mpsc::unbounded_channel();
        let (output_tx, output_rx) = mpsc::channel();
        let handle = thread::spawn(move || {
            standalone_host_worker(session, driver, input_rx, output_tx);
        });

        Self {
            runtime,
            input_tx,
            output_rx: Mutex::new(output_rx),
            worker: Mutex::new(Some(handle)),
        }
    }

    fn runtime(&self) -> &clawin_core::SessionRuntime {
        &self.runtime
    }
}

impl Drop for StandaloneBridgeHost {
    fn drop(&mut self) {
        let _ = self.input_tx.send(HostInput::Shutdown);
        if let Some(handle) = self
            .worker
            .lock()
            .expect("remote control host worker lock should be available")
            .take()
        {
            let _ = handle.join();
        }
    }
}

impl BridgeSessionHost for StandaloneBridgeHost {
    fn send_input(&self, message: StructuredInputMessage) -> ClawinResult<()> {
        self.input_tx
            .send(HostInput::Message(message))
            .map_err(|error| ClawinError::EngineProtocol {
                message: format!("remote control host is unavailable: {error}"),
            })
    }

    fn recv_output(&self, timeout: Duration) -> ClawinResult<Option<StructuredOutputMessage>> {
        let receiver = self
            .output_rx
            .lock()
            .expect("remote control host output lock should be available");
        match receiver.recv_timeout(timeout) {
            Ok(message) => Ok(Some(message)),
            Err(RecvTimeoutError::Timeout) => Ok(None),
            Err(RecvTimeoutError::Disconnected) => Ok(None),
        }
    }

    fn notify_transport_closed(&self, reason: &str) -> ClawinResult<()> {
        self.input_tx
            .send(HostInput::TransportClosed(reason.to_owned()))
            .map_err(|error| ClawinError::EngineProtocol {
                message: format!(
                    "failed to notify remote control host about transport close: {error}"
                ),
            })
    }
}

fn standalone_host_worker(
    mut session: BootstrappedSession,
    driver: Arc<dyn ModelDriver>,
    mut input_rx: tokio_mpsc::UnboundedReceiver<HostInput>,
    output_tx: Sender<StructuredOutputMessage>,
) {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build();
    let Ok(runtime) = runtime else {
        let _ = output_tx.send(StructuredOutputMessage::Error {
            code: "engine_protocol_failed".to_owned(),
            message: "failed to create remote-control runtime".to_owned(),
        });
        return;
    };

    runtime.block_on(async move {
        let resolver = ChannelPermissionResolver::new(output_tx.clone());
        loop {
            match input_rx.recv().await {
                Some(HostInput::Message(StructuredInputMessage::User { content })) => {
                    let request = request_from_text(content);
                    let cancellation = CancellationFlag::new();
                    let submit = execute_request(
                        &mut session,
                        driver.as_ref(),
                        request,
                        &resolver,
                        cancellation.clone(),
                        &output_tx,
                    );
                    tokio::pin!(submit);

                    loop {
                        tokio::select! {
                            outcome = &mut submit => {
                                match outcome {
                                    Ok(result) => {
                                        let _ = output_tx.send(StructuredOutputMessage::Result {
                                            result: Box::new(result),
                                        });
                                    }
                                    Err(error) => {
                                        let _ = output_tx.send(StructuredOutputMessage::Error {
                                            code: structured_error_code(&error).to_owned(),
                                            message: error.to_string(),
                                        });
                                    }
                                }
                                break;
                            }
                            input = input_rx.recv() => {
                                match input {
                                    Some(HostInput::Message(message)) => {
                                        handle_active_input(message, &resolver, &output_tx, &cancellation);
                                    }
                                    Some(HostInput::TransportClosed(reason)) => {
                                        cancellation.cancel();
                                        let _ = resolver.cancel_all(&reason, true);
                                    }
                                    Some(HostInput::Shutdown) | None => {
                                        cancellation.cancel();
                                        let _ = resolver.cancel_all("host_shutdown", true);
                                        return;
                                    }
                                }
                            }
                        }
                    }
                }
                Some(HostInput::Message(message)) => {
                    handle_idle_input(message, &resolver, &output_tx);
                }
                Some(HostInput::TransportClosed(reason)) => {
                    let _ = resolver.cancel_all(&reason, true);
                }
                Some(HostInput::Shutdown) | None => {
                    let _ = resolver.cancel_all("host_shutdown", false);
                    break;
                }
            }
        }
    });
}

async fn execute_request(
    session: &mut BootstrappedSession,
    driver: &dyn ModelDriver,
    request: ConversationRequest,
    resolver: &dyn PermissionResolver,
    cancellation: CancellationFlag,
    output_tx: &Sender<StructuredOutputMessage>,
) -> ClawinResult<StructuredRunResult> {
    let mut command_output = None;
    let mut output_error = None;
    let outcome = session
        .submit_with_driver_and_resolver(
            driver,
            request,
            headless_turn_config(),
            resolver,
            cancellation,
            |event| {
                if let EngineEvent::CommandExecuted { output, .. } = &event {
                    command_output = Some(output.clone());
                }
                if output_error.is_none() {
                    if let Err(error) = output_tx.send(StructuredOutputMessage::StreamEvent {
                        event: event.clone(),
                    }) {
                        output_error = Some(error.to_string());
                    }
                }
            },
        )
        .await?;

    if let Some(message) = output_error {
        return Err(ClawinError::EngineProtocol {
            message: format!("failed to emit remote-control stream output: {message}"),
        });
    }

    Ok(StructuredRunResult {
        outcome,
        command_output,
    })
}

fn handle_idle_input(
    message: StructuredInputMessage,
    resolver: &ChannelPermissionResolver,
    output_tx: &Sender<StructuredOutputMessage>,
) {
    match message {
        StructuredInputMessage::User { .. } => {
            let _ = output_tx.send(StructuredOutputMessage::Error {
                code: "busy".to_owned(),
                message: "a remote-control turn is already running".to_owned(),
            });
        }
        StructuredInputMessage::ControlRequest { .. } => {
            let _ = output_tx.send(StructuredOutputMessage::Error {
                code: "no_active_turn".to_owned(),
                message: "interrupt control request requires an active turn".to_owned(),
            });
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

fn handle_active_input(
    message: StructuredInputMessage,
    resolver: &ChannelPermissionResolver,
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

fn request_from_text(content: String) -> ConversationRequest {
    let normalized = content.trim_end_matches(['\r', '\n']).to_owned();
    if normalized.trim_start().starts_with('/') {
        ConversationRequest::SlashCommand(normalized)
    } else {
        ConversationRequest::Prompt(normalized)
    }
}

fn headless_turn_config() -> TurnLoopConfig {
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

fn render_remote_control_status(status: &clawin_core::BridgeStatusSnapshot) -> String {
    let mut lines = vec![format!("Remote control bridge: {}", status.state.as_str())];
    if let Some(mode) = status.mode {
        lines.push(format!("mode={}", mode.as_str()));
    }
    if let Some(source) = status.source {
        lines.push(format!("source={}", source.as_str()));
    }
    if let Some(name) = status.name.as_ref() {
        lines.push(format!("name={name}"));
    }
    if let Some(environment_id) = status.environment_id.as_ref() {
        lines.push(format!("environment_id={environment_id}"));
    }
    if let Some(bridge_session_id) = status.bridge_session_id.as_ref() {
        lines.push(format!("bridge_session_id={bridge_session_id}"));
    }
    if let Some(local_session_id) = status.local_session_id.as_ref() {
        lines.push(format!("local_session_id={local_session_id}"));
    }
    if let Some(transcript_path) = status.transcript_path.as_ref() {
        lines.push(format!("transcript_path={}", transcript_path.display()));
    }
    if let Some(last_error) = status.last_error.as_ref() {
        lines.push(format!("last_error={last_error}"));
    }
    lines.push(String::new());
    lines.join("\n")
}

struct ChannelPermissionResolver {
    output_tx: Sender<StructuredOutputMessage>,
    next_request_id: AtomicU64,
    pending: Mutex<BTreeMap<String, oneshot::Sender<PermissionDecision>>>,
}

impl ChannelPermissionResolver {
    fn new(output_tx: Sender<StructuredOutputMessage>) -> Self {
        Self {
            output_tx,
            next_request_id: AtomicU64::new(1),
            pending: Mutex::new(BTreeMap::new()),
        }
    }

    fn apply_response(&self, response: StructuredControlResponse) -> Result<bool> {
        let StructuredControlResponse::CanUseTool {
            request_id,
            behavior,
            message,
        } = response;
        if behavior == PermissionBehavior::Ask {
            return Err(anyhow!(
                "control_response can_use_tool must resolve to allow or deny"
            ));
        }

        let sender = self
            .pending
            .lock()
            .map_err(|_| anyhow!("permission resolver state lock should be available"))?
            .remove(&request_id);

        if let Some(sender) = sender {
            let _ = sender.send(PermissionDecision::new(behavior, message));
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn cancel_all(&self, reason: &str, emit_cancel_request: bool) -> Result<()> {
        let mut pending = self
            .pending
            .lock()
            .map_err(|_| anyhow!("permission resolver state lock should be available"))?;
        let entries = std::mem::take(&mut *pending);
        drop(pending);

        for (request_id, sender) in entries {
            if emit_cancel_request {
                self.output_tx
                    .send(StructuredOutputMessage::ControlCancelRequest {
                        request_id: request_id.clone(),
                        reason: reason.to_owned(),
                    })
                    .map_err(|error| anyhow!("failed to emit control cancel request: {error}"))?;
            }
            let _ = sender.send(PermissionDecision::new(
                PermissionBehavior::Deny,
                Some(reason.to_owned()),
            ));
        }

        Ok(())
    }
}

impl PermissionResolver for ChannelPermissionResolver {
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
