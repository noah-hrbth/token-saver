mod common;

use std::fs;
use std::process::Command;

#[test]
fn passthrough_without_env_var() {
    let repo = common::create_temp_repo();

    let output = Command::new(common::binary_path())
        .args(["git", "status"])
        .env_remove("TOKEN_SAVER")
        .current_dir(repo.path())
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("On branch") || stdout.contains("No commits yet"),
        "Expected raw git output, got: {}",
        stdout
    );
}

#[test]
fn passthrough_for_skip_flags() {
    let repo = common::create_temp_repo();

    // git log --oneline has a compressor but --oneline is a skip flag — should passthrough
    let output = Command::new(common::binary_path())
        .args(["git", "log", "--oneline"])
        .env("TOKEN_SAVER", "1")
        .current_dir(repo.path())
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("init"),
        "Expected git log output with commit message, got: {}",
        stdout
    );
}

#[test]
fn passthrough_for_unknown_subcommand() {
    let repo = common::create_temp_repo();

    // git shortlog has no compressor — should passthrough
    let output = Command::new(common::binary_path())
        .args(["git", "shortlog", "HEAD"])
        .env("TOKEN_SAVER", "1")
        .current_dir(repo.path())
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Test") || stdout.contains("init"),
        "Expected git shortlog output, got: {}",
        stdout
    );
}

#[test]
fn passthrough_command_not_found() {
    let output = Command::new(common::binary_path())
        .args(["nonexistent_command_xyz"])
        .env("TOKEN_SAVER", "1")
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("command not found"),
        "Expected 'command not found', got: {}",
        stderr
    );
    assert_eq!(output.status.code(), Some(127));
}

#[test]
fn passthrough_preserves_exit_code() {
    let repo = common::create_temp_repo();

    // git diff --check on a file with trailing whitespace should exit non-zero
    fs::write(repo.path().join("trailing.txt"), "hello   \n").unwrap();
    Command::new("git")
        .args(["add", "trailing.txt"])
        .current_dir(repo.path())
        .output()
        .unwrap();

    let output = Command::new(common::binary_path())
        .args(["git", "diff", "--check", "--cached"])
        .env_remove("TOKEN_SAVER")
        .current_dir(repo.path())
        .output()
        .unwrap();

    // git diff --check exits non-zero when there are whitespace issues
    assert!(
        !output.status.success(),
        "Expected non-zero exit for trailing whitespace check"
    );
}
