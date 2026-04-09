use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use clawin_commands::builtin_command_registry;
use clawin_core::{
    BridgeController, BridgeMode, BridgePointer, BridgePointerSource, BridgeSessionHost,
    BridgeState, BridgeStatusSnapshot, ClawinResult, CommandEffect, PermissionMode,
    RuntimeCapabilities, SessionId, SessionRuntime, WorktreeExitAction, WorktreeManager,
};

#[test]
fn remote_control_start_command_returns_bridge_control_effect() {
    let registry = builtin_command_registry();
    let runtime = runtime();

    let result = registry
        .execute("/rc demo", &runtime)
        .expect("/rc demo should execute");

    assert_eq!(result.command_name, "remote-control");
    assert!(result.output.contains("demo"));
    assert!(matches!(
        result.effect,
        Some(CommandEffect::BridgeControl { .. })
    ));
}

#[test]
fn remote_control_stop_command_returns_bridge_control_effect() {
    let registry = builtin_command_registry();
    let runtime = runtime();

    let result = registry
        .execute("/remote-control stop", &runtime)
        .expect("/remote-control stop should execute");

    assert_eq!(result.command_name, "remote-control");
    assert!(matches!(
        result.effect,
        Some(CommandEffect::BridgeControl { .. })
    ));
}

#[test]
fn remote_control_status_renders_snapshot_output() {
    let registry = builtin_command_registry();
    let transcript_path = std::env::temp_dir().join("remote-control-status.jsonl");
    let runtime = runtime().with_bridge_controller(Arc::new(StatusBridgeController {
        status: BridgeStatusSnapshot {
            state: BridgeState::Connected,
            mode: Some(BridgeMode::ReplAttached),
            source: Some(BridgePointerSource::Repl),
            name: Some("demo".to_owned()),
            bridge_session_id: Some("bridge-session-1".to_owned()),
            environment_id: Some("env-1".to_owned()),
            local_session_id: Some(SessionId::from_static("commands-remote-control")),
            transcript_path: Some(transcript_path.clone()),
            last_error: None,
        },
    }));

    let result = registry
        .execute("/remote-control status", &runtime)
        .expect("/remote-control status should execute");

    assert_eq!(
        normalize_status_output(&result.output, &transcript_path),
        fixture_text("tests/fixtures/remote_control_status_output.txt")
    );
    assert!(result.effect.is_none());
}

#[test]
fn remote_control_rejects_unknown_subcommand() {
    let registry = builtin_command_registry();
    let runtime = runtime();

    let error = registry
        .execute("/remote-control status extra", &runtime)
        .expect_err("unknown remote-control subcommand should fail");

    assert!(matches!(
        error,
        clawin_core::ClawinError::InvalidCommandInvocation { ref message }
            if message == "usage: /remote-control [name|status|stop]"
    ));
}

fn runtime() -> SessionRuntime {
    SessionRuntime::new(
        SessionId::from_static("commands-remote-control"),
        RuntimeCapabilities::new(false, false),
        std::env::temp_dir(),
        std::env::temp_dir(),
        PermissionMode::Default,
    )
    .with_worktree_manager(Arc::new(NoopWorktreeManager))
}

fn fixture_text(path: &str) -> String {
    let fixture_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(path);
    fs::read_to_string(fixture_path).expect("fixture should exist")
}

fn normalize_status_output(output: &str, transcript_path: &std::path::Path) -> String {
    output.replace(
        &format!("transcript_path={}", transcript_path.display()),
        "transcript_path=<transcript-path>",
    )
}

#[derive(Debug)]
struct NoopWorktreeManager;

impl WorktreeManager for NoopWorktreeManager {
    fn enter_worktree(
        &self,
        _runtime: &SessionRuntime,
        _name: Option<&str>,
    ) -> clawin_core::ClawinResult<clawin_core::PersistedWorktreeSession> {
        unreachable!("worktree manager should not be used in remote-control command tests")
    }

    fn exit_worktree(
        &self,
        _runtime: &SessionRuntime,
        _action: WorktreeExitAction,
        _discard_changes: bool,
    ) -> clawin_core::ClawinResult<Option<clawin_core::PersistedWorktreeSession>> {
        unreachable!("worktree manager should not be used in remote-control command tests")
    }
}

#[derive(Debug)]
struct StatusBridgeController {
    status: BridgeStatusSnapshot,
}

impl BridgeController for StatusBridgeController {
    fn status(&self) -> ClawinResult<BridgeStatusSnapshot> {
        Ok(self.status.clone())
    }

    fn start(
        &self,
        _runtime: &SessionRuntime,
        _host: Arc<dyn BridgeSessionHost>,
        _mode: BridgeMode,
        _source: BridgePointerSource,
        _name: Option<String>,
        _pointer: Option<BridgePointer>,
    ) -> ClawinResult<BridgeStatusSnapshot> {
        unreachable!("status bridge controller should not start a bridge")
    }

    fn stop(&self) -> ClawinResult<BridgeStatusSnapshot> {
        unreachable!("status bridge controller should not stop a bridge")
    }

    fn wait_for_terminal_state(&self) -> ClawinResult<BridgeStatusSnapshot> {
        unreachable!("status bridge controller should not wait for terminal state")
    }
}
