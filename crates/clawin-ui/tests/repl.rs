// Phase 5 tests continue under DIFF-2026-001: Clawin keeps its own namespace while rebuilding the interactive REPL.

use std::collections::VecDeque;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::{Mutex, MutexGuard};

use clawin_commands::builtin_command_registry;
use clawin_core::{
    BridgeController, BridgeMode, BridgePointer, BridgePointerSource, BridgeSessionHost,
    BridgeState, BridgeStatusSnapshot, CancellationFlag, ClawinError, ConversationMessage,
    ModelDriver, ModelDriverFuture, ModelFinishReason, ModelRequest, ModelStreamEvent,
    PermissionMode, RestoredSession, ResumeInterruptionState, ResumeQuery, RuntimeCapabilities,
    SessionId, SessionPreview, SessionRuntime, SessionStore, StructuredInputMessage,
    StructuredOutputMessage, ToolCall, WorktreeExitAction, WorktreeManager,
};
use clawin_engine::ConversationEngine;
use clawin_platform::{
    FakeTerminalSession, TerminalEvent, TerminalKeyCode, TerminalKeyEvent, TerminalKeyModifiers,
    TerminalSize,
};
use clawin_tools::builtin_tool_registry;
use clawin_ui::{ReplConfig, ReplController, render_repl_snapshot, run_repl_session};
use serde_json::json;
use tempfile::TempDir;

#[test]
fn renders_help_command_output_and_exits_cleanly() {
    let harness = UiHarness::new();
    let driver = Arc::new(ScriptedModelDriver::new(vec![]));
    let mut controller = harness.controller(driver);
    let mut terminal = FakeTerminalSession::new(
        TerminalSize::new(100, 30),
        vec![
            Some(TerminalEvent::Key(TerminalKeyEvent::from_char('/'))),
            Some(TerminalEvent::Key(TerminalKeyEvent::from_char('h'))),
            Some(TerminalEvent::Key(TerminalKeyEvent::from_char('e'))),
            Some(TerminalEvent::Key(TerminalKeyEvent::from_char('l'))),
            Some(TerminalEvent::Key(TerminalKeyEvent::from_char('p'))),
            Some(TerminalEvent::Key(TerminalKeyEvent::new(
                TerminalKeyCode::Enter,
                TerminalKeyModifiers::NONE,
            ))),
            None,
            None,
            Some(TerminalEvent::Key(TerminalKeyEvent::new(
                TerminalKeyCode::Char('c'),
                TerminalKeyModifiers::CONTROL,
            ))),
        ],
    );

    let exit = run_repl_session(&mut controller, &mut terminal, ReplConfig::default())
        .expect("repl session should exit cleanly");

    assert_eq!(exit.reason_label(), "user_exit");

    let snapshot = render_repl_snapshot(controller.view_state(), TerminalSize::new(80, 20));
    assert!(snapshot.contains("/help"));
    assert!(snapshot.contains("Available commands:"));
}

#[test]
fn streams_prompt_tool_progress_and_final_assistant_message() {
    let harness = UiHarness::new();
    fs::write(harness.project_root.join("notes.txt"), "alpha\nbeta\n")
        .expect("tool fixture should exist");
    let driver = Arc::new(ScriptedModelDriver::new(vec![
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
            ModelStreamEvent::ModelFinished {
                finish_reason: ModelFinishReason::ToolUse,
            },
        ]),
        Ok(vec![
            ModelStreamEvent::TextDelta {
                delta: "notes.txt contains alpha\\nbeta".to_owned(),
            },
            ModelStreamEvent::AssistantMessageFinished,
            ModelStreamEvent::ModelFinished {
                finish_reason: ModelFinishReason::Completed,
            },
        ]),
    ]));
    let mut controller = harness.controller(driver);
    let mut terminal = FakeTerminalSession::new(
        TerminalSize::new(100, 30),
        vec![
            Some(TerminalEvent::Key(TerminalKeyEvent::from_char('r'))),
            Some(TerminalEvent::Key(TerminalKeyEvent::from_char('e'))),
            Some(TerminalEvent::Key(TerminalKeyEvent::from_char('a'))),
            Some(TerminalEvent::Key(TerminalKeyEvent::from_char('d'))),
            Some(TerminalEvent::Key(TerminalKeyEvent::from_char(' '))),
            Some(TerminalEvent::Key(TerminalKeyEvent::from_char('n'))),
            Some(TerminalEvent::Key(TerminalKeyEvent::from_char('o'))),
            Some(TerminalEvent::Key(TerminalKeyEvent::from_char('t'))),
            Some(TerminalEvent::Key(TerminalKeyEvent::from_char('e'))),
            Some(TerminalEvent::Key(TerminalKeyEvent::from_char('s'))),
            Some(TerminalEvent::Key(TerminalKeyEvent::new(
                TerminalKeyCode::Enter,
                TerminalKeyModifiers::NONE,
            ))),
            None,
            None,
            None,
            Some(TerminalEvent::Key(TerminalKeyEvent::new(
                TerminalKeyCode::Char('c'),
                TerminalKeyModifiers::CONTROL,
            ))),
        ],
    );

    run_repl_session(&mut controller, &mut terminal, ReplConfig::default())
        .expect("repl session should exit cleanly");

    let snapshot = render_repl_snapshot(controller.view_state(), TerminalSize::new(80, 20));
    assert!(snapshot.contains("Reading notes.txt"));
    assert!(snapshot.contains("notes.txt contains alpha\\nbeta"));
    assert!(snapshot.contains("file_read"));
}

#[test]
fn ctrl_c_cancels_running_prompt_and_restores_idle_state() {
    let harness = UiHarness::new();
    let cancellation_slot = Arc::new(Mutex::new(None));
    let driver = Arc::new(BlockingModelDriver::new(cancellation_slot.clone()));
    let mut controller = harness.controller_with_cancellation(driver, cancellation_slot);
    let mut terminal = FakeTerminalSession::new(
        TerminalSize::new(100, 30),
        vec![
            Some(TerminalEvent::Key(TerminalKeyEvent::from_char('h'))),
            Some(TerminalEvent::Key(TerminalKeyEvent::from_char('i'))),
            Some(TerminalEvent::Key(TerminalKeyEvent::new(
                TerminalKeyCode::Enter,
                TerminalKeyModifiers::NONE,
            ))),
            None,
            Some(TerminalEvent::Key(TerminalKeyEvent::new(
                TerminalKeyCode::Char('c'),
                TerminalKeyModifiers::CONTROL,
            ))),
            None,
            None,
            Some(TerminalEvent::Key(TerminalKeyEvent::new(
                TerminalKeyCode::Char('c'),
                TerminalKeyModifiers::CONTROL,
            ))),
        ],
    );

    run_repl_session(&mut controller, &mut terminal, ReplConfig::default())
        .expect("repl session should exit cleanly");

    let snapshot = render_repl_snapshot(controller.view_state(), TerminalSize::new(80, 20));
    assert!(snapshot.contains("Cancelled current request."));
    assert!(!controller.is_busy());
}

#[test]
fn resume_command_hot_swaps_runtime_and_restored_transcript() {
    let harness = UiHarness::new();
    let driver = Arc::new(ScriptedModelDriver::new(vec![]));
    let runtime = SessionRuntime::new(
        SessionId::from_static("ui-original"),
        RuntimeCapabilities::new(true, false),
        harness.project_root.clone(),
        harness.project_root.clone(),
        PermissionMode::Default,
    )
    .with_session_store(Arc::new(FakeSessionStore::with_restored(RestoredSession {
        session_id: SessionId::from_owned("ui-restored"),
        transcript_path: harness.project_root.join("ui-restored.jsonl"),
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
    })))
    .with_worktree_manager(Arc::new(NoopWorktreeManager));
    let mut controller = ReplController::new(
        runtime,
        builtin_command_registry(),
        builtin_tool_registry(),
        ConversationEngine::new(SessionId::from_static("ui-original")),
        driver,
        Arc::new(Mutex::new(None)),
    );
    let mut terminal = FakeTerminalSession::new(
        TerminalSize::new(100, 30),
        vec![
            Some(TerminalEvent::Key(TerminalKeyEvent::from_char('/'))),
            Some(TerminalEvent::Key(TerminalKeyEvent::from_char('r'))),
            Some(TerminalEvent::Key(TerminalKeyEvent::from_char('e'))),
            Some(TerminalEvent::Key(TerminalKeyEvent::from_char('s'))),
            Some(TerminalEvent::Key(TerminalKeyEvent::from_char('u'))),
            Some(TerminalEvent::Key(TerminalKeyEvent::from_char('m'))),
            Some(TerminalEvent::Key(TerminalKeyEvent::from_char('e'))),
            Some(TerminalEvent::Key(TerminalKeyEvent::from_char(' '))),
            Some(TerminalEvent::Key(TerminalKeyEvent::from_char('u'))),
            Some(TerminalEvent::Key(TerminalKeyEvent::from_char('i'))),
            Some(TerminalEvent::Key(TerminalKeyEvent::from_char('-'))),
            Some(TerminalEvent::Key(TerminalKeyEvent::from_char('r'))),
            Some(TerminalEvent::Key(TerminalKeyEvent::from_char('e'))),
            Some(TerminalEvent::Key(TerminalKeyEvent::from_char('s'))),
            Some(TerminalEvent::Key(TerminalKeyEvent::from_char('t'))),
            Some(TerminalEvent::Key(TerminalKeyEvent::from_char('o'))),
            Some(TerminalEvent::Key(TerminalKeyEvent::from_char('r'))),
            Some(TerminalEvent::Key(TerminalKeyEvent::from_char('e'))),
            Some(TerminalEvent::Key(TerminalKeyEvent::from_char('d'))),
            Some(TerminalEvent::Key(TerminalKeyEvent::new(
                TerminalKeyCode::Enter,
                TerminalKeyModifiers::NONE,
            ))),
            None,
            None,
            Some(TerminalEvent::Key(TerminalKeyEvent::new(
                TerminalKeyCode::Char('c'),
                TerminalKeyModifiers::CONTROL,
            ))),
        ],
    );

    run_repl_session(&mut controller, &mut terminal, ReplConfig::default())
        .expect("repl session should exit cleanly");

    assert_eq!(controller.runtime().session_id().as_str(), "ui-restored");
    let snapshot = render_repl_snapshot(controller.view_state(), TerminalSize::new(80, 20));
    assert!(snapshot.contains("hello"));
    assert!(snapshot.contains("world"));
}

#[test]
fn remote_control_command_starts_bridge_controller_from_repl() {
    let harness = UiHarness::new();
    let bridge_controller = Arc::new(FakeBridgeController::default());
    let runtime = SessionRuntime::new(
        SessionId::from_static("ui-remote-control"),
        RuntimeCapabilities::new(true, false),
        harness.project_root.clone(),
        harness.project_root.clone(),
        PermissionMode::Default,
    )
    .with_bridge_controller(bridge_controller.clone());
    let mut controller = ReplController::new(
        runtime,
        builtin_command_registry(),
        builtin_tool_registry(),
        ConversationEngine::new(SessionId::from_static("ui-remote-control")),
        Arc::new(ScriptedModelDriver::new(vec![])),
        Arc::new(Mutex::new(None)),
    );
    let mut terminal = FakeTerminalSession::new(
        TerminalSize::new(100, 30),
        vec![
            Some(TerminalEvent::Key(TerminalKeyEvent::from_char('/'))),
            Some(TerminalEvent::Key(TerminalKeyEvent::from_char('r'))),
            Some(TerminalEvent::Key(TerminalKeyEvent::from_char('c'))),
            Some(TerminalEvent::Key(TerminalKeyEvent::from_char(' '))),
            Some(TerminalEvent::Key(TerminalKeyEvent::from_char('d'))),
            Some(TerminalEvent::Key(TerminalKeyEvent::from_char('e'))),
            Some(TerminalEvent::Key(TerminalKeyEvent::from_char('m'))),
            Some(TerminalEvent::Key(TerminalKeyEvent::from_char('o'))),
            Some(TerminalEvent::Key(TerminalKeyEvent::new(
                TerminalKeyCode::Enter,
                TerminalKeyModifiers::NONE,
            ))),
            None,
            None,
            Some(TerminalEvent::Key(TerminalKeyEvent::new(
                TerminalKeyCode::Char('c'),
                TerminalKeyModifiers::CONTROL,
            ))),
        ],
    );

    run_repl_session(&mut controller, &mut terminal, ReplConfig::default())
        .expect("repl session should exit cleanly");

    assert_eq!(bridge_controller.start_count(), 1);
    let snapshot = render_repl_snapshot(controller.view_state(), TerminalSize::new(80, 20));
    assert!(snapshot.contains("Remote control bridge connected."));
}

#[test]
fn remote_control_host_routes_remote_help_through_live_repl_session() {
    let harness = UiHarness::new();
    let bridge_controller = Arc::new(FakeBridgeController::default());
    let runtime = SessionRuntime::new(
        SessionId::from_static("ui-remote-host"),
        RuntimeCapabilities::new(true, false),
        harness.project_root.clone(),
        harness.project_root.clone(),
        PermissionMode::Default,
    )
    .with_bridge_controller(bridge_controller.clone());
    let mut controller = ReplController::new(
        runtime,
        builtin_command_registry(),
        builtin_tool_registry(),
        ConversationEngine::new(SessionId::from_static("ui-remote-host")),
        Arc::new(ScriptedModelDriver::new(vec![])),
        Arc::new(Mutex::new(None)),
    );
    let remote = {
        let bridge_controller = Arc::clone(&bridge_controller);
        std::thread::spawn(move || {
            let host = bridge_controller
                .wait_for_host(std::time::Duration::from_millis(500))
                .expect("bridge host should become available");
            host.send_input(StructuredInputMessage::User {
                content: "/help".to_owned(),
            })
            .expect("remote help input should send");

            let deadline = std::time::Instant::now() + std::time::Duration::from_millis(500);
            while std::time::Instant::now() < deadline {
                if let Some(StructuredOutputMessage::Result { result }) = host
                    .recv_output(std::time::Duration::from_millis(25))
                    .expect("bridge host output should be readable")
                {
                    if result
                        .command_output
                        .as_deref()
                        .is_some_and(|output| output.contains("Available commands:"))
                    {
                        return true;
                    }
                }
            }
            false
        })
    };
    let mut terminal = FakeTerminalSession::new(
        TerminalSize::new(100, 30),
        vec![
            Some(TerminalEvent::Key(TerminalKeyEvent::from_char('/'))),
            Some(TerminalEvent::Key(TerminalKeyEvent::from_char('r'))),
            Some(TerminalEvent::Key(TerminalKeyEvent::from_char('c'))),
            Some(TerminalEvent::Key(TerminalKeyEvent::new(
                TerminalKeyCode::Enter,
                TerminalKeyModifiers::NONE,
            ))),
            None,
            None,
            None,
            None,
            Some(TerminalEvent::Key(TerminalKeyEvent::new(
                TerminalKeyCode::Char('c'),
                TerminalKeyModifiers::CONTROL,
            ))),
        ],
    );

    run_repl_session(&mut controller, &mut terminal, ReplConfig::default())
        .expect("repl session should exit cleanly");

    assert!(
        remote.join().expect("remote bridge thread should join"),
        "remote bridge should receive /help output"
    );
    let snapshot = render_repl_snapshot(controller.view_state(), TerminalSize::new(80, 20));
    assert!(snapshot.contains("/help"));
    assert!(snapshot.contains("Available commands:"));
}

struct UiHarness {
    _tempdir: TempDir,
    project_root: PathBuf,
}

impl UiHarness {
    fn new() -> Self {
        let tempdir = tempfile::tempdir().expect("tempdir should exist");
        let project_root = tempdir.path().join("project");
        fs::create_dir_all(&project_root).expect("project root should exist");

        Self {
            _tempdir: tempdir,
            project_root,
        }
    }

    fn controller(&self, driver: Arc<dyn ModelDriver>) -> ReplController {
        self.controller_with_cancellation(driver, Arc::new(Mutex::new(None)))
    }

    fn controller_with_cancellation(
        &self,
        driver: Arc<dyn ModelDriver>,
        cancellation_slot: Arc<Mutex<Option<CancellationFlag>>>,
    ) -> ReplController {
        let runtime = SessionRuntime::new(
            SessionId::from_static("ui-test"),
            RuntimeCapabilities::new(true, false),
            self.project_root.clone(),
            self.project_root.clone(),
            PermissionMode::Default,
        );

        ReplController::new(
            runtime,
            builtin_command_registry(),
            builtin_tool_registry(),
            ConversationEngine::new(SessionId::from_static("ui-test")),
            driver,
            cancellation_slot,
        )
    }
}

struct ScriptedModelDriver {
    responses: Mutex<VecDeque<Result<Vec<ModelStreamEvent>, ClawinError>>>,
}

impl ScriptedModelDriver {
    fn new(responses: Vec<Result<Vec<ModelStreamEvent>, ClawinError>>) -> Self {
        Self {
            responses: Mutex::new(VecDeque::from(responses)),
        }
    }
}

impl ModelDriver for ScriptedModelDriver {
    fn stream(&self, _request: ModelRequest) -> ModelDriverFuture<'_> {
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

struct BlockingModelDriver {
    cancellation_slot: Arc<Mutex<Option<CancellationFlag>>>,
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
    fn initialize_session(&self, _runtime: &SessionRuntime) -> clawin_core::ClawinResult<()> {
        Ok(())
    }

    fn save_last_prompt(
        &self,
        _runtime: &SessionRuntime,
        _prompt: &str,
    ) -> clawin_core::ClawinResult<()> {
        Ok(())
    }

    fn append_message(
        &self,
        _runtime: &SessionRuntime,
        _message: &ConversationMessage,
    ) -> clawin_core::ClawinResult<()> {
        Ok(())
    }

    fn save_worktree_state(
        &self,
        _runtime: &SessionRuntime,
        _worktree: Option<&clawin_core::PersistedWorktreeSession>,
    ) -> clawin_core::ClawinResult<()> {
        Ok(())
    }

    fn list_recent_sessions(
        &self,
        _runtime: &SessionRuntime,
    ) -> clawin_core::ClawinResult<Vec<SessionPreview>> {
        Ok(Vec::new())
    }

    fn resolve_resume(
        &self,
        _runtime: &SessionRuntime,
        _query: ResumeQuery,
    ) -> clawin_core::ClawinResult<Option<RestoredSession>> {
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
    ) -> clawin_core::ClawinResult<clawin_core::PersistedWorktreeSession> {
        unreachable!("resume REPL test should not enter a worktree")
    }

    fn exit_worktree(
        &self,
        _runtime: &SessionRuntime,
        _action: WorktreeExitAction,
        _discard_changes: bool,
    ) -> clawin_core::ClawinResult<Option<clawin_core::PersistedWorktreeSession>> {
        unreachable!("resume REPL test should not exit a worktree")
    }
}

impl BlockingModelDriver {
    fn new(cancellation_slot: Arc<Mutex<Option<CancellationFlag>>>) -> Self {
        Self { cancellation_slot }
    }
}

impl ModelDriver for BlockingModelDriver {
    fn stream(&self, _request: ModelRequest) -> ModelDriverFuture<'_> {
        let cancellation_slot = self.cancellation_slot.clone();
        Box::pin(async move {
            loop {
                let current = current_cancellation(&cancellation_slot);
                if current.as_ref().is_some_and(CancellationFlag::is_cancelled) {
                    return Ok(vec![ModelStreamEvent::ModelFinished {
                        finish_reason: ModelFinishReason::Cancelled,
                    }]);
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        })
    }
}

fn current_cancellation(slot: &Arc<Mutex<Option<CancellationFlag>>>) -> Option<CancellationFlag> {
    let guard: MutexGuard<'_, Option<CancellationFlag>> =
        slot.lock().expect("cancellation slot should be available");
    guard.clone()
}

#[derive(Default)]
struct FakeBridgeController {
    state: Mutex<BridgeStatusSnapshot>,
    start_count: Mutex<usize>,
    host: Mutex<Option<Arc<dyn BridgeSessionHost>>>,
}

impl FakeBridgeController {
    fn start_count(&self) -> usize {
        *self
            .start_count
            .lock()
            .expect("fake bridge controller start count lock should be available")
    }

    fn wait_for_host(&self, timeout: std::time::Duration) -> Option<Arc<dyn BridgeSessionHost>> {
        let deadline = std::time::Instant::now() + timeout;
        while std::time::Instant::now() < deadline {
            if let Some(host) = self
                .host
                .lock()
                .expect("fake bridge controller host lock should be available")
                .as_ref()
                .cloned()
            {
                return Some(host);
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        None
    }
}

impl BridgeController for FakeBridgeController {
    fn status(&self) -> clawin_core::ClawinResult<BridgeStatusSnapshot> {
        Ok(self
            .state
            .lock()
            .expect("fake bridge controller state lock should be available")
            .clone())
    }

    fn start(
        &self,
        runtime: &SessionRuntime,
        host: Arc<dyn BridgeSessionHost>,
        mode: BridgeMode,
        source: BridgePointerSource,
        name: Option<String>,
        _pointer: Option<BridgePointer>,
    ) -> clawin_core::ClawinResult<BridgeStatusSnapshot> {
        *self
            .start_count
            .lock()
            .expect("fake bridge controller start count lock should be available") += 1;
        *self
            .host
            .lock()
            .expect("fake bridge controller host lock should be available") = Some(host);
        let status = BridgeStatusSnapshot {
            state: BridgeState::Connected,
            mode: Some(mode),
            source: Some(source),
            name,
            bridge_session_id: Some("bridge-ui".to_owned()),
            environment_id: Some("env-ui".to_owned()),
            local_session_id: Some(runtime.session_id().clone()),
            transcript_path: None,
            last_error: None,
        };
        *self
            .state
            .lock()
            .expect("fake bridge controller state lock should be available") = status.clone();
        Ok(status)
    }

    fn stop(&self) -> clawin_core::ClawinResult<BridgeStatusSnapshot> {
        let mut state = self
            .state
            .lock()
            .expect("fake bridge controller state lock should be available");
        *self
            .host
            .lock()
            .expect("fake bridge controller host lock should be available") = None;
        state.state = BridgeState::Stopped;
        Ok(state.clone())
    }

    fn wait_for_terminal_state(&self) -> clawin_core::ClawinResult<BridgeStatusSnapshot> {
        self.status()
    }
}
