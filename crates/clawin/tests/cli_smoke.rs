use assert_cmd::Command;

#[test]
fn prints_help_with_no_arguments() {
    let assert = Command::cargo_bin("clawin")
        .expect("binary should exist")
        .assert()
        .success();

    let stdout = String::from_utf8(assert.get_output().stdout.clone()).expect("stdout is utf-8");

    assert!(stdout.contains("Usage: clawin"));
    assert!(stdout.contains("Terminal coding agent rebuilt in Rust."));
    assert!(!stdout.contains("Claude"));
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
fn rejects_unknown_flags() {
    let assert = Command::cargo_bin("clawin")
        .expect("binary should exist")
        .arg("--bad-flag")
        .assert()
        .failure();

    let stderr = String::from_utf8(assert.get_output().stderr.clone()).expect("stderr is utf-8");

    assert!(stderr.contains("--bad-flag"));
}
