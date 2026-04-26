mod common;

fn tsc_available() -> bool {
    common::tsc::is_available()
}

#[test]
fn tsc_clean() {
    if !tsc_available() {
        eprintln!("Skipping tsc test: tsc and npm not available");
        return;
    }
    common::run_test_with_exit_codes(&common::tsc::scenarios()[0], &[0, 1, 2]);
}

#[test]
fn tsc_single_file_errors() {
    if !tsc_available() {
        eprintln!("Skipping tsc test: tsc and npm not available");
        return;
    }
    common::run_test_with_exit_codes(&common::tsc::scenarios()[1], &[0, 1, 2]);
}

#[test]
fn tsc_multi_file_errors() {
    if !tsc_available() {
        eprintln!("Skipping tsc test: tsc and npm not available");
        return;
    }
    common::run_test_with_exit_codes(&common::tsc::scenarios()[2], &[0, 1, 2]);
}

#[test]
fn tsc_many_errors() {
    if !tsc_available() {
        eprintln!("Skipping tsc test: tsc and npm not available");
        return;
    }
    common::run_test_with_exit_codes(&common::tsc::scenarios()[3], &[0, 1, 2]);
}

#[test]
fn tsc_dedup_heavy() {
    if !tsc_available() {
        eprintln!("Skipping tsc test: tsc and npm not available");
        return;
    }
    common::run_test_with_exit_codes(&common::tsc::scenarios()[4], &[0, 1, 2]);
}

#[test]
fn tsc_chain_errors() {
    if !tsc_available() {
        eprintln!("Skipping tsc test: tsc and npm not available");
        return;
    }
    common::run_test_with_exit_codes(&common::tsc::scenarios()[5], &[0, 1, 2]);
}

#[test]
fn tsc_repeated_pattern() {
    if !tsc_available() {
        eprintln!("Skipping tsc test: tsc and npm not available");
        return;
    }
    common::run_test_with_exit_codes(&common::tsc::scenarios()[6], &[0, 1, 2]);
}
