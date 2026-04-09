use std::collections::VecDeque;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

use clawin_bootstrap::{bootstrap_session_from, run_remote_control_session};
use clawin_core::{
    BridgeController, ClawinError, ModelDriver, ModelDriverFuture, ModelFinishReason, ModelRequest,
    ModelStreamEvent, PermissionBehavior, StructuredControlRequest, StructuredControlResponse,
    StructuredInputControlRequest, StructuredInputMessage, StructuredOutputMessage, ToolCall,
};
use clawin_integrations::{BridgeManager, FakeBridgeConnector, ReconnectPolicy};
use clawin_platform::{FakeGitWorktreeAdapter, PathPolicy, StaticTerminalCapabilities};
use serde_json::Value;
use tempfile::TempDir;

const TEST_MESSAGE_TIMEOUT: Duration = Duration::from_secs(2);
const TEST_POLL_INTERVAL: Duration = Duration::from_millis(50);

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
        remotes[0].recv_timeout(TEST_MESSAGE_TIMEOUT),
        Some(StructuredOutputMessage::SessionStarted { session_id })
            if session_id == runtime.session_id().as_str()
    ));

    remotes[0]
        .send(StructuredInputMessage::User {
            content: "/help".to_owned(),
        })
        .expect("fake remote user input should send");

    let mut saw_command_result = false;
    let deadline = std::time::Instant::now() + TEST_MESSAGE_TIMEOUT;
    while std::time::Instant::now() < deadline {
        if let Some(StructuredOutputMessage::Result { result }) =
            remotes[0].recv_timeout(TEST_POLL_INTERVAL)
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

#[test]
fn standalone_remote_control_permission_allow_sequence_matches_fixture() {
    let harness = Harness::new();
    fs::write(
        harness
            .project_dir
            .parent()
            .expect("workspace should exist")
            .join("secret.txt"),
        "secret note",
    )
    .expect("secret note should be written");
    let session = bootstrap_session_from(
        harness.project_dir.clone(),
        StaticTerminalCapabilities::new(false, false),
        harness.path_policy(),
    )
    .expect("bootstrap session should assemble");
    let git = Arc::new(FakeGitWorktreeAdapter::new());
    git.register_repository(
        harness.project_dir.clone(),
        vec![harness.project_dir.clone()],
    );
    let (connector, remotes) = FakeBridgeConnector::with_sessions(vec![(
        "bridge-remote-allow".to_owned(),
        "env-remote-allow".to_owned(),
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
            Arc::new(ScriptedModelDriver::new(vec![
                Ok(vec![
                    ModelStreamEvent::ToolCallRequested {
                        call: ToolCall::new(
                            "toolu_1",
                            "file_read",
                            serde_json::json!({ "file_path": "../secret.txt" }),
                        ),
                    },
                    ModelStreamEvent::ModelFinished {
                        finish_reason: ModelFinishReason::ToolUse,
                    },
                ]),
                Ok(vec![
                    ModelStreamEvent::TextDelta {
                        delta: "allowed read".to_owned(),
                    },
                    ModelStreamEvent::AssistantMessageFinished,
                    ModelStreamEvent::ModelFinished {
                        finish_reason: ModelFinishReason::Completed,
                    },
                ]),
            ])),
            Some("demo".to_owned()),
            None,
        )
        .expect("remote control runner should succeed")
    });

    let mut messages = vec![wait_for_message(&remotes[0], Duration::from_millis(250))];
    remotes[0]
        .send(StructuredInputMessage::User {
            content: "read the secret".to_owned(),
        })
        .expect("fake remote user input should send");

    let request_id = collect_until_control_request(&remotes[0], &mut messages);
    remotes[0]
        .send(StructuredInputMessage::ControlResponse {
            response: StructuredControlResponse::CanUseTool {
                request_id,
                behavior: PermissionBehavior::Allow,
                message: None,
            },
        })
        .expect("allow response should send");
    collect_until_result(&remotes[0], &mut messages);

    let stopped = manager.stop().expect("bridge manager should stop");
    assert_eq!(stopped.state.as_str(), "stopped");
    let exit = runner.join().expect("runner thread should join");
    assert_eq!(exit, ExitCode::SUCCESS);

    assert_eq!(
        normalized_json_lines(&messages),
        fixture_json_lines("tests/fixtures/remote_control_permission_allow.jsonl")
    );
}

#[test]
fn standalone_remote_control_interrupt_emits_cancel_request_fixture() {
    let harness = Harness::new();
    let session = bootstrap_session_from(
        harness.project_dir.clone(),
        StaticTerminalCapabilities::new(false, false),
        harness.path_policy(),
    )
    .expect("bootstrap session should assemble");
    let git = Arc::new(FakeGitWorktreeAdapter::new());
    git.register_repository(
        harness.project_dir.clone(),
        vec![harness.project_dir.clone()],
    );
    let (connector, remotes) = FakeBridgeConnector::with_sessions(vec![(
        "bridge-remote-interrupt".to_owned(),
        "env-remote-interrupt".to_owned(),
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
            Arc::new(ScriptedModelDriver::new(vec![Ok(vec![
                ModelStreamEvent::ToolCallRequested {
                    call: ToolCall::new(
                        "toolu_1",
                        "file_read",
                        serde_json::json!({ "file_path": "../secret.txt" }),
                    ),
                },
                ModelStreamEvent::ModelFinished {
                    finish_reason: ModelFinishReason::ToolUse,
                },
            ])])),
            Some("demo".to_owned()),
            None,
        )
        .expect("remote control runner should succeed")
    });

    let mut messages = vec![wait_for_message(&remotes[0], Duration::from_millis(250))];
    remotes[0]
        .send(StructuredInputMessage::User {
            content: "read the secret".to_owned(),
        })
        .expect("fake remote user input should send");

    let _request_id = collect_until_control_request(&remotes[0], &mut messages);
    remotes[0]
        .send(StructuredInputMessage::ControlRequest {
            request: StructuredInputControlRequest::Interrupt,
        })
        .expect("interrupt request should send");
    collect_until_result(&remotes[0], &mut messages);

    let stopped = manager.stop().expect("bridge manager should stop");
    assert_eq!(stopped.state.as_str(), "stopped");
    let exit = runner.join().expect("runner thread should join");
    assert_eq!(exit, ExitCode::SUCCESS);

    assert_eq!(
        normalized_json_lines(&messages),
        fixture_json_lines("tests/fixtures/remote_control_permission_interrupt.jsonl")
    );
}

#[test]
fn standalone_remote_control_emits_busy_error_for_concurrent_user_input() {
    let harness = Harness::new();
    let session = bootstrap_session_from(
        harness.project_dir.clone(),
        StaticTerminalCapabilities::new(false, false),
        harness.path_policy(),
    )
    .expect("bootstrap session should assemble");
    let git = Arc::new(FakeGitWorktreeAdapter::new());
    git.register_repository(
        harness.project_dir.clone(),
        vec![harness.project_dir.clone()],
    );
    let (connector, remotes) = FakeBridgeConnector::with_sessions(vec![(
        "bridge-remote-busy".to_owned(),
        "env-remote-busy".to_owned(),
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
            Arc::new(DelayedModelDriver::new(
                Duration::from_millis(50),
                vec![
                    ModelStreamEvent::TextDelta {
                        delta: "slow reply".to_owned(),
                    },
                    ModelStreamEvent::AssistantMessageFinished,
                    ModelStreamEvent::ModelFinished {
                        finish_reason: ModelFinishReason::Completed,
                    },
                ],
            )),
            Some("demo".to_owned()),
            None,
        )
        .expect("remote control runner should succeed")
    });

    let mut messages = vec![wait_for_message(&remotes[0], Duration::from_millis(250))];
    remotes[0]
        .send(StructuredInputMessage::User {
            content: "hello".to_owned(),
        })
        .expect("first fake remote user input should send");
    collect_until_turn_started(&remotes[0], &mut messages);
    remotes[0]
        .send(StructuredInputMessage::User {
            content: "again".to_owned(),
        })
        .expect("second fake remote user input should send");
    collect_until_result(&remotes[0], &mut messages);

    let stopped = manager.stop().expect("bridge manager should stop");
    assert_eq!(stopped.state.as_str(), "stopped");
    let exit = runner.join().expect("runner thread should join");
    assert_eq!(exit, ExitCode::SUCCESS);

    assert_eq!(
        normalized_json_lines(&messages),
        fixture_json_lines("tests/fixtures/remote_control_busy.jsonl")
    );
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

struct DelayedModelDriver {
    delay: Duration,
    events: Vec<ModelStreamEvent>,
}

impl DelayedModelDriver {
    fn new(delay: Duration, events: Vec<ModelStreamEvent>) -> Self {
        Self { delay, events }
    }
}

impl ModelDriver for DelayedModelDriver {
    fn stream(&self, _request: ModelRequest) -> ModelDriverFuture<'_> {
        let delay = self.delay;
        let events = self.events.clone();
        Box::pin(async move {
            tokio::time::sleep(delay).await;
            Ok(events)
        })
    }
}

fn wait_for_message(
    remote: &Arc<clawin_integrations::FakeBridgeRemote>,
    timeout: Duration,
) -> StructuredOutputMessage {
    remote
        .recv_timeout(timeout)
        .expect("fake bridge remote should emit a message before timeout")
}

fn collect_until_control_request(
    remote: &Arc<clawin_integrations::FakeBridgeRemote>,
    messages: &mut Vec<StructuredOutputMessage>,
) -> String {
    let deadline = std::time::Instant::now() + TEST_MESSAGE_TIMEOUT;
    while std::time::Instant::now() < deadline {
        if let Some(message) = remote.recv_timeout(TEST_POLL_INTERVAL) {
            let request_id = if let StructuredOutputMessage::ControlRequest {
                request: StructuredControlRequest::CanUseTool { request_id, .. },
            } = &message
            {
                Some(request_id.clone())
            } else {
                None
            };
            messages.push(message);
            if let Some(request_id) = request_id {
                return request_id;
            }
        }
    }

    panic!("timed out waiting for control request");
}

fn collect_until_result(
    remote: &Arc<clawin_integrations::FakeBridgeRemote>,
    messages: &mut Vec<StructuredOutputMessage>,
) {
    let deadline = std::time::Instant::now() + TEST_MESSAGE_TIMEOUT;
    while std::time::Instant::now() < deadline {
        if let Some(message) = remote.recv_timeout(TEST_POLL_INTERVAL) {
            let is_result = matches!(message, StructuredOutputMessage::Result { .. });
            messages.push(message);
            if is_result {
                return;
            }
        }
    }

    panic!("timed out waiting for result");
}

fn collect_until_turn_started(
    remote: &Arc<clawin_integrations::FakeBridgeRemote>,
    messages: &mut Vec<StructuredOutputMessage>,
) {
    let deadline = std::time::Instant::now() + TEST_MESSAGE_TIMEOUT;
    while std::time::Instant::now() < deadline {
        if let Some(message) = remote.recv_timeout(TEST_POLL_INTERVAL) {
            let is_turn_started = matches!(
                &message,
                StructuredOutputMessage::StreamEvent {
                    event: clawin_core::EngineEvent::TurnStarted { .. }
                }
            );
            messages.push(message);
            if is_turn_started {
                return;
            }
        }
    }

    panic!("timed out waiting for turn_started");
}

fn fixture_text(path: &str) -> String {
    let fixture_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(path);
    fs::read_to_string(fixture_path).expect("fixture should exist")
}

fn fixture_json_lines(path: &str) -> Vec<Value> {
    fixture_text(path)
        .lines()
        .map(|line| serde_json::from_str(line).expect("fixture line should be valid json"))
        .collect()
}

fn normalized_json_lines(messages: &[StructuredOutputMessage]) -> Vec<Value> {
    messages
        .iter()
        .map(|message| {
            let mut value =
                serde_json::to_value(message).expect("structured output should serialize");
            normalize_json_value(&mut value);
            value
        })
        .collect()
}

fn normalize_json_value(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (key, child) in map.iter_mut() {
                if key == "session_id" {
                    *child = Value::String("<session-id>".to_owned());
                } else {
                    normalize_json_value(child);
                }
            }
        }
        Value::Array(items) => {
            for child in items {
                normalize_json_value(child);
            }
        }
        _ => {}
    }
}
