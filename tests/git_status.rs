mod common;

#[test]
fn compressed_clean_repo() {
    common::run_test(&common::git_status::scenarios()[0]);
}

#[test]
fn compressed_modified_file() {
    common::run_test(&common::git_status::scenarios()[1]);
}

#[test]
fn compressed_untracked_files() {
    common::run_test(&common::git_status::scenarios()[2]);
}

#[test]
fn compressed_staged_files() {
    common::run_test(&common::git_status::scenarios()[3]);
}

#[test]
fn compressed_mixed_changes() {
    common::run_test(&common::git_status::scenarios()[4]);
}

#[test]
fn compressed_deleted_file() {
    common::run_test(&common::git_status::scenarios()[5]);
}

#[test]
fn compressed_many_files() {
    common::run_test(&common::git_status::scenarios()[6]);
}
