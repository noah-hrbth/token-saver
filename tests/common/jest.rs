use std::fs;
use std::path::Path;

use super::{Assertion, Scenario};

/// All jest scenarios. Requires jest installed via npx (uses local package.json).
pub fn scenarios() -> Vec<Scenario> {
    vec![
        Scenario {
            name: "Jest basic failures",
            command: "jest",
            args: &[],
            setup: setup_basic_failures,
            assertions: vec![
                Assertion::Contains("FAIL"),
                Assertion::Contains("✗"),
                Assertion::Contains("failed"),
                Assertion::Contains("passed"),
                Assertion::Contains("suites:"),
            ],
        },
        Scenario {
            name: "Jest all pass",
            command: "jest",
            args: &[],
            setup: setup_all_pass,
            assertions: vec![
                Assertion::NotContains("FAIL"),
                Assertion::NotContains("✗"),
                Assertion::Contains("passed"),
                Assertion::Contains("suites:"),
            ],
        },
        Scenario {
            name: "Jest mixed with skipped",
            command: "jest",
            args: &[],
            setup: setup_mixed_with_skipped,
            assertions: vec![
                Assertion::Contains("passed"),
                Assertion::Contains("skipped"),
                Assertion::Contains("suites:"),
            ],
        },
    ]
}

/// Same scenarios routed through `npx jest`.
pub fn npx_scenarios() -> Vec<Scenario> {
    vec![
        Scenario {
            name: "npx jest basic failures",
            command: "npx",
            args: &["jest"],
            setup: setup_basic_failures,
            assertions: vec![
                Assertion::Contains("FAIL"),
                Assertion::Contains("✗"),
                Assertion::Contains("failed"),
                Assertion::Contains("passed"),
                Assertion::Contains("suites:"),
            ],
        },
        Scenario {
            name: "npx jest all pass",
            command: "npx",
            args: &["jest"],
            setup: setup_all_pass,
            assertions: vec![
                Assertion::NotContains("FAIL"),
                Assertion::NotContains("✗"),
                Assertion::Contains("passed"),
                Assertion::Contains("suites:"),
            ],
        },
        Scenario {
            name: "npx jest mixed with skipped",
            command: "npx",
            args: &["jest"],
            setup: setup_mixed_with_skipped,
            assertions: vec![
                Assertion::Contains("passed"),
                Assertion::Contains("skipped"),
                Assertion::Contains("suites:"),
            ],
        },
    ]
}

fn write_package_json(repo: &Path) {
    fs::write(
        repo.join("package.json"),
        r#"{
  "name": "test-project",
  "private": true,
  "devDependencies": {
    "jest": "^29.0.0"
  }
}"#,
    )
    .unwrap();
}

fn setup_basic_failures(repo: &Path) {
    write_package_json(repo);

    fs::create_dir_all(repo.join("src")).unwrap();

    // A passing test file
    fs::write(
        repo.join("src/utils.test.js"),
        r#"
describe('utils', () => {
    test('adds numbers', () => {
        expect(1 + 1).toBe(2);
    });
    test('concatenates strings', () => {
        expect('a' + 'b').toBe('ab');
    });
});
"#,
    )
    .unwrap();

    // A failing test file
    fs::write(
        repo.join("src/math.test.js"),
        r#"
describe('math', () => {
    test('should add correctly', () => {
        expect(1 + 1).toBe(2);
    });
    test('should handle negative numbers', () => {
        expect(1 + -2).toBe(1);
    });
    test('should multiply', () => {
        expect(2 * 3).toBe(7);
    });
});
"#,
    )
    .unwrap();
}

fn setup_all_pass(repo: &Path) {
    write_package_json(repo);

    fs::create_dir_all(repo.join("src")).unwrap();
    fs::create_dir_all(repo.join("src/api")).unwrap();

    fs::write(
        repo.join("src/utils.test.js"),
        r#"
describe('utils', () => {
    test('adds numbers', () => {
        expect(1 + 1).toBe(2);
    });
    test('subtracts numbers', () => {
        expect(5 - 3).toBe(2);
    });
});
"#,
    )
    .unwrap();

    fs::write(
        repo.join("src/api/auth.test.js"),
        r#"
describe('auth', () => {
    test('returns true for valid token', () => {
        expect(true).toBe(true);
    });
});
"#,
    )
    .unwrap();
}

fn setup_mixed_with_skipped(repo: &Path) {
    write_package_json(repo);

    fs::create_dir_all(repo.join("src")).unwrap();

    fs::write(
        repo.join("src/features.test.js"),
        r#"
describe('features', () => {
    test('implemented feature', () => {
        expect(true).toBe(true);
    });
    test.skip('pending feature', () => {
        expect(false).toBe(true);
    });
    test.todo('future feature');
});
"#,
    )
    .unwrap();
}
