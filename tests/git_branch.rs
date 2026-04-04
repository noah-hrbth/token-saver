mod common;

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
