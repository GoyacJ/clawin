use std::collections::BTreeMap;
use std::io::{BufRead, IsTerminal, Read, Write};
use std::process::ExitCode;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result, anyhow};
use clap::error::ErrorKind;
use clawin_core::{
    CancellationFlag, ClawinError, ClawinResult, ConversationRequest, EngineEvent, ModelDriver,
    PassthroughPermissionResolver, PermissionBehavior, PermissionDecision, PermissionResolver,
    PermissionResolverFuture, StopReason, StructuredControlRequest, StructuredControlResponse,
    StructuredInputControlRequest, StructuredInputMessage, StructuredOutputMessage,
    StructuredRunResult, TurnLoopConfig,
};
use tokio::sync::{mpsc, oneshot};

use crate::cli::{PrintInputFormat, PrintOptions, PrintOutputFormat};
use crate::run::BootstrappedSession;

type SharedWriter = Arc<Mutex<Box<dyn Write + Send>>>;

#[derive(Debug)]
pub(super) enum PrintModeError {
    Cli(clap::Error),
    Runtime(anyhow::Error),
}

impl From<anyhow::Error> for PrintModeError {
    fn from(error: anyhow::Error) -> Self {
        Self::Runtime(error)
    }
}

enum RunnerInput {
    Message(StructuredInputMessage),
    Invalid(String),
    Eof,
}

pub(super) fn run_print_mode(
    session: BootstrappedSession,
    driver: Arc<dyn ModelDriver>,
    options: PrintOptions,
) -> std::result::Result<ExitCode, PrintModeError> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("failed to create headless print runtime")?;
    let stdout = shared_writer(std::io::stdout());
    let stderr = shared_writer(std::io::stderr());

    match options.input_format {
        PrintInputFormat::Text => {
            let stdin = Box::new(std::io::stdin()) as Box<dyn Read + Send>;
            runtime.block_on(run_text_mode(
                session,
                driver,
                options,
                stdin,
                std::io::stdin().is_terminal(),
                stdout,
                stderr,
            ))
        }
        PrintInputFormat::StreamJson => {
            let reader =
                Box::new(std::io::BufReader::new(std::io::stdin())) as Box<dyn BufRead + Send>;
            runtime.block_on(run_stream_json_mode(session, driver, reader, stdout))
        }
    }
}

async fn run_text_mode(
    mut session: BootstrappedSession,
    driver: Arc<dyn ModelDriver>,
    options: PrintOptions,
    mut stdin: Box<dyn Read + Send>,
    stdin_is_terminal: bool,
    stdout: SharedWriter,
    stderr: SharedWriter,
) -> std::result::Result<ExitCode, PrintModeError> {
    let request = collect_text_request(&options, &mut stdin, stdin_is_terminal)?;

    if options.output_format == PrintOutputFormat::StreamJson {
        emit_structured(
            &stdout,
            &StructuredOutputMessage::SessionStarted {
                session_id: session.runtime().session_id().as_str().to_owned(),
            },
        )?;
    }

    match execute_request(
        &mut session,
        driver.as_ref(),
        request,
        &PassthroughPermissionResolver,
        CancellationFlag::new(),
        (options.output_format == PrintOutputFormat::StreamJson).then_some(&stdout),
    )
    .await
    {
        Ok(result) => {
            match options.output_format {
                PrintOutputFormat::Text => {
                    if let Some(text) = text_result(&result) {
                        write_text(&stdout, &text)?;
                    }
                }
                PrintOutputFormat::Json => {
                    emit_structured(
                        &stdout,
                        &StructuredOutputMessage::Result {
                            result: Box::new(result),
                        },
                    )?;
                }
                PrintOutputFormat::StreamJson => {
                    emit_structured(
                        &stdout,
                        &StructuredOutputMessage::Result {
                            result: Box::new(result),
                        },
                    )?;
                }
            }
            Ok(ExitCode::SUCCESS)
        }
        Err(error) => {
            match options.output_format {
                PrintOutputFormat::Text => write_text(&stderr, &structured_error_message(&error))?,
                PrintOutputFormat::Json | PrintOutputFormat::StreamJson => {
                    emit_structured(
                        &stdout,
                        &StructuredOutputMessage::Error {
                            code: structured_error_code(&error).to_owned(),
                            message: structured_error_message(&error),
                        },
                    )?;
                }
            }
            Ok(ExitCode::from(1))
        }
    }
}

async fn run_stream_json_mode(
    mut session: BootstrappedSession,
    driver: Arc<dyn ModelDriver>,
    reader: Box<dyn BufRead + Send>,
    stdout: SharedWriter,
) -> std::result::Result<ExitCode, PrintModeError> {
    emit_structured(
        &stdout,
        &StructuredOutputMessage::SessionStarted {
            session_id: session.runtime().session_id().as_str().to_owned(),
        },
    )?;

    let resolver = StructuredPermissionResolver::new(Arc::clone(&stdout));
    let (tx, mut rx) = mpsc::unbounded_channel();
    let input_thread = std::thread::spawn(move || read_structured_input(reader, tx));
    let mut input_closed = false;

    loop {
        match rx.recv().await {
            Some(RunnerInput::Message(StructuredInputMessage::User { content })) => {
                let request = request_from_text(content);
                let cancellation = CancellationFlag::new();
                let submit = execute_request(
                    &mut session,
                    driver.as_ref(),
                    request,
                    &resolver,
                    cancellation.clone(),
                    Some(&stdout),
                );
                tokio::pin!(submit);

                loop {
                    tokio::select! {
                        biased;

                        outcome = &mut submit => {
                            match outcome {
                                Ok(result) => emit_structured(
                                    &stdout,
                                    &StructuredOutputMessage::Result {
                                        result: Box::new(result),
                                    },
                                )?,
                                Err(error) => emit_structured(
                                    &stdout,
                                    &StructuredOutputMessage::Error {
                                        code: structured_error_code(&error).to_owned(),
                                        message: structured_error_message(&error),
                                    },
                                )?,
                            }
                            break;
                        }
                        input = rx.recv() => {
                            match input {
                                Some(RunnerInput::Eof) => {
                                    input_closed = true;
                                    handle_active_input(
                                        RunnerInput::Eof,
                                        &stdout,
                                        &resolver,
                                        Some(&cancellation),
                                    )?;
                                }
                                Some(event) => handle_active_input(
                                    event,
                                    &stdout,
                                    &resolver,
                                    Some(&cancellation),
                                )?,
                                None => {
                                    input_closed = true;
                                    resolver.cancel_all("stdin_closed", false)?;
                                }
                            }
                        }
                    }
                }

                if input_closed {
                    break;
                }
            }
            Some(RunnerInput::Message(StructuredInputMessage::ControlRequest { .. })) => {
                emit_structured(
                    &stdout,
                    &StructuredOutputMessage::Error {
                        code: "no_active_turn".to_owned(),
                        message: "interrupt control request requires an active turn".to_owned(),
                    },
                )?;
            }
            Some(RunnerInput::Message(StructuredInputMessage::ControlResponse { .. })) => {
                emit_structured(
                    &stdout,
                    &StructuredOutputMessage::Error {
                        code: "unexpected_control_response".to_owned(),
                        message: "received a control response without an active turn".to_owned(),
                    },
                )?;
            }
            Some(RunnerInput::Message(StructuredInputMessage::KeepAlive)) => {
                emit_structured(&stdout, &StructuredOutputMessage::KeepAlive)?;
            }
            Some(RunnerInput::Invalid(message)) => emit_structured(
                &stdout,
                &StructuredOutputMessage::Error {
                    code: "invalid_input_message".to_owned(),
                    message,
                },
            )?,
            Some(RunnerInput::Eof) | None => break,
        }
    }

    let _ = input_thread.join();
    Ok(ExitCode::SUCCESS)
}

async fn execute_request(
    session: &mut BootstrappedSession,
    driver: &dyn ModelDriver,
    request: ConversationRequest,
    resolver: &dyn PermissionResolver,
    cancellation: CancellationFlag,
    stream_output: Option<&SharedWriter>,
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

                if let Some(writer) = stream_output {
                    if output_error.is_none() {
                        if let Err(error) = emit_structured(
                            writer,
                            &StructuredOutputMessage::StreamEvent {
                                event: event.clone(),
                            },
                        ) {
                            output_error = Some(error.to_string());
                        }
                    }
                }
            },
        )
        .await?;

    if let Some(message) = output_error {
        return Err(ClawinError::EngineProtocol {
            message: format!("failed to write structured output: {message}"),
        });
    }

    Ok(StructuredRunResult {
        outcome,
        command_output,
    })
}

fn collect_text_request(
    options: &PrintOptions,
    stdin: &mut dyn Read,
    stdin_is_terminal: bool,
) -> std::result::Result<ConversationRequest, PrintModeError> {
    let stdin_text = if stdin_is_terminal {
        None
    } else {
        let mut buffer = String::new();
        stdin
            .read_to_string(&mut buffer)
            .context("failed to read prompt text from stdin")?;
        normalize_text_input(buffer)
    };

    match (options.prompt.clone(), stdin_text) {
        (Some(_), Some(_)) => Err(PrintModeError::Cli(clap::Error::raw(
            ErrorKind::ArgumentConflict,
            "--print --input-format=text accepts either a prompt argument or piped stdin, not both",
        ))),
        (Some(prompt), None) => Ok(request_from_text(prompt)),
        (None, Some(stdin_text)) => Ok(request_from_text(stdin_text)),
        (None, None) => Err(PrintModeError::Cli(clap::Error::raw(
            ErrorKind::MissingRequiredArgument,
            "--print requires either a prompt argument or piped stdin text",
        ))),
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

fn normalize_text_input(value: String) -> Option<String> {
    let trimmed = value.trim_end_matches(['\r', '\n']).to_owned();
    (!trimmed.trim().is_empty()).then_some(trimmed)
}

fn read_structured_input(
    mut reader: Box<dyn BufRead + Send>,
    tx: mpsc::UnboundedSender<RunnerInput>,
) {
    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => {
                let _ = tx.send(RunnerInput::Eof);
                return;
            }
            Ok(_) => {
                if line.trim().is_empty() {
                    continue;
                }

                match serde_json::from_str::<StructuredInputMessage>(line.trim_end()) {
                    Ok(message) => {
                        let _ = tx.send(RunnerInput::Message(message));
                    }
                    Err(error) => {
                        let _ = tx.send(RunnerInput::Invalid(error.to_string()));
                    }
                }
            }
            Err(error) => {
                let _ = tx.send(RunnerInput::Invalid(error.to_string()));
                let _ = tx.send(RunnerInput::Eof);
                return;
            }
        }
    }
}

fn handle_active_input(
    event: RunnerInput,
    stdout: &SharedWriter,
    resolver: &StructuredPermissionResolver,
    cancellation: Option<&CancellationFlag>,
) -> std::result::Result<(), PrintModeError> {
    match event {
        RunnerInput::Message(StructuredInputMessage::User { .. }) => emit_structured(
            stdout,
            &StructuredOutputMessage::Error {
                code: "busy".to_owned(),
                message: "a headless turn is already running".to_owned(),
            },
        )?,
        RunnerInput::Message(StructuredInputMessage::ControlRequest {
            request: StructuredInputControlRequest::Interrupt,
        }) => {
            if let Some(flag) = cancellation {
                flag.cancel();
            }
            resolver.cancel_all("interrupt", true)?;
        }
        RunnerInput::Message(StructuredInputMessage::ControlResponse { response }) => {
            match resolver.apply_response(response)? {
                true => {}
                false => emit_structured(
                    stdout,
                    &StructuredOutputMessage::Error {
                        code: "unexpected_control_response".to_owned(),
                        message: "no pending permission request matched the provided request_id"
                            .to_owned(),
                    },
                )?,
            }
        }
        RunnerInput::Message(StructuredInputMessage::KeepAlive) => {
            emit_structured(stdout, &StructuredOutputMessage::KeepAlive)?;
        }
        RunnerInput::Invalid(message) => emit_structured(
            stdout,
            &StructuredOutputMessage::Error {
                code: "invalid_input_message".to_owned(),
                message,
            },
        )?,
        RunnerInput::Eof => resolver.cancel_all("stdin_closed", false)?,
    }

    Ok(())
}

fn emit_structured(writer: &SharedWriter, message: &StructuredOutputMessage) -> Result<()> {
    let mut guard = writer
        .lock()
        .map_err(|_| anyhow!("structured output writer lock should be available"))?;
    serde_json::to_writer(&mut *guard, message)?;
    guard.write_all(b"\n")?;
    guard.flush()?;
    Ok(())
}

fn write_text(writer: &SharedWriter, text: &str) -> Result<()> {
    let mut guard = writer
        .lock()
        .map_err(|_| anyhow!("text output writer lock should be available"))?;
    writeln!(&mut *guard, "{text}")?;
    guard.flush()?;
    Ok(())
}

fn shared_writer(writer: impl Write + Send + 'static) -> SharedWriter {
    Arc::new(Mutex::new(Box::new(writer)))
}

fn text_result(result: &StructuredRunResult) -> Option<String> {
    result
        .command_output
        .clone()
        .or_else(|| result.outcome.final_assistant_message.clone())
        .or_else(|| {
            (result.outcome.stop_reason == StopReason::Cancelled)
                .then_some("headless turn cancelled".to_owned())
        })
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

fn structured_error_message(error: &ClawinError) -> String {
    error.to_string()
}

fn headless_turn_config() -> TurnLoopConfig {
    TurnLoopConfig {
        max_turns: 4,
        token_budget: None,
        compaction_policy: clawin_core::CompactionPolicy::Disabled,
        allow_budget_continuation: false,
    }
}

struct StructuredPermissionResolver {
    output: SharedWriter,
    next_request_id: AtomicU64,
    pending: Mutex<BTreeMap<String, oneshot::Sender<PermissionDecision>>>,
}

impl StructuredPermissionResolver {
    fn new(output: SharedWriter) -> Self {
        Self {
            output,
            next_request_id: AtomicU64::new(1),
            pending: Mutex::new(BTreeMap::new()),
        }
    }

    fn apply_response(
        &self,
        response: StructuredControlResponse,
    ) -> std::result::Result<bool, PrintModeError> {
        let StructuredControlResponse::CanUseTool {
            request_id,
            behavior,
            message,
        } = response;

        if behavior == PermissionBehavior::Ask {
            return Err(PrintModeError::Runtime(anyhow!(
                "control_response can_use_tool must resolve to allow or deny"
            )));
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

    fn cancel_all(
        &self,
        reason: &str,
        emit_cancel_request: bool,
    ) -> std::result::Result<(), PrintModeError> {
        let mut pending = self
            .pending
            .lock()
            .map_err(|_| anyhow!("permission resolver state lock should be available"))?;
        let entries = std::mem::take(&mut *pending);
        drop(pending);

        for (request_id, sender) in entries {
            if emit_cancel_request {
                emit_structured(
                    &self.output,
                    &StructuredOutputMessage::ControlCancelRequest {
                        request_id: request_id.clone(),
                        reason: reason.to_owned(),
                    },
                )?;
            }
            let _ = sender.send(PermissionDecision::new(
                PermissionBehavior::Deny,
                Some(reason.to_owned()),
            ));
        }

        Ok(())
    }
}

impl PermissionResolver for StructuredPermissionResolver {
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

            if let Err(error) = emit_structured(
                &self.output,
                &StructuredOutputMessage::ControlRequest {
                    request: StructuredControlRequest::CanUseTool {
                        request_id: request_id.clone(),
                        call_id,
                        tool_name,
                        input,
                        message,
                    },
                },
            ) {
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

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::io::{Cursor, Write};
    use std::path::PathBuf;
    use std::sync::Arc;

    use clawin_core::{
        ModelDriverFuture, ModelFinishReason, ModelRequest, ModelStreamEvent,
        StructuredOutputMessage,
    };
    use clawin_platform::{PathPolicy, StaticTerminalCapabilities};
    use tempfile::TempDir;

    use super::*;
    use crate::run::bootstrap_session_from;

    #[tokio::test]
    async fn stream_json_mode_emits_session_started_events_and_result() {
        let harness = Harness::new();
        let session = bootstrap_session_from(
            harness.project_dir.clone(),
            StaticTerminalCapabilities::new(false, false),
            TestPathPolicy {
                home_dir: harness.home_dir.clone(),
            },
        )
        .expect("bootstrap session should assemble");
        let driver = Arc::new(ScriptedModelDriver::new(vec![Ok(vec![
            ModelStreamEvent::TextDelta {
                delta: "stub reply".to_owned(),
            },
            ModelStreamEvent::AssistantMessageFinished,
            ModelStreamEvent::ModelFinished {
                finish_reason: ModelFinishReason::Completed,
            },
        ])]));
        let stdout = BufferWriter::shared();
        let exit = run_stream_json_mode(
            session,
            driver,
            Box::new(Cursor::new(
                b"{\"type\":\"user\",\"content\":\"hello\"}\n".to_vec(),
            )),
            stdout.writer(),
        )
        .await
        .expect("stream-json mode should succeed");

        assert_eq!(exit, ExitCode::SUCCESS);
        let messages = stdout.messages();
        assert!(matches!(
            messages.first(),
            Some(StructuredOutputMessage::SessionStarted { .. })
        ));
        assert!(messages.iter().any(|message| matches!(
            message,
            StructuredOutputMessage::StreamEvent {
                event: EngineEvent::AssistantTextDelta { delta, .. }
            } if delta == "stub reply"
        )));
        assert!(matches!(
            messages.last(),
            Some(StructuredOutputMessage::Result { result })
                if result.outcome.final_assistant_message.as_deref() == Some("stub reply")
        ));
    }

    #[tokio::test]
    async fn stream_json_mode_reuses_session_for_multiple_user_messages() {
        let harness = Harness::new();
        let session = bootstrap_session_from(
            harness.project_dir.clone(),
            StaticTerminalCapabilities::new(false, false),
            TestPathPolicy {
                home_dir: harness.home_dir.clone(),
            },
        )
        .expect("bootstrap session should assemble");
        let driver = Arc::new(ScriptedModelDriver::new(vec![
            Ok(vec![
                ModelStreamEvent::TextDelta {
                    delta: "reply one".to_owned(),
                },
                ModelStreamEvent::AssistantMessageFinished,
                ModelStreamEvent::ModelFinished {
                    finish_reason: ModelFinishReason::Completed,
                },
            ]),
            Ok(vec![
                ModelStreamEvent::TextDelta {
                    delta: "reply two".to_owned(),
                },
                ModelStreamEvent::AssistantMessageFinished,
                ModelStreamEvent::ModelFinished {
                    finish_reason: ModelFinishReason::Completed,
                },
            ]),
        ]));
        let stdout = BufferWriter::shared();
        let exit = run_stream_json_mode(
            session,
            Arc::clone(&driver) as Arc<dyn ModelDriver>,
            Box::new(Cursor::new(
                b"{\"type\":\"user\",\"content\":\"hello\"}\n{\"type\":\"user\",\"content\":\"again\"}\n"
                    .to_vec(),
            )),
            stdout.writer(),
        )
        .await
        .expect("stream-json mode should succeed");

        assert_eq!(exit, ExitCode::SUCCESS);
        let requests = driver.requests();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].transcript.len(), 1);
        assert!(requests[1].transcript.iter().any(|message| matches!(
            message,
            clawin_core::ConversationMessage::Assistant { content } if content == "reply one"
        )));
    }

    #[tokio::test]
    async fn structured_permission_resolver_accepts_host_response() {
        let stdout = BufferWriter::shared();
        let resolver = StructuredPermissionResolver::new(stdout.writer());
        let call = clawin_core::ToolCall::new(
            "toolu_1",
            "file_read",
            serde_json::json!({ "file_path": "../secret.txt" }),
        );
        let mut pending = Box::pin(resolver.resolve(
            &call,
            PermissionDecision::new(
                PermissionBehavior::Ask,
                Some("requested path is outside the project root".to_owned()),
            ),
        ));

        tokio::select! {
            _ = &mut pending => panic!("permission future should wait for host response"),
            _ = tokio::time::sleep(std::time::Duration::from_millis(10)) => {}
        }

        let request_id = match stdout.messages().last() {
            Some(StructuredOutputMessage::ControlRequest {
                request: StructuredControlRequest::CanUseTool { request_id, .. },
            }) => request_id.clone(),
            other => panic!("unexpected control request output: {other:?}"),
        };

        assert!(
            resolver
                .apply_response(StructuredControlResponse::CanUseTool {
                    request_id,
                    behavior: PermissionBehavior::Allow,
                    message: None,
                })
                .expect("response should apply")
        );

        let decision = pending.await.expect("pending future should resolve");
        assert_eq!(decision.behavior, PermissionBehavior::Allow);
    }

    #[tokio::test]
    async fn structured_permission_resolver_emits_cancel_request_on_interrupt() {
        let stdout = BufferWriter::shared();
        let resolver = StructuredPermissionResolver::new(stdout.writer());
        let call = clawin_core::ToolCall::new(
            "toolu_1",
            "file_read",
            serde_json::json!({ "file_path": "../secret.txt" }),
        );
        let mut pending = Box::pin(resolver.resolve(
            &call,
            PermissionDecision::new(
                PermissionBehavior::Ask,
                Some("requested path is outside the project root".to_owned()),
            ),
        ));

        tokio::select! {
            _ = &mut pending => panic!("permission future should wait for host response"),
            _ = tokio::time::sleep(std::time::Duration::from_millis(10)) => {}
        }

        resolver
            .cancel_all("interrupt", true)
            .expect("interrupt should cancel the pending request");

        let decision = pending.await.expect("pending future should resolve");
        assert_eq!(decision.behavior, PermissionBehavior::Deny);
        assert_eq!(decision.message.as_deref(), Some("interrupt"));
        assert!(stdout.messages().iter().any(|message| matches!(
            message,
            StructuredOutputMessage::ControlCancelRequest { reason, .. } if reason == "interrupt"
        )));
    }

    struct BufferWriter {
        inner: Arc<Mutex<Vec<u8>>>,
    }

    impl BufferWriter {
        fn shared() -> Self {
            Self {
                inner: Arc::new(Mutex::new(Vec::new())),
            }
        }

        fn writer(&self) -> SharedWriter {
            shared_writer(Self {
                inner: Arc::clone(&self.inner),
            })
        }

        fn messages(&self) -> Vec<StructuredOutputMessage> {
            let bytes = self.inner.lock().expect("buffer lock should be available");
            String::from_utf8(bytes.clone())
                .expect("stdout should be utf-8")
                .lines()
                .map(|line| serde_json::from_str(line).expect("line should be valid json"))
                .collect()
        }
    }

    impl Write for BufferWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.inner
                .lock()
                .expect("buffer lock should be available")
                .extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    struct Harness {
        _tempdir: TempDir,
        home_dir: PathBuf,
        project_dir: PathBuf,
    }

    impl Harness {
        fn new() -> Self {
            let tempdir = tempfile::tempdir().expect("tempdir should exist");
            let home_dir = tempdir.path().join("home");
            let project_dir = tempdir.path().join("workspace").join("app");

            std::fs::create_dir_all(&home_dir).expect("home dir should exist");
            std::fs::create_dir_all(&project_dir).expect("project dir should exist");

            Self {
                _tempdir: tempdir,
                home_dir,
                project_dir,
            }
        }
    }

    #[derive(Clone, Debug)]
    struct TestPathPolicy {
        home_dir: PathBuf,
    }

    impl PathPolicy for TestPathPolicy {
        fn home_dir(&self) -> Option<PathBuf> {
            Some(self.home_dir.clone())
        }

        fn normalize_for_config_key(&self, path: &std::path::Path) -> String {
            path.to_string_lossy().replace('\\', "/")
        }

        fn project_directory_name(&self) -> &'static str {
            ".clawin"
        }

        fn project_manifest_name(&self) -> &'static str {
            "CLAWIN.md"
        }
    }

    struct ScriptedModelDriver {
        responses: Mutex<VecDeque<Result<Vec<ModelStreamEvent>, ClawinError>>>,
        requests: Mutex<Vec<ModelRequest>>,
    }

    impl ScriptedModelDriver {
        fn new(responses: Vec<Result<Vec<ModelStreamEvent>, ClawinError>>) -> Self {
            Self {
                responses: Mutex::new(VecDeque::from(responses)),
                requests: Mutex::new(Vec::new()),
            }
        }

        fn requests(&self) -> Vec<ModelRequest> {
            self.requests
                .lock()
                .expect("requests lock should be available")
                .clone()
        }
    }

    impl ModelDriver for ScriptedModelDriver {
        fn stream(&self, request: ModelRequest) -> ModelDriverFuture<'_> {
            self.requests
                .lock()
                .expect("requests lock should be available")
                .push(request);
            let response = self
                .responses
                .lock()
                .expect("responses lock should be available")
                .pop_front()
                .unwrap_or_else(|| {
                    Err(ClawinError::ModelDriver {
                        message: "unexpected model request".to_owned(),
                    })
                });

            Box::pin(async move { response })
        }
    }
}
