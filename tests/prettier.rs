mod common;

use common::{prettier, scenario_by_name};

fn prettier_available() -> bool {
    std::process::Command::new("prettier")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[test]
fn prettier_check_single_file() {
    if !prettier_available() {
        eprintln!("Skipping prettier test: prettier not found in PATH");
        return;
    }
    common::run_test_with_exit_codes(
        &scenario_by_name(prettier::scenarios(), "Prettier --check single file"),
        &[0, 1],
    );
}

#[test]
fn prettier_check_many_files() {
    if !prettier_available() {
        eprintln!("Skipping prettier test: prettier not found in PATH");
        return;
    }
    common::run_test_with_exit_codes(
        &scenario_by_name(prettier::scenarios(), "Prettier --check many files"),
        &[0, 1],
    );
}

#[test]
fn prettier_check_nested_dirs() {
    if !prettier_available() {
        eprintln!("Skipping prettier test: prettier not found in PATH");
        return;
    }
    common::run_test_with_exit_codes(
        &scenario_by_name(prettier::scenarios(), "Prettier --check nested dirs"),
        &[0, 1],
    );
}

#[test]
fn prettier_check_clean() {
    if !prettier_available() {
        eprintln!("Skipping prettier test: prettier not found in PATH");
        return;
    }
    common::run_test_with_exit_codes(
        &scenario_by_name(prettier::scenarios(), "Prettier --check clean project"),
        &[0, 1],
    );
}

#[test]
fn prettier_write_many_files() {
    if !prettier_available() {
        eprintln!("Skipping prettier test: prettier not found in PATH");
        return;
    }
    common::run_test_with_exit_codes(
        &scenario_by_name(prettier::scenarios(), "Prettier --write many files"),
        &[0, 1],
    );
}
