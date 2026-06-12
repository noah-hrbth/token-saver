mod common;

use common::{jest, scenario_by_name};

fn jest_available() -> bool {
    std::process::Command::new("jest")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[test]
fn jest_basic_failures() {
    if !jest_available() {
        eprintln!("Skipping jest test: jest not found in PATH");
        return;
    }
    common::run_test_with_exit_codes(
        &scenario_by_name(jest::scenarios(), "Jest basic failures"),
        &[0, 1],
    );
}

#[test]
fn jest_all_pass() {
    if !jest_available() {
        eprintln!("Skipping jest test: jest not found in PATH");
        return;
    }
    common::run_test_with_exit_codes(
        &scenario_by_name(jest::scenarios(), "Jest all pass"),
        &[0, 1],
    );
}

#[test]
fn jest_mixed_with_skipped() {
    if !jest_available() {
        eprintln!("Skipping jest test: jest not found in PATH");
        return;
    }
    common::run_test_with_exit_codes(
        &scenario_by_name(jest::scenarios(), "Jest mixed with skipped"),
        &[0, 1],
    );
}
