mod common;

fn eslint_available() -> bool {
    std::process::Command::new("eslint")
        .arg("--version")
        .output()
        .ok()
        .and_then(|o| {
            let v = String::from_utf8_lossy(&o.stdout);
            let major: u32 = v
                .trim()
                .trim_start_matches('v')
                .split('.')
                .next()?
                .parse()
                .ok()?;
            Some(major >= 9)
        })
        .unwrap_or(false)
}

#[test]
fn eslint_basic_errors() {
    if !eslint_available() {
        eprintln!("Skipping eslint test: eslint 9+ not found in PATH");
        return;
    }
    common::run_test_with_exit_codes(&common::eslint::scenarios()[0], &[0, 1]);
}

#[test]
fn eslint_clean_project() {
    if !eslint_available() {
        eprintln!("Skipping eslint test: eslint 9+ not found in PATH");
        return;
    }
    common::run_test_with_exit_codes(&common::eslint::scenarios()[1], &[0, 1]);
}

#[test]
fn eslint_warnings_and_errors_grouped() {
    if !eslint_available() {
        eprintln!("Skipping eslint test: eslint 9+ not found in PATH");
        return;
    }
    common::run_test_with_exit_codes(&common::eslint::scenarios()[2], &[0, 1]);
}
