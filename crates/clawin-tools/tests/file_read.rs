// Phase 3 tests continue under DIFF-2026-001: Clawin keeps its own namespace and project metadata paths.

use std::fs;
use std::path::PathBuf;

use clawin_core::{
    ClawinError, PermissionBehavior, PermissionMode, RuntimeCapabilities, SessionId,
    SessionRuntime, ToolCall,
};
use clawin_tools::builtin_tool_registry;
use serde_json::{Value, json};
use tempfile::TempDir;

#[test]
fn reads_text_files_with_offset_and_limit() {
    let harness = ToolHarness::new();
    fs::write(harness.project_file("notes.txt"), "one\ntwo\nthree\n")
        .expect("test file should be written");

    let execution = builtin_tool_registry()
        .execute(
            ToolCall::new(
                "toolu_001",
                "file_read",
                json!({
                    "file_path": "notes.txt",
                    "offset": 2,
                    "limit": 2
                }),
            ),
            &harness.runtime(false),
        )
        .expect("file_read should succeed");

    assert_eq!(execution.permission_behavior, PermissionBehavior::Allow);
    assert!(!execution.result.is_error);
    assert_eq!(
        execution.result.content,
        fixture_json("tests/fixtures/file_read_success.json")
    );
}

#[test]
fn rejects_paths_outside_project_root_in_non_interactive_mode() {
    let harness = ToolHarness::new();
    let outside_file = harness.tempdir.path().join("outside.txt");
    fs::write(&outside_file, "secret").expect("outside file should be written");

    let execution = builtin_tool_registry()
        .execute(
            ToolCall::new(
                "toolu_002",
                "file_read",
                json!({
                    "file_path": outside_file
                }),
            ),
            &harness.runtime(false),
        )
        .expect("permission fallback should return a structured result");

    assert_eq!(execution.permission_behavior, PermissionBehavior::Ask);
    assert!(execution.result.is_error);
    assert_eq!(
        execution.result.content,
        fixture_json("tests/fixtures/file_read_permission_denied.json")
    );
}

#[test]
fn rejects_invalid_input_schema() {
    let harness = ToolHarness::new();
    let error = builtin_tool_registry()
        .execute(
            ToolCall::new("toolu_003", "file_read", json!({ "offset": 1 })),
            &harness.runtime(false),
        )
        .expect_err("invalid input should fail");

    assert!(matches!(
        error,
        ClawinError::ToolInputInvalid { ref tool, .. } if tool == "file_read"
    ));
}

#[test]
fn rejects_unsupported_file_types() {
    let harness = ToolHarness::new();
    fs::write(harness.project_file("manual.pdf"), b"%PDF-1.7").expect("pdf stub should be written");

    let execution = builtin_tool_registry()
        .execute(
            ToolCall::new(
                "toolu_004",
                "file_read",
                json!({
                    "file_path": "manual.pdf"
                }),
            ),
            &harness.runtime(false),
        )
        .expect("unsupported file types should return a structured result");

    assert_eq!(execution.permission_behavior, PermissionBehavior::Allow);
    assert!(execution.result.is_error);
    assert_eq!(execution.result.content["code"], "unsupported_file_type");
}

struct ToolHarness {
    tempdir: TempDir,
    project_root: PathBuf,
}

impl ToolHarness {
    fn new() -> Self {
        let tempdir = tempfile::tempdir().expect("tempdir should exist");
        let project_root = tempdir.path().join("project");
        fs::create_dir_all(&project_root).expect("project root should exist");
        Self {
            tempdir,
            project_root,
        }
    }

    fn project_file(&self, name: &str) -> PathBuf {
        self.project_root.join(name)
    }

    fn runtime(&self, interactive: bool) -> SessionRuntime {
        SessionRuntime::new(
            SessionId::from_static("tools-test"),
            RuntimeCapabilities::new(interactive, false),
            self.project_root.clone(),
            self.project_root.clone(),
            PermissionMode::Default,
        )
    }
}

fn fixture_json(path: &str) -> Value {
    let fixture_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(path);
    let contents = fs::read_to_string(fixture_path).expect("fixture should exist");
    serde_json::from_str(&contents).expect("fixture should be valid json")
}
