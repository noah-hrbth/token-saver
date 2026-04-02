mod common;

#[test]
fn compressed_mixed_types() {
    common::run_test(&common::ls::scenarios()[0]);
}

#[test]
fn compressed_hidden_files() {
    common::run_test(&common::ls::scenarios()[1]);
}

#[test]
fn compressed_l_normalizes_to_la() {
    common::run_test(&common::ls::scenarios()[2]);
}

#[test]
fn compressed_symlinks() {
    common::run_test(&common::ls::scenarios()[3]);
}

#[test]
fn compressed_with_path_arg() {
    common::run_test(&common::ls::scenarios()[4]);
}
