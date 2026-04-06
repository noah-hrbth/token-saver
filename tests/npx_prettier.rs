mod common;

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
    common::run_test_with_exit_codes(&common::prettier::npx_scenarios()[0], &[0, 1]);
}

#[test]
fn npx_prettier_check_many_files() {
    require_npx_prettier!();
    common::run_test_with_exit_codes(&common::prettier::npx_scenarios()[1], &[0, 1]);
}

#[test]
fn npx_prettier_check_nested_dirs() {
    require_npx_prettier!();
    common::run_test_with_exit_codes(&common::prettier::npx_scenarios()[2], &[0, 1]);
}

#[test]
fn npx_prettier_check_clean() {
    require_npx_prettier!();
    common::run_test_with_exit_codes(&common::prettier::npx_scenarios()[3], &[0, 1]);
}

#[test]
fn npx_prettier_write_many_files() {
    require_npx_prettier!();
    common::run_test_with_exit_codes(&common::prettier::npx_scenarios()[4], &[0, 1]);
}
