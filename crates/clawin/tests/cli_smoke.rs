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
fn remote_control_subcommand_fails_cleanly_when_connector_is_unavailable() {
    let harness = CliHarness::new();
    let assert = harness
        .command()
        .args(["remote-control"])
        .assert()
        .failure();
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).expect("stderr is utf-8");

    assert!(stderr.contains("remote control bridge is unavailable"));
    assert!(stderr.contains("no bridge connector is configured"));
}

#[test]
fn remote_control_continue_fails_cleanly_when_no_pointer_is_available() {
    let harness = CliHarness::new();
    let assert = harness
        .command()
        .args(["remote-control", "--continue"])
        .assert()
        .failure();
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).expect("stderr is utf-8");

    assert!(stderr.contains("no valid bridge pointer found"));
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
fn print_mode_rejects_positional_prompt_with_stream_json_input() {
    let harness = CliHarness::new();
    let assert = harness
        .command()
        .args(["--print", "--input-format=stream-json", "hello"])
        .assert()
        .failure();
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).expect("stderr is utf-8");

    assert!(stderr.contains("positional prompt"));
    assert!(stderr.contains("stream-json"));
}

#[test]
fn print_mode_text_output_matches_help_fixture() {
    let harness = CliHarness::new();
    let assert = harness
        .command()
        .args(["--print", "/help"])
        .assert()
        .success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).expect("stdout is utf-8");

    assert_eq!(stdout, fixture_text("tests/fixtures/print_help_text.txt"));
}

#[test]
fn print_mode_json_output_matches_help_fixture() {
    let harness = CliHarness::new();
    let assert = harness
        .command()
        .args(["--print", "--output-format=json", "/help"])
        .assert()
        .success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).expect("stdout is utf-8");

    assert_eq!(stdout, fixture_text("tests/fixtures/print_help_json.json"));
}

#[test]
fn print_mode_stream_json_output_matches_help_fixture() {
    let harness = CliHarness::new();
    let assert = harness
        .command()
        .args([
            "--print",
            "--input-format=stream-json",
            "--output-format=stream-json",
            "--verbose",
        ])
        .write_stdin("{\"type\":\"user\",\"content\":\"/help\"}\n")
        .assert()
        .success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).expect("stdout is utf-8");

    assert_eq!(
        normalized_json_lines(&stdout),
        fixture_json_lines("tests/fixtures/print_help_stream_json.jsonl")
    );
}

#[test]
fn print_mode_continue_reuses_existing_session_id() {
    let harness = CliHarness::new();
    harness
        .command()
        .args(["--print", "/help"])
        .assert()
        .success();
    let transcript = harness.single_transcript_path();
    let session_id = transcript
        .file_stem()
        .and_then(|value| value.to_str())
        .expect("session transcript should have a stem")
        .to_owned();

    let assert = harness
        .command()
        .args([
            "--print",
            "--continue",
            "--input-format=stream-json",
            "--output-format=stream-json",
            "--verbose",
        ])
        .write_stdin("{\"type\":\"user\",\"content\":\"/help\"}\n")
        .assert()
        .success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).expect("stdout is utf-8");

    assert_eq!(
        normalized_json_lines(&stdout),
        fixture_json_lines("tests/fixtures/print_help_stream_json.jsonl")
    );
    assert_eq!(session_ids(&stdout), vec![session_id]);
}

#[test]
fn print_mode_resume_by_path_reuses_existing_session_id() {
    let harness = CliHarness::new();
    harness
        .command()
        .args(["--print", "/help"])
        .assert()
        .success();
    let transcript = harness.single_transcript_path();
    let session_id = transcript
        .file_stem()
        .and_then(|value| value.to_str())
        .expect("session transcript should have a stem")
        .to_owned();

    let assert = harness
        .command()
        .args([
            "--print",
            "--resume",
            &transcript.display().to_string(),
            "--input-format=stream-json",
            "--output-format=stream-json",
            "--verbose",
        ])
        .write_stdin("{\"type\":\"user\",\"content\":\"/help\"}\n")
        .assert()
        .success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).expect("stdout is utf-8");

    assert_eq!(
        normalized_json_lines(&stdout),
        fixture_json_lines("tests/fixtures/print_help_stream_json.jsonl")
    );
    assert_eq!(session_ids(&stdout), vec![session_id]);
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

    fn single_transcript_path(&self) -> PathBuf {
        let projects_root = self.global_root().join("projects");
        let mut paths = Vec::new();
        for entry in fs::read_dir(&projects_root).expect("projects root should exist") {
            let entry = entry.expect("project directory entry should exist");
            if !entry
                .file_type()
                .expect("project entry type should be readable")
                .is_dir()
            {
                continue;
            }
            for child in fs::read_dir(entry.path()).expect("session directory should be readable") {
                let child = child.expect("session file should exist");
                if child
                    .file_type()
                    .expect("session file type should be readable")
                    .is_file()
                    && child.path().extension().and_then(|ext| ext.to_str()) == Some("jsonl")
                {
                    paths.push(child.path());
                }
            }
        }

        assert_eq!(
            paths.len(),
            1,
            "expected a single transcript, got {paths:?}"
        );
        paths.pop().expect("single transcript path should exist")
    }

    fn read_global_config(&self) -> Value {
        let contents = fs::read_to_string(self.global_config_file()).expect("config should exist");
        serde_json::from_str(&contents).expect("config should be valid json")
    }
}

fn fixture_text(path: &str) -> String {
    let fixture_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(path);
    fs::read_to_string(fixture_path).expect("fixture should exist")
}

fn fixture_json_lines(path: &str) -> Vec<Value> {
    fixture_text(path)
        .lines()
        .map(|line| serde_json::from_str(line).expect("fixture line should be valid json"))
        .collect()
}

fn normalized_json_lines(output: &str) -> Vec<Value> {
    output
        .lines()
        .map(|line| {
            let mut value: Value = serde_json::from_str(line).expect("line should be valid json");
            normalize_json_value(&mut value);
            value
        })
        .collect()
}

fn session_ids(output: &str) -> Vec<String> {
    let mut values = Vec::new();
    for line in output.lines() {
        let value: Value = serde_json::from_str(line).expect("line should be valid json");
        collect_session_ids(&value, &mut values);
    }
    values.sort();
    values.dedup();
    values
}

fn collect_session_ids(value: &Value, values: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                if key == "session_id" {
                    if let Some(session_id) = child.as_str() {
                        values.push(session_id.to_owned());
                    }
                } else {
                    collect_session_ids(child, values);
                }
            }
        }
        Value::Array(items) => {
            for child in items {
                collect_session_ids(child, values);
            }
        }
        _ => {}
    }
}

fn normalize_json_value(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (key, child) in map.iter_mut() {
                if key == "session_id" {
                    *child = Value::String("<session-id>".to_owned());
                } else {
                    normalize_json_value(child);
                }
            }
        }
        Value::Array(items) => {
            for child in items {
                normalize_json_value(child);
            }
        }
        _ => {}
    }
}
