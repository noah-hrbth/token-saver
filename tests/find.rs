mod common;

use common::{find, scenario_by_name};

#[test]
fn basic_find_with_noise_filtering() {
    common::run_test(&scenario_by_name(
        find::scenarios(),
        "Basic find with noise filtering",
    ));
}

#[test]
fn targeted_find_with_name() {
    common::run_test(&scenario_by_name(
        find::scenarios(),
        "Targeted find with -name",
    ));
}

#[test]
fn find_directories_only() {
    common::run_test(&scenario_by_name(
        find::scenarios(),
        "Find directories only",
    ));
}

#[test]
fn tree_structure_with_sorting() {
    common::run_test(&scenario_by_name(
        find::scenarios(),
        "Tree structure with sorting",
    ));
}
