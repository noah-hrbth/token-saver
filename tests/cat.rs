mod common;

use common::{cat, scenario_by_name};

#[test]
fn basic_file_content() {
    common::run_test(&scenario_by_name(cat::scenarios(), "Basic file content"));
}

#[test]
fn truncation_at_1000_lines() {
    common::run_test(&scenario_by_name(
        cat::scenarios(),
        "Truncation at 1000 lines",
    ));
}

#[test]
fn binary_file_detection() {
    common::run_test(&scenario_by_name(cat::scenarios(), "Binary file detection"));
}

#[test]
fn minified_line_collapsing() {
    common::run_test(&scenario_by_name(
        cat::scenarios(),
        "Minified line collapsing",
    ));
}

#[test]
fn empty_file() {
    common::run_test(&scenario_by_name(cat::scenarios(), "Empty file"));
}

#[test]
fn multi_file_concatenation_with_cap() {
    common::run_test(&scenario_by_name(
        cat::scenarios(),
        "Multi-file concatenation with cap",
    ));
}
