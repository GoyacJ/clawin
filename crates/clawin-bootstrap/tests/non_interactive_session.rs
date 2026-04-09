// Phase 4 tests continue under DIFF-2026-001: bootstrap assembles Clawin-owned runtime and config paths.

use std::collections::VecDeque;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use clawin_bootstrap::bootstrap_session_from;
use clawin_core::{
    CancellationFlag, ClawinError, ConversationRequest, EngineEvent, ModelDriver,
    ModelDriverFuture, ModelFinishReason, ModelRequest, ModelStreamEvent, StopReason,
    TurnLoopConfig,
};
use clawin_platform::{PathPolicy, StaticTerminalCapabilities};
use serde_json::Value;
use tempfile::TempDir;

#[tokio::test]
async fn assembles_non_interactive_bootstrap_session_and_runs_prompt_submit() {
    let harness = BootstrapHarness::new();
    let mut session = bootstrap_session_from(
        harness.project_dir.clone(),
        StaticTerminalCapabilities::new(false, false),
        TestPathPolicy {
            home_dir: harness.home_dir.clone(),
        },
    )
    .expect("bootstrap should assemble");
    let driver = ScriptedModelDriver::new(vec![Ok(vec![
        ModelStreamEvent::TextDelta {
            delta: "stub reply".to_owned(),
        },
        ModelStreamEvent::AssistantMessageFinished,
        ModelStreamEvent::ModelFinished {
            finish_reason: ModelFinishReason::Completed,
        },
    ])]);
    let mut events = Vec::new();

    let outcome = session
        .submit_with_driver(
            &driver,
            ConversationRequest::Prompt("hello".to_owned()),
            TurnLoopConfig {
                max_turns: 2,
                token_budget: None,
                compaction_policy: clawin_core::CompactionPolicy::Disabled,
                allow_budget_continuation: false,
            },
            CancellationFlag::new(),
            |event| events.push(event),
        )
        .await
        .expect("prompt submit should succeed");

    assert_eq!(outcome.stop_reason, StopReason::Completed);
    assert_eq!(
        outcome.final_assistant_message.as_deref(),
        Some("stub reply")
    );
    assert_eq!(
        session.runtime().project_root(),
        fs::canonicalize(&harness.project_dir)
            .expect("project dir should canonicalize")
            .as_path()
    );
    assert_eq!(
        session.config().paths().global_root(),
        harness.home_dir.join(".clawin")
    );
    assert!(session.commands().spec("help").is_some());
    assert!(session.tools().spec("file_read").is_some());
    assert!(events
        .iter()
        .any(|event| matches!(event, EngineEvent::AssistantTextDelta { delta, .. } if delta == "stub reply")));

    let config: Value = serde_json::from_str(
        &fs::read_to_string(harness.home_dir.join(".clawin/config.json"))
            .expect("config should be written"),
    )
    .expect("config should be valid json");
    assert_eq!(config["schema_version"], 1);

    let transcript_dir = harness.home_dir.join(".clawin/projects").join(
        TestPathPolicy {
            home_dir: harness.home_dir.clone(),
        }
        .sanitize_for_session_dir(session.runtime().active_project_root().as_path()),
    );
    let transcript_path =
        transcript_dir.join(format!("{}.jsonl", session.runtime().session_id().as_str()));
    let transcript =
        fs::read_to_string(transcript_path).expect("session transcript should be written");
    assert!(transcript.contains("\"type\":\"session_header\""));
    assert!(transcript.contains("\"type\":\"last_prompt\""));
    assert!(transcript.contains("\"hello\""));
    assert!(transcript.contains("\"stub reply\""));
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

        fs::create_dir_all(&home_dir).expect("home dir should exist");
        fs::create_dir_all(&project_dir).expect("project dir should exist");

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
