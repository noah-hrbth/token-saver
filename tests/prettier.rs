mod common;

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
    common::run_test_with_exit_codes(&common::prettier::scenarios()[0], &[0, 1]);
}

#[test]
fn prettier_check_many_files() {
    if !prettier_available() {
        eprintln!("Skipping prettier test: prettier not found in PATH");
        return;
    }
    common::run_test_with_exit_codes(&common::prettier::scenarios()[1], &[0, 1]);
}

#[test]
fn prettier_check_nested_dirs() {
    if !prettier_available() {
        eprintln!("Skipping prettier test: prettier not found in PATH");
        return;
    }
    common::run_test_with_exit_codes(&common::prettier::scenarios()[2], &[0, 1]);
}

#[test]
fn prettier_check_clean() {
    if !prettier_available() {
        eprintln!("Skipping prettier test: prettier not found in PATH");
        return;
    }
    common::run_test_with_exit_codes(&common::prettier::scenarios()[3], &[0, 1]);
}

#[test]
fn prettier_write_many_files() {
    if !prettier_available() {
        eprintln!("Skipping prettier test: prettier not found in PATH");
        return;
    }
    common::run_test_with_exit_codes(&common::prettier::scenarios()[4], &[0, 1]);
}
