mod common;

#[test]
fn compressed_show_basic() {
    common::run_test(&common::git_show::scenarios()[0]);
}

#[test]
fn compressed_show_no_patch() {
    common::run_test(&common::git_show::scenarios()[1]);
}

#[test]
fn compressed_show_with_body() {
    common::run_test(&common::git_show::scenarios()[2]);
}

#[test]
fn compressed_show_new_file() {
    common::run_test(&common::git_show::scenarios()[3]);
}

#[test]
fn compressed_show_deleted_file() {
    common::run_test(&common::git_show::scenarios()[4]);
}

#[test]
fn compressed_show_multi_file() {
    common::run_test(&common::git_show::scenarios()[5]);
}

#[test]
fn compressed_show_annotated_tag() {
    common::run_test(&common::git_show::scenarios()[6]);
}
