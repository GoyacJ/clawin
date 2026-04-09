use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use clawin_bootstrap::bootstrap_session_from_with_process_spawner;
use clawin_platform::{
    FakeProcessPlan, FakeProcessSpawner, PathPolicy, StaticTerminalCapabilities,
};
use serde_json::json;
use tempfile::TempDir;

#[test]
fn bootstrap_assembles_mcp_command_tools_and_runtime_capability() {
    let harness = Harness::new();
    harness.write_global_settings(json!({
        "schema_version": 1,
        "mcpServers": {
            "demo": {
                "command": "/tmp/fake-mcp"
            },
            "broken": {
                "type": "http",
                "url": "https://example.com/mcp"
            }
        }
    }));
    let spawner = Arc::new(FakeProcessSpawner::new(vec![
        FakeProcessPlan::from_json_messages(vec![
            initialize_response(1),
            tools_list_response(2),
            resources_list_response(3),
        ]),
    ]));

    let session = bootstrap_session_from_with_process_spawner(
        harness.project_dir.clone(),
        StaticTerminalCapabilities::new(false, false),
        TestPathPolicy {
            home_dir: harness.home_dir.clone(),
        },
        spawner,
    )
    .expect("bootstrap should assemble with MCP");

    assert!(session.runtime().capabilities().mcp_available());
    assert!(session.commands().spec("mcp").is_some());
    assert!(session.tools().spec("list_mcp_resources").is_some());
    assert!(session.tools().spec("read_mcp_resource").is_some());
    assert!(session.tools().spec("mcp__demo__echo").is_some());

    let output = session
        .commands()
        .execute("/mcp list", session.runtime())
        .expect("mcp list should render");

    assert!(output.output.contains("demo"));
    assert!(output.output.contains("connected"));
    assert!(output.output.contains("broken"));
    assert!(output.output.contains("failed"));
}

#[test]
fn bootstrap_fails_before_runtime_when_mcp_servers_is_invalid() {
    let harness = Harness::new();
    harness.write_global_settings(json!({
        "schema_version": 1,
        "mcpServers": true
    }));

    let error = bootstrap_session_from_with_process_spawner(
        harness.project_dir.clone(),
        StaticTerminalCapabilities::new(false, false),
        TestPathPolicy {
            home_dir: harness.home_dir.clone(),
        },
        Arc::new(FakeProcessSpawner::default()),
    )
    .expect_err("invalid mcpServers should fail bootstrap");

    assert!(error.to_string().contains("mcpServers"));
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
        fs::create_dir_all(home_dir.join(".clawin")).expect("global dir should exist");
        fs::create_dir_all(&project_dir).expect("project dir should exist");

        Self {
            _tempdir: tempdir,
            home_dir,
            project_dir,
        }
    }

    fn write_global_settings(&self, value: serde_json::Value) {
        fs::write(
            self.home_dir.join(".clawin/settings.json"),
            serde_json::to_vec_pretty(&value).expect("settings json should serialize"),
        )
        .expect("global settings should write");
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

fn initialize_response(id: u64) -> serde_json::Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "protocolVersion": "2025-03-26",
            "capabilities": {
                "tools": {},
                "resources": {}
            },
            "serverInfo": {
                "name": "fake-mcp",
                "version": "0.1.0"
            }
        }
    })
}

fn tools_list_response(id: u64) -> serde_json::Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "tools": [
                {
                    "name": "echo",
                    "description": "Echo text",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "text": { "type": "string" }
                        },
                        "required": ["text"]
                    }
                }
            ]
        }
    })
}

fn resources_list_response(id: u64) -> serde_json::Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "resources": [
                {
                    "uri": "memo://alpha",
                    "name": "alpha",
                    "mimeType": "text/plain"
                }
            ]
        }
    })
}
