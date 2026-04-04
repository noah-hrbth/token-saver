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

#[test]
fn passthrough_log_oneline() {
    let repo = common::create_temp_repo();

    let output = Command::new(common::binary_path())
        .args(["git", "log", "--oneline"])
        .env("TOKEN_SAVER", "1")
        .current_dir(repo.path())
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("init"),
        "Expected raw git log --oneline output, got: {}",
        stdout
    );
    assert!(
        !stdout.contains("[Test]"),
        "Should not contain compressed author format: {}",
        stdout
    );
}

#[test]
fn passthrough_log_custom_format() {
    let repo = common::create_temp_repo();

    let output = Command::new(common::binary_path())
        .args(["git", "log", "--format=%H %s"])
        .env("TOKEN_SAVER", "1")
        .current_dir(repo.path())
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("init"),
        "Expected raw git log output, got: {}",
        stdout
    );
}

#[test]
fn passthrough_log_graph() {
    let repo = common::create_temp_repo();

    let output = Command::new(common::binary_path())
        .args(["git", "log", "--graph", "--oneline"])
        .env("TOKEN_SAVER", "1")
        .current_dir(repo.path())
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("init"),
        "Expected raw git log --graph output, got: {}",
        stdout
    );
}

#[test]
fn passthrough_show_blob_reference() {
    let repo = common::create_temp_repo();

    // git show HEAD:README.md should passthrough (blob reference)
    let output = Command::new(common::binary_path())
        .args(["git", "show", "HEAD:README.md"])
        .env("TOKEN_SAVER", "1")
        .current_dir(repo.path())
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("init"),
        "Expected raw file content, got: {}",
        stdout
    );
    // Should NOT contain compressed format markers
    assert!(
        !stdout.contains("* "),
        "Should not contain compressed commit format: {}",
        stdout
    );
}

#[test]
fn passthrough_show_stat() {
    let repo = common::create_temp_repo();

    let output = Command::new(common::binary_path())
        .args(["git", "show", "--stat"])
        .env("TOKEN_SAVER", "1")
        .current_dir(repo.path())
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("init"),
        "Expected raw git show --stat output, got: {}",
        stdout
    );
}

#[test]
fn passthrough_show_name_only() {
    let repo = common::create_temp_repo();

    let output = Command::new(common::binary_path())
        .args(["git", "show", "--name-only"])
        .env("TOKEN_SAVER", "1")
        .current_dir(repo.path())
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("README.md"),
        "Expected file name in output, got: {}",
        stdout
    );
}

#[test]
fn passthrough_bare_ls() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("file.txt"), "hello").unwrap();

    let output = Command::new(common::binary_path())
        .args(["ls"])
        .env("TOKEN_SAVER", "1")
        .current_dir(dir.path())
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("file.txt"),
        "Expected raw ls output, got: {}",
        stdout
    );
    // Should NOT have compressed format (no size in parens)
    assert!(
        !stdout.contains("("),
        "Bare ls should passthrough without compression: {}",
        stdout
    );
}

#[test]
fn passthrough_ls_recursive() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("sub")).unwrap();
    fs::write(dir.path().join("sub/nested.txt"), "hi").unwrap();

    let output = Command::new(common::binary_path())
        .args(["ls", "-lR"])
        .env("TOKEN_SAVER", "1")
        .current_dir(dir.path())
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    // -R should passthrough, showing subdirectory contents
    assert!(
        stdout.contains("sub") && stdout.contains("nested.txt"),
        "Expected recursive ls output, got: {}",
        stdout
    );
}

#[test]
fn passthrough_ls_without_env() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("file.txt"), "hello").unwrap();

    let output = Command::new(common::binary_path())
        .args(["ls", "-la"])
        .env_remove("TOKEN_SAVER")
        .current_dir(dir.path())
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    // Without TOKEN_SAVER, should get raw ls -la output with permissions
    assert!(
        stdout.contains("drw") || stdout.contains("-rw"),
        "Expected raw ls -la output with permissions, got: {}",
        stdout
    );
}

#[test]
fn passthrough_grep_list_files() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("file.txt"), "hello world\n").unwrap();

    let output = Command::new(common::binary_path())
        .args(["grep", "-l", "hello", "file.txt"])
        .env("TOKEN_SAVER", "1")
        .current_dir(dir.path())
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("file.txt"),
        "Expected grep output with filename, got: {}",
        stdout
    );
}
