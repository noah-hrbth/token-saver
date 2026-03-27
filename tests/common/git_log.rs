use std::path::Path;
use std::process::Command;

use super::{Assertion, Scenario};

/// All git log scenarios. Shared by integration tests and compare runner.
pub fn scenarios() -> Vec<Scenario> {
    vec![
        Scenario {
            name: "Basic log with multiple commits",
            command: "git",
            args: &["log"],
            setup: setup_multiple_commits,
            assertions: vec![
                Assertion::Contains("*"),
                Assertion::Contains("[Test]"),
                Assertion::Contains("init"),
                Assertion::Contains("second commit"),
                Assertion::Contains("third commit"),
                Assertion::NotContains("Author:"),
                Assertion::NotContains("Date:"),
            ],
        },
        Scenario {
            name: "Log with commit body",
            command: "git",
            args: &["log", "-n", "1"],
            setup: setup_commit_with_body,
            assertions: vec![
                Assertion::Contains("commit with body"),
                Assertion::Contains("  This is the detailed body"),
            ],
        },
        Scenario {
            name: "Log with -p (patches)",
            command: "git",
            args: &["log", "-p", "-n", "1"],
            setup: setup_file_change,
            assertions: vec![
                Assertion::Contains("modify file"),
                Assertion::Contains("+modified content"),
            ],
        },
        Scenario {
            name: "Log with --stat",
            command: "git",
            args: &["log", "--stat", "-n", "1"],
            setup: setup_file_change,
            assertions: vec![
                Assertion::Contains("modify file"),
                Assertion::Contains("feature.rs"),
            ],
        },
        Scenario {
            name: "Log with -n 2",
            command: "git",
            args: &["log", "-n", "2"],
            setup: setup_multiple_commits,
            assertions: vec![
                Assertion::Contains("third commit"),
                Assertion::Contains("second commit"),
                Assertion::NotContains("(showing"),
            ],
        },
        Scenario {
            name: "Empty log result",
            command: "git",
            args: &["log", "--author=nobody@example.com"],
            setup: |_| {},
            assertions: vec![Assertion::Contains("(empty)")],
        },
    ]
}

fn setup_multiple_commits(path: &Path) {
    std::fs::write(path.join("file1.txt"), "first").unwrap();
    Command::new("git")
        .args(["add", "file1.txt"])
        .current_dir(path)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "second commit"])
        .current_dir(path)
        .output()
        .unwrap();

    std::fs::write(path.join("file2.txt"), "second").unwrap();
    Command::new("git")
        .args(["add", "file2.txt"])
        .current_dir(path)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "third commit"])
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

fn setup_file_change(path: &Path) {
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
