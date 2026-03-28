use std::path::Path;
use std::process::Command;

use super::{Assertion, Scenario};

/// All git show scenarios. Shared by integration tests and compare runner.
pub fn scenarios() -> Vec<Scenario> {
    vec\![
        Scenario {
            name: "Show HEAD commit (no diff)",
            command: "git",
            args: &["show", "--no-patch", "HEAD"],
            setup: setup_simple_commit,
            assertions: vec\![
                Assertion::Contains("add feature"),
                Assertion::Contains("[Test]"),
            ],
        },
        Scenario {
            name: "Show HEAD commit with diff",
            command: "git",
            args: &["show", "HEAD"],
            setup: setup_simple_commit,
            assertions: vec\![
                Assertion::Contains("add feature"),
                Assertion::Contains("feature.rs"),
                Assertion::Contains("+original content"),
            ],
        },
        Scenario {
            name: "Show commit with body",
            command: "git",
            args: &["show", "--no-patch", "HEAD"],
            setup: setup_commit_with_body,
            assertions: vec\![
                Assertion::Contains("commit with body"),
                Assertion::Contains("  This is the detailed body"),
            ],
        },
        Scenario {
            name: "Show commit with multi-file diff",
            command: "git",
            args: &["show", "HEAD"],
            setup: setup_multi_file_commit,
            assertions: vec\![
                Assertion::Contains("add multiple files"),
                Assertion::Contains("files changed"),
                Assertion::Contains("alpha.rs"),
                Assertion::Contains("beta.rs"),
            ],
        },
        Scenario {
            name: "Show commit with file modification",
            command: "git",
            args: &["show", "HEAD"],
            setup: setup_file_modification,
            assertions: vec\![
                Assertion::Contains("modify file"),
                Assertion::Contains("feature.rs"),
                Assertion::Contains("+modified content"),
                Assertion::Contains("-original content"),
            ],
        },
    ]
}

fn setup_simple_commit(path: &Path) {
    std::fs::write(path.join("feature.rs"), "original content").unwrap();
    Command::new("git")
        .args(["add", "feature.rs"])
        .current_dir(path)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "add feature"])
        .current_dir(path)
        .output()
        .unwrap();
}

fn setup_commit_with_body(path: &Path) {
    std::fs::write(path.join("body.txt"), "content").unwrap();
    Command::new("git")
        .args(["add", "body.txt"])
        .current_dir(path)
        .output()
        .unwrap();
    Command::new("git")
        .args([
            "commit",
            "-m",
            "commit with body\n\nThis is the detailed body.\nWith multiple lines.",
        ])
        .current_dir(path)
        .output()
        .unwrap();
}

fn setup_multi_file_commit(path: &Path) {
    std::fs::write(path.join("alpha.rs"), "fn alpha() {}").unwrap();
    std::fs::write(path.join("beta.rs"), "fn beta() {}").unwrap();
    Command::new("git")
        .args(["add", "alpha.rs", "beta.rs"])
        .current_dir(path)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "add multiple files"])
        .current_dir(path)
        .output()
        .unwrap();
}

fn setup_file_modification(path: &Path) {
    std::fs::write(path.join("feature.rs"), "original content").unwrap();
    Command::new("git")
        .args(["add", "feature.rs"])
        .current_dir(path)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "add feature"])
        .current_dir(path)
        .output()
        .unwrap();

    std::fs::write(path.join("feature.rs"), "modified content").unwrap();
    Command::new("git")
        .args(["add", "feature.rs"])
        .current_dir(path)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "modify file"])
        .current_dir(path)
        .output()
        .unwrap();
}
