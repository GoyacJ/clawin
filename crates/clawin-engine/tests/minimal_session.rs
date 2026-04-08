// Phase 3 tests continue under DIFF-2026-001: the Rust runtime uses Clawin-owned namespace behavior.

use std::fs;
use std::path::PathBuf;

use clawin_commands::builtin_command_registry;
use clawin_core::{
    MinimalSessionRequest, MinimalSessionResponse, PermissionBehavior, PermissionMode,
    RuntimeCapabilities, SessionEvent, SessionId, SessionRuntime, ToolCall, TurnId,
};
use clawin_engine::ConversationEngine;
use clawin_tools::builtin_tool_registry;
use serde_json::json;
use tempfile::TempDir;

#[test]
fn runs_help_command_through_minimal_session() {
    let harness = EngineHarness::new();
    let mut engine = ConversationEngine::new(harness.runtime.session_id().clone());

    let outcome = engine
        .run_minimal_session(
            &harness.runtime,
            &builtin_command_registry(),
            &builtin_tool_registry(),
            MinimalSessionRequest::SlashCommand("/help".to_owned()),
        )
        .expect("help session should run");

    assert_eq!(engine.turn_count(), 1);
    assert_eq!(
        outcome.events,
        vec![
            SessionEvent::SessionStarted {
                session_id: "engine-test".to_owned(),
            },
            SessionEvent::TurnStarted {
                turn_id: TurnId::new(1),
            },
            SessionEvent::CommandParsed {
                raw_name: "help".to_owned(),
                command_name: "help".to_owned(),
            },
            SessionEvent::CommandExecuted {
                command_name: "help".to_owned(),
            },
            SessionEvent::SessionFinished {
                turn_id: TurnId::new(1),
            },
        ]
    );
    assert!(matches!(
        outcome.response,
        MinimalSessionResponse::Command(ref result) if result.command_name == "help"
    ));
}

#[test]
fn runs_file_read_tool_through_minimal_session() {
    let harness = EngineHarness::new();
    fs::write(harness.project_file("notes.txt"), "alpha\nbeta\n").expect("file should exist");

    let mut engine = ConversationEngine::new(harness.runtime.session_id().clone());
    let outcome = engine
        .run_minimal_session(
            &harness.runtime,
            &builtin_command_registry(),
            &builtin_tool_registry(),
            MinimalSessionRequest::ToolCall(ToolCall::new(
                "toolu_123",
                "file_read",
                json!({
                    "file_path": "notes.txt"
                }),
            )),
        )
        .expect("tool session should run");

    assert_eq!(engine.turn_count(), 1);
    assert_eq!(
        outcome.events,
        vec![
            SessionEvent::SessionStarted {
                session_id: "engine-test".to_owned(),
            },
            SessionEvent::TurnStarted {
                turn_id: TurnId::new(1),
            },
            SessionEvent::ToolRequested {
                call_id: "toolu_123".to_owned(),
                tool_name: "file_read".to_owned(),
            },
            SessionEvent::ToolPermissionResolved {
                call_id: "toolu_123".to_owned(),
                tool_name: "file_read".to_owned(),
                behavior: PermissionBehavior::Allow,
            },
            SessionEvent::ToolCompleted {
                call_id: "toolu_123".to_owned(),
                tool_name: "file_read".to_owned(),
                is_error: false,
            },
            SessionEvent::SessionFinished {
                turn_id: TurnId::new(1),
            },
        ]
    );
    assert!(matches!(
        outcome.response,
        MinimalSessionResponse::Tool(ref result)
            if result.tool_name == "file_read" && result.content["content"] == "alpha\nbeta"
    ));
}

struct EngineHarness {
    _tempdir: TempDir,
    runtime: SessionRuntime,
    project_root: PathBuf,
}

impl EngineHarness {
    fn new() -> Self {
        let tempdir = tempfile::tempdir().expect("tempdir should exist");
        let project_root = tempdir.path().join("project");
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
            project_root,
        }
    }

    fn project_file(&self, name: &str) -> PathBuf {
        self.project_root.join(name)
    }
}
