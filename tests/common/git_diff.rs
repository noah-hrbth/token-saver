use std::fs;
use std::path::Path;
use std::process::Command;

use super::{Assertion, Scenario};

/// All git diff scenarios. Shared by integration tests and compare runner.
pub fn scenarios() -> Vec<Scenario> {
    vec![
        Scenario {
            name: "Unstaged working tree changes",
            command: "git",
            args: &["diff"],
            setup: setup_unstaged_changes,
            assertions: vec![
                Assertion::Contains("README.md"),
                Assertion::Contains("+modified content"),
            ],
        },
        Scenario {
            name: "Staged changes",
            command: "git",
            args: &["diff", "--staged"],
            setup: setup_staged_changes,
            assertions: vec![
                Assertion::Contains("feature.rs"),
                Assertion::Contains("+fn feature()"),
            ],
        },
        Scenario {
            name: "Commit-to-commit comparison",
            command: "git",
            args: &["diff", "HEAD~1..HEAD"],
            setup: setup_two_commits,
            assertions: vec![
                Assertion::Contains("src/app.rs"),
                Assertion::Contains("+fn app()"),
            ],
        },
        Scenario {
            name: "New file added (staged)",
            command: "git",
            args: &["diff", "--staged"],
            setup: setup_new_file,
            assertions: vec![
                Assertion::Contains("brand_new.rs"),
                Assertion::Contains("(new)"),
                Assertion::Contains("+fn brand_new()"),
            ],
        },
        Scenario {
            name: "File deleted (staged)",
            command: "git",
            args: &["diff", "--staged"],
            setup: setup_deleted_file,
            assertions: vec![
                Assertion::Contains("doomed.rs"),
                Assertion::Contains("(deleted)"),
            ],
        },
        Scenario {
            name: "Multiple files changed",
            command: "git",
            args: &["diff"],
            setup: setup_multiple_files,
            assertions: vec![
                Assertion::Contains("files changed"),
                Assertion::Contains("file_a.txt"),
                Assertion::Contains("file_b.txt"),
            ],
        },
    ]
}

fn setup_unstaged_changes(repo: &Path) {
    fs::write(repo.join("README.md"), "modified content").unwrap();
}

fn setup_staged_changes(repo: &Path) {
    fs::write(repo.join("feature.rs"), "fn feature() {}\n").unwrap();
    Command::new("git")
        .args(["add", "feature.rs"])
        .current_dir(repo)
        .output()
        .unwrap();
}

fn setup_two_commits(repo: &Path) {
    fs::create_dir_all(repo.join("src")).unwrap();
    fs::write(repo.join("src/app.rs"), "fn app() {}\n").unwrap();
    Command::new("git")
        .args(["add", "src/app.rs"])
        .current_dir(repo)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "add app"])
        .current_dir(repo)
        .output()
        .unwrap();
}

fn setup_new_file(repo: &Path) {
    fs::write(repo.join("brand_new.rs"), "fn brand_new() {}\n").unwrap();
    Command::new("git")
        .args(["add", "brand_new.rs"])
        .current_dir(repo)
        .output()
        .unwrap();
}

fn setup_deleted_file(repo: &Path) {
    fs::write(repo.join("doomed.rs"), "fn doomed() {}\n").unwrap();
    Command::new("git")
        .args(["add", "doomed.rs"])
        .current_dir(repo)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "add doomed"])
        .current_dir(repo)
        .output()
        .unwrap();
    fs::remove_file(repo.join("doomed.rs")).unwrap();
    Command::new("git")
        .args(["add", "doomed.rs"])
        .current_dir(repo)
        .output()
        .unwrap();
}

fn setup_multiple_files(repo: &Path) {
    fs::write(repo.join("file_a.txt"), "original a").unwrap();
    fs::write(repo.join("file_b.txt"), "original b").unwrap();
    Command::new("git")
        .args(["add", "file_a.txt", "file_b.txt"])
        .current_dir(repo)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "add files"])
        .current_dir(repo)
        .output()
        .unwrap();

    // Modify them (unstaged)
    fs::write(repo.join("file_a.txt"), "changed a").unwrap();
    fs::write(repo.join("file_b.txt"), "changed b").unwrap();
}
