use std::path::Path;
use std::process::Command;

use super::{Assertion, Scenario};

/// All git show scenarios. Shared by integration tests and compare runner.
pub fn scenarios() -> Vec<Scenario> {
    vec![
        Scenario {
            name: "Basic git show HEAD",
            command: "git",
            args: &["show"],
            setup: setup_single_change,
            assertions: vec![
                Assertion::Contains("*"),
                Assertion::Contains("[Test]"),
                Assertion::Contains("modify file"),
                Assertion::Contains("+modified content"),
                Assertion::NotContains("Author:"),
                Assertion::NotContains("Date:"),
                Assertion::NotContains("index "),
            ],
        },
        Scenario {
            name: "Show with --no-patch",
            command: "git",
            args: &["show", "--no-patch"],
            setup: setup_single_change,
            assertions: vec![
                Assertion::Contains("*"),
                Assertion::Contains("[Test]"),
                Assertion::Contains("modify file"),
                Assertion::NotContains("@@"),
                Assertion::NotContains("diff --git"),
            ],
        },
        Scenario {
            name: "Show with commit body",
            command: "git",
            args: &["show"],
            setup: setup_commit_with_body,
            assertions: vec![
                Assertion::Contains("commit with body"),
                Assertion::Contains("  This is the detailed body"),
            ],
        },
        Scenario {
            name: "Show new file",
            command: "git",
            args: &["show"],
            setup: setup_new_file,
            assertions: vec![
                Assertion::Contains("(new)"),
                Assertion::Contains("+new file content"),
            ],
        },
        Scenario {
            name: "Show deleted file",
            command: "git",
            args: &["show"],
            setup: setup_deleted_file,
            assertions: vec![
                Assertion::Contains("(deleted)"),
                Assertion::Contains("-to be deleted"),
            ],
        },
        Scenario {
            name: "Show multi-file commit",
            command: "git",
            args: &["show"],
            setup: setup_multi_file_change,
            assertions: vec![
                Assertion::Contains("files changed"),
                Assertion::Contains("a.rs"),
                Assertion::Contains("b.rs"),
            ],
        },
        Scenario {
            name: "Show annotated tag",
            command: "git",
            args: &["show", "v1.0"],
            setup: setup_annotated_tag,
            assertions: vec![
                Assertion::Contains("tag: v1.0"),
                Assertion::Contains("Release version 1.0"),
                Assertion::Contains("*"),
            ],
        },
    ]
}

fn setup_single_change(path: &Path) {
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

fn setup_new_file(path: &Path) {
    std::fs::write(path.join("new_file.rs"), "new file content").unwrap();
    Command::new("git")
        .args(["add", "new_file.rs"])
        .current_dir(path)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "add new file"])
        .current_dir(path)
        .output()
        .unwrap();
}

fn setup_deleted_file(path: &Path) {
    std::fs::write(path.join("doomed.rs"), "to be deleted").unwrap();
    Command::new("git")
        .args(["add", "doomed.rs"])
        .current_dir(path)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "add file to delete"])
        .current_dir(path)
        .output()
        .unwrap();

    std::fs::remove_file(path.join("doomed.rs")).unwrap();
    Command::new("git")
        .args(["add", "doomed.rs"])
        .current_dir(path)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "delete file"])
        .current_dir(path)
        .output()
        .unwrap();
}

fn setup_multi_file_change(path: &Path) {
    std::fs::write(path.join("a.rs"), "fn a() {}").unwrap();
    std::fs::write(path.join("b.rs"), "fn b() {}").unwrap();
    std::fs::write(path.join("c.rs"), "fn c() {}").unwrap();
    Command::new("git")
        .args(["add", "a.rs", "b.rs", "c.rs"])
        .current_dir(path)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "add three files"])
        .current_dir(path)
        .output()
        .unwrap();

    std::fs::write(path.join("a.rs"), "fn a() { changed }").unwrap();
    std::fs::write(path.join("b.rs"), "fn b() { changed }").unwrap();
    Command::new("git")
        .args(["add", "a.rs", "b.rs"])
        .current_dir(path)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "modify two files"])
        .current_dir(path)
        .output()
        .unwrap();
}

fn setup_annotated_tag(path: &Path) {
    std::fs::write(path.join("release.txt"), "v1.0 content").unwrap();
    Command::new("git")
        .args(["add", "release.txt"])
        .current_dir(path)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "release commit"])
        .current_dir(path)
        .output()
        .unwrap();
    Command::new("git")
        .args(["tag", "-a", "v1.0", "-m", "Release version 1.0"])
        .current_dir(path)
        .output()
        .unwrap();
}
