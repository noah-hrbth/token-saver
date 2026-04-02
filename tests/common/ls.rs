use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use super::{Assertion, Scenario};

/// All ls scenarios. Shared by integration tests and compare runner.
pub fn scenarios() -> Vec<Scenario> {
    vec![
        Scenario {
            name: "Mixed file types",
            command: "ls",
            args: &["-la"],
            setup: setup_mixed_types,
            assertions: vec![
                Assertion::Contains("src/"),
                Assertion::Contains("Cargo.toml ("),
                Assertion::Contains("run.sh* ("),
                Assertion::NotContains("\n.\n"),
                Assertion::NotContains("\n..\n"),
            ],
        },
        Scenario {
            name: "Hidden files included",
            command: "ls",
            args: &["-la"],
            setup: setup_hidden_files,
            assertions: vec![
                Assertion::Contains(".env ("),
                Assertion::Contains(".config/"),
            ],
        },
        Scenario {
            name: "ls -l normalizes to include hidden",
            command: "ls",
            args: &["-l"],
            setup: setup_hidden_files,
            assertions: vec![
                // Even though -l was passed (not -la), normalization adds -a
                Assertion::Contains(".env ("),
                Assertion::Contains(".config/"),
            ],
        },
        Scenario {
            name: "Symlinks show targets",
            command: "ls",
            args: &["-la"],
            setup: setup_with_symlink,
            assertions: vec![Assertion::Contains("link -> ")],
        },
        Scenario {
            name: "ls -l with path argument",
            command: "ls",
            args: &["-la", "subdir"],
            setup: setup_subdir,
            assertions: vec![
                Assertion::Contains("inner.txt ("),
                Assertion::NotContains("outer.txt"),
            ],
        },
    ]
}

fn setup_mixed_types(repo: &Path) {
    // Directory
    fs::create_dir_all(repo.join("src")).unwrap();

    // Regular file
    fs::write(repo.join("Cargo.toml"), "a]".repeat(600)).unwrap();

    // Executable
    fs::write(repo.join("run.sh"), "#!/bin/bash\necho hi").unwrap();
    let perms = fs::Permissions::from_mode(0o755);
    fs::set_permissions(repo.join("run.sh"), perms).unwrap();
}

fn setup_hidden_files(repo: &Path) {
    fs::write(repo.join(".env"), "SECRET=foo").unwrap();
    fs::create_dir_all(repo.join(".config")).unwrap();
}

fn setup_with_symlink(repo: &Path) {
    fs::write(repo.join("target.txt"), "content").unwrap();
    std::os::unix::fs::symlink("target.txt", repo.join("link")).unwrap();
}

fn setup_subdir(repo: &Path) {
    fs::write(repo.join("outer.txt"), "outside").unwrap();
    fs::create_dir_all(repo.join("subdir")).unwrap();
    fs::write(repo.join("subdir/inner.txt"), "inside").unwrap();
}
