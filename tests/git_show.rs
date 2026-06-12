mod common;

use common::{git_show, scenario_by_name};

#[test]
fn compressed_show_basic() {
    common::run_test(&scenario_by_name(
        git_show::scenarios(),
        "Basic git show HEAD",
    ));
}

#[test]
fn compressed_show_no_patch() {
    common::run_test(&scenario_by_name(
        git_show::scenarios(),
        "Show with --no-patch",
    ));
}

#[test]
fn compressed_show_with_body() {
    common::run_test(&scenario_by_name(
        git_show::scenarios(),
        "Show with commit body",
    ));
}

#[test]
fn compressed_show_new_file() {
    common::run_test(&scenario_by_name(git_show::scenarios(), "Show new file"));
}

#[test]
fn compressed_show_deleted_file() {
    common::run_test(&scenario_by_name(
        git_show::scenarios(),
        "Show deleted file",
    ));
}

#[test]
fn compressed_show_multi_file() {
    common::run_test(&scenario_by_name(
        git_show::scenarios(),
        "Show multi-file commit",
    ));
}

#[test]
fn compressed_show_annotated_tag() {
    common::run_test(&scenario_by_name(
        git_show::scenarios(),
        "Show annotated tag",
    ));
}
