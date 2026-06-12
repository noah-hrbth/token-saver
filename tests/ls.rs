mod common;

use common::{ls, scenario_by_name};

#[test]
fn compressed_mixed_types() {
    common::run_test(&scenario_by_name(ls::scenarios(), "Mixed file types"));
}

#[test]
fn compressed_hidden_files() {
    common::run_test(&scenario_by_name(ls::scenarios(), "Hidden files included"));
}

#[test]
fn compressed_l_normalizes_to_la() {
    common::run_test(&scenario_by_name(
        ls::scenarios(),
        "ls -l normalizes to include hidden",
    ));
}

#[test]
fn compressed_symlinks() {
    common::run_test(&scenario_by_name(ls::scenarios(), "Symlinks show targets"));
}

#[test]
fn compressed_with_path_arg() {
    common::run_test(&scenario_by_name(
        ls::scenarios(),
        "ls -l with path argument",
    ));
}

#[test]
fn compressed_multi_space_filename() {
    common::run_test(&scenario_by_name(
        ls::scenarios(),
        "Filename with consecutive spaces",
    ));
}
