use std::fs;
use std::path::PathBuf;

use clawin_config::load_startup_config;
use clawin_integrations::{
    PluginRuntimeSource, PluginRuntimeStatus, SkillSource, load_plugins_snapshot,
    load_skills_snapshot,
};
use clawin_platform::PathPolicy;
use serde_json::json;
use tempfile::TempDir;

#[test]
fn loads_skills_with_project_override_and_reports_invalid_frontmatter() {
    let harness = Harness::new();
    harness.write_skill(
        SkillLocation::User,
        "review-rust",
        r#"---
name: review-rust
description: User review skill
tools:
  - file_read
---
# Review Rust
Review the global Rust changes carefully.
"#,
    );
    harness.write_skill(
        SkillLocation::Project,
        "review-rust",
        r#"---
name: review-rust
description: Project override review skill
tools:
  - file_read
  - list_mcp_resources
model_trigger: rust
---
# Review Rust
Review the Rust changes carefully.
"#,
    );
    harness.write_skill(
        SkillLocation::Project,
        "broken-skill",
        r#"---
name: broken-skill
description: "missing quote
---
# Broken
"#,
    );

    let snapshot = load_skills_snapshot(&harness.load_config());

    assert_eq!(snapshot.skills().len(), 1);
    let skill = &snapshot.skills()[0];
    assert_eq!(skill.name(), "review-rust");
    assert_eq!(skill.command_name(), "review-rust");
    assert_eq!(skill.description(), "Project override review skill");
    assert_eq!(skill.source(), SkillSource::Project);
    assert_eq!(
        skill.tools(),
        &["file_read".to_owned(), "list_mcp_resources".to_owned()]
    );
    assert!(
        skill
            .markdown()
            .contains("Review the Rust changes carefully.")
    );
    assert_eq!(snapshot.errors().len(), 1);
    assert!(
        snapshot.errors()[0]
            .message()
            .contains("invalid skill frontmatter")
    );
}

#[test]
fn loads_plugins_collects_failures_and_preserves_project_precedence() {
    let harness = Harness::new();
    harness.write_plugin_manifest(
        PluginLocation::User,
        "demo-plugin",
        json!({
            "name": "demo-plugin",
            "description": "User plugin",
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
description: Deploy from user plugin
---
# Deploy
Deploy from the user plugin.
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

    harness.write_raw_plugin_file(PluginLocation::User, "broken-plugin", "plugin.json", b"{}");

    let snapshot = load_plugins_snapshot(&harness.load_config());

    assert_eq!(snapshot.plugins().len(), 3);

    let loaded = snapshot
        .plugins()
        .iter()
        .find(|plugin| {
            plugin.id() == "demo-plugin" && plugin.status() == PluginRuntimeStatus::Loaded
        })
        .expect("loaded plugin should exist");
    assert_eq!(loaded.description(), "Project plugin override");
    assert_eq!(
        loaded.command_names(),
        vec!["demo-plugin:deploy".to_owned()]
    );
    assert!(loaded.skill_command_names().is_empty());
    assert!(loaded.errors().is_empty());

    let ignored = snapshot
        .plugins()
        .iter()
        .find(|plugin| {
            plugin.id() == "demo-plugin" && plugin.status() == PluginRuntimeStatus::Ignored
        })
        .expect("duplicate user plugin should be ignored");
    assert!(ignored.errors()[0].contains("overridden by higher-precedence project plugin"));

    let failed = snapshot
        .plugins()
        .iter()
        .find(|plugin| plugin.id() == "broken-plugin")
        .expect("failed plugin should exist");
    assert_eq!(failed.status(), PluginRuntimeStatus::Failed);
    assert_eq!(
        failed.errors(),
        vec!["invalid plugin manifest: missing or invalid `name`".to_owned()].as_slice()
    );
}

#[test]
fn project_plugin_overrides_failed_user_plugin_with_same_id() {
    let harness = Harness::new();
    harness.write_raw_plugin_file(PluginLocation::User, "demo-plugin", "plugin.json", b"{}");
    harness.write_plugin_manifest(
        PluginLocation::Project,
        "demo-plugin",
        json!({
            "name": "demo-plugin",
            "description": "Project plugin override"
        }),
    );

    let snapshot = load_plugins_snapshot(&harness.load_config());

    let loaded = snapshot
        .plugins()
        .iter()
        .find(|plugin| {
            plugin.id() == "demo-plugin"
                && plugin.source() == PluginRuntimeSource::Project
                && plugin.status() == PluginRuntimeStatus::Loaded
        })
        .expect("project plugin should load");
    assert_eq!(loaded.description(), "Project plugin override");

    let ignored = snapshot
        .plugins()
        .iter()
        .find(|plugin| {
            plugin.id() == "demo-plugin"
                && plugin.source() == PluginRuntimeSource::User
                && plugin.status() == PluginRuntimeStatus::Ignored
        })
        .expect("failed user plugin should be ignored once the project override exists");
    assert!(
        ignored
            .errors()
            .iter()
            .any(|error| error.contains("overridden by higher-precedence project plugin"))
    );
}

#[test]
fn failed_project_plugin_masks_loaded_user_plugin_with_same_id() {
    let harness = Harness::new();
    harness.write_plugin_manifest(
        PluginLocation::User,
        "demo-plugin",
        json!({
            "name": "demo-plugin",
            "description": "User plugin",
            "commands": ["./commands"]
        }),
    );
    harness.write_plugin_command(
        PluginLocation::User,
        "demo-plugin",
        "deploy",
        r#"---
description: Deploy from user plugin
---
# Deploy
Deploy from the user plugin.
"#,
    );
    harness.write_raw_plugin_file(PluginLocation::Project, "demo-plugin", "plugin.json", b"{}");

    let snapshot = load_plugins_snapshot(&harness.load_config());

    let failed = snapshot
        .plugins()
        .iter()
        .find(|plugin| {
            plugin.id() == "demo-plugin"
                && plugin.source() == PluginRuntimeSource::Project
                && plugin.status() == PluginRuntimeStatus::Failed
        })
        .expect("project plugin should remain the active failed entry");
    assert!(
        failed
            .errors()
            .iter()
            .any(|error| error.contains("missing or invalid `name`"))
    );

    let ignored = snapshot
        .plugins()
        .iter()
        .find(|plugin| {
            plugin.id() == "demo-plugin"
                && plugin.source() == PluginRuntimeSource::User
                && plugin.status() == PluginRuntimeStatus::Ignored
        })
        .expect("loaded user plugin should be ignored by the failed project plugin");
    assert!(
        ignored
            .errors()
            .iter()
            .any(|error| error.contains("overridden by higher-precedence project plugin"))
    );
    assert!(snapshot.loaded_commands().is_empty());
}

#[test]
fn normalizes_skill_command_names_and_reports_normalized_duplicates() {
    let harness = Harness::new();
    harness.write_skill(
        SkillLocation::Project,
        "code-review-a",
        r#"---
name: Code Review
description: Review code carefully
tools:
  - file_read
---
# Code Review
Review code carefully.
"#,
    );
    harness.write_skill(
        SkillLocation::Project,
        "code-review-b",
        r#"---
name: Code   Review!
description: Duplicate after normalization
tools:
  - file_read
---
# Code Review
Duplicate after normalization.
"#,
    );

    let snapshot = load_skills_snapshot(&harness.load_config());

    assert_eq!(snapshot.skills().len(), 1);
    let skill = &snapshot.skills()[0];
    assert_eq!(skill.name(), "Code Review");
    assert_eq!(skill.command_name(), "code-review");
    assert!(snapshot.errors().iter().any(|error| {
        error
            .message()
            .contains("duplicate skill command `code-review`")
    }));
}

#[test]
fn plugin_skills_use_normalized_command_tokens() {
    let harness = Harness::new();
    harness.write_plugin_manifest(
        PluginLocation::User,
        "demo-plugin",
        json!({
            "name": "demo-plugin",
            "description": "Demo plugin",
            "skills": ["./skills"]
        }),
    );
    harness.write_plugin_skill(
        PluginLocation::User,
        "demo-plugin",
        "code-review",
        r#"---
name: Code Review
description: Review deployment state
tools:
  - file_read
---
# Code Review
Review deployment state.
"#,
    );

    let snapshot = load_plugins_snapshot(&harness.load_config());

    let plugin = snapshot
        .plugins()
        .iter()
        .find(|plugin| plugin.id() == "demo-plugin")
        .expect("plugin should exist");
    assert_eq!(
        plugin.skill_command_names(),
        vec!["demo-plugin:code-review".to_owned()]
    );
    assert_eq!(plugin.skills()[0].name(), "Code Review");
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
            SkillLocation::User => self.home_dir.join(".clawin/skills").join(name),
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
            serde_json::to_vec_pretty(&value).expect("plugin manifest should serialize"),
        )
        .expect("plugin manifest should write");
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
        fs::create_dir_all(path.parent().expect("command dir exists"))
            .expect("command dir should exist");
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
    User,
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
