mod common;

#[test]
fn grep_recursive_multifile_grouping() {
    common::run_test(&common::grep::scenarios()[0]);
}

#[test]
fn grep_with_context() {
    common::run_test(&common::grep::scenarios()[1]);
}

#[test]
fn grep_single_file_no_grouping() {
    common::run_test(&common::grep::scenarios()[2]);
}

#[test]
fn grep_many_matches_cap() {
    common::run_test(&common::grep::scenarios()[3]);
}

#[test]
fn rg_recursive_multifile_grouping() {
    // Skip if rg is not available
    if std::process::Command::new("rg")
        .arg("--version")
        .output()
        .is_err()
    {
        eprintln!("Skipping rg test: rg not found in PATH");
        return;
    }
    common::run_test(&common::grep::scenarios()[4]);
}
