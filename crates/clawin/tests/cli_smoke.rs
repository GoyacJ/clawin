use std::fs;
use std::path::PathBuf;

use assert_cmd::Command;
use serde_json::Value;
use tempfile::TempDir;

#[test]
fn enters_bootstrap_flow_with_no_arguments() {
    let harness = CliHarness::new();
    let assert = harness.command().assert().success();

    let stdout = String::from_utf8(assert.get_output().stdout.clone()).expect("stdout is utf-8");

    assert!(stdout.contains("non-interactive mode is not implemented yet"));
    assert!(!stdout.contains("Usage: clawin"));

    let config = harness.read_global_config();
    assert_eq!(config["schema_version"], 1);
    assert_eq!(config["num_startups"], 1);
}

#[test]
fn prints_version() {
    let assert = Command::cargo_bin("clawin")
        .expect("binary should exist")
        .arg("--version")
        .assert()
        .success();

    let stdout = String::from_utf8(assert.get_output().stdout.clone()).expect("stdout is utf-8");

    assert!(stdout.contains("clawin"));
    assert!(stdout.contains(env!("CARGO_PKG_VERSION")));
}

#[test]
fn prints_help_without_loading_config() {
    let harness = CliHarness::new();
    let assert = harness.command().arg("--help").assert().success();

    let stdout = String::from_utf8(assert.get_output().stdout.clone()).expect("stdout is utf-8");

    assert!(stdout.contains("Usage: clawin"));
    assert!(stdout.contains("Terminal coding agent rebuilt in Rust."));
    assert!(stdout.contains("--continue"));
    assert!(stdout.contains("--resume <SESSION>"));
    assert!(stdout.contains("--print"));
    assert!(stdout.contains("remote-control"));
    assert!(stdout.contains("--input-format <INPUT_FORMAT>"));
    assert!(stdout.contains("--output-format <OUTPUT_FORMAT>"));
    assert!(stdout.contains("--verbose"));
    assert!(!harness.global_root().exists());
}

#[test]
fn prints_remote_control_subcommand_help() {
    let harness = CliHarness::new();
    let assert = harness
        .command()
        .args(["remote-control", "--help"])
        .assert()
        .success();

    let stdout = String::from_utf8(assert.get_output().stdout.clone()).expect("stdout is utf-8");

    assert!(stdout.contains("Usage: clawin remote-control"));
    assert!(stdout.contains("--continue"));
}

#[test]
fn prints_remote_control_alias_help() {
    let harness = CliHarness::new();
    let assert = harness.command().args(["rc", "--help"]).assert().success();

    let stdout = String::from_utf8(assert.get_output().stdout.clone()).expect("stdout is utf-8");

    assert!(stdout.contains("Usage: clawin remote-control"));
}

#[test]
fn rejects_unknown_flags() {
    let assert = Command::cargo_bin("clawin")
        .expect("binary should exist")
        .arg("--bad-flag")
        .assert()
        .failure();

    let stderr = String::from_utf8(assert.get_output().stderr.clone()).expect("stderr is utf-8");

    assert!(stderr.contains("--bad-flag"));
}

#[test]
fn fails_when_global_config_is_invalid() {
    let harness = CliHarness::new();
    fs::create_dir_all(harness.global_root()).expect("global root should exist");
    fs::write(harness.global_config_file(), "{ invalid json")
        .expect("invalid config should be written");

    let assert = harness.command().assert().failure();
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).expect("stderr is utf-8");

    assert!(stderr.contains("config.json"));
    assert!(stderr.contains("parse"));
}

#[test]
fn continue_flag_fails_cleanly_when_no_session_is_available() {
    let harness = CliHarness::new();
    let assert = harness.command().arg("--continue").assert().failure();
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).expect("stderr is utf-8");

    assert!(stderr.contains("no resumable session found"));
}

#[test]
fn resume_flag_fails_cleanly_for_missing_jsonl_path() {
    let harness = CliHarness::new();
    let missing = harness.project_dir.join("missing-session.jsonl");
    let assert = harness
        .command()
        .args(["--resume", &missing.display().to_string()])
        .assert()
        .failure();
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).expect("stderr is utf-8");

    assert!(stderr.contains("failed to open session transcript"));
    assert!(stderr.contains("missing-session.jsonl"));
}

#[test]
fn print_mode_requires_verbose_for_stream_json_output() {
    let harness = CliHarness::new();
    let assert = harness
        .command()
        .args([
            "--print",
            "--input-format=stream-json",
            "--output-format=stream-json",
        ])
        .write_stdin("{\"type\":\"keep_alive\"}\n")
        .assert()
        .failure();
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).expect("stderr is utf-8");

    assert!(stderr.contains("--verbose"));
}

#[test]
fn print_mode_requires_prompt_argument_or_piped_stdin() {
    let harness = CliHarness::new();
    let assert = harness.command().arg("--print").assert().failure();
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).expect("stderr is utf-8");

    assert!(stderr.contains("prompt argument"));
    assert!(stderr.contains("stdin"));
}

#[test]
fn print_mode_json_output_returns_structured_error_when_driver_is_unavailable() {
    let harness = CliHarness::new();
    let assert = harness
        .command()
        .args(["--print", "--output-format=json", "hello"])
        .assert()
        .failure();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).expect("stdout is utf-8");
    let json: Value = serde_json::from_str(stdout.trim()).expect("stdout should be valid json");

    assert_eq!(json["type"], "error");
    assert_eq!(json["code"], "model_driver_failed");
}

struct CliHarness {
    _tempdir: TempDir,
    home_dir: PathBuf,
    project_dir: PathBuf,
}

impl CliHarness {
    fn new() -> Self {
        let tempdir = tempfile::tempdir().expect("tempdir should exist");
        let home_dir = tempdir.path().join("home");
        let project_dir = tempdir.path().join("workspace").join("app");

        fs::create_dir_all(&home_dir).expect("home dir should exist");
        fs::create_dir_all(&project_dir).expect("project dir should exist");

        Self {
            _tempdir: tempdir,
            home_dir,
            project_dir,
        }
    }

    fn command(&self) -> Command {
        let mut command = Command::cargo_bin("clawin").expect("binary should exist");
        command.current_dir(&self.project_dir);
        command.env("HOME", &self.home_dir);
        command.env("USERPROFILE", &self.home_dir);
        command
    }

    fn global_root(&self) -> PathBuf {
        self.home_dir.join(".clawin")
    }

    fn global_config_file(&self) -> PathBuf {
        self.global_root().join("config.json")
    }

    fn read_global_config(&self) -> Value {
        let contents = fs::read_to_string(self.global_config_file()).expect("config should exist");
        serde_json::from_str(&contents).expect("config should be valid json")
    }
}
