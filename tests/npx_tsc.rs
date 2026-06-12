mod common;

use common::{scenario_by_name, tsc};

fn npx_tsc_available() -> bool {
    common::tsc::is_available()
}

#[test]
fn npx_tsc_clean() {
    if !npx_tsc_available() {
        eprintln!("Skipping npx tsc test: tsc and npm not available");
        return;
    }
    common::run_test_with_exit_codes(
        &scenario_by_name(tsc::npx_scenarios(), "npx tsc clean"),
        &[0, 1, 2],
    );
}

#[test]
fn npx_tsc_single_file_errors() {
    if !npx_tsc_available() {
        eprintln!("Skipping npx tsc test: tsc and npm not available");
        return;
    }
    common::run_test_with_exit_codes(
        &scenario_by_name(tsc::npx_scenarios(), "npx tsc single-file errors"),
        &[0, 1, 2],
    );
}

#[test]
fn npx_tsc_multi_file_errors() {
    if !npx_tsc_available() {
        eprintln!("Skipping npx tsc test: tsc and npm not available");
        return;
    }
    common::run_test_with_exit_codes(
        &scenario_by_name(tsc::npx_scenarios(), "npx tsc multi-file errors"),
        &[0, 1, 2],
    );
}

#[test]
fn npx_tsc_many_errors() {
    if !npx_tsc_available() {
        eprintln!("Skipping npx tsc test: tsc and npm not available");
        return;
    }
    common::run_test_with_exit_codes(
        &scenario_by_name(tsc::npx_scenarios(), "npx tsc many errors across files"),
        &[0, 1, 2],
    );
}

#[test]
fn npx_tsc_dedup_heavy() {
    if !npx_tsc_available() {
        eprintln!("Skipping npx tsc test: tsc and npm not available");
        return;
    }
    common::run_test_with_exit_codes(
        &scenario_by_name(
            tsc::npx_scenarios(),
            "npx tsc dedup heavy — 8 identical errors in one file",
        ),
        &[0, 1, 2],
    );
}

#[test]
fn npx_tsc_chain_errors() {
    if !npx_tsc_available() {
        eprintln!("Skipping npx tsc test: tsc and npm not available");
        return;
    }
    common::run_test_with_exit_codes(
        &scenario_by_name(
            tsc::npx_scenarios(),
            "npx tsc chain errors — interface mismatch with continuations",
        ),
        &[0, 1, 2],
    );
}

#[test]
fn npx_tsc_repeated_pattern() {
    if !npx_tsc_available() {
        eprintln!("Skipping npx tsc test: tsc and npm not available");
        return;
    }
    common::run_test_with_exit_codes(
        &scenario_by_name(
            tsc::npx_scenarios(),
            "npx tsc repeated pattern — 4 files × 3 identical errors",
        ),
        &[0, 1, 2],
    );
}
