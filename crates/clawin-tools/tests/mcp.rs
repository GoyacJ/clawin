use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use clawin_config::load_startup_config;
use clawin_core::{
    PermissionBehavior, PermissionMode, RuntimeCapabilities, SessionId, SessionRuntime, ToolCall,
};
use clawin_integrations::McpManager;
use clawin_platform::{FakeProcessPlan, FakeProcessSpawner, PathPolicy};
use clawin_tools::builtin_tool_registry_with_mcp;
use serde_json::{Value, json};
use tempfile::TempDir;

#[test]
fn lists_cached_mcp_resources() {
    let harness = Harness::new();
    harness.write_global_settings(json!({
        "schema_version": 1,
        "mcpServers": {
            "demo": {
                "command": "/tmp/fake-mcp"
            }
        }
    }));

    let manager = Arc::new(
        McpManager::from_loaded_config(
            &harness.load_config(),
            Arc::new(FakeProcessSpawner::new(vec![
                FakeProcessPlan::from_json_messages(vec![
                    initialize_response(
                        1,
                        json!({
                            "resources": {}
                        }),
                    ),
                    resources_list_response(
                        2,
                        vec![json!({
                            "uri": "memo://alpha",
                            "name": "alpha",
                            "mimeType": "text/plain"
                        })],
                    ),
                ]),
            ])),
        )
        .expect("mcp manager should assemble"),
    );
    let registry = builtin_tool_registry_with_mcp(manager);

    let execution = registry
        .execute(
            ToolCall::new(
                "toolu_001",
                "list_mcp_resources",
                json!({ "server": "demo" }),
            ),
            &harness.runtime(),
        )
        .expect("list_mcp_resources should execute");

    assert_eq!(execution.permission_behavior, PermissionBehavior::Allow);
    assert!(!execution.result.is_error);
    assert_eq!(
        execution.result.content,
        json!({
            "resources": [
                {
                    "server": "demo",
                    "uri": "memo://alpha",
                    "name": "alpha",
                    "mimeType": "text/plain",
                    "description": null
                }
            ]
        })
    );
}

#[test]
fn reads_text_mcp_resource_and_rejects_binary_payloads() {
    let harness = Harness::new();
    harness.write_global_settings(json!({
        "schema_version": 1,
        "mcpServers": {
            "demo": {
                "command": "/tmp/fake-mcp"
            }
        }
    }));

    let manager = Arc::new(
        McpManager::from_loaded_config(
            &harness.load_config(),
            Arc::new(FakeProcessSpawner::new(vec![
                FakeProcessPlan::from_json_messages(vec![
                    initialize_response(
                        1,
                        json!({
                            "resources": {}
                        }),
                    ),
                    resources_list_response(2, vec![]),
                    resource_read_response(
                        3,
                        json!([
                            {
                                "uri": "memo://alpha",
                                "mimeType": "text/plain",
                                "text": "hello from MCP"
                            }
                        ]),
                    ),
                ]),
            ])),
        )
        .expect("mcp manager should assemble"),
    );
    let registry = builtin_tool_registry_with_mcp(manager);

    let execution = registry
        .execute(
            ToolCall::new(
                "toolu_002",
                "read_mcp_resource",
                json!({
                    "server": "demo",
                    "uri": "memo://alpha"
                }),
            ),
            &harness.runtime(),
        )
        .expect("read_mcp_resource should execute");

    assert_eq!(execution.permission_behavior, PermissionBehavior::Allow);
    assert!(!execution.result.is_error);
    assert_eq!(
        execution.result.content,
        json!({
            "server": "demo",
            "uri": "memo://alpha",
            "contents": [
                {
                    "uri": "memo://alpha",
                    "mimeType": "text/plain",
                    "text": "hello from MCP"
                }
            ]
        })
    );

    let binary_manager = Arc::new(
        McpManager::from_loaded_config(
            &harness.load_config(),
            Arc::new(FakeProcessSpawner::new(vec![
                FakeProcessPlan::from_json_messages(vec![
                    initialize_response(
                        1,
                        json!({
                            "resources": {}
                        }),
                    ),
                    resources_list_response(2, vec![]),
                    resource_read_response(
                        3,
                        json!([
                            {
                                "uri": "memo://binary",
                                "mimeType": "application/octet-stream",
                                "blob": "AAEC"
                            }
                        ]),
                    ),
                ]),
            ])),
        )
        .expect("binary manager should assemble"),
    );
    let binary_registry = builtin_tool_registry_with_mcp(binary_manager);

    let binary = binary_registry
        .execute(
            ToolCall::new(
                "toolu_003",
                "read_mcp_resource",
                json!({
                    "server": "demo",
                    "uri": "memo://binary"
                }),
            ),
            &harness.runtime(),
        )
        .expect("binary read should return a structured tool result");

    assert!(binary.result.is_error);
    assert_eq!(binary.result.content["code"], "unsupported_binary_resource");
}

#[test]
fn surfaces_mcp_resource_failures_as_structured_tool_errors() {
    let harness = Harness::new();
    let manager = Arc::new(
        McpManager::from_loaded_config(
            &harness.load_config(),
            Arc::new(FakeProcessSpawner::default()),
        )
        .expect("empty mcp manager should assemble"),
    );
    let registry = builtin_tool_registry_with_mcp(manager);

    let execution = registry
        .execute(
            ToolCall::new(
                "toolu_004",
                "list_mcp_resources",
                json!({
                    "server": "missing"
                }),
            ),
            &harness.runtime(),
        )
        .expect("missing server should return a structured tool result");

    assert!(execution.result.is_error);
    assert_eq!(execution.result.content["code"], "mcp_resource_list_failed");
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
        let project_dir = tempdir.path().join("project");
        fs::create_dir_all(home_dir.join(".clawin")).expect("home dir should exist");
        fs::create_dir_all(&project_dir).expect("project dir should exist");

        Self {
            _tempdir: tempdir,
            home_dir,
            project_dir,
        }
    }

    fn write_global_settings(&self, value: Value) {
        fs::write(
            self.home_dir.join(".clawin/settings.json"),
            serde_json::to_vec_pretty(&value).expect("settings should serialize"),
        )
        .expect("settings should write");
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

    fn runtime(&self) -> SessionRuntime {
        SessionRuntime::new(
            SessionId::from_static("tools-mcp-test"),
            RuntimeCapabilities::new(false, true),
            self.project_dir.clone(),
            self.project_dir.clone(),
            PermissionMode::Default,
        )
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

fn initialize_response(id: u64, capabilities: Value) -> Value {
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

fn resources_list_response(id: u64, resources: Vec<Value>) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "resources": resources
        }
    })
}

fn resource_read_response(id: u64, contents: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "contents": contents
        }
    })
}
