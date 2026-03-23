use std::fs;
use std::path::Path;
use std::process::Command;

use super::{Assertion, Scenario};

/// All git status scenarios. Shared by integration tests and compare runner.
pub fn scenarios() -> Vec<Scenario> {
    vec![
        Scenario {
            name: "Clean repository",
            command: "git",
            args: &["status"],
            setup: setup_clean,
            assertions: vec![
                Assertion::Contains("branch: "),
                Assertion::Contains("clean"),
            ],
        },
        Scenario {
            name: "Modified file (unstaged)",
            command: "git",
            args: &["status"],
            setup: setup_modified,
            assertions: vec![
                Assertion::Contains("branch: "),
                Assertion::Contains("modified: README.md"),
                Assertion::NotContains("clean"),
            ],
        },
        Scenario {
            name: "Untracked files",
            command: "git",
            args: &["status"],
            setup: setup_untracked,
            assertions: vec![
                Assertion::Contains("branch: "),
                Assertion::Contains("untracked: new_file.txt"),
            ],
        },
        Scenario {
            name: "Staged files",
            command: "git",
            args: &["status"],
            setup: setup_staged,
            assertions: vec![
                Assertion::Contains("branch: "),
                Assertion::Contains("staged: feature.rs"),
                Assertion::Contains("utils.rs"),
                Assertion::NotContains("clean"),
            ],
        },
        Scenario {
            name: "Mixed changes (staged + modified + untracked)",
            command: "git",
            args: &["status"],
            setup: setup_mixed,
            assertions: vec![
                Assertion::Contains("branch: "),
                Assertion::Contains("staged: "),
                Assertion::Contains("modified: README.md"),
                Assertion::Contains("untracked: "),
                Assertion::NotContains("clean"),
            ],
        },
        Scenario {
            name: "Deleted file",
            command: "git",
            args: &["status"],
            setup: setup_deleted,
            assertions: vec![
                Assertion::Contains("branch: "),
                Assertion::Contains("deleted: doomed.rs"),
                Assertion::NotContains("clean"),
            ],
        },
        Scenario {
            name: "Many files (modified + deleted + staged + untracked)",
            command: "git",
            args: &["status"],
            setup: setup_many_files,
            assertions: vec![
                Assertion::Contains("branch: "),
                Assertion::Contains("staged: new_staged.rs"),
                Assertion::Contains("modified: "),
                Assertion::Contains("deleted: "),
                Assertion::Contains("untracked: "),
                Assertion::NotContains("clean"),
            ],
        },
    ]
}

fn setup_clean(_repo: &Path) {
    // Already clean from create_temp_repo
}

fn setup_modified(repo: &Path) {
    fs::write(repo.join("README.md"), "modified content").unwrap();
}

fn setup_untracked(repo: &Path) {
    fs::write(repo.join("new_file.txt"), "new").unwrap();
    fs::create_dir_all(repo.join("src")).unwrap();
    fs::write(repo.join("src/app.rs"), "code").unwrap();
}

fn setup_staged(repo: &Path) {
    fs::write(repo.join("feature.rs"), "new feature").unwrap();
    fs::write(repo.join("utils.rs"), "another").unwrap();
    Command::new("git")
        .args(["add", "feature.rs", "utils.rs"])
        .current_dir(repo)
        .output()
        .unwrap();
}

fn setup_mixed(repo: &Path) {
    // Staged
    fs::write(repo.join("staged.rs"), "staged content").unwrap();
    Command::new("git")
        .args(["add", "staged.rs"])
        .current_dir(repo)
        .output()
        .unwrap();

    // Modified (tracked)
    fs::write(repo.join("README.md"), "modified").unwrap();

    // Untracked
    fs::write(repo.join("temp.log"), "untracked").unwrap();
    fs::write(repo.join("notes.txt"), "also new").unwrap();
}

fn setup_deleted(repo: &Path) {
    fs::write(repo.join("doomed.rs"), "will be deleted").unwrap();
    Command::new("git")
        .args(["add", "doomed.rs"])
        .current_dir(repo)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "add doomed file"])
        .current_dir(repo)
        .output()
        .unwrap();
    fs::remove_file(repo.join("doomed.rs")).unwrap();
}

fn setup_many_files(repo: &Path) {
    for i in 1..=10 {
        fs::write(repo.join(format!("file_{}.txt", i)), format!("file {}", i)).unwrap();
    }
    Command::new("git")
        .args(["add", "."])
        .current_dir(repo)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "add many files"])
        .current_dir(repo)
        .output()
        .unwrap();

    // Modify some
    fs::write(repo.join("file_1.txt"), "changed").unwrap();
    fs::write(repo.join("file_3.txt"), "changed").unwrap();
    fs::write(repo.join("file_7.txt"), "changed").unwrap();

    // Delete some
    fs::remove_file(repo.join("file_5.txt")).unwrap();
    fs::remove_file(repo.join("file_9.txt")).unwrap();

    // Stage a new file
    fs::write(repo.join("new_staged.rs"), "new staged").unwrap();
    Command::new("git")
        .args(["add", "new_staged.rs"])
        .current_dir(repo)
        .output()
        .unwrap();

    // Untracked
    fs::write(repo.join("random.log"), "untracked").unwrap();
    fs::write(repo.join("debug.out"), "untracked").unwrap();
}
