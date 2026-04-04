mod common;

#[test]
fn basic_find_with_noise_filtering() {
    common::run_test(&common::find::scenarios()[0]);
}

#[test]
fn targeted_find_with_name() {
    common::run_test(&common::find::scenarios()[1]);
}

#[test]
fn find_directories_only() {
    common::run_test(&common::find::scenarios()[2]);
}

#[test]
fn tree_structure_with_sorting() {
    common::run_test(&common::find::scenarios()[3]);
}
