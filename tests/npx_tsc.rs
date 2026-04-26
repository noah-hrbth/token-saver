mod common;

fn npx_tsc_available() -> bool {
    common::tsc::is_available()
}

#[test]
fn npx_tsc_clean() {
    if !npx_tsc_available() {
        eprintln!("Skipping npx tsc test: tsc and npm not available");
        return;
    }
    common::run_test_with_exit_codes(&common::tsc::npx_scenarios()[0], &[0, 1, 2]);
}

#[test]
fn npx_tsc_single_file_errors() {
    if !npx_tsc_available() {
        eprintln!("Skipping npx tsc test: tsc and npm not available");
        return;
    }
    common::run_test_with_exit_codes(&common::tsc::npx_scenarios()[1], &[0, 1, 2]);
}

#[test]
fn npx_tsc_multi_file_errors() {
    if !npx_tsc_available() {
        eprintln!("Skipping npx tsc test: tsc and npm not available");
        return;
    }
    common::run_test_with_exit_codes(&common::tsc::npx_scenarios()[2], &[0, 1, 2]);
}

#[test]
fn npx_tsc_many_errors() {
    if !npx_tsc_available() {
        eprintln!("Skipping npx tsc test: tsc and npm not available");
        return;
    }
    common::run_test_with_exit_codes(&common::tsc::npx_scenarios()[3], &[0, 1, 2]);
}

#[test]
fn npx_tsc_dedup_heavy() {
    if !npx_tsc_available() {
        eprintln!("Skipping npx tsc test: tsc and npm not available");
        return;
    }
    common::run_test_with_exit_codes(&common::tsc::npx_scenarios()[4], &[0, 1, 2]);
}

#[test]
fn npx_tsc_chain_errors() {
    if !npx_tsc_available() {
        eprintln!("Skipping npx tsc test: tsc and npm not available");
        return;
    }
    common::run_test_with_exit_codes(&common::tsc::npx_scenarios()[5], &[0, 1, 2]);
}

#[test]
fn npx_tsc_repeated_pattern() {
    if !npx_tsc_available() {
        eprintln!("Skipping npx tsc test: tsc and npm not available");
        return;
    }
    common::run_test_with_exit_codes(&common::tsc::npx_scenarios()[6], &[0, 1, 2]);
}
