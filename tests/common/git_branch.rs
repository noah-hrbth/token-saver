use std::path::Path;
use std::process::Command;

use super::{Assertion, Scenario};

pub fn scenarios() -> Vec<Scenario> {
    vec![
        Scenario {
            name: "Single branch (only main)",
            command: "git",
            args: &["branch"],
            setup: setup_default,
            assertions: vec![Assertion::Contains("* "), Assertion::Contains("main")],
        },
        Scenario {
            name: "Multiple local branches",
            command: "git",
            args: &["branch"],
            setup: setup_multiple_branches,
            assertions: vec![
                Assertion::Contains("* "),
                Assertion::Contains("feature-x"),
                Assertion::Contains("hotfix"),
            ],
        },
        Scenario {
            name: "Current branch pinned first",
            command: "git",
            args: &["branch"],
            setup: setup_alphabetical,
            assertions: vec![Assertion::Contains("* "), Assertion::Contains("aaa-first")],
        },
        Scenario {
            name: "Many branches (60+, triggers cap)",
            command: "git",
            args: &["branch"],
            setup: setup_many_branches,
            assertions: vec![
                Assertion::Contains("* "),
                Assertion::Contains("... and"),
                Assertion::Contains("total)"),
            ],
        },
        Scenario {
            name: "All branches with remote",
            command: "git",
            args: &["branch", "-a"],
            setup: setup_with_remote,
            assertions: vec![Assertion::Contains("* "), Assertion::Contains("remotes/")],
        },
        Scenario {
            name: "Remote branches only",
            command: "git",
            args: &["branch", "-r"],
            setup: setup_with_remote,
            assertions: vec![
                Assertion::Contains("remotes/"),
                Assertion::NotContains("* "),
            ],
        },
    ]
}

fn setup_default(_repo: &Path) {}

fn setup_multiple_branches(repo: &Path) {
    Command::new("git")
        .args(["branch", "feature-x"])
        .current_dir(repo)
        .output()
        .unwrap();
    Command::new("git")
        .args(["branch", "hotfix"])
        .current_dir(repo)
        .output()
        .unwrap();
}

fn setup_alphabetical(repo: &Path) {
    Command::new("git")
        .args(["branch", "aaa-first"])
        .current_dir(repo)
        .output()
        .unwrap();
}

fn setup_many_branches(repo: &Path) {
    for i in 1..=60 {
        Command::new("git")
            .args(["branch", &format!("feature-{:03}", i)])
            .current_dir(repo)
            .output()
            .unwrap();
    }
}

fn setup_with_remote(repo: &Path) {
    let bare_path = repo.join("bare-remote.git");
    Command::new("git")
        .args(["clone", "--bare", ".", bare_path.to_str().unwrap()])
        .current_dir(repo)
        .output()
        .unwrap();
    Command::new("git")
        .args(["remote", "add", "origin", bare_path.to_str().unwrap()])
        .current_dir(repo)
        .output()
        .unwrap();
    Command::new("git")
        .args(["fetch", "origin"])
        .current_dir(repo)
        .output()
        .unwrap();
}
