mod common;

use common::{git_status, scenario_by_name};

#[test]
fn compressed_clean_repo() {
    common::run_test(&scenario_by_name(
        git_status::scenarios(),
        "Clean repository",
    ));
}

#[test]
fn compressed_modified_file() {
    common::run_test(&scenario_by_name(
        git_status::scenarios(),
        "Modified file (unstaged)",
    ));
}

#[test]
fn compressed_untracked_files() {
    common::run_test(&scenario_by_name(
        git_status::scenarios(),
        "Untracked files",
    ));
}

#[test]
fn compressed_staged_files() {
    common::run_test(&scenario_by_name(git_status::scenarios(), "Staged files"));
}

#[test]
fn compressed_mixed_changes() {
    common::run_test(&scenario_by_name(
        git_status::scenarios(),
        "Mixed changes (staged + modified + untracked)",
    ));
}

#[test]
fn compressed_deleted_file() {
    common::run_test(&scenario_by_name(git_status::scenarios(), "Deleted file"));
}

#[test]
fn compressed_many_files() {
    common::run_test(&scenario_by_name(
        git_status::scenarios(),
        "Many files (modified + deleted + staged + untracked)",
    ));
}
