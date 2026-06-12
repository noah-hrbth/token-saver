mod common;

use common::{git_log, scenario_by_name};

#[test]
fn compressed_basic_log() {
    common::run_test(&scenario_by_name(
        git_log::scenarios(),
        "Basic log with multiple commits",
    ));
}

#[test]
fn compressed_log_with_body() {
    common::run_test(&scenario_by_name(
        git_log::scenarios(),
        "Log with commit body",
    ));
}

#[test]
fn compressed_log_with_patch() {
    common::run_test(&scenario_by_name(
        git_log::scenarios(),
        "Log with -p (patches)",
    ));
}

#[test]
fn compressed_log_with_stat() {
    common::run_test(&scenario_by_name(git_log::scenarios(), "Log with --stat"));
}

#[test]
fn compressed_log_with_n() {
    common::run_test(&scenario_by_name(git_log::scenarios(), "Log with -n 2"));
}

#[test]
fn compressed_log_empty_result() {
    common::run_test(&scenario_by_name(git_log::scenarios(), "Empty log result"));
}
