use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use clawin_bootstrap::{bootstrap_session_from, run_remote_control_session};
use clawin_core::{
    BridgeController, ModelDriver, ModelDriverFuture, ModelRequest, StructuredInputMessage,
    StructuredOutputMessage,
};
use clawin_integrations::{BridgeManager, FakeBridgeConnector, ReconnectPolicy};
use clawin_platform::{FakeGitWorktreeAdapter, PathPolicy, StaticTerminalCapabilities};
use tempfile::TempDir;

#[test]
fn standalone_remote_control_runs_help_command_over_fake_bridge() {
    let harness = Harness::new();
    let session = bootstrap_session_from(
        harness.project_dir.clone(),
        StaticTerminalCapabilities::new(false, false),
        harness.path_policy(),
    )
    .expect("bootstrap session should assemble");
    let runtime = session.runtime().clone();
    let git = Arc::new(FakeGitWorktreeAdapter::new());
    git.register_repository(
        harness.project_dir.clone(),
        vec![harness.project_dir.clone()],
    );
    let (connector, remotes) = FakeBridgeConnector::with_sessions(vec![(
        "bridge-remote-1".to_owned(),
        "env-remote-1".to_owned(),
        FakeBridgeConnector::empty_remote(),
    )]);
    let manager = Arc::new(BridgeManager::with_policy(
        session.config().paths().clone(),
        harness.path_policy(),
        git,
        connector,
        ReconnectPolicy {
            initial_delay: Duration::from_millis(5),
            max_delay: Duration::from_millis(10),
            give_up_after: Duration::from_millis(25),
            poll_interval: Duration::from_millis(5),
        },
    ));
    session.runtime().set_bridge_controller(manager.clone());

    let runner = thread::spawn(move || {
        run_remote_control_session(
            session,
            Arc::new(PanicModelDriver),
            Some("demo".to_owned()),
            None,
        )
        .expect("remote control runner should succeed")
    });

    assert!(matches!(
        remotes[0].recv_timeout(Duration::from_millis(250)),
        Some(StructuredOutputMessage::SessionStarted { session_id })
            if session_id == runtime.session_id().as_str()
    ));

    remotes[0]
        .send(StructuredInputMessage::User {
            content: "/help".to_owned(),
        })
        .expect("fake remote user input should send");

    let mut saw_command_result = false;
    let deadline = std::time::Instant::now() + Duration::from_millis(500);
    while std::time::Instant::now() < deadline {
        if let Some(StructuredOutputMessage::Result { result }) =
            remotes[0].recv_timeout(Duration::from_millis(25))
        {
            if result
                .command_output
                .as_deref()
                .is_some_and(|output| output.contains("Available commands:"))
            {
                saw_command_result = true;
                break;
            }
        }
    }

    assert!(
        saw_command_result,
        "standalone bridge should return /help output"
    );

    let stopped = manager.stop().expect("bridge manager should stop");
    assert_eq!(stopped.state.as_str(), "stopped");

    let exit = runner.join().expect("runner thread should join");
    assert_eq!(exit, ExitCode::SUCCESS);
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

    fn path_policy(&self) -> TestPathPolicy {
        TestPathPolicy {
            home_dir: self.home_dir.clone(),
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

struct PanicModelDriver;

impl ModelDriver for PanicModelDriver {
    fn stream(&self, _request: ModelRequest) -> ModelDriverFuture<'_> {
        Box::pin(async {
            panic!("model driver should not be used for /help bridge test");
        })
    }
}
