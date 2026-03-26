mod common;

#[test]
fn compressed_unstaged_changes() {
    common::run_test(&common::git_diff::scenarios()[0]);
}

#[test]
fn compressed_staged_changes() {
    common::run_test(&common::git_diff::scenarios()[1]);
}

#[test]
fn compressed_commit_comparison() {
    common::run_test(&common::git_diff::scenarios()[2]);
}

#[test]
fn compressed_clean_repo_diff() {
    let repo = common::create_temp_repo();
    let output = std::process::Command::new(common::binary_path())
        .args(["git", "diff"])
        .env("TOKEN_SAVER", "1")
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).trim().is_empty());
}

#[test]
fn compressed_new_file_staged() {
    common::run_test(&common::git_diff::scenarios()[3]);
}

#[test]
fn compressed_deleted_file_staged() {
    common::run_test(&common::git_diff::scenarios()[4]);
}

#[test]
fn compressed_multiple_files() {
    common::run_test(&common::git_diff::scenarios()[5]);
}

#[test]
fn passthrough_diff_stat() {
    let repo = common::create_temp_repo();
    std::fs::write(repo.path().join("README.md"), "changed").unwrap();

    let raw = std::process::Command::new("git")
        .args(["diff", "--stat"])
        .current_dir(repo.path())
        .output()
        .unwrap();

    let compressed = std::process::Command::new(common::binary_path())
        .args(["git", "diff", "--stat"])
        .env("TOKEN_SAVER", "1")
        .current_dir(repo.path())
        .output()
        .unwrap();

    assert_eq!(
        String::from_utf8_lossy(&raw.stdout),
        String::from_utf8_lossy(&compressed.stdout),
        "Skip flag --stat should passthrough unchanged"
    );
}
