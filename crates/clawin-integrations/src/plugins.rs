use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use clawin_config::LoadedConfigSnapshot;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::McpConfigScope;
use crate::skills::{
    LoadedSkill, SKILLS_DIRECTORY_NAME, SkillSource, load_skills_from_root,
    normalize_markdown_body as normalize_skill_markdown,
};

pub const PLUGINS_DIRECTORY_NAME: &str = "plugins";
const PLUGIN_MANIFEST_FILE_NAME: &str = "plugin.json";
const COMMANDS_DIRECTORY_NAME: &str = "commands";

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginRuntimeSource {
    User,
    Project,
}

impl PluginRuntimeSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Project => "project",
        }
    }

    fn precedence(self) -> u8 {
        match self {
            Self::User => 0,
            Self::Project => 1,
        }
    }

    fn to_mcp_scope(self) -> McpConfigScope {
        match self {
            Self::User => McpConfigScope::User,
            Self::Project => McpConfigScope::Project,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginRuntimeStatus {
    Loaded,
    Failed,
    Ignored,
}

impl PluginRuntimeStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Loaded => "loaded",
            Self::Failed => "failed",
            Self::Ignored => "ignored",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct LoadedPluginCommand {
    name: String,
    description: String,
    markdown: String,
    plugin_id: String,
    path: PathBuf,
}

impl LoadedPluginCommand {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn description(&self) -> &str {
        &self.description
    }

    pub fn markdown(&self) -> &str {
        &self.markdown
    }

    pub fn plugin_id(&self) -> &str {
        &self.plugin_id
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PluginMcpServerEntry {
    name: String,
    scope: McpConfigScope,
    value: Value,
}

impl PluginMcpServerEntry {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn scope(&self) -> McpConfigScope {
        self.scope
    }

    pub fn value(&self) -> &Value {
        &self.value
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct LoadedPlugin {
    id: String,
    description: String,
    source: PluginRuntimeSource,
    status: PluginRuntimeStatus,
    root_path: PathBuf,
    commands: Vec<LoadedPluginCommand>,
    skills: Vec<LoadedSkill>,
    mcp_servers: Vec<PluginMcpServerEntry>,
    errors: Vec<String>,
}

impl LoadedPlugin {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn description(&self) -> &str {
        &self.description
    }

    pub fn source(&self) -> PluginRuntimeSource {
        self.source
    }

    pub fn status(&self) -> PluginRuntimeStatus {
        self.status
    }

    pub fn command_names(&self) -> Vec<String> {
        self.commands
            .iter()
            .map(|command| command.name.clone())
            .collect()
    }

    pub fn skill_command_names(&self) -> Vec<String> {
        self.skills
            .iter()
            .map(|skill| skill.command_name().to_owned())
            .collect()
    }

    pub fn mcp_server_names(&self) -> Vec<String> {
        self.mcp_servers
            .iter()
            .map(|entry| entry.name.clone())
            .collect()
    }

    pub fn commands(&self) -> &[LoadedPluginCommand] {
        &self.commands
    }

    pub fn skills(&self) -> &[LoadedSkill] {
        &self.skills
    }

    pub fn mcp_servers(&self) -> &[PluginMcpServerEntry] {
        &self.mcp_servers
    }

    pub fn errors(&self) -> &[String] {
        &self.errors
    }

    pub fn primary_error(&self) -> Option<&str> {
        match self.status {
            PluginRuntimeStatus::Ignored => self.errors.last().map(String::as_str),
            _ => self.errors.first().map(String::as_str),
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct LoadedPluginsSnapshot {
    plugins: Vec<LoadedPlugin>,
}

impl LoadedPluginsSnapshot {
    pub fn new(mut plugins: Vec<LoadedPlugin>) -> Self {
        plugins.sort_by(|left, right| {
            left.id
                .cmp(&right.id)
                .then_with(|| left.status.as_str().cmp(right.status.as_str()))
        });
        Self { plugins }
    }

    pub fn plugins(&self) -> &[LoadedPlugin] {
        &self.plugins
    }

    pub fn loaded_skills(&self) -> Vec<LoadedSkill> {
        self.plugins
            .iter()
            .filter(|plugin| plugin.status == PluginRuntimeStatus::Loaded)
            .flat_map(|plugin| plugin.skills.clone())
            .collect()
    }

    pub fn loaded_commands(&self) -> Vec<LoadedPluginCommand> {
        self.plugins
            .iter()
            .filter(|plugin| plugin.status == PluginRuntimeStatus::Loaded)
            .flat_map(|plugin| plugin.commands.clone())
            .collect()
    }

    pub fn mcp_server_entries(&self) -> Vec<PluginMcpServerEntry> {
        self.plugins
            .iter()
            .filter(|plugin| plugin.status == PluginRuntimeStatus::Loaded)
            .flat_map(|plugin| plugin.mcp_servers.clone())
            .collect()
    }
}

pub fn load_plugins_snapshot(snapshot: &LoadedConfigSnapshot) -> LoadedPluginsSnapshot {
    let mut plugins: Vec<LoadedPlugin> = Vec::new();
    let mut loaded_by_id = BTreeMap::<String, usize>::new();

    let user_root = snapshot.paths().global_root().join(PLUGINS_DIRECTORY_NAME);
    let project_root = snapshot
        .paths()
        .project_directory()
        .join(PLUGINS_DIRECTORY_NAME);

    for (source, root) in [
        (PluginRuntimeSource::User, user_root),
        (PluginRuntimeSource::Project, project_root),
    ] {
        let mut roots = Vec::new();
        discover_plugin_roots(&root, &mut roots);
        roots.sort();

        for plugin_root in roots {
            let mut plugin = load_plugin_from_root(&plugin_root, source);
            if let Some(previous_index) = loaded_by_id.get(plugin.id()).copied() {
                let previous_source = plugins[previous_index].source;
                if source.precedence() > previous_source.precedence() {
                    ignore_plugin(
                        &mut plugins[previous_index],
                        "overridden by higher-precedence project plugin",
                    );
                    loaded_by_id.insert(plugin.id.clone(), plugins.len());
                } else {
                    ignore_plugin(&mut plugin, "duplicate plugin id already loaded");
                }
            } else {
                loaded_by_id.insert(plugin.id.clone(), plugins.len());
            }

            plugins.push(plugin);
        }
    }

    LoadedPluginsSnapshot::new(plugins)
}

fn ignore_plugin(plugin: &mut LoadedPlugin, reason: &str) {
    plugin.status = PluginRuntimeStatus::Ignored;
    plugin.commands.clear();
    plugin.skills.clear();
    plugin.mcp_servers.clear();
    plugin.errors.push(reason.to_owned());
}

fn load_plugin_from_root(root: &Path, source: PluginRuntimeSource) -> LoadedPlugin {
    let fallback_id = root
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown-plugin".to_owned());
    let manifest_path = root.join(PLUGIN_MANIFEST_FILE_NAME);

    let manifest_value = match fs::read_to_string(&manifest_path) {
        Ok(content) => match serde_json::from_str::<Value>(&content) {
            Ok(value) => value,
            Err(_) => {
                return failed_plugin(
                    fallback_id,
                    source,
                    root,
                    "invalid plugin manifest: invalid JSON".to_owned(),
                );
            }
        },
        Err(error) => {
            return failed_plugin(
                fallback_id,
                source,
                root,
                format!("invalid plugin manifest: {error}"),
            );
        }
    };

    let Some(object) = manifest_value.as_object() else {
        return failed_plugin(
            fallback_id,
            source,
            root,
            "invalid plugin manifest: top-level JSON object is required".to_owned(),
        );
    };

    let Some(name) = object
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(ToOwned::to_owned)
    else {
        return failed_plugin(
            fallback_id,
            source,
            root,
            "invalid plugin manifest: missing or invalid `name`".to_owned(),
        );
    };

    let description = object
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_owned();

    let command_paths = match parse_relative_paths(object.get("commands"), "commands") {
        Ok(Some(paths)) => paths,
        Ok(None) => default_plugin_paths(root, COMMANDS_DIRECTORY_NAME),
        Err(message) => return failed_plugin(name, source, root, message),
    };
    let skill_paths = match parse_relative_paths(object.get("skills"), "skills") {
        Ok(Some(paths)) => paths,
        Ok(None) => default_plugin_paths(root, SKILLS_DIRECTORY_NAME),
        Err(message) => return failed_plugin(name, source, root, message),
    };
    let raw_mcp_servers = match parse_mcp_servers(object.get("mcpServers")) {
        Ok(value) => value,
        Err(message) => return failed_plugin(name, source, root, message),
    };

    let mut errors = Vec::new();
    let mut names = BTreeSet::new();

    let mut commands = Vec::new();
    for relative_path in command_paths {
        load_commands_from_path(root, &name, &relative_path, &mut commands, &mut errors);
    }
    commands.sort_by(|left, right| left.name.cmp(&right.name));
    commands.retain(|command| {
        if names.insert(command.name.clone()) {
            true
        } else {
            errors.push(format!("duplicate plugin command `{}`", command.name));
            false
        }
    });

    let mut skills = Vec::new();
    for relative_path in skill_paths {
        let plugin_skill_root = root.join(&relative_path);
        let (discovered, discovered_errors) =
            load_skills_from_root(&plugin_skill_root, SkillSource::Plugin, Some(&name));
        skills.extend(discovered);
        errors.extend(
            discovered_errors
                .into_iter()
                .map(|error| format!("{} ({})", error.message(), error.path().display())),
        );
    }
    skills.sort_by(|left, right| left.command_name().cmp(right.command_name()));
    skills.retain(|skill| {
        if names.insert(skill.command_name().to_owned()) {
            true
        } else {
            errors.push(format!(
                "duplicate plugin contribution `{}`",
                skill.command_name()
            ));
            false
        }
    });

    let mut mcp_servers = raw_mcp_servers
        .into_iter()
        .map(|(server_name, value)| PluginMcpServerEntry {
            name: namespaced_plugin_server_name(&name, &server_name),
            scope: source.to_mcp_scope(),
            value,
        })
        .collect::<Vec<_>>();
    mcp_servers.sort_by(|left, right| left.name.cmp(&right.name));

    LoadedPlugin {
        id: name,
        description,
        source,
        status: PluginRuntimeStatus::Loaded,
        root_path: root.to_path_buf(),
        commands,
        skills,
        mcp_servers,
        errors,
    }
}

fn failed_plugin(
    id: String,
    source: PluginRuntimeSource,
    root: &Path,
    message: String,
) -> LoadedPlugin {
    LoadedPlugin {
        id,
        description: String::new(),
        source,
        status: PluginRuntimeStatus::Failed,
        root_path: root.to_path_buf(),
        commands: Vec::new(),
        skills: Vec::new(),
        mcp_servers: Vec::new(),
        errors: vec![message],
    }
}

fn parse_relative_paths(
    value: Option<&Value>,
    field: &str,
) -> Result<Option<Vec<PathBuf>>, String> {
    let Some(value) = value else {
        return Ok(None);
    };

    let values = match value {
        Value::String(value) => vec![value.clone()],
        Value::Array(values) => values
            .iter()
            .map(|value| {
                value.as_str().map(ToOwned::to_owned).ok_or_else(|| {
                    format!(
                        "invalid plugin manifest: `{field}` must be a string or array of strings"
                    )
                })
            })
            .collect::<Result<Vec<_>, _>>()?,
        _ => {
            return Err(format!(
                "invalid plugin manifest: `{field}` must be a string or array of strings"
            ));
        }
    };

    let mut paths = Vec::new();
    for value in values {
        if value.is_empty() {
            continue;
        }
        if Path::new(&value).is_absolute() {
            return Err(format!(
                "invalid plugin manifest: `{field}` entries must be relative paths"
            ));
        }
        let relative = value.strip_prefix("./").unwrap_or(&value);
        paths.push(PathBuf::from(relative));
    }

    Ok(Some(paths))
}

fn default_plugin_paths(root: &Path, dir_name: &str) -> Vec<PathBuf> {
    let candidate = root.join(dir_name);
    if candidate.is_dir() {
        vec![PathBuf::from(dir_name)]
    } else {
        Vec::new()
    }
}

fn parse_mcp_servers(value: Option<&Value>) -> Result<BTreeMap<String, Value>, String> {
    let Some(value) = value else {
        return Ok(BTreeMap::new());
    };
    let Some(object) = value.as_object() else {
        return Err("invalid plugin manifest: `mcpServers` must be an object".to_owned());
    };
    Ok(object
        .iter()
        .map(|(name, value)| (name.clone(), value.clone()))
        .collect())
}

fn discover_plugin_roots(root: &Path, plugin_roots: &mut Vec<PathBuf>) {
    let Ok(metadata) = fs::metadata(root) else {
        return;
    };
    if !metadata.is_dir() {
        return;
    }

    if root.join(PLUGIN_MANIFEST_FILE_NAME).is_file() {
        plugin_roots.push(root.to_path_buf());
        return;
    }

    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    let mut children = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect::<Vec<_>>();
    children.sort();

    for child in children {
        discover_plugin_roots(&child, plugin_roots);
    }
}

fn load_commands_from_path(
    plugin_root: &Path,
    plugin_id: &str,
    relative_path: &Path,
    commands: &mut Vec<LoadedPluginCommand>,
    errors: &mut Vec<String>,
) {
    let target = plugin_root.join(relative_path);
    if !target.exists() {
        errors.push(format!(
            "missing plugin command path `{}`",
            relative_path.display()
        ));
        return;
    }

    let mut files = Vec::new();
    collect_markdown_files(&target, &mut files);
    files.sort();

    for file in files {
        match load_command_file(plugin_root, plugin_id, &file) {
            Ok(command) => commands.push(command),
            Err(message) => errors.push(message),
        }
    }
}

fn collect_markdown_files(target: &Path, files: &mut Vec<PathBuf>) {
    if target.is_file() {
        if target.extension().is_some_and(|ext| ext == "md") {
            files.push(target.to_path_buf());
        }
        return;
    }

    let Ok(entries) = fs::read_dir(target) else {
        return;
    };
    let mut children = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
    children.sort();
    for child in children {
        if child.is_dir() {
            collect_markdown_files(&child, files);
        } else if child.extension().is_some_and(|ext| ext == "md") {
            files.push(child);
        }
    }
}

fn load_command_file(
    plugin_root: &Path,
    plugin_id: &str,
    path: &Path,
) -> Result<LoadedPluginCommand, String> {
    let content = fs::read_to_string(path).map_err(|error| {
        format!(
            "failed to read plugin command `{}`: {error}",
            path.display()
        )
    })?;
    let (description_override, markdown) =
        split_optional_frontmatter(&content).map_err(|error| {
            format!(
                "invalid plugin command frontmatter `{}`: {error}",
                path.display()
            )
        })?;
    let markdown = normalize_skill_markdown(&markdown).ok_or_else(|| {
        format!(
            "invalid plugin command markdown `{}`: body is empty",
            path.display()
        )
    })?;
    let suffix = command_suffix(plugin_root, path).ok_or_else(|| {
        format!(
            "failed to derive plugin command name from `{}`",
            path.display()
        )
    })?;
    let description = description_override
        .or_else(|| first_markdown_line(&markdown))
        .unwrap_or_else(|| "Plugin command".to_owned());

    Ok(LoadedPluginCommand {
        name: format!("{plugin_id}:{suffix}"),
        description,
        markdown,
        plugin_id: plugin_id.to_owned(),
        path: path.to_path_buf(),
    })
}

fn split_optional_frontmatter(content: &str) -> Result<(Option<String>, String), String> {
    let normalized = content.replace("\r\n", "\n");
    let Some(stripped) = normalized.strip_prefix("---\n") else {
        return Ok((None, normalized));
    };

    let Some(marker_index) = stripped
        .find("\n---\n")
        .or_else(|| stripped.strip_suffix("\n---").map(|value| value.len()))
    else {
        return Err("missing closing `---` marker".to_owned());
    };

    let frontmatter = &stripped[..marker_index];
    let markdown = if stripped[marker_index..].starts_with("\n---\n") {
        stripped[(marker_index + 5)..].to_owned()
    } else {
        String::new()
    };

    let value: Value = serde_yaml::from_str(frontmatter)
        .map_err(|error| format!("failed to parse YAML: {error}"))?;
    let description = value
        .get("description")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    Ok((description, markdown))
}

fn command_suffix(plugin_root: &Path, path: &Path) -> Option<String> {
    let relative = path.strip_prefix(plugin_root).ok()?;
    let mut parts = relative
        .components()
        .map(|component| component.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<_>>();

    let file_name = parts.pop()?;
    let stem = Path::new(&file_name)
        .file_stem()
        .map(|value| value.to_string_lossy().to_string())?;
    if stem.eq_ignore_ascii_case("SKILL") {
        let parent = parts.pop()?;
        if !parts.is_empty() && parts[0] == SKILLS_DIRECTORY_NAME {
            parts.remove(0);
        }
        if !parts.is_empty() && parts[0] == COMMANDS_DIRECTORY_NAME {
            parts.remove(0);
        }
        parts.push(parent);
    } else {
        if !parts.is_empty() && parts[0] == COMMANDS_DIRECTORY_NAME {
            parts.remove(0);
        }
        parts.push(stem);
    }

    Some(parts.join(":"))
}

fn first_markdown_line(markdown: &str) -> Option<String> {
    markdown
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(|line| line.trim_start_matches('#').trim().to_owned())
        .filter(|line| !line.is_empty())
}

pub(crate) fn namespaced_plugin_server_name(plugin_id: &str, server_name: &str) -> String {
    format!("plugin:{plugin_id}:{server_name}")
}
