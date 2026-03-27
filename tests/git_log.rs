mod common;

#[test]
fn compressed_basic_log() {
    common::run_test(&common::git_log::scenarios()[0]);
}

#[test]
fn compressed_log_with_body() {
    common::run_test(&common::git_log::scenarios()[1]);
}

#[test]
fn compressed_log_with_patch() {
    common::run_test(&common::git_log::scenarios()[2]);
}

#[test]
fn compressed_log_with_stat() {
    common::run_test(&common::git_log::scenarios()[3]);
}

#[test]
fn compressed_log_with_n() {
    common::run_test(&common::git_log::scenarios()[4]);
}

#[test]
fn compressed_log_empty_result() {
    common::run_test(&common::git_log::scenarios()[5]);
}
