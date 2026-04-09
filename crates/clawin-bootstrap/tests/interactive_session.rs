// Phase 5 tests continue under DIFF-2026-001: interactive no-arg routing enters the Rust REPL instead of the Phase 4 placeholder.

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use clawin_bootstrap::{bootstrap_session_from, run_bootstrapped_session_with_terminal};
use clawin_core::{ClawinError, ModelDriver, ModelDriverFuture, ModelRequest};
use clawin_platform::{
    FakeTerminalSession, PathPolicy, StaticTerminalCapabilities, TerminalEvent, TerminalKeyCode,
    TerminalKeyEvent, TerminalKeyModifiers, TerminalSize,
};
use tempfile::TempDir;

#[test]
fn interactive_bootstrap_routes_into_repl_and_restores_terminal_session() {
    let harness = BootstrapHarness::new();
    let session = bootstrap_session_from(
        harness.project_dir.clone(),
        StaticTerminalCapabilities::new(true, true),
        TestPathPolicy {
            home_dir: harness.home_dir.clone(),
        },
    )
    .expect("interactive bootstrap session should assemble");
    let driver = Arc::new(IdleModelDriver);
    let mut terminal = FakeTerminalSession::new(
        TerminalSize::new(100, 30),
        vec![Some(TerminalEvent::Key(TerminalKeyEvent::new(
            TerminalKeyCode::Char('c'),
            TerminalKeyModifiers::CONTROL,
        )))],
    );

    let exit = run_bootstrapped_session_with_terminal(session, driver, &mut terminal)
        .expect("interactive routing should succeed");

    assert_eq!(exit, ExitCode::SUCCESS);
    assert!(terminal.entered());
    assert!(terminal.left());
}

struct BootstrapHarness {
    _tempdir: TempDir,
    home_dir: PathBuf,
    project_dir: PathBuf,
}

impl BootstrapHarness {
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

struct IdleModelDriver;

impl ModelDriver for IdleModelDriver {
    fn stream(&self, _request: ModelRequest) -> ModelDriverFuture<'_> {
        Box::pin(async {
            Err(ClawinError::ModelDriver {
                message: "idle driver should not be used in ctrl-c exit test".to_owned(),
            })
        })
    }
}
