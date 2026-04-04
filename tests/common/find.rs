use std::fs;
use std::path::Path;

use super::{Assertion, Scenario};

/// All find scenarios. Shared by integration tests and compare runner.
pub fn scenarios() -> Vec<Scenario> {
    vec![
        Scenario {
            name: "Basic find with noise filtering",
            command: "find",
            args: &["."],
            setup: setup_basic_with_noise,
            assertions: vec![
                Assertion::Contains("src/"),
                Assertion::Contains("main.rs"),
                Assertion::Contains("Cargo.toml"),
                Assertion::NotContains("__pycache__"),
                Assertion::NotContains(".DS_Store"),
                Assertion::NotContains(".git/"),
                Assertion::Contains("entries filtered"),
            ],
        },
        Scenario {
            name: "Targeted find with -name",
            command: "find",
            args: &[".", "-name", "*.rs"],
            setup: setup_name_filter,
            assertions: vec![
                Assertion::Contains("main.rs"),
                Assertion::Contains("lib.rs"),
                Assertion::NotContains("Cargo.toml"),
                Assertion::NotContains("README.md"),
            ],
        },
        Scenario {
            name: "Find directories only",
            command: "find",
            args: &[".", "-type", "d"],
            setup: setup_dirs_only,
            assertions: vec![
                Assertion::Contains("src/"),
                Assertion::Contains("compressors/"),
                Assertion::Contains("tests/"),
            ],
        },
        Scenario {
            name: "Tree structure with sorting",
            command: "find",
            args: &["."],
            setup: setup_sorting,
            assertions: vec![
                Assertion::Contains("aaa_dir/"),
                Assertion::Contains("alpha.txt"),
                Assertion::Contains("zebra.txt"),
                Assertion::Contains("  inner.txt"),
                Assertion::Contains("  file.txt"),
            ],
        },
    ]
}

fn setup_basic_with_noise(repo: &Path) {
    fs::create_dir_all(repo.join("src")).unwrap();
    fs::write(repo.join("src/main.rs"), "fn main() {}").unwrap();
    fs::write(repo.join("src/lib.rs"), "// lib").unwrap();
    fs::write(repo.join("Cargo.toml"), "[package]").unwrap();
    fs::create_dir_all(repo.join("__pycache__")).unwrap();
    fs::write(repo.join("__pycache__/module.pyc"), "bytecode").unwrap();
    fs::write(repo.join(".DS_Store"), "store").unwrap();
}

fn setup_name_filter(repo: &Path) {
    fs::create_dir_all(repo.join("src")).unwrap();
    fs::write(repo.join("src/main.rs"), "fn main() {}").unwrap();
    fs::write(repo.join("src/lib.rs"), "// lib").unwrap();
    fs::write(repo.join("Cargo.toml"), "[package]").unwrap();
    fs::write(repo.join("README.md"), "# Project").unwrap();
}

fn setup_dirs_only(repo: &Path) {
    fs::create_dir_all(repo.join("src/compressors")).unwrap();
    fs::create_dir_all(repo.join("tests")).unwrap();
}

fn setup_sorting(repo: &Path) {
    fs::write(repo.join("zebra.txt"), "z").unwrap();
    fs::write(repo.join("alpha.txt"), "a").unwrap();
    fs::create_dir_all(repo.join("middle")).unwrap();
    fs::write(repo.join("middle/inner.txt"), "inner").unwrap();
    fs::create_dir_all(repo.join("aaa_dir")).unwrap();
    fs::write(repo.join("aaa_dir/file.txt"), "file").unwrap();
}
