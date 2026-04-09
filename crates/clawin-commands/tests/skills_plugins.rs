use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use clawin_commands::builtin_command_registry_with_extensions;
use clawin_config::load_startup_config;
use clawin_core::{PermissionMode, RuntimeCapabilities, SessionId, SessionRuntime};
use clawin_integrations::{McpManager, load_plugins_snapshot, load_skills_snapshot};
use clawin_platform::{FakeProcessSpawner, PathPolicy};
use tempfile::TempDir;

#[test]
fn renders_skills_plugin_and_dynamic_skill_commands() {
    let harness = Harness::new();
    harness.write_skill(
        SkillLocation::Project,
        "review-rust",
        r#"---
name: review-rust
description: Project override review skill
tools:
  - file_read
  - list_mcp_resources
---
# Review Rust
Review the Rust changes carefully.
"#,
    );
    harness.write_plugin_manifest(
        PluginLocation::User,
        "demo-plugin",
        serde_json::json!({
            "name": "demo-plugin",
            "description": "Demo plugin",
            "skills": ["./skills"],
            "mcpServers": {
                "notes": {
                    "command": "/tmp/fake-mcp"
                }
            }
        }),
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

    let config = harness.load_config();
    let skills = load_skills_snapshot(&config);
    let plugins = load_plugins_snapshot(&config);
    let manager = Arc::new(
        McpManager::from_loaded_config(&config, Arc::new(FakeProcessSpawner::default()))
            .expect("empty manager should assemble"),
    );
    manager
        .set_plugin_servers(&plugins)
        .expect("plugin servers should merge");
    let registry = builtin_command_registry_with_extensions(manager, skills, plugins);

    let skills_output = registry
        .execute("/skills", &runtime(&harness.project_dir))
        .expect("/skills should execute");
    assert_eq!(skills_output.command_name, "skills");
    assert_eq!(
        skills_output.output,
        include_str!("fixtures/skills_output.txt")
    );

    let skill_output = registry
        .execute("/review-rust", &runtime(&harness.project_dir))
        .expect("dynamic skill should execute");
    assert_eq!(skill_output.command_name, "review-rust");
    assert_eq!(
        skill_output.output,
        include_str!("fixtures/skill_command_output.txt")
    );

    let plugin_output = registry
        .execute("/plugin", &runtime(&harness.project_dir))
        .expect("/plugin should execute");
    assert_eq!(plugin_output.command_name, "plugin");
    assert_eq!(
        plugin_output.output,
        include_str!("fixtures/plugin_output.txt")
    );

    assert!(registry.spec("demo-plugin:audit").is_some());
    assert_eq!(
        registry
            .spec("demo-plugin:audit")
            .expect("plugin skill spec should exist")
            .origin_label
            .as_deref(),
        Some("plugin:demo-plugin")
    );
}

#[test]
fn executes_normalized_skill_commands_and_keeps_display_name() {
    let harness = Harness::new();
    harness.write_skill(
        SkillLocation::Project,
        "code-review",
        r#"---
name: Code Review
description: Review staged code carefully
tools:
  - file_read
---
# Code Review
Review staged code carefully.
"#,
    );
    harness.write_plugin_manifest(
        PluginLocation::User,
        "demo-plugin",
        serde_json::json!({
            "name": "demo-plugin",
            "description": "Demo plugin",
            "skills": ["./skills"]
        }),
    );
    harness.write_plugin_skill(
        PluginLocation::User,
        "demo-plugin",
        "release-audit",
        r#"---
name: Release Audit
description: Audit the release state
tools:
  - file_read
---
# Release Audit
Audit the release state.
"#,
    );

    let config = harness.load_config();
    let skills = load_skills_snapshot(&config);
    let plugins = load_plugins_snapshot(&config);
    let manager = Arc::new(
        McpManager::from_loaded_config(&config, Arc::new(FakeProcessSpawner::default()))
            .expect("empty manager should assemble"),
    );
    let registry = builtin_command_registry_with_extensions(manager, skills, plugins);

    let skills_listing = registry
        .execute("/skills", &runtime(&harness.project_dir))
        .expect("/skills should render normalized skill labels");
    assert_eq!(
        skills_listing.output,
        include_str!("fixtures/skills_normalized_output.txt")
    );

    let result = registry
        .execute("/code-review", &runtime(&harness.project_dir))
        .expect("normalized skill command should execute");
    assert_eq!(result.command_name, "code-review");
    assert_eq!(
        result.output,
        include_str!("fixtures/skill_command_display_output.txt")
    );

    assert!(registry.spec("demo-plugin:release-audit").is_some());
}

#[test]
fn renders_plugin_precedence_and_ignored_failure_reason() {
    let harness = Harness::new();
    harness.write_raw_plugin_file(PluginLocation::User, "demo-plugin", "plugin.json", b"{}");
    harness.write_plugin_manifest(
        PluginLocation::Project,
        "demo-plugin",
        serde_json::json!({
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
    harness.write_raw_plugin_file(PluginLocation::User, "broken-plugin", "plugin.json", b"{}");

    let config = harness.load_config();
    let skills = load_skills_snapshot(&config);
    let plugins = load_plugins_snapshot(&config);
    let manager = Arc::new(
        McpManager::from_loaded_config(&config, Arc::new(FakeProcessSpawner::default()))
            .expect("empty manager should assemble"),
    );
    let registry = builtin_command_registry_with_extensions(manager, skills, plugins);

    let plugin_output = registry
        .execute("/plugin", &runtime(&harness.project_dir))
        .expect("/plugin should render precedence and ignored failure details");
    assert_eq!(
        plugin_output.output,
        include_str!("fixtures/plugin_precedence_output.txt")
    );

    assert!(registry.spec("demo-plugin:deploy").is_some());
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

    fn load_config(&self) -> clawin_config::LoadedConfigSnapshot {
        load_startup_config(
            self.project_dir.clone(),
            &TestPathPolicy {
                home_dir: self.home_dir.clone(),
            },
        )
        .expect("startup config should load")
    }

    fn write_skill(&self, location: SkillLocation, name: &str, content: &str) {
        let path = match location {
            SkillLocation::Project => self.project_dir.join(".clawin/skills").join(name),
        };
        fs::create_dir_all(&path).expect("skill directory should exist");
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
        fs::create_dir_all(path.parent().expect("parent should exist"))
            .expect("parent dir should exist");
        fs::write(path, content).expect("plugin command should write");
    }

    fn write_raw_plugin_file(
        &self,
        location: PluginLocation,
        plugin_name: &str,
        relative_path: &str,
        content: &[u8],
    ) {
        let path = self.plugin_root(location, plugin_name).join(relative_path);
        fs::create_dir_all(path.parent().expect("parent should exist"))
            .expect("parent dir should exist");
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

fn runtime(project_dir: &std::path::Path) -> SessionRuntime {
    SessionRuntime::new(
        SessionId::from_static("commands-skills-test"),
        RuntimeCapabilities::new(false, true),
        project_dir.to_path_buf(),
        project_dir.to_path_buf(),
        PermissionMode::Default,
    )
}
