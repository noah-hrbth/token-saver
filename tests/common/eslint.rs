use std::fs;
use std::path::Path;

use super::{Assertion, Scenario};

/// All eslint scenarios. Requires eslint installed globally.
pub fn scenarios() -> Vec<Scenario> {
    vec![
        Scenario {
            name: "ESLint basic errors",
            command: "eslint",
            args: &["src/main.js"],
            setup: setup_basic_errors,
            assertions: vec![
                Assertion::Contains("src/main.js"),
                Assertion::Contains("error"),
                Assertion::Contains("problems"),
            ],
        },
        Scenario {
            name: "ESLint clean project",
            command: "eslint",
            args: &["src/clean.js"],
            setup: setup_clean,
            assertions: vec![
                Assertion::NotContains("error"),
                Assertion::NotContains("warn"),
            ],
        },
        Scenario {
            name: "ESLint warnings and errors grouped",
            command: "eslint",
            args: &["src/"],
            setup: setup_mixed,
            assertions: vec![
                Assertion::Contains("error"),
                Assertion::Contains("warn"),
                Assertion::Contains("problems"),
            ],
        },
    ]
}

fn setup_basic_errors(repo: &Path) {
    fs::create_dir_all(repo.join("src")).unwrap();

    // ESLint 9+ flat config
    fs::write(
        repo.join("eslint.config.js"),
        r#"module.exports = [{ rules: { "no-undef": "error", "no-unused-vars": "warn" } }];"#,
    )
    .unwrap();

    fs::write(
        repo.join("src/main.js"),
        "const x = 1;\nconsole.log(undefinedVar);\n",
    )
    .unwrap();
}

fn setup_clean(repo: &Path) {
    fs::create_dir_all(repo.join("src")).unwrap();

    fs::write(
        repo.join("eslint.config.js"),
        r#"module.exports = [{ rules: {} }];"#,
    )
    .unwrap();

    fs::write(repo.join("src/clean.js"), "var x = 1;\n").unwrap();
}

fn setup_mixed(repo: &Path) {
    fs::create_dir_all(repo.join("src")).unwrap();

    fs::write(
        repo.join("eslint.config.js"),
        r#"module.exports = [{ rules: { "no-undef": "error", "no-console": "warn", "no-unused-vars": "warn" } }];"#,
    )
    .unwrap();

    fs::write(
        repo.join("src/a.js"),
        "const x = 1;\nconsole.log(undefinedVar);\n",
    )
    .unwrap();

    fs::write(repo.join("src/b.js"), "console.log('hello');\n").unwrap();
}
