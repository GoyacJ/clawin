use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use clawin_config::LoadedConfigSnapshot;
use serde::{Deserialize, Serialize};
use serde_yaml::Value as YamlValue;

pub const SKILLS_DIRECTORY_NAME: &str = "skills";
const SKILL_ENTRYPOINT_FILE_NAME: &str = "SKILL.md";

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillSource {
    User,
    Project,
    Plugin,
}

impl SkillSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Project => "project",
            Self::Plugin => "plugin",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct LoadedSkill {
    name: String,
    command_name: String,
    description: String,
    tools: Vec<String>,
    markdown: String,
    source: SkillSource,
    plugin_id: Option<String>,
    origin_label: String,
    path: PathBuf,
}

impl LoadedSkill {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn command_name(&self) -> &str {
        &self.command_name
    }

    pub fn description(&self) -> &str {
        &self.description
    }

    pub fn tools(&self) -> &[String] {
        &self.tools
    }

    pub fn markdown(&self) -> &str {
        &self.markdown
    }

    pub fn source(&self) -> SkillSource {
        self.source
    }

    pub fn plugin_id(&self) -> Option<&str> {
        self.plugin_id.as_deref()
    }

    pub fn origin_label(&self) -> &str {
        &self.origin_label
    }

    pub fn source_label(&self) -> String {
        match self.plugin_id() {
            Some(plugin_id) => format!("plugin({plugin_id})"),
            None => self.source.as_str().to_owned(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SkillLoadError {
    path: PathBuf,
    message: String,
}

impl SkillLoadError {
    pub fn new(path: PathBuf, message: impl Into<String>) -> Self {
        Self {
            path,
            message: message.into(),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct LoadedSkillsSnapshot {
    skills: Vec<LoadedSkill>,
    errors: Vec<SkillLoadError>,
}

impl LoadedSkillsSnapshot {
    pub fn from_parts(skills: Vec<LoadedSkill>, errors: Vec<SkillLoadError>) -> Self {
        let mut skills = skills;
        skills.sort_by(|left, right| left.command_name.cmp(&right.command_name));
        Self { skills, errors }
    }

    pub fn skills(&self) -> &[LoadedSkill] {
        &self.skills
    }

    pub fn errors(&self) -> &[SkillLoadError] {
        &self.errors
    }
}

#[derive(Debug, Deserialize)]
struct RawSkillFrontmatter {
    name: Option<String>,
    description: Option<String>,
    tools: Option<YamlValue>,
}

pub fn load_skills_snapshot(snapshot: &LoadedConfigSnapshot) -> LoadedSkillsSnapshot {
    let mut merged = BTreeMap::new();
    let mut errors = Vec::new();

    let user_root = snapshot.paths().global_root().join(SKILLS_DIRECTORY_NAME);
    let (user_skills, user_errors) = load_skills_from_root(&user_root, SkillSource::User, None);
    errors.extend(user_errors);
    for skill in user_skills {
        merged.insert(skill.command_name.clone(), skill);
    }

    let project_root = snapshot
        .paths()
        .project_directory()
        .join(SKILLS_DIRECTORY_NAME);
    let (project_skills, project_errors) =
        load_skills_from_root(&project_root, SkillSource::Project, None);
    errors.extend(project_errors);
    for skill in project_skills {
        merged.insert(skill.command_name.clone(), skill);
    }

    LoadedSkillsSnapshot::from_parts(merged.into_values().collect(), errors)
}

pub(crate) fn load_skills_from_root(
    root: &Path,
    source: SkillSource,
    plugin_id: Option<&str>,
) -> (Vec<LoadedSkill>, Vec<SkillLoadError>) {
    let mut files = Vec::new();
    let mut errors = Vec::new();
    discover_skill_files(root, &mut files, &mut errors);
    files.sort();

    let mut deduped = BTreeMap::new();
    for file in files {
        match parse_skill_file(&file, source, plugin_id) {
            Ok(skill) => {
                if deduped.contains_key(skill.command_name()) {
                    errors.push(SkillLoadError::new(
                        file,
                        format!("duplicate skill command `{}`", skill.command_name()),
                    ));
                } else {
                    deduped.insert(skill.command_name.clone(), skill);
                }
            }
            Err(message) => errors.push(SkillLoadError::new(file, message)),
        }
    }

    (deduped.into_values().collect(), errors)
}

pub(crate) fn parse_skill_file(
    path: &Path,
    source: SkillSource,
    plugin_id: Option<&str>,
) -> Result<LoadedSkill, String> {
    let content = fs::read_to_string(path)
        .map_err(|error| format!("failed to read skill markdown: {error}"))?;
    let (frontmatter, markdown) = split_required_frontmatter(&content)?;
    let frontmatter: RawSkillFrontmatter = serde_yaml::from_str(&frontmatter)
        .map_err(|error| format!("invalid skill frontmatter: {error}"))?;

    let name = frontmatter
        .name
        .map(|name| name.trim().to_owned())
        .filter(|name| !name.is_empty())
        .or_else(|| {
            path.parent()
                .and_then(Path::file_name)
                .map(|name| name.to_string_lossy().to_string())
        })
        .ok_or_else(|| "invalid skill frontmatter: missing or invalid `name`".to_owned())?;

    let description = frontmatter
        .description
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .or_else(|| extract_description(&markdown))
        .unwrap_or_else(|| "Skill".to_owned());

    let tools = parse_tools(frontmatter.tools)?;
    let markdown = normalize_markdown_body(&markdown)
        .ok_or_else(|| "invalid skill markdown: markdown body is empty".to_owned())?;
    let command_name = match plugin_id {
        Some(plugin_id) => format!("{plugin_id}:{name}"),
        None => name.clone(),
    };
    let origin_label = match plugin_id {
        Some(plugin_id) => format!("plugin:{plugin_id}"),
        None => source.as_str().to_owned(),
    };

    Ok(LoadedSkill {
        name,
        command_name,
        description,
        tools,
        markdown,
        source,
        plugin_id: plugin_id.map(str::to_owned),
        origin_label,
        path: path.to_path_buf(),
    })
}

fn discover_skill_files(root: &Path, files: &mut Vec<PathBuf>, errors: &mut Vec<SkillLoadError>) {
    let Ok(metadata) = fs::metadata(root) else {
        return;
    };
    if !metadata.is_dir() {
        return;
    }

    let Ok(entries) = fs::read_dir(root) else {
        errors.push(SkillLoadError::new(
            root.to_path_buf(),
            "failed to read skills directory".to_owned(),
        ));
        return;
    };

    let mut children = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
    children.sort();

    for child in children {
        if child.is_dir() {
            discover_skill_files(&child, files, errors);
        } else if child
            .file_name()
            .is_some_and(|name| name == SKILL_ENTRYPOINT_FILE_NAME)
        {
            files.push(child);
        }
    }
}

fn split_required_frontmatter(content: &str) -> Result<(String, String), String> {
    let normalized = content.replace("\r\n", "\n");
    let stripped = normalized
        .strip_prefix("---\n")
        .ok_or_else(|| "invalid skill frontmatter: frontmatter header is required".to_owned())?;

    let Some(marker_index) = stripped
        .find("\n---\n")
        .or_else(|| stripped.strip_suffix("\n---").map(|value| value.len()))
    else {
        return Err("invalid skill frontmatter: missing closing `---` marker".to_owned());
    };

    let frontmatter = stripped[..marker_index].to_owned();
    let markdown = if stripped[marker_index..].starts_with("\n---\n") {
        stripped[(marker_index + 5)..].to_owned()
    } else {
        String::new()
    };

    Ok((frontmatter, markdown))
}

fn parse_tools(value: Option<YamlValue>) -> Result<Vec<String>, String> {
    match value {
        None => Ok(Vec::new()),
        Some(YamlValue::String(value)) => Ok(vec![value]),
        Some(YamlValue::Sequence(values)) => values
            .into_iter()
            .map(|value| match value {
                YamlValue::String(value) => Ok(value),
                _ => Err("invalid skill frontmatter: `tools` must contain strings".to_owned()),
            })
            .collect(),
        Some(_) => Err("invalid skill frontmatter: `tools` must be a string or list".to_owned()),
    }
}

fn extract_description(markdown: &str) -> Option<String> {
    markdown
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(|line| line.trim_start_matches('#').trim().to_owned())
        .filter(|line| !line.is_empty())
}

pub(crate) fn normalize_markdown_body(markdown: &str) -> Option<String> {
    let trimmed = markdown.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_owned())
    }
}
