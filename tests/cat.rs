mod common;

#[test]
fn basic_file_content() {
    common::run_test(&common::cat::scenarios()[0]);
}

#[test]
fn truncation_at_1000_lines() {
    common::run_test(&common::cat::scenarios()[1]);
}

#[test]
fn binary_file_detection() {
    common::run_test(&common::cat::scenarios()[2]);
}

#[test]
fn minified_line_collapsing() {
    common::run_test(&common::cat::scenarios()[3]);
}

#[test]
fn empty_file() {
    common::run_test(&common::cat::scenarios()[4]);
}

#[test]
fn multi_file_concatenation_with_cap() {
    common::run_test(&common::cat::scenarios()[5]);
}
