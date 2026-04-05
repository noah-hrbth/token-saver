use std::fs;
use std::path::Path;

use super::{Assertion, Scenario};

/// All cat scenarios. Shared by integration tests and compare runner.
pub fn scenarios() -> Vec<Scenario> {
    vec![
        Scenario {
            name: "Basic file content",
            command: "cat",
            args: &["file.txt"],
            setup: setup_basic_file,
            assertions: vec![
                Assertion::Contains("hello world"),
                Assertion::Contains("second line"),
            ],
        },
        Scenario {
            name: "Truncation at 1000 lines",
            command: "cat",
            args: &["big.txt"],
            setup: setup_large_file,
            assertions: vec![
                Assertion::Contains("line 0"),
                Assertion::Contains("line 999"),
                Assertion::NotContains("line 1000"),
                Assertion::Contains("... 500 more lines"),
            ],
        },
        Scenario {
            name: "Binary file detection",
            command: "cat",
            args: &["binary.bin"],
            setup: setup_binary_file,
            assertions: vec![
                Assertion::Contains("(binary content, "),
                Assertion::Contains(" bytes)"),
            ],
        },
        Scenario {
            name: "Minified line collapsing",
            command: "cat",
            args: &["minified.js"],
            setup: setup_minified_file,
            assertions: vec![
                Assertion::Contains("likely minified"),
                Assertion::Contains("chars"),
            ],
        },
        Scenario {
            name: "Empty file",
            command: "cat",
            args: &["empty.txt"],
            setup: setup_empty_file,
            assertions: vec![
                Assertion::NotContains("binary"),
                Assertion::NotContains("error"),
            ],
        },
        Scenario {
            name: "Multi-file concatenation with cap",
            command: "cat",
            args: &["a.txt", "b.txt"],
            setup: setup_multi_file,
            assertions: vec![
                Assertion::Contains("file a line 0"),
                Assertion::Contains("file b line 0"),
                Assertion::Contains("more lines"),
            ],
        },
    ]
}

fn setup_basic_file(repo: &Path) {
    fs::write(
        repo.join("file.txt"),
        "hello world\nsecond line\nthird line\n",
    )
    .unwrap();
}

fn setup_large_file(repo: &Path) {
    let content: String = (0..1500).map(|i| format!("line {}\n", i)).collect();
    fs::write(repo.join("big.txt"), content).unwrap();
}

fn setup_binary_file(repo: &Path) {
    let mut content = Vec::new();
    content.extend_from_slice(b"ELF");
    content.push(0x00);
    content.extend_from_slice(&[0x01, 0x02, 0x03]);
    content.extend_from_slice(b"binary data follows");
    fs::write(repo.join("binary.bin"), content).unwrap();
}

fn setup_minified_file(repo: &Path) {
    let long_line = "var a=1;".repeat(500); // 4000 chars
    let content = format!("// header comment\n{}\n", long_line);
    fs::write(repo.join("minified.js"), content).unwrap();
}

fn setup_empty_file(repo: &Path) {
    fs::write(repo.join("empty.txt"), "").unwrap();
}

fn setup_multi_file(repo: &Path) {
    let content_a: String = (0..600).map(|i| format!("file a line {}\n", i)).collect();
    let content_b: String = (0..600).map(|i| format!("file b line {}\n", i)).collect();
    fs::write(repo.join("a.txt"), content_a).unwrap();
    fs::write(repo.join("b.txt"), content_b).unwrap();
}
