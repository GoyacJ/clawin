use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use clawin_commands::builtin_command_registry_with_mcp;
use clawin_config::load_startup_config;
use clawin_core::{PermissionMode, RuntimeCapabilities, SessionId, SessionRuntime};
use clawin_integrations::McpManager;
use clawin_platform::{FakeProcessPlan, FakeProcessSpawner, PathPolicy};
use serde_json::{Value, json};
use tempfile::TempDir;

#[test]
fn executes_mcp_list_and_reload_commands() {
    let harness = Harness::new();
    harness.write_global_settings(json!({
        "schema_version": 1,
        "mcpServers": {
            "demo": {
                "command": "/tmp/fake-mcp"
            }
        }
    }));

    let spawner = Arc::new(FakeProcessSpawner::new(vec![
        FakeProcessPlan::from_json_messages(vec![
            initialize_response(
                1,
                json!({
                    "tools": {},
                    "resources": {}
                }),
            ),
            tools_list_response(2, vec![]),
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
            tools_list_response(2, vec![]),
            resources_list_response(3, vec![]),
        ]),
    ]));

    let manager = Arc::new(
        McpManager::from_loaded_config(&harness.load_config(), spawner.clone())
            .expect("mcp manager should assemble"),
    );
    let registry = builtin_command_registry_with_mcp(manager);
    let runtime = harness.runtime();

    let list = registry
        .execute("/mcp list", &runtime)
        .expect("/mcp list should execute");
    assert!(list.output.contains("demo"));
    assert!(list.output.contains("connected"));

    let reload = registry
        .execute("/mcp reload", &runtime)
        .expect("/mcp reload should execute");
    assert!(reload.output.contains("MCP servers reloaded"));
    assert_eq!(spawner.invocations().len(), 2);
}

#[test]
fn rejects_unknown_mcp_subcommand() {
    let harness = Harness::new();
    let manager = Arc::new(
        McpManager::from_loaded_config(
            &harness.load_config(),
            Arc::new(FakeProcessSpawner::default()),
        )
        .expect("empty mcp manager should assemble"),
    );
    let registry = builtin_command_registry_with_mcp(manager);

    let error = registry
        .execute("/mcp bad", &harness.runtime())
        .expect_err("unknown /mcp subcommand should fail");

    assert!(matches!(
        error,
        clawin_core::ClawinError::InvalidCommandInvocation { ref message }
            if message == "usage: /mcp [list|reload]"
    ));
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
            SessionId::from_static("commands-mcp-test"),
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

fn tools_list_response(id: u64, tools: Vec<Value>) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "tools": tools
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
