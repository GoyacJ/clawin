use std::fs;
use std::path::{Path, PathBuf};

use clawin_config::{CURRENT_SCHEMA_VERSION, ConfigError, load_startup_config};
use clawin_platform::PathPolicy;
use serde_json::json;
use tempfile::TempDir;

#[test]
fn initializes_default_global_config_for_non_git_workspace() {
    let harness = ConfigHarness::new();
    let policy = TestPathPolicy::new(harness.home_dir());

    let snapshot =
        load_startup_config(harness.project_dir(), &policy).expect("startup config should load");

    assert_eq!(
        snapshot.paths().original_cwd(),
        canonical(harness.project_dir()).as_path()
    );
    assert_eq!(
        snapshot.paths().project_root(),
        canonical(harness.project_dir()).as_path()
    );
    assert_eq!(
        snapshot.global_config().schema_version,
        CURRENT_SCHEMA_VERSION
    );
    assert_eq!(snapshot.global_config().num_startups, 1);
    assert!(snapshot.global_settings().is_none());
    assert!(snapshot.project_settings().is_none());
    assert!(snapshot.migration_report().entries().is_empty());
    assert_eq!(
        snapshot.project_key(),
        policy.normalize_for_config_key(canonical(harness.project_dir()).as_path())
    );
    assert!(harness.global_config_file().exists());
}

#[test]
fn prefers_canonical_git_root_for_project_key() {
    let harness = ConfigHarness::new();
    let policy = TestPathPolicy::new(harness.home_dir());
    let git_root = harness.project_dir().join("repo");
    let nested = git_root.join("src").join("nested");

    fs::create_dir_all(git_root.join(".git")).expect(".git should exist");
    fs::create_dir_all(&nested).expect("nested dir should exist");

    let snapshot =
        load_startup_config(nested, &policy).expect("startup config should resolve git root");

    assert_eq!(
        snapshot.paths().project_root(),
        canonical(git_root.clone()).as_path()
    );
    assert_eq!(
        snapshot.project_key(),
        policy.normalize_for_config_key(canonical(git_root).as_path())
    );
}

#[test]
fn migrates_old_schema_and_preserves_unknown_fields() {
    let harness = ConfigHarness::new();
    let policy = TestPathPolicy::new(harness.home_dir());
    fs::create_dir_all(harness.global_root()).expect("global root should exist");

    let legacy = json!({
        "num_startups": 5,
        "projects": {},
        "custom_field": { "enabled": true }
    });
    fs::write(
        harness.global_config_file(),
        serde_json::to_vec_pretty(&legacy).expect("legacy json should serialize"),
    )
    .expect("legacy config should be written");

    let snapshot =
        load_startup_config(harness.project_dir(), &policy).expect("legacy config should migrate");

    assert_eq!(
        snapshot.global_config().schema_version,
        CURRENT_SCHEMA_VERSION
    );
    assert_eq!(snapshot.global_config().num_startups, 6);
    assert_eq!(
        snapshot.global_config().extra["custom_field"]["enabled"],
        json!(true)
    );
    assert_eq!(snapshot.migration_report().entries().len(), 1);
    assert!(
        snapshot.migration_report().entries()[0]
            .backup_path
            .exists()
    );
}

#[test]
fn rejects_invalid_project_settings_json() {
    let harness = ConfigHarness::new();
    let policy = TestPathPolicy::new(harness.home_dir());
    fs::create_dir_all(
        harness
            .project_settings_file()
            .parent()
            .expect("project settings parent"),
    )
    .expect("project settings dir should exist");
    fs::write(harness.project_settings_file(), "{ invalid json")
        .expect("settings should be written");

    let error = load_startup_config(harness.project_dir(), &policy)
        .expect_err("invalid settings should fail");

    match error {
        ConfigError::JsonParse { document, .. } => {
            assert_eq!(document.as_str(), "project settings");
        }
        other => panic!("unexpected error: {other:?}"),
    }
}

struct ConfigHarness {
    _tempdir: TempDir,
    root: PathBuf,
}

impl ConfigHarness {
    fn new() -> Self {
        let tempdir = tempfile::tempdir().expect("tempdir should exist");
        let root = tempdir.path().to_path_buf();
        fs::create_dir_all(root.join("home")).expect("home dir should exist");
        fs::create_dir_all(root.join("workspace").join("app")).expect("project dir should exist");

        Self {
            _tempdir: tempdir,
            root,
        }
    }

    fn home_dir(&self) -> PathBuf {
        self.root.join("home")
    }

    fn project_dir(&self) -> PathBuf {
        self.root.join("workspace").join("app")
    }

    fn global_root(&self) -> PathBuf {
        self.home_dir().join(".clawin")
    }

    fn global_config_file(&self) -> PathBuf {
        self.global_root().join("config.json")
    }

    fn project_settings_file(&self) -> PathBuf {
        self.project_dir().join(".clawin").join("settings.json")
    }
}

#[derive(Clone, Debug)]
struct TestPathPolicy {
    home_dir: PathBuf,
}

impl TestPathPolicy {
    fn new(home_dir: PathBuf) -> Self {
        Self { home_dir }
    }
}

impl PathPolicy for TestPathPolicy {
    fn home_dir(&self) -> Option<PathBuf> {
        Some(self.home_dir.clone())
    }

    fn normalize_for_config_key(&self, path: &Path) -> String {
        path.to_string_lossy().replace('\\', "/")
    }

    fn project_directory_name(&self) -> &'static str {
        ".clawin"
    }

    fn project_manifest_name(&self) -> &'static str {
        "CLAWIN.md"
    }
}

fn canonical(path: PathBuf) -> PathBuf {
    fs::canonicalize(path).expect("path should canonicalize")
}
