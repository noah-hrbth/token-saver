mod common;

use common::{prettier, scenario_by_name};

fn npx_prettier_available() -> bool {
    std::process::Command::new("npx")
        .args(["prettier", "--version"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

macro_rules! require_npx_prettier {
    () => {
        if !npx_prettier_available() {
            eprintln!("Skipping npx prettier test: npx prettier not available");
            return;
        }
    };
}

#[test]
fn npx_prettier_check_single_file() {
    require_npx_prettier!();
    common::run_test_with_exit_codes(
        &scenario_by_name(
            prettier::npx_scenarios(),
            "npx prettier --check single file",
        ),
        &[0, 1],
    );
}

#[test]
fn npx_prettier_check_many_files() {
    require_npx_prettier!();
    common::run_test_with_exit_codes(
        &scenario_by_name(prettier::npx_scenarios(), "npx prettier --check many files"),
        &[0, 1],
    );
}

#[test]
fn npx_prettier_check_nested_dirs() {
    require_npx_prettier!();
    common::run_test_with_exit_codes(
        &scenario_by_name(
            prettier::npx_scenarios(),
            "npx prettier --check nested dirs",
        ),
        &[0, 1],
    );
}

#[test]
fn npx_prettier_check_clean() {
    require_npx_prettier!();
    common::run_test_with_exit_codes(
        &scenario_by_name(
            prettier::npx_scenarios(),
            "npx prettier --check clean project",
        ),
        &[0, 1],
    );
}

#[test]
fn npx_prettier_write_many_files() {
    require_npx_prettier!();
    common::run_test_with_exit_codes(
        &scenario_by_name(prettier::npx_scenarios(), "npx prettier --write many files"),
        &[0, 1],
    );
}
