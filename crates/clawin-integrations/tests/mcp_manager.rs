use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use clawin_config::load_startup_config;
use clawin_integrations::{McpManager, normalize_name_for_mcp};
use clawin_platform::{FakeProcessPlan, FakeProcessSpawner, PathPolicy};
use serde_json::json;
use tempfile::TempDir;

#[test]
fn rejects_non_object_top_level_mcp_servers() {
    let harness = Harness::new();
    harness.write_global_settings(json!({
        "schema_version": 1,
        "mcpServers": []
    }));
    let snapshot = harness.load_config();

    let error = McpManager::from_loaded_config(&snapshot, Arc::new(FakeProcessSpawner::default()))
        .expect_err("non-object mcpServers should fail before bootstrap");

    assert!(error.to_string().contains("mcpServers"));
    assert!(error.to_string().contains("object"));
}

#[test]
fn merges_project_scope_over_global_scope_and_expands_environment_values() {
    let harness = Harness::new();
    harness.write_global_settings(json!({
        "schema_version": 1,
        "mcpServers": {
            "demo": {
                "command": "$FAKE_MCP_BIN",
                "args": ["--server", "${SERVER_MODE}"]
            },
            "http-only": {
                "type": "http",
                "url": "https://example.com/mcp"
            }
        }
    }));
    harness.write_project_settings(json!({
        "schema_version": 1,
        "mcpServers": {
            "demo": {
                "command": "$FAKE_MCP_BIN",
                "args": ["--project-override"],
                "env": {
                    "PROJECT_ROOT": "${PROJECT_ROOT}"
                }
            }
        }
    }));

    let snapshot = harness.load_config();
    let spawner = Arc::new(FakeProcessSpawner::new(vec![
        FakeProcessPlan::from_json_messages(vec![
            initialize_response(
                1,
                json!({
                    "tools": {},
                    "resources": {}
                }),
            ),
            tools_list_response(
                2,
                vec![json!({
                    "name": "echo",
                    "description": "Echo text",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "text": { "type": "string" }
                        },
                        "required": ["text"]
                    }
                })],
            ),
            resources_list_response(
                3,
                vec![json!({
                    "uri": "memo://alpha",
                    "name": "alpha",
                    "mimeType": "text/plain"
                })],
            ),
        ]),
    ]));

    set_env_var("FAKE_MCP_BIN", "/tmp/fake-mcp");
    set_env_var("SERVER_MODE", "global");
    set_env_var(
        "PROJECT_ROOT",
        harness.project_dir.to_string_lossy().as_ref(),
    );

    let manager = McpManager::from_loaded_config(&snapshot, spawner.clone())
        .expect("manager should accept mixed server config");
    let servers = manager.server_snapshots();

    assert_eq!(servers.len(), 2);
    assert!(servers.iter().any(|server| {
        server.name == "demo"
            && server.scope_label() == "project"
            && server.status_label() == "connected"
            && server.tool_count == 1
            && server.resource_count == 1
    }));
    assert!(servers.iter().any(|server| {
        server.name == "http-only"
            && server.scope_label() == "user"
            && server.status_label() == "failed"
            && server
                .last_error
                .as_deref()
                .is_some_and(|message| message.contains("stdio"))
    }));

    let invocations = spawner.invocations();
    assert_eq!(invocations.len(), 1);
    assert_eq!(invocations[0].command, "/tmp/fake-mcp");
    assert_eq!(invocations[0].args, vec!["--project-override"]);
    assert_eq!(
        invocations[0].env.get("PROJECT_ROOT"),
        Some(&harness.project_dir.to_string_lossy().to_string())
    );

    let tool_names = manager
        .tool_specs()
        .into_iter()
        .map(|spec| spec.name)
        .collect::<Vec<_>>();
    assert_eq!(
        tool_names,
        vec![format!("mcp__{}__echo", normalize_name_for_mcp("demo"))]
    );
}

#[test]
fn reload_refreshes_discovered_tools_without_rebuilding_manager() {
    let harness = Harness::new();
    harness.write_global_settings(json!({
        "schema_version": 1,
        "mcpServers": {
            "demo": {
                "command": "/tmp/fake-mcp"
            }
        }
    }));
    let snapshot = harness.load_config();
    let spawner = Arc::new(FakeProcessSpawner::new(vec![
        FakeProcessPlan::from_json_messages(vec![
            initialize_response(
                1,
                json!({
                    "tools": {},
                    "resources": {}
                }),
            ),
            tools_list_response(
                2,
                vec![json!({
                    "name": "echo",
                    "inputSchema": { "type": "object" }
                })],
            ),
            resources_list_response(3, vec![]),
        ]),
        FakeProcessPlan::from_json_messages(vec![
            initialize_response(
                1,
                json!({
                    "tools": {},
                    "resources": {}
                }),
            ),
            tools_list_response(
                2,
                vec![json!({
                    "name": "echo_v2",
                    "inputSchema": { "type": "object" }
                })],
            ),
            resources_list_response(3, vec![]),
        ]),
    ]));
    let manager =
        McpManager::from_loaded_config(&snapshot, spawner).expect("manager should assemble");

    assert_eq!(
        manager
            .tool_specs()
            .into_iter()
            .map(|spec| spec.name)
            .collect::<Vec<_>>(),
        vec!["mcp__demo__echo".to_owned()]
    );

    manager.reload().expect("reload should succeed");

    assert_eq!(
        manager
            .tool_specs()
            .into_iter()
            .map(|spec| spec.name)
            .collect::<Vec<_>>(),
        vec!["mcp__demo__echo_v2".to_owned()]
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
        fs::create_dir_all(home_dir.join(".clawin")).expect("global dir should exist");
        fs::create_dir_all(project_dir.join(".clawin")).expect("project dir should exist");

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

    fn write_project_settings(&self, value: serde_json::Value) {
        fs::write(
            self.project_dir.join(".clawin/settings.json"),
            serde_json::to_vec_pretty(&value).expect("settings json should serialize"),
        )
        .expect("project settings should write");
    }

    fn load_config(&self) -> clawin_config::LoadedConfigSnapshot {
        load_startup_config(
            self.project_dir.clone(),
            &TestPathPolicy {
                home_dir: self.home_dir.clone(),
            },
        )
        .expect("startup config should load")
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

fn set_env_var(key: &str, value: &str) {
    // Integration tests mutate process env before any MCP manager work begins.
    unsafe {
        std::env::set_var(key, value);
    }
}

fn initialize_response(id: u64, capabilities: serde_json::Value) -> serde_json::Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "protocolVersion": "2025-03-26",
            "capabilities": capabilities,
            "serverInfo": {
                "name": "fake-mcp",
                "version": "0.1.0"
            }
        }
    })
}

fn tools_list_response(id: u64, tools: Vec<serde_json::Value>) -> serde_json::Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "tools": tools
        }
    })
}

fn resources_list_response(id: u64, resources: Vec<serde_json::Value>) -> serde_json::Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "resources": resources
        }
    })
}
