mod common;

use common::{scenario_by_name, tsc};

fn tsc_available() -> bool {
    common::tsc::is_available()
}

#[test]
fn tsc_clean() {
    if !tsc_available() {
        eprintln!("Skipping tsc test: tsc and npm not available");
        return;
    }
    common::run_test_with_exit_codes(&scenario_by_name(tsc::scenarios(), "TSC clean"), &[0, 1, 2]);
}

#[test]
fn tsc_single_file_errors() {
    if !tsc_available() {
        eprintln!("Skipping tsc test: tsc and npm not available");
        return;
    }
    common::run_test_with_exit_codes(
        &scenario_by_name(tsc::scenarios(), "TSC single-file errors"),
        &[0, 1, 2],
    );
}

#[test]
fn tsc_multi_file_errors() {
    if !tsc_available() {
        eprintln!("Skipping tsc test: tsc and npm not available");
        return;
    }
    common::run_test_with_exit_codes(
        &scenario_by_name(tsc::scenarios(), "TSC multi-file errors"),
        &[0, 1, 2],
    );
}

#[test]
fn tsc_many_errors() {
    if !tsc_available() {
        eprintln!("Skipping tsc test: tsc and npm not available");
        return;
    }
    common::run_test_with_exit_codes(
        &scenario_by_name(tsc::scenarios(), "TSC many errors across files"),
        &[0, 1, 2],
    );
}

#[test]
fn tsc_dedup_heavy() {
    if !tsc_available() {
        eprintln!("Skipping tsc test: tsc and npm not available");
        return;
    }
    common::run_test_with_exit_codes(
        &scenario_by_name(
            tsc::scenarios(),
            "TSC dedup heavy — 8 identical errors in one file",
        ),
        &[0, 1, 2],
    );
}

#[test]
fn tsc_chain_errors() {
    if !tsc_available() {
        eprintln!("Skipping tsc test: tsc and npm not available");
        return;
    }
    common::run_test_with_exit_codes(
        &scenario_by_name(
            tsc::scenarios(),
            "TSC chain errors — interface mismatch with continuations",
        ),
        &[0, 1, 2],
    );
}

#[test]
fn tsc_repeated_pattern() {
    if !tsc_available() {
        eprintln!("Skipping tsc test: tsc and npm not available");
        return;
    }
    common::run_test_with_exit_codes(
        &scenario_by_name(
            tsc::scenarios(),
            "TSC repeated pattern — 4 files × 3 identical errors",
        ),
        &[0, 1, 2],
    );
}
