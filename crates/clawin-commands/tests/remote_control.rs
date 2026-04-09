use std::sync::Arc;

use clawin_commands::builtin_command_registry;
use clawin_core::{
    CommandEffect, PermissionMode, RuntimeCapabilities, SessionId, SessionRuntime,
    WorktreeExitAction, WorktreeManager,
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
