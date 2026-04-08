#![forbid(unsafe_code)]

//! Startup configuration loading, path discovery, and migration support for Clawin.

use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use clawin_platform::PathPolicy;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

/// Clawin global namespace directory name.
pub const GLOBAL_NAMESPACE_DIR_NAME: &str = ".clawin";

/// Clawin project-local metadata directory name.
pub const PROJECT_DIRECTORY_NAME: &str = ".clawin";

/// Clawin project-local instruction manifest.
pub const PROJECT_MANIFEST_NAME: &str = "CLAWIN.md";

/// Global configuration document file name.
pub const GLOBAL_CONFIG_FILE_NAME: &str = "config.json";

/// Shared settings document file name.
pub const SETTINGS_FILE_NAME: &str = "settings.json";

/// Backup directory name used for migration write-ahead copies.
pub const BACKUPS_DIRECTORY_NAME: &str = "backups";

/// Current schema version for all persisted Phase 2 documents.
pub const CURRENT_SCHEMA_VERSION: u32 = 1;

/// Resolved path bundle for the current startup.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClawinPaths {
    original_cwd: PathBuf,
    project_root: PathBuf,
    global_root: PathBuf,
    global_config_file: PathBuf,
    global_settings_file: PathBuf,
    project_settings_file: PathBuf,
    project_directory: PathBuf,
    project_manifest: PathBuf,
    backups_dir: PathBuf,
}

impl ClawinPaths {
    fn discover<P: PathPolicy>(
        original_cwd: PathBuf,
        path_policy: &P,
    ) -> Result<Self, ConfigError> {
        let original_cwd = canonicalize_path(&original_cwd)?;
        let project_root = resolve_project_root(&original_cwd)?;
        let home_dir = path_policy.home_dir().ok_or(ConfigError::PathDiscovery {
            path: PathBuf::from("HOME"),
            message: "could not resolve user home directory".to_owned(),
        })?;
        let global_root = home_dir.join(GLOBAL_NAMESPACE_DIR_NAME);
        let project_directory = project_root.join(path_policy.project_directory_name());

        Ok(Self {
            original_cwd,
            global_root: global_root.clone(),
            global_config_file: global_root.join(GLOBAL_CONFIG_FILE_NAME),
            global_settings_file: global_root.join(SETTINGS_FILE_NAME),
            project_settings_file: project_directory.join(SETTINGS_FILE_NAME),
            project_directory,
            project_manifest: project_root.join(path_policy.project_manifest_name()),
            backups_dir: global_root.join(BACKUPS_DIRECTORY_NAME),
            project_root,
        })
    }

    /// Borrow the cwd that Clawin started from.
    pub fn original_cwd(&self) -> &Path {
        &self.original_cwd
    }

    /// Borrow the resolved project root.
    pub fn project_root(&self) -> &Path {
        &self.project_root
    }

    /// Borrow the global root.
    pub fn global_root(&self) -> &Path {
        &self.global_root
    }

    /// Borrow the global config file path.
    pub fn global_config_file(&self) -> &Path {
        &self.global_config_file
    }

    /// Borrow the global settings file path.
    pub fn global_settings_file(&self) -> &Path {
        &self.global_settings_file
    }

    /// Borrow the project settings file path.
    pub fn project_settings_file(&self) -> &Path {
        &self.project_settings_file
    }

    /// Borrow the project-local metadata directory.
    pub fn project_directory(&self) -> &Path {
        &self.project_directory
    }

    /// Borrow the project manifest path.
    pub fn project_manifest(&self) -> &Path {
        &self.project_manifest
    }

    /// Borrow the migration backups directory.
    pub fn backups_dir(&self) -> &Path {
        &self.backups_dir
    }
}

/// Minimal global config document for Phase 2.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct GlobalConfigDocument {
    #[serde(default)]
    pub schema_version: u32,
    #[serde(default)]
    pub num_startups: u64,
    #[serde(default)]
    pub projects: BTreeMap<String, ProjectConfigDocument>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl Default for GlobalConfigDocument {
    fn default() -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            num_startups: 0,
            projects: BTreeMap::new(),
            extra: BTreeMap::new(),
        }
    }
}

/// Minimal project config entry stored under `projects[project_key]`.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct ProjectConfigDocument {
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// Minimal settings document preserved as structured JSON.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct SettingsDocument {
    #[serde(default)]
    pub schema_version: u32,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// Document category used for error reporting and migration bookkeeping.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConfigDocumentKind {
    /// `~/.clawin/config.json`
    GlobalConfig,
    /// `~/.clawin/settings.json`
    GlobalSettings,
    /// `<project_root>/.clawin/settings.json`
    ProjectSettings,
}

impl ConfigDocumentKind {
    /// Return a stable human-readable label for the document kind.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::GlobalConfig => "global config",
            Self::GlobalSettings => "global settings",
            Self::ProjectSettings => "project settings",
        }
    }

    fn backup_stem(self) -> &'static str {
        match self {
            Self::GlobalConfig => "config",
            Self::GlobalSettings => "global-settings",
            Self::ProjectSettings => "project-settings",
        }
    }
}

impl fmt::Display for ConfigDocumentKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Structured config loading failures used by bootstrap.
#[derive(Debug, Error)]
pub enum ConfigError {
    /// Returned when a required path cannot be derived or canonicalized.
    #[error("failed to resolve path {path}: {message}")]
    PathDiscovery { path: PathBuf, message: String },

    /// Returned when a document cannot be parsed as JSON.
    #[error("failed to parse {document} file {path}: {source}")]
    JsonParse {
        document: ConfigDocumentKind,
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },

    /// Returned when a JSON file has a valid shape but violates schema rules.
    #[error("invalid {document} schema in {path}: {message}")]
    SchemaValidation {
        document: ConfigDocumentKind,
        path: PathBuf,
        message: String,
    },

    /// Returned when a migration cannot be applied.
    #[error("failed to migrate {document} file {path}: {message}")]
    Migration {
        document: ConfigDocumentKind,
        path: PathBuf,
        message: String,
    },

    /// Returned when backup or writeback steps fail.
    #[error("failed to write {document} file {path}: {message}")]
    Write {
        document: ConfigDocumentKind,
        path: PathBuf,
        message: String,
    },
}

/// A single migration event captured during startup.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MigrationEntry {
    /// Which document was migrated.
    pub document: ConfigDocumentKind,
    /// The schema version observed on disk.
    pub from_version: u32,
    /// The schema version written after migration.
    pub to_version: u32,
    /// The write-ahead backup created before mutating the file.
    pub backup_path: PathBuf,
}

/// Aggregated migration events for the current startup.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MigrationReport {
    entries: Vec<MigrationEntry>,
}

impl MigrationReport {
    /// Borrow the migration entries captured for this startup.
    pub fn entries(&self) -> &[MigrationEntry] {
        &self.entries
    }
}

/// Fully loaded startup config snapshot returned to bootstrap.
#[derive(Clone, Debug, PartialEq)]
pub struct LoadedConfigSnapshot {
    paths: ClawinPaths,
    project_key: String,
    global_config: GlobalConfigDocument,
    current_project_config: ProjectConfigDocument,
    global_settings: Option<SettingsDocument>,
    project_settings: Option<SettingsDocument>,
    migration_report: MigrationReport,
}

impl LoadedConfigSnapshot {
    /// Borrow the resolved startup paths.
    pub fn paths(&self) -> &ClawinPaths {
        &self.paths
    }

    /// Borrow the normalized key used for project-scoped config lookup.
    pub fn project_key(&self) -> &str {
        &self.project_key
    }

    /// Borrow the active global config document.
    pub fn global_config(&self) -> &GlobalConfigDocument {
        &self.global_config
    }

    /// Borrow the current project config entry.
    pub fn current_project_config(&self) -> &ProjectConfigDocument {
        &self.current_project_config
    }

    /// Borrow the loaded global settings document, if present.
    pub fn global_settings(&self) -> Option<&SettingsDocument> {
        self.global_settings.as_ref()
    }

    /// Borrow the loaded project settings document, if present.
    pub fn project_settings(&self) -> Option<&SettingsDocument> {
        self.project_settings.as_ref()
    }

    /// Borrow the migration report for the current startup.
    pub fn migration_report(&self) -> &MigrationReport {
        &self.migration_report
    }
}

/// Load startup configuration, apply migrations, and persist startup bookkeeping.
pub fn load_startup_config<P: PathPolicy>(
    original_cwd: PathBuf,
    path_policy: &P,
) -> Result<LoadedConfigSnapshot, ConfigError> {
    let paths = ClawinPaths::discover(original_cwd, path_policy)?;
    let project_key = path_policy.normalize_for_config_key(paths.project_root());
    let mut migration_report = MigrationReport::default();

    let mut global_config = load_global_config(paths.global_config_file())?;
    let mut global_settings = load_settings(
        paths.global_settings_file(),
        ConfigDocumentKind::GlobalSettings,
    )?;
    let mut project_settings = load_settings(
        paths.project_settings_file(),
        ConfigDocumentKind::ProjectSettings,
    )?;

    if let Some(from_version) = prepare_schema(
        &mut global_config,
        ConfigDocumentKind::GlobalConfig,
        paths.global_config_file(),
    )? {
        let backup_path = backup_document(
            ConfigDocumentKind::GlobalConfig,
            paths.global_config_file(),
            paths.backups_dir(),
        )?;
        write_document(
            paths.global_config_file(),
            ConfigDocumentKind::GlobalConfig,
            &global_config,
        )?;
        migration_report.entries.push(MigrationEntry {
            document: ConfigDocumentKind::GlobalConfig,
            from_version,
            to_version: CURRENT_SCHEMA_VERSION,
            backup_path,
        });
    }

    migrate_settings_if_needed(
        &mut global_settings,
        paths.global_settings_file(),
        paths.backups_dir(),
        ConfigDocumentKind::GlobalSettings,
        &mut migration_report,
    )?;
    migrate_settings_if_needed(
        &mut project_settings,
        paths.project_settings_file(),
        paths.backups_dir(),
        ConfigDocumentKind::ProjectSettings,
        &mut migration_report,
    )?;

    global_config.num_startups = global_config.num_startups.saturating_add(1);
    write_document(
        paths.global_config_file(),
        ConfigDocumentKind::GlobalConfig,
        &global_config,
    )?;

    let current_project_config = global_config
        .projects
        .get(&project_key)
        .cloned()
        .unwrap_or_default();

    Ok(LoadedConfigSnapshot {
        paths,
        project_key,
        global_config,
        current_project_config,
        global_settings,
        project_settings,
        migration_report,
    })
}

fn load_global_config(path: &Path) -> Result<GlobalConfigDocument, ConfigError> {
    if !path.exists() {
        return Ok(GlobalConfigDocument::default());
    }

    parse_json_file(path, ConfigDocumentKind::GlobalConfig)
}

fn load_settings(
    path: &Path,
    document: ConfigDocumentKind,
) -> Result<Option<SettingsDocument>, ConfigError> {
    if !path.exists() {
        return Ok(None);
    }

    parse_json_file(path, document).map(Some)
}

fn parse_json_file<T>(path: &Path, document: ConfigDocumentKind) -> Result<T, ConfigError>
where
    T: for<'de> Deserialize<'de>,
{
    let contents = fs::read_to_string(path).map_err(|error| ConfigError::PathDiscovery {
        path: path.to_path_buf(),
        message: error.to_string(),
    })?;

    serde_json::from_str(&contents).map_err(|source| ConfigError::JsonParse {
        document,
        path: path.to_path_buf(),
        source,
    })
}

fn prepare_schema<T>(
    document: &mut T,
    kind: ConfigDocumentKind,
    path: &Path,
) -> Result<Option<u32>, ConfigError>
where
    T: HasSchemaVersion,
{
    match document.schema_version() {
        CURRENT_SCHEMA_VERSION => Ok(None),
        0 => {
            document.set_schema_version(CURRENT_SCHEMA_VERSION);
            Ok(Some(0))
        }
        other if other < CURRENT_SCHEMA_VERSION => {
            document.set_schema_version(CURRENT_SCHEMA_VERSION);
            Ok(Some(other))
        }
        other => Err(ConfigError::SchemaValidation {
            document: kind,
            path: path.to_path_buf(),
            message: format!(
                "unsupported schema version {other}; current version is {CURRENT_SCHEMA_VERSION}"
            ),
        }),
    }
}

fn migrate_settings_if_needed(
    settings: &mut Option<SettingsDocument>,
    path: &Path,
    backups_dir: &Path,
    document: ConfigDocumentKind,
    migration_report: &mut MigrationReport,
) -> Result<(), ConfigError> {
    let Some(settings) = settings.as_mut() else {
        return Ok(());
    };

    let Some(from_version) = prepare_schema(settings, document, path)? else {
        return Ok(());
    };

    let backup_path = backup_document(document, path, backups_dir)?;
    write_document(path, document, settings)?;
    migration_report.entries.push(MigrationEntry {
        document,
        from_version,
        to_version: CURRENT_SCHEMA_VERSION,
        backup_path,
    });
    Ok(())
}

fn write_document<T>(
    path: &Path,
    document: ConfigDocumentKind,
    value: &T,
) -> Result<(), ConfigError>
where
    T: Serialize,
{
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| ConfigError::Write {
            document,
            path: parent.to_path_buf(),
            message: error.to_string(),
        })?;
    }

    let bytes = serde_json::to_vec_pretty(value).map_err(|error| ConfigError::Write {
        document,
        path: path.to_path_buf(),
        message: error.to_string(),
    })?;

    fs::write(path, bytes).map_err(|error| ConfigError::Write {
        document,
        path: path.to_path_buf(),
        message: error.to_string(),
    })
}

fn backup_document(
    document: ConfigDocumentKind,
    source_path: &Path,
    backups_dir: &Path,
) -> Result<PathBuf, ConfigError> {
    fs::create_dir_all(backups_dir).map_err(|error| ConfigError::Write {
        document,
        path: backups_dir.to_path_buf(),
        message: error.to_string(),
    })?;

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let backup_path = backups_dir.join(format!("{}-{timestamp}.json", document.backup_stem()));

    fs::copy(source_path, &backup_path).map_err(|error| ConfigError::Migration {
        document,
        path: source_path.to_path_buf(),
        message: format!("failed to create backup: {error}"),
    })?;

    Ok(backup_path)
}

fn canonicalize_path(path: &Path) -> Result<PathBuf, ConfigError> {
    fs::canonicalize(path).map_err(|error| ConfigError::PathDiscovery {
        path: path.to_path_buf(),
        message: error.to_string(),
    })
}

fn resolve_project_root(original_cwd: &Path) -> Result<PathBuf, ConfigError> {
    let git_root = find_git_root(original_cwd)?;
    Ok(git_root.unwrap_or_else(|| original_cwd.to_path_buf()))
}

fn find_git_root(start: &Path) -> Result<Option<PathBuf>, ConfigError> {
    for ancestor in start.ancestors() {
        let marker = ancestor.join(".git");
        if marker.is_dir() {
            return canonicalize_path(ancestor).map(Some);
        }
        if marker.is_file() {
            return parse_gitdir_reference(&marker, ancestor).map(Some);
        }
    }

    Ok(None)
}

fn parse_gitdir_reference(marker: &Path, workspace_root: &Path) -> Result<PathBuf, ConfigError> {
    let contents = fs::read_to_string(marker).map_err(|error| ConfigError::PathDiscovery {
        path: marker.to_path_buf(),
        message: error.to_string(),
    })?;
    let Some(raw_gitdir) = contents.trim().strip_prefix("gitdir:") else {
        return canonicalize_path(workspace_root);
    };

    let gitdir_path = Path::new(raw_gitdir.trim());
    let resolved_gitdir = if gitdir_path.is_absolute() {
        gitdir_path.to_path_buf()
    } else {
        workspace_root.join(gitdir_path)
    };
    let resolved_gitdir = canonicalize_path(&resolved_gitdir)?;

    for ancestor in resolved_gitdir.ancestors() {
        if ancestor.file_name() == Some(OsStr::new(".git")) {
            return ancestor
                .parent()
                .map(Path::to_path_buf)
                .ok_or(ConfigError::PathDiscovery {
                    path: marker.to_path_buf(),
                    message: "failed to derive repository root from gitdir reference".to_owned(),
                });
        }
    }

    canonicalize_path(workspace_root)
}

trait HasSchemaVersion {
    fn schema_version(&self) -> u32;
    fn set_schema_version(&mut self, value: u32);
}

impl HasSchemaVersion for GlobalConfigDocument {
    fn schema_version(&self) -> u32 {
        self.schema_version
    }

    fn set_schema_version(&mut self, value: u32) {
        self.schema_version = value;
    }
}

impl HasSchemaVersion for SettingsDocument {
    fn schema_version(&self) -> u32 {
        self.schema_version
    }

    fn set_schema_version(&mut self, value: u32) {
        self.schema_version = value;
    }
}
