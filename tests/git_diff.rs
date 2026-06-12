mod common;

use common::{git_diff, scenario_by_name};

#[test]
fn compressed_unstaged_changes() {
    common::run_test(&scenario_by_name(
        git_diff::scenarios(),
        "Unstaged working tree changes",
    ));
}

#[test]
fn compressed_staged_changes() {
    common::run_test(&scenario_by_name(git_diff::scenarios(), "Staged changes"));
}

#[test]
fn compressed_commit_comparison() {
    common::run_test(&scenario_by_name(
        git_diff::scenarios(),
        "Commit-to-commit comparison",
    ));
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
    common::run_test(&scenario_by_name(
        git_diff::scenarios(),
        "New file added (staged)",
    ));
}

#[test]
fn compressed_deleted_file_staged() {
    common::run_test(&scenario_by_name(
        git_diff::scenarios(),
        "File deleted (staged)",
    ));
}

#[test]
fn compressed_multiple_files() {
    common::run_test(&scenario_by_name(
        git_diff::scenarios(),
        "Multiple files changed",
    ));
}

#[test]
fn compressed_diff_stat() {
    common::run_test(&scenario_by_name(
        git_diff::scenarios(),
        "Diff stat compressed",
    ));
}

#[test]
fn compressed_internal_whitespace_not_collapsed() {
    common::run_test(&scenario_by_name(
        git_diff::scenarios(),
        "Internal whitespace change not collapsed",
    ));
}

#[test]
fn compressed_python_indentation_not_collapsed() {
    common::run_test(&scenario_by_name(
        git_diff::scenarios(),
        "Python indentation change not collapsed",
    ));
}
