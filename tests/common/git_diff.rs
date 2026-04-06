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
        Scenario {
            name: "Diff stat compressed",
            command: "git",
            args: &["diff", "--stat"],
            setup: setup_stat_heavy,
            assertions: vec![
                Assertion::Contains("src/config.rs"),
                Assertion::Contains("src/handlers.rs"),
                Assertion::Contains("src/models.rs"),
                Assertion::Contains("files changed"),
                // Bar graphs (e.g. "| 45 +++++++++++++++++--------")
                // should be replaced with numeric counts (e.g. "| 30+ 15-")
                Assertion::NotContains("++++"),
                Assertion::NotContains("----"),
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

fn setup_stat_heavy(repo: &Path) {
    fs::create_dir_all(repo.join("src")).unwrap();

    // Create files with enough content to produce long stat bars
    let original_config: String = (0..30)
        .map(|i| format!("config_line_{} = {}\n", i, i))
        .collect();
    let original_handlers: String = (0..50)
        .map(|i| format!("fn handler_{}() {{}}\n", i))
        .collect();
    let original_models: String = (0..40)
        .map(|i| format!("struct Model{} {{}}\n", i))
        .collect();
    let original_utils: String = (0..20).map(|i| format!("fn util_{}() {{}}\n", i)).collect();

    fs::write(repo.join("src/config.rs"), &original_config).unwrap();
    fs::write(repo.join("src/handlers.rs"), &original_handlers).unwrap();
    fs::write(repo.join("src/models.rs"), &original_models).unwrap();
    fs::write(repo.join("src/utils.rs"), &original_utils).unwrap();

    Command::new("git")
        .args(["add", "src/"])
        .current_dir(repo)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "add src files"])
        .current_dir(repo)
        .output()
        .unwrap();

    // Rewrite files with different content to get a mix of insertions/deletions
    let new_config: String = (0..45)
        .map(|i| format!("new_config_{} = \"{}\"\n", i, i * 10))
        .collect();
    let new_handlers: String = (0..35)
        .map(|i| format!("fn new_handler_{}(ctx: &Ctx) {{}}\n", i))
        .collect();
    let new_models: String = (0..60)
        .map(|i| format!("pub struct NewModel{} {{ id: u64 }}\n", i))
        .collect();
    // utils: delete entirely
    fs::remove_file(repo.join("src/utils.rs")).unwrap();

    fs::write(repo.join("src/config.rs"), &new_config).unwrap();
    fs::write(repo.join("src/handlers.rs"), &new_handlers).unwrap();
    fs::write(repo.join("src/models.rs"), &new_models).unwrap();
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
