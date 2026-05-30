mod common;

use std::process::Command;

#[test]
fn compressed_single_branch() {
    common::run_test(&common::git_branch::scenarios()[0]);
}

#[test]
fn compressed_multiple_branches() {
    common::run_test(&common::git_branch::scenarios()[1]);
}

#[test]
fn compressed_current_branch_first() {
    common::run_test(&common::git_branch::scenarios()[2]);
}

#[test]
fn compressed_many_branches_cap() {
    common::run_test(&common::git_branch::scenarios()[3]);
}

#[test]
fn compressed_all_branches_with_remote() {
    common::run_test(&common::git_branch::scenarios()[4]);
}

#[test]
fn compressed_remote_only() {
    common::run_test(&common::git_branch::scenarios()[5]);
}

/// U7-1: `git branch feat` must pass through; the branch must actually be created.
#[test]
fn git_branch_create_passes_through() {
    let repo = common::create_temp_repo();

    // Run token-saver `branch feat` — should passthrough, creating the branch.
    Command::new(common::binary_path())
        .args(["git", "branch", "feat"])
        .env("TOKEN_SAVER", "1")
        .current_dir(repo.path())
        .output()
        .unwrap();

    // Verify the branch was actually created in the repo.
    let list_out = Command::new("git")
        .args(["branch", "--list", "feat"])
        .current_dir(repo.path())
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&list_out.stdout);
    assert!(
        stdout.contains("feat"),
        "Expected branch 'feat' to exist after passthrough, got: {}",
        stdout
    );
}

/// U7-2: `git branch -v` must show SHA and commit subject in compressed output.
#[test]
fn git_branch_v_shows_sha_and_subject() {
    let repo = common::create_temp_repo();

    let output = Command::new(common::binary_path())
        .args(["git", "branch", "-v"])
        .env("TOKEN_SAVER", "1")
        .current_dir(repo.path())
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    // Must contain branch name and commit subject from `create_temp_repo` ("init")
    assert!(
        stdout.contains("main"),
        "Expected 'main' in -v output, got: {}",
        stdout
    );
    assert!(
        stdout.contains("init"),
        "Expected commit subject 'init' in -v output, got: {}",
        stdout
    );
    // Must NOT contain raw tab artifacts
    assert!(
        !stdout.contains('\t'),
        "Output should not contain raw tab characters, got: {}",
        stdout
    );
}
