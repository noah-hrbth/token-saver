# Token Saver Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a transparent CLI proxy that compresses verbose command output for LLM agents, starting with `git status`.

**Architecture:** Single Rust binary using the busybox symlink pattern. Reads `argv[0]` to determine which command to proxy. When `TOKEN_SAVER=1` env var is set, runs the real command with machine-parseable flags, parses output, and returns a compressed version. Falls back to raw output on any failure.

**Tech Stack:** Rust (no external dependencies needed — stdlib only for v1)

**Spec:** `docs/superpowers/specs/2026-03-22-token-saver-design.md`

---

## File Structure

```
token-saver/
  Cargo.toml                           # Project manifest
  src/
    main.rs                            # Entry point: argv[0] dispatch, env check, direct invocation
    runner.rs                          # Find real binary in PATH, execute, capture output
    compressors/
      mod.rs                           # Compressor trait + command registry
      git/
        mod.rs                         # Git subcommand dispatcher
        status.rs                      # git status parser + compressor
  tests/
    integration_test.rs                # End-to-end tests with real git repos
  scripts/
    install.sh                         # Build, install, symlink, print config
```

---

### Task 1: Initialize Rust project

**Files:**
- Create: `Cargo.toml`
- Create: `src/main.rs`

- [ ] **Step 1: Initialize the Cargo project**

Run: `cargo init --name token-saver` (in the project root)

This creates `Cargo.toml` and `src/main.rs` with a hello-world template.

- [ ] **Step 2: Verify it compiles and runs**

Run: `cargo run`
Expected: prints "Hello, world!"

- [ ] **Step 3: Verify tests pass**

Run: `cargo test`
Expected: 0 tests, all passing

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml Cargo.lock src/main.rs
git commit -m "chore: initialize Rust project with cargo"
```

---

### Task 2: Implement the runner — find real binary and execute

The runner is responsible for finding the real binary (skipping token-saver's own directory in PATH) and executing it. This is the foundation everything else builds on.

**Files:**
- Create: `src/runner.rs`
- Modify: `src/main.rs` (add `mod runner;`)

- [ ] **Step 1: Write the failing test for `find_real_binary`**

In `src/runner.rs`:

```rust
use std::env;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// Find the real binary for `command_name` by walking PATH,
/// skipping any directory that matches `skip_dir`.
pub fn find_real_binary(command_name: &str, skip_dir: &Path) -> Option<PathBuf> {
    todo!()
}

/// Execute a command with the given args, capturing stdout and stderr.
pub fn execute_captured(binary: &PathBuf, args: &[String]) -> std::io::Result<Output> {
    todo!()
}

/// Execute a command by replacing the current process (passthrough mode).
/// This function does not return on success.
pub fn exec_passthrough(binary: &PathBuf, args: &[String]) -> std::io::Result<()> {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn find_git_skipping_nonexistent_dir() {
        // Should find git even when skip_dir is some random path
        let result = find_real_binary("git", Path::new("/nonexistent/path"));
        assert!(result.is_some(), "git should be found in PATH");
        let path = result.unwrap();
        assert!(path.exists());
        assert!(path.to_string_lossy().contains("git"));
    }

    #[test]
    fn find_binary_skips_specified_dir() {
        // Find git's actual directory, then ask to skip it — should find nothing
        // (unless git is installed in multiple places)
        let first = find_real_binary("git", Path::new("/nonexistent"));
        if let Some(first_path) = first {
            let skip = first_path.parent().unwrap();
            // We can't guarantee git is installed in only one place,
            // but we can verify the returned path (if any) is NOT in skip_dir
            if let Some(second_path) = find_real_binary("git", skip) {
                assert_ne!(second_path.parent().unwrap(), skip);
            }
        }
    }

    #[test]
    fn find_nonexistent_binary() {
        let result = find_real_binary("this_binary_does_not_exist_xyz", Path::new("/nonexistent"));
        assert!(result.is_none());
    }

    #[test]
    fn execute_captured_runs_echo() {
        // Use 'echo' as a simple test — it exists on all unix systems
        let echo = find_real_binary("echo", Path::new("/nonexistent")).unwrap();
        let output = execute_captured(&echo, &["hello".to_string()]).unwrap();
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert_eq!(stdout.trim(), "hello");
    }
}
```

- [ ] **Step 2: Register the module in main.rs**

Replace `src/main.rs` with:

```rust
mod runner;

fn main() {
    println!("Hello, world!");
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test`
Expected: FAIL — `todo!()` panics

- [ ] **Step 4: Implement `find_real_binary`**

In `src/runner.rs`, replace the `find_real_binary` todo:

```rust
pub fn find_real_binary(command_name: &str, skip_dir: &Path) -> Option<PathBuf> {
    let path_var = env::var("PATH").ok()?;
    let skip_canonical = skip_dir.canonicalize().ok();

    for dir in env::split_paths(&path_var) {
        // Skip our own bin directory
        if let Some(ref skip) = skip_canonical {
            if let Ok(canonical) = dir.canonicalize() {
                if &canonical == skip {
                    continue;
                }
            }
        }

        let candidate = dir.join(command_name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}
```

- [ ] **Step 5: Implement `execute_captured`**

```rust
pub fn execute_captured(binary: &PathBuf, args: &[String]) -> std::io::Result<Output> {
    Command::new(binary).args(args).output()
}
```

- [ ] **Step 6: Implement `exec_passthrough`**

```rust
pub fn exec_passthrough(binary: &PathBuf, args: &[String]) -> std::io::Result<()> {
    use std::os::unix::process::CommandExt;
    // exec replaces the current process — does not return on success
    let err = Command::new(binary).args(args).exec();
    Err(err)
}
```

- [ ] **Step 7: Run tests to verify they pass**

Run: `cargo test`
Expected: all 4 tests pass

- [ ] **Step 8: Commit**

```bash
git add src/runner.rs src/main.rs
git commit -m "feat: add runner module — find real binary in PATH and execute"
```

---

### Task 3: Implement the Compressor trait and registry

**Files:**
- Create: `src/compressors/mod.rs`
- Create: `src/compressors/git/mod.rs` (empty dispatcher for now)
- Modify: `src/main.rs` (add `mod compressors;`)

- [ ] **Step 1: Create the compressor trait and registry**

Create `src/compressors/mod.rs`:

```rust
pub mod git;

/// Trait for command output compressors.
/// Each compressor knows how to parse a specific command's output
/// and return a compressed, LLM-friendly version.
pub trait Compressor {
    /// Can this compressor handle the given args?
    /// For git, args would be e.g. ["status", "-sb"].
    fn can_compress(&self, args: &[String]) -> bool;

    /// Normalized args to pass to the real binary for machine-parseable output.
    /// e.g., ["status", "-sb"] -> ["status", "--porcelain=v2", "--branch", "-z"]
    fn normalized_args(&self, original_args: &[String]) -> Vec<String>;

    /// Parse raw output and return compressed version.
    /// Returns None on parse failure (caller falls back to raw output).
    /// exit_code lets the compressor decide whether to skip compression on errors.
    fn compress(&self, stdout: &str, stderr: &str, exit_code: i32) -> Option<String>;
}

/// Look up a compressor for the given command and args.
/// Returns None if no compressor is registered for this command/args combo.
pub fn find_compressor(command: &str, args: &[String]) -> Option<Box<dyn Compressor>> {
    match command {
        "git" => git::find_compressor(args),
        _ => None,
    }
}
```

- [ ] **Step 2: Create the git subcommand dispatcher**

Create `src/compressors/git/mod.rs`:

```rust
pub mod status;

use super::Compressor;

/// Find a compressor for the given git subcommand args.
pub fn find_compressor(args: &[String]) -> Option<Box<dyn Compressor>> {
    let compressors: Vec<Box<dyn Compressor>> = vec![
        Box::new(status::GitStatusCompressor),
    ];

    for compressor in compressors {
        if compressor.can_compress(args) {
            return Some(compressor);
        }
    }
    None
}
```

- [ ] **Step 3: Create a stub for git status compressor**

Create `src/compressors/git/status.rs`:

```rust
use crate::compressors::Compressor;

pub struct GitStatusCompressor;

impl Compressor for GitStatusCompressor {
    fn can_compress(&self, args: &[String]) -> bool {
        args.first().map(|s| s.as_str()) == Some("status")
    }

    fn normalized_args(&self, _original_args: &[String]) -> Vec<String> {
        vec![
            "status".to_string(),
            "--porcelain=v2".to_string(),
            "--branch".to_string(),
            "-z".to_string(),
        ]
    }

    fn compress(&self, _stdout: &str, _stderr: &str, _exit_code: i32) -> Option<String> {
        todo!()
    }
}
```

- [ ] **Step 4: Register modules in main.rs**

Update `src/main.rs`:

```rust
mod compressors;
mod runner;

fn main() {
    println!("Hello, world!");
}
```

- [ ] **Step 5: Verify it compiles**

Run: `cargo build`
Expected: compiles with no errors (the `todo!()` is fine — it only panics at runtime)

- [ ] **Step 6: Commit**

```bash
git add src/compressors/
git commit -m "feat: add Compressor trait, registry, and git status stub"
```

---

### Task 4: Implement the git status compressor (TDD)

This is the core logic. We'll build it test-first, one scenario at a time.

**Files:**
- Modify: `src/compressors/git/status.rs`

- [ ] **Step 1: Write the test for a clean repo**

Add to `src/compressors/git/status.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn compress(input: &str) -> Option<String> {
        // Replace NUL bytes in test strings: use \0 literal
        GitStatusCompressor.compress(input, "", 0)
    }

    #[test]
    fn test_clean_repo() {
        let input = "# branch.oid abc123def456\0# branch.head main\0# branch.upstream origin/main\0# branch.ab +0 -0\0";
        let result = compress(input);
        assert_eq!(
            result,
            Some("branch: main (up to date with origin/main)\nclean".to_string())
        );
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test test_clean_repo`
Expected: FAIL — `todo!()` panics

- [ ] **Step 3: Implement the parser — branch info + clean detection**

Replace the entire `src/compressors/git/status.rs` with:

```rust
use crate::compressors::Compressor;

pub struct GitStatusCompressor;

impl Compressor for GitStatusCompressor {
    fn can_compress(&self, args: &[String]) -> bool {
        args.first().map(|s| s.as_str()) == Some("status")
    }

    fn normalized_args(&self, _original_args: &[String]) -> Vec<String> {
        vec![
            "status".to_string(),
            "--porcelain=v2".to_string(),
            "--branch".to_string(),
            "-z".to_string(),
        ]
    }

    fn compress(&self, stdout: &str, _stderr: &str, exit_code: i32) -> Option<String> {
        if exit_code != 0 {
            return None;
        }
        parse_porcelain_v2(stdout)
    }
}

struct BranchInfo {
    head: String,
    oid: String,
    upstream: Option<String>,
    ahead: i32,
    behind: i32,
}

struct FileChanges {
    staged: Vec<String>,
    modified: Vec<String>,
    deleted: Vec<String>,
    renamed: Vec<String>,
    conflict: Vec<String>,
    untracked: Vec<String>,
}

fn parse_porcelain_v2(output: &str) -> Option<String> {
    let mut branch = BranchInfo {
        head: String::new(),
        oid: String::new(),
        upstream: None,
        ahead: 0,
        behind: 0,
    };
    let mut files = FileChanges {
        staged: Vec::new(),
        modified: Vec::new(),
        deleted: Vec::new(),
        renamed: Vec::new(),
        conflict: Vec::new(),
        untracked: Vec::new(),
    };

    // Split on NUL bytes (from -z flag). Filter empty entries.
    let entries: Vec<&str> = output.split('\0').filter(|s| !s.is_empty()).collect();

    let mut i = 0;
    while i < entries.len() {
        let entry = entries[i];

        if let Some(oid) = entry.strip_prefix("# branch.oid ") {
            branch.oid = oid.to_string();
        } else if let Some(head) = entry.strip_prefix("# branch.head ") {
            branch.head = head.to_string();
        } else if let Some(upstream) = entry.strip_prefix("# branch.upstream ") {
            branch.upstream = Some(upstream.to_string());
        } else if let Some(ab) = entry.strip_prefix("# branch.ab ") {
            let parts: Vec<&str> = ab.split_whitespace().collect();
            if parts.len() == 2 {
                branch.ahead = parts[0].parse().ok()?;
                branch.behind = parts[1].parse().ok()?;
            }
        } else if entry.starts_with("1 ") {
            parse_ordinary_entry(entry, &mut files);
        } else if entry.starts_with("2 ") {
            // Renamed/copied entry: next NUL-delimited field is the original path
            let orig_path = if i + 1 < entries.len() {
                i += 1;
                entries[i]
            } else {
                ""
            };
            parse_rename_entry(entry, orig_path, &mut files);
        } else if entry.starts_with("u ") {
            parse_unmerged_entry(entry, &mut files);
        } else if let Some(path) = entry.strip_prefix("? ") {
            files.untracked.push(path.to_string());
        }

        i += 1;
    }

    Some(format_output(&branch, &files))
}

fn parse_ordinary_entry(entry: &str, files: &mut FileChanges) {
    // Format: 1 XY sub mH mI mW hH hI path
    let parts: Vec<&str> = entry.splitn(9, ' ').collect();
    if parts.len() < 9 {
        return;
    }

    let xy = parts[1].as_bytes();
    if xy.len() < 2 {
        return;
    }
    let x = xy[0];
    let y = xy[1];
    let path = parts[8].to_string();

    // Staged changes (X position)
    if x != b'.' {
        files.staged.push(path.clone());
    }

    // Unstaged changes (Y position)
    match y {
        b'M' | b'T' => files.modified.push(path),
        b'D' => files.deleted.push(path),
        _ => {}
    }
}

fn parse_rename_entry(entry: &str, orig_path: &str, files: &mut FileChanges) {
    // Format: 2 XY sub mH mI mW hH hI Xscore path
    let parts: Vec<&str> = entry.splitn(10, ' ').collect();
    if parts.len() < 10 {
        return;
    }

    let xy = parts[1].as_bytes();
    if xy.len() < 2 {
        return;
    }
    let y = xy[1];
    let score_field = parts[8]; // e.g., "R100" or "C075"
    let new_path = parts[9];

    let is_copy = score_field.starts_with('C');

    if is_copy {
        files.renamed.push(format!("{} -> {} (copy)", orig_path, new_path));
    } else {
        files.renamed.push(format!("{} -> {}", orig_path, new_path));
    }

    // Unstaged changes on the renamed/copied file
    match y {
        b'M' | b'T' => files.modified.push(new_path.to_string()),
        b'D' => files.deleted.push(new_path.to_string()),
        _ => {}
    }
}

fn parse_unmerged_entry(entry: &str, files: &mut FileChanges) {
    // Format: u XY sub m1 m2 m3 mW h1 h2 h3 path
    let parts: Vec<&str> = entry.splitn(11, ' ').collect();
    if parts.len() >= 11 {
        files.conflict.push(parts[10].to_string());
    }
}

fn format_branch_line(branch: &BranchInfo) -> String {
    if branch.head == "(detached)" {
        let short_oid = if branch.oid.len() >= 7 {
            &branch.oid[..7]
        } else {
            &branch.oid
        };
        return format!("branch: HEAD (detached at {})", short_oid);
    }

    let tracking = match &branch.upstream {
        None => "(no upstream)".to_string(),
        Some(upstream) => {
            if branch.ahead == 0 && branch.behind == 0 {
                format!("(up to date with {})", upstream)
            } else if branch.behind == 0 {
                format!("(+{} ahead of {})", branch.ahead, upstream)
            } else if branch.ahead == 0 {
                format!("(-{} behind {})", branch.behind.abs(), upstream)
            } else {
                format!("(+{} {} vs {})", branch.ahead, branch.behind, upstream)
            }
        }
    };

    format!("branch: {} {}", branch.head, tracking)
}

fn format_output(branch: &BranchInfo, files: &FileChanges) -> String {
    let mut lines = vec![format_branch_line(branch)];

    let categories: &[(&str, &Vec<String>)] = &[
        ("staged", &files.staged),
        ("modified", &files.modified),
        ("deleted", &files.deleted),
        ("renamed", &files.renamed),
        ("conflict", &files.conflict),
        ("untracked", &files.untracked),
    ];

    let has_any_files = categories.iter().any(|(_, v)| !v.is_empty());

    if !has_any_files {
        lines.push("clean".to_string());
    } else {
        for (label, paths) in categories {
            if !paths.is_empty() {
                lines.push(format!("{}: {}", label, paths.join(", ")));
            }
        }
    }

    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn compress(input: &str) -> Option<String> {
        GitStatusCompressor.compress(input, "", 0)
    }

    #[test]
    fn test_clean_repo() {
        let input = "# branch.oid abc123def456\0# branch.head main\0# branch.upstream origin/main\0# branch.ab +0 -0\0";
        let result = compress(input);
        assert_eq!(
            result,
            Some("branch: main (up to date with origin/main)\nclean".to_string())
        );
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test test_clean_repo`
Expected: PASS

- [ ] **Step 5: Add test for modified + untracked files**

```rust
    #[test]
    fn test_modified_and_untracked() {
        let input = "\
# branch.oid abc123\0\
# branch.head main\0\
# branch.upstream origin/main\0\
# branch.ab +0 -0\0\
1 .M N... 100644 100644 100644 abc123 def456 src/main.rs\0\
? .claude/\0";
        let result = compress(input);
        assert_eq!(
            result,
            Some("branch: main (up to date with origin/main)\nmodified: src/main.rs\nuntracked: .claude/".to_string())
        );
    }
```

- [ ] **Step 6: Run tests**

Run: `cargo test`
Expected: both tests pass

- [ ] **Step 7: Add test for staged changes**

```rust
    #[test]
    fn test_staged_files() {
        let input = "\
# branch.oid abc123\0\
# branch.head feature-x\0\
# branch.ab +0 -0\0\
1 A. N... 000000 100644 100644 000000 abc123 src/new.rs\0\
1 M. N... 100644 100644 100644 abc123 def456 src/lib.rs\0";
        let result = compress(input);
        assert_eq!(
            result,
            Some("branch: feature-x (no upstream)\nstaged: src/new.rs, src/lib.rs".to_string())
        );
    }
```

- [ ] **Step 8: Run tests**

Run: `cargo test`
Expected: all 3 tests pass

- [ ] **Step 9: Add test for ahead/behind**

```rust
    #[test]
    fn test_ahead_behind() {
        let input = "\
# branch.oid abc123\0\
# branch.head main\0\
# branch.upstream origin/main\0\
# branch.ab +3 -1\0";
        let result = compress(input);
        assert_eq!(
            result,
            Some("branch: main (+3 -1 vs origin/main)\nclean".to_string())
        );
    }

    #[test]
    fn test_ahead_only() {
        let input = "\
# branch.oid abc123\0\
# branch.head main\0\
# branch.upstream origin/main\0\
# branch.ab +3 -0\0";
        let result = compress(input);
        assert_eq!(
            result,
            Some("branch: main (+3 ahead of origin/main)\nclean".to_string())
        );
    }

    #[test]
    fn test_behind_only() {
        let input = "\
# branch.oid abc123\0\
# branch.head main\0\
# branch.upstream origin/main\0\
# branch.ab +0 -2\0";
        let result = compress(input);
        assert_eq!(
            result,
            Some("branch: main (-2 behind origin/main)\nclean".to_string())
        );
    }
```

- [ ] **Step 10: Run tests**

Run: `cargo test`
Expected: all 6 tests pass

- [ ] **Step 11: Add test for detached HEAD**

```rust
    #[test]
    fn test_detached_head() {
        let input = "\
# branch.oid abc123def456789\0\
# branch.head (detached)\0\
1 .M N... 100644 100644 100644 abc123 def456 src/main.rs\0";
        let result = compress(input);
        assert_eq!(
            result,
            Some("branch: HEAD (detached at abc123d)\nmodified: src/main.rs".to_string())
        );
    }
```

- [ ] **Step 12: Run tests**

Run: `cargo test`
Expected: all 7 tests pass

- [ ] **Step 13: Add test for deleted files**

```rust
    #[test]
    fn test_deleted_file() {
        let input = "\
# branch.oid abc123\0\
# branch.head main\0\
# branch.upstream origin/main\0\
# branch.ab +0 -0\0\
1 .D N... 100644 100644 000000 abc123 000000 old_file.rs\0";
        let result = compress(input);
        assert_eq!(
            result,
            Some("branch: main (up to date with origin/main)\ndeleted: old_file.rs".to_string())
        );
    }
```

- [ ] **Step 14: Run tests**

Run: `cargo test`
Expected: all 8 tests pass

- [ ] **Step 15: Add test for renamed files**

```rust
    #[test]
    fn test_renamed_file() {
        let input = "\
# branch.oid abc123\0\
# branch.head main\0\
# branch.upstream origin/main\0\
# branch.ab +0 -0\0\
2 R. N... 100644 100644 100644 abc123 def456 R100 new_name.rs\0old_name.rs\0";
        let result = compress(input);
        assert_eq!(
            result,
            Some("branch: main (up to date with origin/main)\nrenamed: old_name.rs -> new_name.rs".to_string())
        );
    }
```

- [ ] **Step 16: Run tests**

Run: `cargo test`
Expected: all 9 tests pass

- [ ] **Step 17: Add test for conflict files**

```rust
    #[test]
    fn test_conflict_files() {
        let input = "\
# branch.oid abc123\0\
# branch.head main\0\
# branch.upstream origin/main\0\
# branch.ab +0 -0\0\
u UU N... 100644 100644 100644 100644 abc123 def456 789abc src/conflict.rs\0";
        let result = compress(input);
        assert_eq!(
            result,
            Some("branch: main (up to date with origin/main)\nconflict: src/conflict.rs".to_string())
        );
    }
```

- [ ] **Step 18: Run tests**

Run: `cargo test`
Expected: all 10 tests pass

- [ ] **Step 19: Add test for mixed staged + unstaged (same file)**

```rust
    #[test]
    fn test_staged_and_modified_same_file() {
        let input = "\
# branch.oid abc123\0\
# branch.head main\0\
# branch.upstream origin/main\0\
# branch.ab +0 -0\0\
1 MM N... 100644 100644 100644 abc123 def456 src/main.rs\0";
        let result = compress(input);
        assert_eq!(
            result,
            Some("branch: main (up to date with origin/main)\nstaged: src/main.rs\nmodified: src/main.rs".to_string())
        );
    }
```

- [ ] **Step 20: Run tests**

Run: `cargo test`
Expected: all 11 tests pass

- [ ] **Step 21: Add test for non-zero exit code (should return None)**

```rust
    #[test]
    fn test_nonzero_exit_returns_none() {
        let result = GitStatusCompressor.compress("anything", "fatal: error", 128);
        assert_eq!(result, None);
    }
```

- [ ] **Step 22: Run all tests**

Run: `cargo test`
Expected: all 12 tests pass

- [ ] **Step 23: Commit**

```bash
git add src/compressors/git/status.rs
git commit -m "feat: implement git status compressor with porcelain v2 parser"
```

---

### Task 5: Wire up main.rs — argv[0] dispatch and env check

**Files:**
- Modify: `src/main.rs`

- [ ] **Step 1: Write the full main.rs**

Replace `src/main.rs`:

```rust
mod compressors;
mod runner;

use std::env;
use std::path::PathBuf;
use std::process;

fn main() {
    let args: Vec<String> = env::args().collect();
    let argv0 = &args[0];

    // Determine command name and command args.
    // If invoked as a symlink (argv[0] = "git"), command = "git", command_args = rest.
    // If invoked directly (argv[0] ends with "token-saver"), command = args[1], command_args = args[2..].
    let binary_name = PathBuf::from(argv0)
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    let (command, command_args) = if binary_name == "token-saver" {
        // Direct invocation: token-saver git status
        if args.len() < 2 {
            eprintln!("Usage: token-saver <command> [args...]");
            process::exit(1);
        }
        (args[1].clone(), args[2..].to_vec())
    } else {
        // Symlink invocation: argv[0] is the command name
        (binary_name, args[1..].to_vec())
    };

    // Determine our own binary's directory to skip in PATH lookups
    let self_dir = env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_default();

    // Find the real binary
    let real_binary = match runner::find_real_binary(&command, &self_dir) {
        Some(path) => path,
        None => {
            eprintln!("token-saver: {}: command not found", command);
            process::exit(127);
        }
    };

    // If TOKEN_SAVER is not set, passthrough directly
    let token_saver_enabled = env::var("TOKEN_SAVER").unwrap_or_default() == "1";
    if !token_saver_enabled {
        // exec replaces this process — does not return
        if let Err(e) = runner::exec_passthrough(&real_binary, &command_args) {
            eprintln!("token-saver: failed to exec {}: {}", command, e);
            process::exit(1);
        }
        unreachable!();
    }

    // Try to find a compressor
    let compressor = compressors::find_compressor(&command, &command_args);

    match compressor {
        None => {
            // No compressor — passthrough
            if let Err(e) = runner::exec_passthrough(&real_binary, &command_args) {
                eprintln!("token-saver: failed to exec {}: {}", command, e);
                process::exit(1);
            }
        }
        Some(comp) => {
            // Run with normalized args, try to compress
            let normalized = comp.normalized_args(&command_args);
            match runner::execute_captured(&real_binary, &normalized) {
                Ok(output) => {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    let exit_code = output.status.code().unwrap_or(1);

                    match comp.compress(&stdout, &stderr, exit_code) {
                        Some(compressed) => {
                            print!("{}", compressed);
                            process::exit(exit_code);
                        }
                        None => {
                            // Compression failed — fall back to running with original args
                            if let Err(e) = runner::exec_passthrough(&real_binary, &command_args) {
                                eprintln!("token-saver: failed to exec {}: {}", command, e);
                                process::exit(1);
                            }
                        }
                    }
                }
                Err(_) => {
                    // Execution failed — fall back to passthrough
                    if let Err(e) = runner::exec_passthrough(&real_binary, &command_args) {
                        eprintln!("token-saver: failed to exec {}: {}", command, e);
                        process::exit(1);
                    }
                }
            }
        }
    }
}
```

- [ ] **Step 2: Build and verify it compiles**

Run: `cargo build`
Expected: compiles with no errors

- [ ] **Step 3: Test passthrough mode (no env var)**

Run: `cargo run -- git status`
Expected: same output as running `git status` directly

- [ ] **Step 4: Test compressed mode**

Run: `TOKEN_SAVER=1 cargo run -- git status`
Expected: compressed output like `branch: main (up to date with origin/main)\nuntracked: .claude/`

- [ ] **Step 5: Test with unknown command (passthrough)**

Run: `TOKEN_SAVER=1 cargo run -- git log --oneline -3`
Expected: same output as `git log --oneline -3` (no compressor registered for `log`)

- [ ] **Step 6: Run all unit tests still pass**

Run: `cargo test`
Expected: all tests pass

- [ ] **Step 7: Commit**

```bash
git add src/main.rs
git commit -m "feat: wire up main.rs with argv[0] dispatch, env check, and fallback"
```

---

### Task 6: Integration tests

**Files:**
- Create: `tests/integration_test.rs`
- Modify: `Cargo.toml` (add dev-dependency)

- [ ] **Step 1: Add `tempfile` dev-dependency to Cargo.toml**

Add under `[dev-dependencies]`:

```toml
[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 2: Write integration tests**

Create `tests/integration_test.rs`:

```rust
use std::fs;
use std::process::Command;

/// Helper: get the path to the compiled token-saver binary
fn binary_path() -> String {
    env!("CARGO_BIN_EXE_token-saver").to_string()
}

/// Helper: create a temporary git repo and return its path
fn create_temp_repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    Command::new("git")
        .args(["init"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    Command::new("git")
        .args(["config", "user.email", "test@test.com"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    Command::new("git")
        .args(["config", "user.name", "Test"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    dir
}

#[test]
fn test_passthrough_without_env_var() {
    let repo = create_temp_repo();

    let output = Command::new(binary_path())
        .args(["git", "status"])
        .env_remove("TOKEN_SAVER")
        .current_dir(repo.path())
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    // Passthrough mode — should contain verbose git output
    assert!(
        stdout.contains("On branch") || stdout.contains("No commits yet"),
        "Expected raw git output, got: {}",
        stdout
    );
}

#[test]
fn test_compressed_clean_repo() {
    let repo = create_temp_repo();

    // Make an initial commit so the repo is "clean"
    let file_path = repo.path().join("README.md");
    fs::write(&file_path, "hello").unwrap();
    Command::new("git")
        .args(["add", "README.md"])
        .current_dir(repo.path())
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "init"])
        .current_dir(repo.path())
        .output()
        .unwrap();

    let output = Command::new(binary_path())
        .args(["git", "status"])
        .env("TOKEN_SAVER", "1")
        .current_dir(repo.path())
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("branch: "), "Expected compressed output, got: {}", stdout);
    assert!(stdout.contains("clean"), "Expected 'clean', got: {}", stdout);
    assert!(output.status.success());
}

#[test]
fn test_compressed_with_changes() {
    let repo = create_temp_repo();

    // Initial commit
    let file_path = repo.path().join("README.md");
    fs::write(&file_path, "hello").unwrap();
    Command::new("git")
        .args(["add", "README.md"])
        .current_dir(repo.path())
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "init"])
        .current_dir(repo.path())
        .output()
        .unwrap();

    // Modify the file (unstaged change)
    fs::write(&file_path, "hello world").unwrap();

    // Create an untracked file
    fs::write(repo.path().join("new_file.txt"), "new").unwrap();

    let output = Command::new(binary_path())
        .args(["git", "status"])
        .env("TOKEN_SAVER", "1")
        .current_dir(repo.path())
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("branch: "), "Expected branch line, got: {}", stdout);
    assert!(stdout.contains("modified: README.md"), "Expected modified file, got: {}", stdout);
    assert!(stdout.contains("untracked: new_file.txt"), "Expected untracked file, got: {}", stdout);
    assert!(!stdout.contains("clean"), "Should NOT be clean, got: {}", stdout);
}

#[test]
fn test_passthrough_for_unknown_subcommand() {
    let repo = create_temp_repo();

    // git log should passthrough (no compressor)
    let file_path = repo.path().join("README.md");
    fs::write(&file_path, "hello").unwrap();
    Command::new("git")
        .args(["add", "README.md"])
        .current_dir(repo.path())
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "init"])
        .current_dir(repo.path())
        .output()
        .unwrap();

    let output = Command::new(binary_path())
        .args(["git", "log", "--oneline"])
        .env("TOKEN_SAVER", "1")
        .current_dir(repo.path())
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("init"), "Expected git log output with commit message, got: {}", stdout);
}
```

- [ ] **Step 3: Run integration tests**

Run: `cargo test --test integration_test`
Expected: all 4 integration tests pass

- [ ] **Step 4: Run all tests together**

Run: `cargo test`
Expected: all unit + integration tests pass

- [ ] **Step 5: Commit**

```bash
git add tests/integration_test.rs Cargo.toml Cargo.lock
git commit -m "test: add integration tests for passthrough and compressed modes"
```

---

### Task 7: Install script

**Files:**
- Create: `scripts/install.sh`

- [ ] **Step 1: Write the install script**

Create `scripts/install.sh`:

```bash
#!/usr/bin/env bash
set -euo pipefail

INSTALL_DIR="$HOME/.token-saver/bin"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

echo "Building token-saver (release)..."
cd "$PROJECT_DIR"
cargo build --release

echo "Installing to $INSTALL_DIR..."
mkdir -p "$INSTALL_DIR"

# Copy binary
cp target/release/token-saver "$INSTALL_DIR/token-saver"
chmod +x "$INSTALL_DIR/token-saver"

# Create symlinks (remove old ones first for idempotency)
COMMANDS=(git)
for cmd in "${COMMANDS[@]}"; do
    rm -f "$INSTALL_DIR/$cmd"
    ln -s token-saver "$INSTALL_DIR/$cmd"
    echo "  Created symlink: $INSTALL_DIR/$cmd -> token-saver"
done

echo ""
echo "Installation complete!"
echo ""
echo "To configure Claude Code, add this to ~/.claude/settings.json:"
echo ""
echo '{'
echo '  "env": {'
echo "    \"TOKEN_SAVER\": \"1\","
echo "    \"PATH\": \"$INSTALL_DIR:\$PATH\""
echo '  }'
echo '}'
echo ""
echo "To test manually:"
echo "  TOKEN_SAVER=1 $INSTALL_DIR/git status"
```

- [ ] **Step 2: Make it executable**

Run: `chmod +x scripts/install.sh`

- [ ] **Step 3: Test the install script**

Run: `bash scripts/install.sh`
Expected: builds, copies binary, creates symlink, prints config instructions

- [ ] **Step 4: Verify the installed symlink works**

Run: `TOKEN_SAVER=1 ~/.token-saver/bin/git status`
Expected: compressed output

- [ ] **Step 5: Verify passthrough works**

Run: `~/.token-saver/bin/git status`
Expected: normal raw git output (TOKEN_SAVER not set)

- [ ] **Step 6: Commit**

```bash
git add scripts/install.sh
git commit -m "feat: add install script for building and symlinking"
```

---

### Task 8: README

**Files:**
- Create: `README.md`

- [ ] **Step 1: Write the README**

Create `README.md`:

````markdown
# token-saver

A transparent CLI proxy that compresses verbose command output for LLM agents. Saves tokens without the agent knowing.

## How it works

token-saver sits between the agent and real CLI commands using symlinks. When `TOKEN_SAVER=1` is set, it intercepts output and returns a compressed version. When unset, it passes through to the real command unchanged.

```
Agent calls "git status"
  -> token-saver intercepts (via symlink + PATH priority)
  -> runs real git with machine-parseable flags
  -> returns compressed output

# Before (raw git status): ~350 tokens
On branch main
Your branch is up to date with 'origin/main'.

Changes not staged for commit:
  (use "git add <file>..." to update what will be committed)
  (use "git restore <file>..." to discard changes in working directory)
        modified:   src/main.rs

Untracked files:
  (use "git add <file>..." to include in what will be committed)
        .env.example

no changes added to commit (use "git add" and/or "git commit -a")

# After (token-saver): ~30 tokens
branch: main (up to date with origin/main)
modified: src/main.rs
untracked: .env.example
```

## Supported commands

| Command | Status |
|---------|--------|
| `git status` | Supported |
| `git diff` | Planned |
| `git log` | Planned |

## Install

```bash
# Requires Rust toolchain (https://rustup.rs)
git clone <this-repo>
cd token-saver
bash scripts/install.sh
```

## Configure Claude Code

Add to `~/.claude/settings.json`:

```json
{
  "env": {
    "TOKEN_SAVER": "1",
    "PATH": "/Users/<your-username>/.token-saver/bin:$PATH"
  }
}
```

## Development

```bash
# Build
cargo build

# Run tests
cargo test

# Test manually
TOKEN_SAVER=1 cargo run -- git status

# Install locally
bash scripts/install.sh
```

## Adding a new compressor

1. Create `src/compressors/<command>/<subcommand>.rs` implementing the `Compressor` trait
2. Register it in the parent module's dispatcher
3. Add unit tests + integration tests
4. Add a symlink in `scripts/install.sh` if it's a new top-level command
````

- [ ] **Step 2: Commit**

```bash
git add README.md
git commit -m "docs: add README with usage, install, and development instructions"
```

---

## Summary

| Task | What it builds | Tests |
|------|---------------|-------|
| 1 | Cargo project scaffold | cargo compiles |
| 2 | Runner (find binary, execute) | 4 unit tests |
| 3 | Compressor trait + registry | compiles |
| 4 | Git status compressor | 12 unit tests |
| 5 | main.rs dispatch logic | manual verification |
| 6 | Integration tests | 4 integration tests |
| 7 | Install script | manual verification |
| 8 | README | n/a |
