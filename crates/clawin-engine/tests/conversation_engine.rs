// Phase 4 tests continue under DIFF-2026-001: Clawin keeps its own namespace while rebuilding the conversation engine.

use std::collections::VecDeque;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use clawin_commands::builtin_command_registry;
use clawin_config::load_startup_config;
use clawin_core::{
    CancellationFlag, ClawinError, ClawinResult, CommandEffect, CompactionPolicy,
    ConversationMessage, ConversationRequest, EngineEvent, ModelDriver, ModelDriverFuture,
    ModelFinishReason, ModelRequest, ModelStreamEvent, PassthroughPermissionResolver,
    PermissionMode, PersistedWorktreeSession, RestoredSession, ResumeInterruptionState,
    ResumeQuery, RuntimeCapabilities, SessionId, SessionPreview, SessionRuntime, SessionStore,
    StopReason, ToolCall, TurnLoopConfig, WorktreeExitAction, WorktreeManager,
};
use clawin_engine::{ConversationEngine, EngineServices};
use clawin_integrations::McpManager;
use clawin_platform::{FakeProcessPlan, FakeProcessSpawner, PathPolicy};
use clawin_tools::{builtin_tool_registry, builtin_tool_registry_with_mcp};
use serde_json::{Value, json};
use tempfile::TempDir;

#[tokio::test]
async fn routes_help_slash_command_without_touching_model_driver() {
    let harness = EngineHarness::new();
    let driver = ScriptedModelDriver::new(vec![]);
    let mut engine = ConversationEngine::new(harness.runtime.session_id().clone());
    let mut events = Vec::new();

    let outcome = engine
        .submit_message(
            &harness.services(&driver, CancellationFlag::new()),
            ConversationRequest::SlashCommand("/help".to_owned()),
            loop_config(),
            |event| events.push(event),
        )
        .await
        .expect("slash command should execute");

    assert_eq!(driver.call_count(), 0);
    assert_eq!(outcome.stop_reason, StopReason::CommandHandled);
    assert_eq!(outcome.final_assistant_message, None);
    assert_eq!(outcome.turn_count, 1);
    assert!(engine.transcript().is_empty());
    assert_eq!(
        events,
        vec![
            EngineEvent::SessionStarted {
                session_id: "engine-test".to_owned(),
            },
            EngineEvent::TurnStarted { turn_id: 1 },
            EngineEvent::CommandParsed {
                raw_name: "help".to_owned(),
                command_name: "help".to_owned(),
            },
            EngineEvent::CommandExecuted {
                command_name: "help".to_owned(),
                output: "Available commands:\n/help - Show help and available commands\n"
                    .to_owned(),
            },
            EngineEvent::TurnFinished {
                turn_id: 1,
                stop_reason: StopReason::CommandHandled,
            },
            EngineEvent::SessionFinished {
                session_id: "engine-test".to_owned(),
                stop_reason: StopReason::CommandHandled,
            },
        ]
    );
}

#[tokio::test]
async fn slash_resume_returns_command_effect_and_restored_engine_reuses_transcript() {
    let harness = EngineHarness::new();
    let restored = RestoredSession {
        session_id: SessionId::from_owned("restored-session"),
        transcript_path: harness.project_root.join("restored-session.jsonl"),
        canonical_project_root: harness.project_root.clone(),
        active_project_root: harness.project_root.clone(),
        transcript: vec![
            ConversationMessage::User {
                content: "hello".to_owned(),
            },
            ConversationMessage::Assistant {
                content: "world".to_owned(),
            },
        ],
        last_prompt: Some("hello".to_owned()),
        worktree_state: None,
        interruption_state: ResumeInterruptionState::InterruptedPrompt,
    };
    let runtime = harness
        .runtime
        .clone()
        .with_session_store(Arc::new(FakeSessionStore::with_restored(restored.clone())))
        .with_worktree_manager(Arc::new(NoopWorktreeManager));
    let driver = ScriptedModelDriver::new(vec![]);
    let commands = builtin_command_registry();
    let mut engine = ConversationEngine::new(runtime.session_id().clone());

    let outcome = engine
        .submit_message(
            &EngineServices::new(
                &runtime,
                &commands,
                &harness.tools,
                &driver,
                &PassthroughPermissionResolver,
                CancellationFlag::new(),
            ),
            ConversationRequest::SlashCommand("/continue restored-session".to_owned()),
            loop_config(),
            |_| {},
        )
        .await
        .expect("resume slash command should succeed");

    match outcome.command_effect {
        Some(CommandEffect::ResumeSession { session }) => assert_eq!(session, restored),
        other => panic!("unexpected command effect: {other:?}"),
    }

    let restored_engine = ConversationEngine::restore(
        SessionId::from_owned("restored-session"),
        vec![
            ConversationMessage::User {
                content: "hello".to_owned(),
            },
            ConversationMessage::Assistant {
                content: "world".to_owned(),
            },
        ],
    );
    assert_eq!(restored_engine.session_id().as_str(), "restored-session");
    assert_eq!(
        restored_engine.transcript(),
        &[
            ConversationMessage::User {
                content: "hello".to_owned(),
            },
            ConversationMessage::Assistant {
                content: "world".to_owned(),
            },
        ]
    );
}

#[tokio::test]
async fn streams_text_prompt_and_persists_transcript_across_submits() {
    let harness = EngineHarness::new();
    let driver = ScriptedModelDriver::new(vec![
        Ok(vec![
            ModelStreamEvent::TextDelta {
                delta: "Alpha".to_owned(),
            },
            ModelStreamEvent::AssistantMessageFinished,
            ModelStreamEvent::UsageUpdated { total_tokens: 120 },
            ModelStreamEvent::ModelFinished {
                finish_reason: ModelFinishReason::Completed,
            },
        ]),
        Ok(vec![
            ModelStreamEvent::TextDelta {
                delta: "Beta".to_owned(),
            },
            ModelStreamEvent::AssistantMessageFinished,
            ModelStreamEvent::UsageUpdated { total_tokens: 180 },
            ModelStreamEvent::ModelFinished {
                finish_reason: ModelFinishReason::Completed,
            },
        ]),
    ]);
    let mut engine = ConversationEngine::new(harness.runtime.session_id().clone());
    let mut first_events = Vec::new();

    let first = engine
        .submit_message(
            &harness.services(&driver, CancellationFlag::new()),
            ConversationRequest::Prompt("hello".to_owned()),
            loop_config(),
            |event| first_events.push(event),
        )
        .await
        .expect("first prompt should succeed");

    assert_eq!(first.stop_reason, StopReason::Completed);
    assert_eq!(first.final_assistant_message.as_deref(), Some("Alpha"));
    assert_eq!(
        engine.transcript(),
        &[
            ConversationMessage::User {
                content: "hello".to_owned(),
            },
            ConversationMessage::Assistant {
                content: "Alpha".to_owned(),
            },
        ]
    );
    assert_json_fixture("tests/fixtures/text_prompt_events.json", &first_events);

    let second = engine
        .submit_message(
            &harness.services(&driver, CancellationFlag::new()),
            ConversationRequest::Prompt("next".to_owned()),
            loop_config(),
            |_| {},
        )
        .await
        .expect("second prompt should succeed");

    assert_eq!(second.stop_reason, StopReason::Completed);
    assert_eq!(second.final_assistant_message.as_deref(), Some("Beta"));
    assert_eq!(
        engine.transcript(),
        &[
            ConversationMessage::User {
                content: "hello".to_owned(),
            },
            ConversationMessage::Assistant {
                content: "Alpha".to_owned(),
            },
            ConversationMessage::User {
                content: "next".to_owned(),
            },
            ConversationMessage::Assistant {
                content: "Beta".to_owned(),
            },
        ]
    );

    let requests = driver.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(
        requests[1].transcript,
        vec![
            ConversationMessage::User {
                content: "hello".to_owned(),
            },
            ConversationMessage::Assistant {
                content: "Alpha".to_owned(),
            },
            ConversationMessage::User {
                content: "next".to_owned(),
            },
        ]
    );
}

#[tokio::test]
async fn runs_file_read_through_model_tool_model_loop() {
    let harness = EngineHarness::new();
    fs::write(harness.project_file("notes.txt"), "alpha\nbeta\n").expect("file should exist");

    let driver = ScriptedModelDriver::new(vec![
        Ok(vec![
            ModelStreamEvent::TextDelta {
                delta: "Reading notes.txt".to_owned(),
            },
            ModelStreamEvent::ToolCallRequested {
                call: ToolCall::new(
                    "toolu_123",
                    "file_read",
                    json!({
                        "file_path": "notes.txt"
                    }),
                ),
            },
            ModelStreamEvent::AssistantMessageFinished,
            ModelStreamEvent::UsageUpdated { total_tokens: 140 },
            ModelStreamEvent::ModelFinished {
                finish_reason: ModelFinishReason::ToolUse,
            },
        ]),
        Ok(vec![
            ModelStreamEvent::TextDelta {
                delta: "notes.txt contains alpha\\nbeta".to_owned(),
            },
            ModelStreamEvent::AssistantMessageFinished,
            ModelStreamEvent::UsageUpdated { total_tokens: 240 },
            ModelStreamEvent::ModelFinished {
                finish_reason: ModelFinishReason::Completed,
            },
        ]),
    ]);
    let mut engine = ConversationEngine::new(harness.runtime.session_id().clone());
    let mut events = Vec::new();

    let outcome = engine
        .submit_message(
            &harness.services(&driver, CancellationFlag::new()),
            ConversationRequest::Prompt("read notes".to_owned()),
            loop_config(),
            |event| events.push(event),
        )
        .await
        .expect("prompt should succeed");

    assert_eq!(outcome.stop_reason, StopReason::Completed);
    assert_eq!(
        outcome.final_assistant_message.as_deref(),
        Some("notes.txt contains alpha\\nbeta")
    );
    assert_json_fixture("tests/fixtures/tool_prompt_events.json", &events);

    assert_eq!(
        engine.transcript(),
        &[
            ConversationMessage::User {
                content: "read notes".to_owned(),
            },
            ConversationMessage::Assistant {
                content: "Reading notes.txt".to_owned(),
            },
            ConversationMessage::ToolUse {
                call_id: "toolu_123".to_owned(),
                tool_name: "file_read".to_owned(),
                input: json!({
                    "file_path": "notes.txt"
                }),
            },
            ConversationMessage::ToolResult {
                call_id: "toolu_123".to_owned(),
                tool_name: "file_read".to_owned(),
                is_error: false,
                content: json!({
                    "type": "text",
                    "content": "alpha\nbeta",
                    "start_line": 1,
                    "end_line": 2,
                }),
            },
            ConversationMessage::Assistant {
                content: "notes.txt contains alpha\\nbeta".to_owned(),
            },
        ]
    );
}

#[tokio::test]
async fn runs_mcp_dynamic_tool_through_model_tool_model_loop() {
    let harness = EngineHarness::new();
    harness.write_global_settings(json!({
        "schema_version": 1,
        "mcpServers": {
            "demo": {
                "command": "/tmp/fake-mcp"
            }
        }
    }));
    let snapshot = load_startup_config(
        harness.project_root.clone(),
        &TestPathPolicy {
            home_dir: harness.home_dir.clone(),
        },
    )
    .expect("startup config should load");
    let manager = Arc::new(
        McpManager::from_loaded_config(
            &snapshot,
            Arc::new(FakeProcessSpawner::new(vec![
                FakeProcessPlan::from_json_messages(vec![
                    initialize_response_with_capabilities(
                        1,
                        json!({
                            "tools": {},
                            "resources": {}
                        }),
                    ),
                    tools_list_response(
                        2,
                        vec![json!({
                            "name": "echo",
                            "description": "Echo text",
                            "inputSchema": {
                                "type": "object",
                                "properties": {
                                    "text": { "type": "string" }
                                },
                                "required": ["text"]
                            }
                        })],
                    ),
                    resources_list_response(3, vec![]),
                    tool_call_response(
                        4,
                        json!({
                            "content": [
                                {
                                    "type": "text",
                                    "text": "echo from MCP"
                                }
                            ]
                        }),
                    ),
                ]),
            ])),
        )
        .expect("manager should assemble"),
    );
    let driver = ScriptedModelDriver::new(vec![
        Ok(vec![
            ModelStreamEvent::ToolCallRequested {
                call: ToolCall::new(
                    "toolu_mcp",
                    "mcp__demo__echo",
                    json!({
                        "text": "hello"
                    }),
                ),
            },
            ModelStreamEvent::ModelFinished {
                finish_reason: ModelFinishReason::ToolUse,
            },
        ]),
        Ok(vec![
            ModelStreamEvent::TextDelta {
                delta: "MCP tool finished".to_owned(),
            },
            ModelStreamEvent::AssistantMessageFinished,
            ModelStreamEvent::ModelFinished {
                finish_reason: ModelFinishReason::Completed,
            },
        ]),
    ]);
    let mut engine = ConversationEngine::new(harness.runtime.session_id().clone());
    let commands = builtin_command_registry();
    let tools = builtin_tool_registry_with_mcp(manager);
    let mut events = Vec::new();

    let outcome = engine
        .submit_message(
            &EngineServices::new(
                &harness.runtime,
                &commands,
                &tools,
                &driver,
                &PassthroughPermissionResolver,
                CancellationFlag::new(),
            ),
            ConversationRequest::Prompt("use mcp".to_owned()),
            loop_config(),
            |event| events.push(event),
        )
        .await
        .expect("mcp prompt should succeed");

    assert_eq!(outcome.stop_reason, StopReason::Completed);
    assert!(events.iter().any(|event| matches!(
        event,
        EngineEvent::ToolRequested { tool_name, .. } if tool_name == "mcp__demo__echo"
    )));
    assert!(engine.transcript().iter().any(|message| matches!(
        message,
        ConversationMessage::ToolResult { tool_name, content, .. }
            if tool_name == "mcp__demo__echo"
                && content == &json!({
                    "content": [
                        {
                            "type": "text",
                            "text": "echo from MCP"
                        }
                    ]
                })
    )));
}

#[tokio::test]
async fn applies_deterministic_compaction_when_history_crosses_threshold() {
    let harness = EngineHarness::new();
    let driver = ScriptedModelDriver::new(vec![
        Ok(simple_text_response("one", 100)),
        Ok(simple_text_response("two", 200)),
        Ok(simple_text_response("three", 300)),
    ]);
    let mut engine = ConversationEngine::new(harness.runtime.session_id().clone());
    let config = TurnLoopConfig {
        max_turns: 4,
        token_budget: None,
        compaction_policy: CompactionPolicy::MessageCount {
            trigger_message_count: 5,
            keep_recent_messages: 4,
        },
        allow_budget_continuation: false,
    };

    engine
        .submit_message(
            &harness.services(&driver, CancellationFlag::new()),
            ConversationRequest::Prompt("first".to_owned()),
            config.clone(),
            |_| {},
        )
        .await
        .expect("first prompt should succeed");
    engine
        .submit_message(
            &harness.services(&driver, CancellationFlag::new()),
            ConversationRequest::Prompt("second".to_owned()),
            config.clone(),
            |_| {},
        )
        .await
        .expect("second prompt should succeed");

    let mut events = Vec::new();
    let outcome = engine
        .submit_message(
            &harness.services(&driver, CancellationFlag::new()),
            ConversationRequest::Prompt("third".to_owned()),
            config,
            |event| events.push(event),
        )
        .await
        .expect("third prompt should succeed");

    assert_eq!(outcome.stop_reason, StopReason::Completed);
    assert!(events.iter().any(|event| matches!(
        event,
        EngineEvent::CompactionApplied {
            replaced_message_count: 2,
            ..
        }
    )));
    assert_json_fixture(
        "tests/fixtures/compacted_transcript.json",
        engine.transcript(),
    );
}

#[tokio::test]
async fn continues_once_for_budget_then_stops() {
    let harness = EngineHarness::new();
    let driver = ScriptedModelDriver::new(vec![
        Ok(simple_text_response("draft part 1", 400)),
        Ok(simple_text_response("draft part 2", 950)),
    ]);
    let mut engine = ConversationEngine::new(harness.runtime.session_id().clone());
    let mut events = Vec::new();

    let outcome = engine
        .submit_message(
            &harness.services(&driver, CancellationFlag::new()),
            ConversationRequest::Prompt("write summary".to_owned()),
            TurnLoopConfig {
                max_turns: 3,
                token_budget: Some(1_000),
                compaction_policy: CompactionPolicy::Disabled,
                allow_budget_continuation: true,
            },
            |event| events.push(event),
        )
        .await
        .expect("budgeted prompt should succeed");

    assert_eq!(outcome.stop_reason, StopReason::BudgetStopped);
    assert_eq!(driver.call_count(), 2);
    assert!(events.iter().any(|event| matches!(
        event,
        EngineEvent::BudgetContinuationSuggested {
            turn_id: 1,
            continuation_count: 1,
            budget_tokens: 1_000,
            consumed_tokens: 400,
        }
    )));

    let requests = driver.requests();
    assert!(matches!(
        requests[1].transcript.last(),
        Some(ConversationMessage::System { content }) if content.contains("continue within the remaining token budget")
    ));
}

#[tokio::test]
async fn returns_cancelled_without_invoking_model_when_flag_is_set() {
    let harness = EngineHarness::new();
    let driver = ScriptedModelDriver::new(vec![]);
    let cancellation = CancellationFlag::new();
    cancellation.cancel();
    let mut engine = ConversationEngine::new(harness.runtime.session_id().clone());
    let mut events = Vec::new();

    let outcome = engine
        .submit_message(
            &harness.services(&driver, cancellation),
            ConversationRequest::Prompt("stop".to_owned()),
            loop_config(),
            |event| events.push(event),
        )
        .await
        .expect("cancelled prompt should return a controlled outcome");

    assert_eq!(outcome.stop_reason, StopReason::Cancelled);
    assert_eq!(driver.call_count(), 0);
    assert_eq!(
        events,
        vec![
            EngineEvent::SessionStarted {
                session_id: "engine-test".to_owned(),
            },
            EngineEvent::TurnStarted { turn_id: 1 },
            EngineEvent::TurnFinished {
                turn_id: 1,
                stop_reason: StopReason::Cancelled,
            },
            EngineEvent::SessionFinished {
                session_id: "engine-test".to_owned(),
                stop_reason: StopReason::Cancelled,
            },
        ]
    );
}

#[tokio::test]
async fn surfaces_model_driver_failures_and_emits_failure_events() {
    let harness = EngineHarness::new();
    let driver = ScriptedModelDriver::new(vec![Err(ClawinError::ModelDriver {
        message: "stream broke".to_owned(),
    })]);
    let mut engine = ConversationEngine::new(harness.runtime.session_id().clone());
    let mut events = Vec::new();

    let error = engine
        .submit_message(
            &harness.services(&driver, CancellationFlag::new()),
            ConversationRequest::Prompt("fail".to_owned()),
            loop_config(),
            |event| events.push(event),
        )
        .await
        .expect_err("model failures should surface");

    assert!(matches!(
        error,
        ClawinError::ModelDriver { ref message } if message == "stream broke"
    ));
    assert!(matches!(
        events.as_slice(),
        [
            EngineEvent::SessionStarted { .. },
            EngineEvent::TurnStarted { turn_id: 1 },
            EngineEvent::EngineFailed { turn_id: Some(1), message },
            EngineEvent::TurnFinished { turn_id: 1, stop_reason: StopReason::Failed },
            EngineEvent::SessionFinished { stop_reason: StopReason::Failed, .. },
        ] if message == "stream broke"
    ));
}

fn simple_text_response(text: &str, total_tokens: u64) -> Vec<ModelStreamEvent> {
    vec![
        ModelStreamEvent::TextDelta {
            delta: text.to_owned(),
        },
        ModelStreamEvent::AssistantMessageFinished,
        ModelStreamEvent::UsageUpdated { total_tokens },
        ModelStreamEvent::ModelFinished {
            finish_reason: ModelFinishReason::Completed,
        },
    ]
}

fn loop_config() -> TurnLoopConfig {
    TurnLoopConfig {
        max_turns: 4,
        token_budget: None,
        compaction_policy: CompactionPolicy::Disabled,
        allow_budget_continuation: false,
    }
}

struct EngineHarness {
    _tempdir: TempDir,
    runtime: SessionRuntime,
    home_dir: PathBuf,
    project_root: PathBuf,
    commands: clawin_commands::CommandRegistry,
    tools: clawin_tools::ToolRegistry,
}

impl EngineHarness {
    fn new() -> Self {
        let tempdir = tempfile::tempdir().expect("tempdir should exist");
        let home_dir = tempdir.path().join("home");
        let project_root = tempdir.path().join("project");
        fs::create_dir_all(home_dir.join(".clawin")).expect("home dir should exist");
        fs::create_dir_all(&project_root).expect("project root should exist");

        let runtime = SessionRuntime::new(
            SessionId::from_static("engine-test"),
            RuntimeCapabilities::new(false, false),
            project_root.clone(),
            project_root.clone(),
            PermissionMode::Default,
        );

        Self {
            _tempdir: tempdir,
            runtime,
            home_dir,
            project_root,
            commands: builtin_command_registry(),
            tools: builtin_tool_registry(),
        }
    }

    fn project_file(&self, name: &str) -> PathBuf {
        self.project_root.join(name)
    }

    fn write_global_settings(&self, value: Value) {
        fs::write(
            self.home_dir.join(".clawin/settings.json"),
            serde_json::to_vec_pretty(&value).expect("settings should serialize"),
        )
        .expect("global settings should write");
    }

    fn services<'a>(
        &'a self,
        driver: &'a dyn ModelDriver,
        cancellation: CancellationFlag,
    ) -> EngineServices<'a> {
        EngineServices::new(
            &self.runtime,
            &self.commands,
            &self.tools,
            driver,
            &PassthroughPermissionResolver,
            cancellation,
        )
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

fn initialize_response_with_capabilities(id: u64, capabilities: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "protocolVersion": "2025-03-26",
            "capabilities": capabilities,
            "serverInfo": {
                "name": "fake-mcp",
                "version": "0.1.0"
            }
        }
    })
}

fn tools_list_response(id: u64, tools: Vec<Value>) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "tools": tools
        }
    })
}

fn resources_list_response(id: u64, resources: Vec<Value>) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "resources": resources
        }
    })
}

fn tool_call_response(id: u64, result: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result
    })
}

struct ScriptedModelDriver {
    responses: Mutex<VecDeque<Result<Vec<ModelStreamEvent>, ClawinError>>>,
    requests: Mutex<Vec<ModelRequest>>,
    call_count: AtomicUsize,
}

impl ScriptedModelDriver {
    fn new(responses: Vec<Result<Vec<ModelStreamEvent>, ClawinError>>) -> Self {
        Self {
            responses: Mutex::new(VecDeque::from(responses)),
            requests: Mutex::new(Vec::new()),
            call_count: AtomicUsize::new(0),
        }
    }

    fn call_count(&self) -> usize {
        self.call_count.load(Ordering::SeqCst)
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
        self.call_count.fetch_add(1, Ordering::SeqCst);
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

#[derive(Debug)]
struct FakeSessionStore {
    restored: Option<RestoredSession>,
}

impl FakeSessionStore {
    fn with_restored(restored: RestoredSession) -> Self {
        Self {
            restored: Some(restored),
        }
    }
}

impl SessionStore for FakeSessionStore {
    fn initialize_session(&self, _runtime: &SessionRuntime) -> ClawinResult<()> {
        Ok(())
    }

    fn save_last_prompt(&self, _runtime: &SessionRuntime, _prompt: &str) -> ClawinResult<()> {
        Ok(())
    }

    fn append_message(
        &self,
        _runtime: &SessionRuntime,
        _message: &ConversationMessage,
    ) -> ClawinResult<()> {
        Ok(())
    }

    fn save_worktree_state(
        &self,
        _runtime: &SessionRuntime,
        _worktree: Option<&PersistedWorktreeSession>,
    ) -> ClawinResult<()> {
        Ok(())
    }

    fn list_recent_sessions(&self, _runtime: &SessionRuntime) -> ClawinResult<Vec<SessionPreview>> {
        Ok(Vec::new())
    }

    fn resolve_resume(
        &self,
        _runtime: &SessionRuntime,
        _query: ResumeQuery,
    ) -> ClawinResult<Option<RestoredSession>> {
        Ok(self.restored.clone())
    }
}

#[derive(Debug)]
struct NoopWorktreeManager;

impl WorktreeManager for NoopWorktreeManager {
    fn enter_worktree(
        &self,
        _runtime: &SessionRuntime,
        _name: Option<&str>,
    ) -> ClawinResult<PersistedWorktreeSession> {
        unreachable!("resume engine test should not enter a worktree")
    }

    fn exit_worktree(
        &self,
        _runtime: &SessionRuntime,
        _action: WorktreeExitAction,
        _discard_changes: bool,
    ) -> ClawinResult<Option<PersistedWorktreeSession>> {
        unreachable!("resume engine test should not exit a worktree")
    }
}

fn assert_json_fixture(path: &str, actual: &(impl serde::Serialize + ?Sized)) {
    let fixture_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(path);
    let contents = fs::read_to_string(fixture_path).expect("fixture should exist");
    let expected: Value = serde_json::from_str(&contents).expect("fixture should be valid json");
    let actual = serde_json::to_value(actual).expect("value should serialize");
    assert_eq!(actual, expected);
}
