use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use clawin_config::load_startup_config;
use clawin_integrations::McpManager;
use clawin_platform::{PathPolicy, SystemProcessSpawner};
use serde_json::json;
use tempfile::TempDir;

#[test]
fn connects_to_real_fake_stdio_server_binary() {
    let harness = Harness::new();
    harness.write_global_settings(json!({
        "schema_version": 1,
        "mcpServers": {
            "demo": {
                "command": env!("CARGO_BIN_EXE_fake_stdio_mcp")
            }
        }
    }));

    let manager =
        McpManager::from_loaded_config(&harness.load_config(), Arc::new(SystemProcessSpawner))
            .expect("real fake stdio server should connect");

    let servers = manager.server_snapshots();
    assert_eq!(servers.len(), 1);
    assert_eq!(servers[0].status_label(), "connected");
    assert_eq!(
        manager
            .tool_specs()
            .into_iter()
            .map(|spec| spec.name)
            .collect::<Vec<_>>(),
        vec!["mcp__demo__echo".to_owned()]
    );
    assert_eq!(
        manager
            .list_resources(Some("demo"))
            .expect("resources should list")
            .len(),
        1
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
        let project_dir = tempdir.path().join("project");
        fs::create_dir_all(home_dir.join(".clawin")).expect("home dir should exist");
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
