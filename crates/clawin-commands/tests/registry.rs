// Phase 3 tests continue under DIFF-2026-001: Clawin keeps its own namespace and command surface.

use clawin_commands::builtin_command_registry;
use clawin_core::{
    CommandKind, CommandSource, PermissionMode, RuntimeCapabilities, SessionId, SessionRuntime,
};

#[test]
fn parses_aliases_and_executes_help_command() {
    let registry = builtin_command_registry();
    let invocation = registry.parse("/?").expect("alias should resolve");

    assert_eq!(invocation.raw_name, "?");
    assert_eq!(invocation.command_name, "help");
    assert_eq!(invocation.args, "");

    let help_spec = registry.spec("help").expect("help spec should exist");
    assert_eq!(help_spec.kind, CommandKind::Local);
    assert_eq!(help_spec.source, CommandSource::Builtin);

    let result = registry
        .execute("/help", &runtime())
        .expect("help command should execute");

    assert_eq!(result.command_name, "help");
    assert_eq!(result.output, include_str!("fixtures/help_output.txt"));
}

#[test]
fn rejects_unknown_slash_commands() {
    let registry = builtin_command_registry();
    let error = registry
        .execute("/missing", &runtime())
        .expect_err("unknown command should fail");

    assert!(matches!(
        error,
        clawin_core::ClawinError::UnknownCommand { ref name } if name == "missing"
    ));
}

fn runtime() -> SessionRuntime {
    SessionRuntime::new(
        SessionId::from_static("commands-test"),
        RuntimeCapabilities::new(false, false),
        std::env::temp_dir(),
        std::env::temp_dir(),
        PermissionMode::Default,
    )
}
