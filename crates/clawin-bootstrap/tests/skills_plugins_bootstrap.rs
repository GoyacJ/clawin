use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use clawin_bootstrap::bootstrap_session_from_with_process_spawner;
use clawin_integrations::normalize_name_for_mcp;
use clawin_platform::{
    FakeProcessPlan, FakeProcessSpawner, PathPolicy, StaticTerminalCapabilities,
};
use serde_json::json;
use tempfile::TempDir;

#[test]
fn bootstrap_assembles_skills_plugins_and_plugin_mcp_servers() {
    let harness = Harness::new();
    harness.write_skill(
        SkillLocation::Project,
        "review-rust",
        r#"---
name: review-rust
description: Project override review skill
tools:
  - file_read
---
# Review Rust
Review the Rust changes carefully.
"#,
    );
    harness.write_plugin_manifest(
        PluginLocation::User,
        "demo-plugin",
        json!({
            "name": "demo-plugin",
            "description": "Demo plugin",
            "commands": ["./commands"],
            "skills": ["./skills"],
            "mcpServers": {
                "notes": {
                    "command": "/tmp/fake-mcp"
                }
            }
        }),
    );
    harness.write_plugin_command(
        PluginLocation::User,
        "demo-plugin",
        "deploy",
        r#"---
description: Deploy from plugin
---
# Deploy
Deploy from plugin.
"#,
    );
    harness.write_plugin_skill(
        PluginLocation::User,
        "demo-plugin",
        "audit",
        r#"---
name: audit
description: Audit deployment state
tools:
  - file_read
---
# Audit
Audit deployment state.
"#,
    );
    harness.write_raw_plugin_file(PluginLocation::User, "broken-plugin", "plugin.json", b"{}");

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
    .expect("bootstrap should assemble skills/plugins");

    assert!(session.commands().spec("skills").is_some());
    assert!(session.commands().spec("plugin").is_some());
    assert!(session.commands().spec("review-rust").is_some());
    assert!(session.commands().spec("demo-plugin:deploy").is_some());
    assert!(session.commands().spec("demo-plugin:audit").is_some());
    assert_eq!(session.skills().skills().len(), 2);
    assert_eq!(session.plugins().plugins().len(), 2);
    assert!(session.runtime().capabilities().mcp_available());

    let mcp_server = "plugin:demo-plugin:notes";
    let dynamic_tool_name = format!("mcp__{}__echo", normalize_name_for_mcp(mcp_server));
    assert!(session.tools().spec(&dynamic_tool_name).is_some());

    let mcp_output = session
        .commands()
        .execute("/mcp list", session.runtime())
        .expect("/mcp list should render");
    assert!(mcp_output.output.contains(mcp_server));

    let skills_output = session
        .commands()
        .execute("/skills", session.runtime())
        .expect("/skills should render");
    assert!(skills_output.output.contains("review-rust"));
    assert!(skills_output.output.contains("demo-plugin:audit"));

    let plugin_output = session
        .commands()
        .execute("/plugin", session.runtime())
        .expect("/plugin should render");
    assert!(plugin_output.output.contains("demo-plugin"));
    assert!(plugin_output.output.contains("broken-plugin"));
    assert!(plugin_output.output.contains("failed"));
}

#[test]
fn bootstrap_starts_with_project_plugin_override_and_surfaces_ignored_user_entry() {
    let harness = Harness::new();
    harness.write_raw_plugin_file(PluginLocation::User, "demo-plugin", "plugin.json", b"{}");
    harness.write_plugin_manifest(
        PluginLocation::Project,
        "demo-plugin",
        json!({
            "name": "demo-plugin",
            "description": "Project plugin override",
            "commands": ["./commands"]
        }),
    );
    harness.write_plugin_command(
        PluginLocation::Project,
        "demo-plugin",
        "deploy",
        r#"---
description: Deploy from project plugin
---
# Deploy
Deploy from the project plugin.
"#,
    );

    let session = bootstrap_session_from_with_process_spawner(
        harness.project_dir.clone(),
        StaticTerminalCapabilities::new(false, false),
        TestPathPolicy {
            home_dir: harness.home_dir.clone(),
        },
        Arc::new(FakeProcessSpawner::default()),
    )
    .expect("bootstrap should keep project plugin precedence without blocking startup");

    assert!(session.commands().spec("demo-plugin:deploy").is_some());

    let plugin_output = session
        .commands()
        .execute("/plugin", session.runtime())
        .expect("/plugin should render");
    assert!(
        plugin_output
            .output
            .contains("scope=user status=ignored commands=0 skills=0 mcp_servers=0 error=overridden by higher-precedence project plugin")
    );
    assert!(
        plugin_output
            .output
            .contains("scope=project status=loaded commands=1 skills=0 mcp_servers=0")
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

    fn write_skill(&self, location: SkillLocation, name: &str, content: &str) {
        let path = match location {
            SkillLocation::Project => self.project_dir.join(".clawin/skills").join(name),
        };
        fs::create_dir_all(&path).expect("skill dir should exist");
        fs::write(path.join("SKILL.md"), content).expect("skill file should write");
    }

    fn write_plugin_manifest(
        &self,
        location: PluginLocation,
        name: &str,
        value: serde_json::Value,
    ) {
        let root = self.plugin_root(location, name);
        fs::create_dir_all(&root).expect("plugin root should exist");
        fs::write(
            root.join("plugin.json"),
            serde_json::to_vec_pretty(&value).expect("manifest should serialize"),
        )
        .expect("manifest should write");
    }

    fn write_plugin_command(
        &self,
        location: PluginLocation,
        plugin_name: &str,
        command_name: &str,
        content: &str,
    ) {
        let path = self
            .plugin_root(location, plugin_name)
            .join("commands")
            .join(format!("{command_name}.md"));
        fs::create_dir_all(path.parent().expect("parent exists")).expect("command dir exists");
        fs::write(path, content).expect("command file should write");
    }

    fn write_plugin_skill(
        &self,
        location: PluginLocation,
        plugin_name: &str,
        skill_name: &str,
        content: &str,
    ) {
        let path = self
            .plugin_root(location, plugin_name)
            .join("skills")
            .join(skill_name);
        fs::create_dir_all(&path).expect("plugin skill dir should exist");
        fs::write(path.join("SKILL.md"), content).expect("plugin skill should write");
    }

    fn write_raw_plugin_file(
        &self,
        location: PluginLocation,
        plugin_name: &str,
        relative_path: &str,
        content: &[u8],
    ) {
        let path = self.plugin_root(location, plugin_name).join(relative_path);
        fs::create_dir_all(path.parent().expect("parent exists")).expect("parent should exist");
        fs::write(path, content).expect("raw plugin file should write");
    }

    fn plugin_root(&self, location: PluginLocation, name: &str) -> PathBuf {
        match location {
            PluginLocation::User => self.home_dir.join(".clawin/plugins").join(name),
            PluginLocation::Project => self.project_dir.join(".clawin/plugins").join(name),
        }
    }
}

#[derive(Clone, Copy)]
enum SkillLocation {
    Project,
}

#[derive(Clone, Copy)]
enum PluginLocation {
    User,
    Project,
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
            "resources": []
        }
    })
}
