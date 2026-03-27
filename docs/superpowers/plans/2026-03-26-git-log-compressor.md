# Git Log Compressor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a `git log` compressor that compresses verbose log output into token-efficient format, supporting most common flags including `-p`, `--stat`, filtering, and ranges.

**Architecture:** Extract shared diff parsing/formatting from `diff.rs` into `diff_parser.rs`, then build `GitLogCompressor` in `log.rs` that uses NUL-delimited `--format=` for reliable parsing. Follows existing compressor patterns (trait impl, dispatcher registration, graceful fallback).

**Tech Stack:** Rust (no new dependencies)

**Spec:** `docs/superpowers/specs/2026-03-26-git-log-compressor-design.md`

---

## File Structure

```
src/compressors/git/
  diff_parser.rs    (new)  — Shared diff types + parsing + formatting, extracted from diff.rs
  diff.rs           (mod)  — GitDiffCompressor, now imports from diff_parser
  log.rs            (new)  — GitLogCompressor: can_compress, normalized_args, compress
  mod.rs            (mod)  — Register GitLogCompressor in dispatcher

tests/
  common/
    mod.rs          (mod)  — Add pub mod git_log
    git_log.rs      (new)  — Scenario definitions for git log
  git_log.rs        (new)  — Integration tests
  compare.rs        (mod)  — Add compare_git_log test
  passthrough.rs    (mod)  — Add git log passthrough tests

scripts/
  install.sh              — No changes needed (git function already installed)
```

---

### Task 1: Extract shared diff types into `diff_parser.rs`

Move data model types from `diff.rs` into a new shared module. Both `diff.rs` and (later) `log.rs` will import from here.

**Files:**
- Create: `src/compressors/git/diff_parser.rs`
- Modify: `src/compressors/git/diff.rs`
- Modify: `src/compressors/git/mod.rs`

- [ ] **Step 1: Create `diff_parser.rs` with the data types**

Create `src/compressors/git/diff_parser.rs` with the types currently in `diff.rs` lines 83-116, made public:

```rust
/// Shared data model for unified diff parsing.
/// Used by both GitDiffCompressor and GitLogCompressor.

#[derive(Debug, PartialEq)]
pub enum FileStatus {
    Normal,
    New,
    Deleted,
    Renamed,
    ModeChanged,
    Binary,
}

#[derive(Debug)]
pub struct DiffFile {
    pub path: String,
    pub status: FileStatus,
    pub old_path: Option<String>,
    pub old_mode: Option<String>,
    pub new_mode: Option<String>,
    pub hunks: Vec<Hunk>,
}

#[derive(Debug)]
pub struct Hunk {
    pub old_start: u32,
    pub new_start: u32,
    pub function_context: Option<String>,
    pub lines: Vec<DiffLine>,
}

#[derive(Debug, PartialEq)]
pub enum DiffLine {
    Context(String),
    Added(String),
    Removed(String),
}
```

- [ ] **Step 2: Register module in `mod.rs`**

Add `pub mod diff_parser;` to `src/compressors/git/mod.rs` (line 1, before existing modules).

- [ ] **Step 3: Update `diff.rs` to import types from `diff_parser`**

In `diff.rs`, remove the type definitions (lines 81-116: the `// --- Data model ---` comment through the `DiffLine` enum) and replace with:

```rust
use super::diff_parser::{DiffFile, DiffLine, FileStatus, Hunk};
```

Keep the `use crate::compressors::Compressor;` import at line 9.

- [ ] **Step 4: Verify all tests still pass**

Run: `cargo test`
Expected: All existing tests pass with no changes to assertions.

- [ ] **Step 5: Commit**

```bash
git add src/compressors/git/diff_parser.rs src/compressors/git/diff.rs src/compressors/git/mod.rs
git commit -m "$(cat <<'EOF'
refactor: extract shared diff types into diff_parser.rs

Move DiffFile, Hunk, DiffLine, and FileStatus into a shared module
so both git diff and git log compressors can use them.
EOF
)"
```

---

### Task 2: Extract shared diff parsing functions into `diff_parser.rs`

Move the parsing functions from `diff.rs` into `diff_parser.rs`.

**Files:**
- Modify: `src/compressors/git/diff_parser.rs`
- Modify: `src/compressors/git/diff.rs`

- [ ] **Step 1: Move parsing functions to `diff_parser.rs`**

Move these functions from `diff.rs` to `diff_parser.rs` (after the type definitions), making them `pub`:

- `parse_diff()` (diff.rs lines 121-141)
- `parse_file_chunk()` (diff.rs lines 144-200)
- `parse_hunks()` (diff.rs lines 203-234)
- `parse_hunk_header()` (diff.rs lines 237-266)

Copy them exactly as-is, just add `pub` to `parse_diff`. The other three remain private (only called by `parse_diff`).

```rust
/// Parse raw unified diff output into structured DiffFile entries.
pub fn parse_diff(raw: &str) -> Vec<DiffFile> {
    // ... exact same implementation as diff.rs lines 121-141
}

fn parse_file_chunk(chunk: &str) -> DiffFile {
    // ... exact same implementation as diff.rs lines 144-200
}

fn parse_hunks(lines: &[&str]) -> Vec<Hunk> {
    // ... exact same implementation as diff.rs lines 203-234
}

fn parse_hunk_header(line: &str) -> Hunk {
    // ... exact same implementation as diff.rs lines 237-266
}
```

- [ ] **Step 2: Update `diff.rs` to import `parse_diff`**

In `diff.rs`, remove the parsing section (lines 118-266: from `// --- Parsing ---` through end of `parse_hunk_header`) and add to the existing import line:

```rust
use super::diff_parser::{parse_diff, DiffFile, DiffLine, FileStatus, Hunk};
```

Remove the now-unused `Hunk` from the import if `diff.rs` no longer references it directly (it does — via `format_hunk` — so keep it).

- [ ] **Step 3: Verify all tests still pass**

Run: `cargo test`
Expected: All existing tests pass. Some unit tests in `diff.rs` test parsing functions directly (e.g., `parse_diff`, `parse_file_chunk`) — these now test the re-exported functions from `diff_parser`. They should still work because the import brings `parse_diff` into scope, and the test module uses `use super::*`.

- [ ] **Step 4: Commit**

```bash
git add src/compressors/git/diff_parser.rs src/compressors/git/diff.rs
git commit -m "$(cat <<'EOF'
refactor: extract diff parsing functions into diff_parser.rs

Move parse_diff, parse_file_chunk, parse_hunks, and parse_hunk_header
into the shared module for reuse by the git log compressor.
EOF
)"
```

---

### Task 3: Extract shared diff formatting functions into `diff_parser.rs`

Move the formatting functions from `diff.rs` into `diff_parser.rs`.

**Files:**
- Modify: `src/compressors/git/diff_parser.rs`
- Modify: `src/compressors/git/diff.rs`

- [ ] **Step 1: Move formatting functions to `diff_parser.rs`**

Move these functions from `diff.rs`, making `format_file` and `stat_summary` public:

- `format_file()` (diff.rs lines 271-297) → `pub`
- `format_hunk()` (diff.rs lines 300-335) → private (only called by `format_file`)
- `is_whitespace_only_hunk()` (diff.rs lines 339-358) → private
- `stat_summary()` (diff.rs lines 361-396) → `pub`

```rust
/// Format a single DiffFile into compressed output.
pub fn format_file(file: &DiffFile) -> String {
    // ... exact same implementation
}

fn format_hunk(hunk: &Hunk) -> String {
    // ... exact same implementation
}

fn is_whitespace_only_hunk(hunk: &Hunk) -> bool {
    // ... exact same implementation
}

/// Build stat summary line for multi-file diffs.
pub fn stat_summary(files: &[DiffFile]) -> String {
    // ... exact same implementation
}
```

- [ ] **Step 2: Update `diff.rs` imports**

In `diff.rs`, remove the formatting section (lines 268-396: from `// --- Formatting ---` through end of `stat_summary`). Update the import:

```rust
use super::diff_parser::{format_file, parse_diff, stat_summary, DiffFile};
```

Note: `DiffLine`, `FileStatus`, and `Hunk` are no longer needed in `diff.rs` — the remaining compress tests only call `GitDiffCompressor.compress()` which returns `Option<String>`, not raw types. Only `DiffFile` is referenced indirectly through `parse_diff` return type (used in `compress` method), and `format_file`/`stat_summary` are called directly.

After this, `diff.rs` should only contain: the TODO comments, the import line, `GitDiffCompressor` struct, `SKIP_FLAGS`, the `impl Compressor` block, and `#[cfg(test)] mod tests`.

- [ ] **Step 3: Fix unit tests that reference now-private functions**

Some unit tests in `diff.rs` call `parse_file_chunk`, `parse_hunks`, `parse_hunk_header`, `format_hunk`, `is_whitespace_only_hunk` directly. These are now private in `diff_parser.rs`. There are two options:

a) Move those tests to `diff_parser.rs` as a `#[cfg(test)] mod tests` block
b) Test them indirectly through `parse_diff` and `format_file`

**Approach (a) is cleaner.** Move the parsing and formatting unit tests that call private functions into `diff_parser.rs`. Keep the compressor-level tests (those calling `GitDiffCompressor.can_compress()`, `.normalized_args()`, `.compress()`) in `diff.rs`.

Tests to move to `diff_parser.rs`:
- `parse_normal_file_header`, `parse_new_file_header`, `parse_deleted_file_header`, `parse_renamed_file_header`, `parse_mode_change_header`, `parse_binary_file`, `parse_multiple_files`, `parse_hunk_content_lines`, `parse_multiple_hunks`, `parse_no_newline_at_end_stripped`

Tests to keep in `diff.rs`:
- All `can_compress_*` and `skip_*` tests
- All `normalized_args_*` tests
- All `compress_*` tests (these test the public `compress()` method end-to-end)
- `skip_non_diff_commands`

- [ ] **Step 4: Verify all tests pass**

Run: `cargo test`
Expected: All tests pass — same count, just some relocated to `diff_parser.rs`.

- [ ] **Step 5: Run clippy**

Run: `cargo clippy`
Expected: No warnings (no unused imports, no dead code).

- [ ] **Step 6: Commit**

```bash
git add src/compressors/git/diff_parser.rs src/compressors/git/diff.rs
git commit -m "$(cat <<'EOF'
refactor: extract diff formatting into diff_parser.rs

Move format_file, format_hunk, is_whitespace_only_hunk, and
stat_summary into the shared module. Relocate unit tests that
test private parsing/formatting functions.
EOF
)"
```

---

### Task 4: Implement `GitLogCompressor` — `can_compress` and skip flags

**Files:**
- Create: `src/compressors/git/log.rs`

- [ ] **Step 1: Write failing tests for `can_compress`**

Create `src/compressors/git/log.rs`:

```rust
use crate::compressors::Compressor;

pub struct GitLogCompressor;

/// Named presets for --pretty/--format that we compress (verbose ones).
const COMPRESSIBLE_PRESETS: &[&str] = &["short", "medium", "full", "fuller"];

/// Named presets we skip (already compact or specialized).
const SKIP_PRESETS: &[&str] = &["oneline", "reference", "email", "raw", "mboxrd"];

/// Flags that trigger passthrough.
const SKIP_FLAGS: &[&str] = &["--oneline", "--graph", "--color", "--color=always"];

impl Compressor for GitLogCompressor {
    fn can_compress(&self, args: &[String]) -> bool {
        todo!()
    }

    fn normalized_args(&self, original_args: &[String]) -> Vec<String> {
        todo!()
    }

    fn compress(&self, stdout: &str, stderr: &str, exit_code: i32) -> Option<String> {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(strs: &[&str]) -> Vec<String> {
        strs.iter().map(|s| s.to_string()).collect()
    }

    // --- positive cases ---

    #[test]
    fn can_compress_bare_log() {
        assert!(GitLogCompressor.can_compress(&args(&["log"])));
    }

    #[test]
    fn can_compress_log_with_n() {
        assert!(GitLogCompressor.can_compress(&args(&["log", "-n", "5"])));
    }

    #[test]
    fn can_compress_log_with_author() {
        assert!(GitLogCompressor.can_compress(&args(&["log", "--author=Alice"])));
    }

    #[test]
    fn can_compress_log_with_patch() {
        assert!(GitLogCompressor.can_compress(&args(&["log", "-p"])));
    }

    #[test]
    fn can_compress_log_with_stat() {
        assert!(GitLogCompressor.can_compress(&args(&["log", "--stat"])));
    }

    #[test]
    fn can_compress_log_with_since() {
        assert!(GitLogCompressor.can_compress(&args(&["log", "--since=2024-01-01"])));
    }

    #[test]
    fn can_compress_log_with_pretty_medium() {
        assert!(GitLogCompressor.can_compress(&args(&["log", "--pretty=medium"])));
    }

    #[test]
    fn can_compress_log_with_pretty_full() {
        assert!(GitLogCompressor.can_compress(&args(&["log", "--pretty=full"])));
    }

    // --- skip flags ---

    #[test]
    fn skip_log_oneline() {
        assert!(!GitLogCompressor.can_compress(&args(&["log", "--oneline"])));
    }

    #[test]
    fn skip_log_graph() {
        assert!(!GitLogCompressor.can_compress(&args(&["log", "--graph"])));
    }

    #[test]
    fn skip_log_color() {
        assert!(!GitLogCompressor.can_compress(&args(&["log", "--color"])));
    }

    #[test]
    fn skip_log_color_always() {
        assert!(!GitLogCompressor.can_compress(&args(&["log", "--color=always"])));
    }

    #[test]
    fn skip_log_custom_format() {
        assert!(!GitLogCompressor.can_compress(&args(&["log", "--format=%H %s"])));
    }

    #[test]
    fn skip_log_pretty_custom() {
        assert!(!GitLogCompressor.can_compress(&args(&["log", "--pretty=%H %an"])));
    }

    #[test]
    fn skip_log_pretty_oneline() {
        assert!(!GitLogCompressor.can_compress(&args(&["log", "--pretty=oneline"])));
    }

    #[test]
    fn skip_log_pretty_reference() {
        assert!(!GitLogCompressor.can_compress(&args(&["log", "--pretty=reference"])));
    }

    #[test]
    fn skip_log_pretty_email() {
        assert!(!GitLogCompressor.can_compress(&args(&["log", "--pretty=email"])));
    }

    #[test]
    fn skip_log_pretty_raw() {
        assert!(!GitLogCompressor.can_compress(&args(&["log", "--pretty=raw"])));
    }

    #[test]
    fn skip_log_pretty_mboxrd() {
        assert!(!GitLogCompressor.can_compress(&args(&["log", "--pretty=mboxrd"])));
    }

    // --- non-log commands ---

    #[test]
    fn skip_non_log_commands() {
        assert!(!GitLogCompressor.can_compress(&args(&["status"])));
        assert!(!GitLogCompressor.can_compress(&args(&["diff"])));
        assert!(!GitLogCompressor.can_compress(&args(&["log-tree"])));
        assert!(!GitLogCompressor.can_compress(&args(&[])));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib log::tests`
Expected: FAIL with `not yet implemented`

- [ ] **Step 3: Implement `can_compress`**

Replace the `todo!()` in `can_compress` with:

```rust
fn can_compress(&self, args: &[String]) -> bool {
    if args.first().map(|s| s.as_str()) != Some("log") {
        return false;
    }

    let tail = &args[1..];

    // Check simple skip flags
    if tail.iter().any(|arg| SKIP_FLAGS.contains(&arg.as_str())) {
        return false;
    }

    // Check --format= and --pretty= flags
    for arg in tail {
        if let Some(fmt) = arg.strip_prefix("--format=") {
            if !COMPRESSIBLE_PRESETS.contains(&fmt) {
                return false;
            }
        }
        if let Some(fmt) = arg.strip_prefix("--pretty=") {
            if SKIP_PRESETS.contains(&fmt) {
                return false;
            }
            if !COMPRESSIBLE_PRESETS.contains(&fmt) {
                // Not a known preset — treat as custom format string
                return false;
            }
        }
    }

    true
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib log::tests`
Expected: All `can_compress` tests pass. The `normalized_args` and `compress` tests will still panic from `todo!()` — that's expected (they won't run unless called).

- [ ] **Step 5: Commit**

```bash
git add src/compressors/git/log.rs
git commit -m "$(cat <<'EOF'
feat: add GitLogCompressor with can_compress and skip flag detection

Handles --oneline, --graph, --color, custom --format/--pretty as skip
flags. Compresses verbose presets (medium, full, fuller, short).
EOF
)"
```

---

### Task 5: Implement `normalized_args`

**Files:**
- Modify: `src/compressors/git/log.rs`

- [ ] **Step 1: Write failing tests for `normalized_args`**

Add to the `tests` module in `log.rs`:

```rust
// --- normalized_args ---

#[test]
fn normalized_args_bare_log() {
    let result = GitLogCompressor.normalized_args(&args(&["log"]));
    assert!(result.contains(&"log".to_string()));
    assert!(result.iter().any(|a| a.starts_with("--format=")));
    assert!(result.contains(&"--no-color".to_string()));
    assert!(result.contains(&"-n".to_string()));
    assert!(result.contains(&"20".to_string()));
}

#[test]
fn normalized_args_injects_default_cap() {
    let result = GitLogCompressor.normalized_args(&args(&["log"]));
    let n_idx = result.iter().position(|a| a == "-n").unwrap();
    assert_eq!(result[n_idx + 1], "20");
}

#[test]
fn normalized_args_preserves_user_n() {
    let result = GitLogCompressor.normalized_args(&args(&["log", "-n", "5"]));
    // Should NOT inject -n 20 since user provided -n 5
    let n_count = result.iter().filter(|a| *a == "-n").count();
    assert_eq!(n_count, 1);
    assert!(result.contains(&"5".to_string()));
}

#[test]
fn normalized_args_preserves_max_count() {
    let result = GitLogCompressor.normalized_args(&args(&["log", "--max-count=10"]));
    let n_count = result.iter().filter(|a| *a == "-n").count();
    assert_eq!(n_count, 0);
    assert!(result.contains(&"--max-count=10".to_string()));
}

#[test]
fn normalized_args_with_patch_adds_diff_flags() {
    let result = GitLogCompressor.normalized_args(&args(&["log", "-p"]));
    assert!(result.contains(&"-p".to_string()));
    assert!(result.contains(&"--unified=1".to_string()));
    assert!(result.contains(&"--no-ext-diff".to_string()));
    assert!(result.contains(&"--diff-algorithm=histogram".to_string()));
}

#[test]
fn normalized_args_patch_alias() {
    let result = GitLogCompressor.normalized_args(&args(&["log", "--patch"]));
    assert!(result.contains(&"-p".to_string()));
    assert!(result.contains(&"--unified=1".to_string()));
}

#[test]
fn normalized_args_preserves_filters() {
    let result = GitLogCompressor.normalized_args(&args(&["log", "--author=Alice", "--since=2024-01-01"]));
    assert!(result.contains(&"--author=Alice".to_string()));
    assert!(result.contains(&"--since=2024-01-01".to_string()));
}

#[test]
fn normalized_args_preserves_stat() {
    let result = GitLogCompressor.normalized_args(&args(&["log", "--stat"]));
    assert!(result.contains(&"--stat".to_string()));
}

#[test]
fn normalized_args_preserves_range() {
    let result = GitLogCompressor.normalized_args(&args(&["log", "HEAD~5..HEAD"]));
    assert!(result.contains(&"HEAD~5..HEAD".to_string()));
}

#[test]
fn normalized_args_strips_pretty_preset() {
    let result = GitLogCompressor.normalized_args(&args(&["log", "--pretty=medium"]));
    // --pretty=medium should be stripped (we override with our format)
    assert!(!result.contains(&"--pretty=medium".to_string()));
}

#[test]
fn normalized_args_numeric_shorthand_count() {
    // git log -5 is shorthand for -n 5
    let result = GitLogCompressor.normalized_args(&args(&["log", "-5"]));
    // Should NOT inject -n 20 since user provided -5
    assert!(!result.contains(&"-n".to_string()) || !result.contains(&"20".to_string()));
    assert!(result.contains(&"-5".to_string()));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib log::tests::normalized_args`
Expected: FAIL with `not yet implemented`

- [ ] **Step 3: Implement `normalized_args`**

Replace the `todo!()` in `normalized_args`:

```rust
fn normalized_args(&self, original_args: &[String]) -> Vec<String> {
    let tail = &original_args[1..];

    let has_patch = tail.iter().any(|a| a == "-p" || a == "--patch" || a == "-u");
    let has_count = tail.iter().any(|a| {
        a == "-n"
            || a.starts_with("-n")
            || a.starts_with("--max-count")
            || (a.starts_with('-') && a[1..].chars().all(|c| c.is_ascii_digit()))
    });

    let mut result = vec![
        "log".to_string(),
        "--format=%x01%h%x00%D%x00%aI%x00%an%x00%s%x00%b".to_string(),
        "--no-color".to_string(),
    ];

    if has_patch {
        result.push("-p".to_string());
        result.push("--unified=1".to_string());
        result.push("--no-ext-diff".to_string());
        result.push("--diff-algorithm=histogram".to_string());
    }

    if !has_count {
        result.push("-n".to_string());
        result.push("20".to_string());
    }

    // Append remaining user args, stripping what we already handle
    for arg in tail {
        if arg == "-p" || arg == "--patch" || arg == "-u" {
            continue; // Already handled above
        }
        if arg.starts_with("--format=") || arg.starts_with("--pretty=") {
            continue; // We override the format
        }
        if arg == "--color" || arg.starts_with("--color=") {
            continue; // We add --no-color
        }
        result.push(arg.clone());
    }

    result
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib log::tests`
Expected: All `can_compress` and `normalized_args` tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/compressors/git/log.rs
git commit -m "$(cat <<'EOF'
feat: implement normalized_args for GitLogCompressor

Injects machine-parseable format string, default -n 20 cap,
diff flags when -p is present. Preserves user filters and ranges.
EOF
)"
```

---

### Task 6: Implement log parsing — commit metadata

**Files:**
- Modify: `src/compressors/git/log.rs`

- [ ] **Step 1: Write the data model and parser skeleton**

Add above the `impl Compressor` block in `log.rs`:

```rust
use super::diff_parser;

struct LogEntry {
    hash: String,
    decorations: Vec<String>,
    date: String,
    author: String,
    subject: String,
    body: Option<String>,
    diff: Option<Vec<diff_parser::DiffFile>>,
    stat: Option<String>,
}

/// Parse the NUL-delimited format output into LogEntry structs.
fn parse_log(raw: &str, has_patch: bool, has_stat: bool) -> Option<Vec<LogEntry>> {
    let chunks: Vec<&str> = raw.split('\x01').collect();
    let mut entries = Vec::new();

    for chunk in &chunks[1..] {
        // Skip empty chunks
        if chunk.trim().is_empty() {
            continue;
        }

        let entry = parse_log_entry(chunk, has_patch, has_stat)?;
        entries.push(entry);
    }

    Some(entries)
}

/// Parse a single commit chunk into a LogEntry.
fn parse_log_entry(chunk: &str, has_patch: bool, has_stat: bool) -> Option<LogEntry> {
    // Split off diff portion first if -p is active
    let (format_and_stat, diff_raw) = if has_patch {
        match chunk.find("\ndiff --git ") {
            Some(pos) => (&chunk[..pos], Some(&chunk[pos + 1..])),
            None => (chunk, None),
        }
    } else {
        (chunk, None)
    };

    // Split off stat portion if --stat is active
    let (format_part, stat_raw) = if has_stat {
        split_stat(format_and_stat)
    } else {
        (format_and_stat, None)
    };

    // Split format fields on NUL
    let fields: Vec<&str> = format_part.split('\x00').collect();
    if fields.len() < 6 {
        return None;
    }

    let hash = fields[0].trim().to_string();
    let decorations = if fields[1].is_empty() {
        Vec::new()
    } else {
        fields[1].split(", ").map(|s| s.to_string()).collect()
    };
    let date = fields[2]
        .get(..10)
        .unwrap_or(fields[2])
        .to_string();
    let author = fields[3].to_string();
    let subject = fields[4].to_string();
    let body_raw = fields[5..].join("\x00"); // rejoin in case body contained NUL (unlikely)
    let body = if body_raw.trim().is_empty() {
        None
    } else {
        Some(body_raw.trim().to_string())
    };

    let diff = match diff_raw {
        Some(raw) => {
            let files = diff_parser::parse_diff(raw);
            if files.is_empty() { None } else { Some(files) }
        }
        None => None,
    };

    let stat = match stat_raw {
        Some(raw) => Some(compress_stat(raw)),
        None => None,
    };

    Some(LogEntry {
        hash,
        decorations,
        date,
        author,
        subject,
        body,
        diff,
        stat,
    })
}

/// Split stat output from the format portion.
/// Stat lines have the pattern " path | N ++--" or "N file(s) changed".
fn split_stat(text: &str) -> (&str, Option<&str>) {
    // Look for the first line matching stat pattern after the format fields
    // The format fields end after the last NUL byte's content
    if let Some(last_nul) = text.rfind('\x00') {
        let after_format = &text[last_nul + 1..];
        // Find first stat-like line in the body/post-body area
        for (i, line) in after_format.lines().enumerate() {
            let trimmed = line.trim();
            if (trimmed.contains(" | ") && (trimmed.contains('+') || trimmed.contains('-')))
                || trimmed.ends_with("changed")
                || (trimmed.contains("file") && trimmed.contains("changed"))
            {
                let line_start = after_format
                    .lines()
                    .take(i)
                    .map(|l| l.len() + 1)
                    .sum::<usize>();
                let split_pos = last_nul + 1 + line_start;
                return (&text[..split_pos], Some(&text[split_pos..]));
            }
        }
    }
    (text, None)
}

/// Compress stat output: replace +++--- bars with N+ N- format.
fn compress_stat(raw: &str) -> String {
    let mut result = String::new();

    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Summary line — pass through as-is
        if trimmed.contains("file") && trimmed.contains("changed") {
            result.push_str(trimmed);
            result.push('\n');
            continue;
        }

        // File stat line: " path | N +++---"
        if let Some(pipe_pos) = trimmed.find(" | ") {
            let path = trimmed[..pipe_pos].trim();
            let after_pipe = trimmed[pipe_pos + 3..].trim();

            // Count + and - characters in the bar
            let insertions = after_pipe.chars().filter(|c| *c == '+').count();
            let deletions = after_pipe.chars().filter(|c| *c == '-').count();

            result.push_str(path);
            result.push_str(" | ");
            if insertions > 0 {
                result.push_str(&format!("{}+", insertions));
            }
            if deletions > 0 {
                if insertions > 0 {
                    result.push(' ');
                }
                result.push_str(&format!("{}-", deletions));
            }
            result.push('\n');
        } else {
            // Unknown line — pass through
            result.push_str(trimmed);
            result.push('\n');
        }
    }

    result
}
```

- [ ] **Step 2: Write tests for metadata parsing**

Add to the `tests` module:

```rust
// --- parsing tests ---

#[test]
fn parse_single_commit() {
    let raw = "\x01abc1234\x00HEAD -> main\x002024-01-15T10:30:00+00:00\x00John Smith\x00Add feature\x00";
    let entries = parse_log(raw, false, false).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].hash, "abc1234");
    assert_eq!(entries[0].decorations, vec!["HEAD -> main"]);
    assert_eq!(entries[0].date, "2024-01-15");
    assert_eq!(entries[0].author, "John Smith");
    assert_eq!(entries[0].subject, "Add feature");
    assert!(entries[0].body.is_none());
}

#[test]
fn parse_multiple_commits() {
    let raw = "\x01abc1234\x00main\x002024-01-15T10:30:00+00:00\x00Alice\x00First\x00\x01def5678\x00\x002024-01-14T10:30:00+00:00\x00Bob\x00Second\x00";
    let entries = parse_log(raw, false, false).unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].subject, "First");
    assert_eq!(entries[1].subject, "Second");
    assert!(entries[1].decorations.is_empty());
}

#[test]
fn parse_commit_with_body() {
    let raw = "\x01abc1234\x00\x002024-01-15T10:30:00+00:00\x00Alice\x00Short subject\x00This is the body.\nSecond line of body.\n";
    let entries = parse_log(raw, false, false).unwrap();
    assert_eq!(entries[0].body, Some("This is the body.\nSecond line of body.".to_string()));
}

#[test]
fn parse_commit_empty_body_trimmed() {
    let raw = "\x01abc1234\x00\x002024-01-15T10:30:00+00:00\x00Alice\x00Subject\x00  \n  \n";
    let entries = parse_log(raw, false, false).unwrap();
    assert!(entries[0].body.is_none());
}

#[test]
fn parse_commit_multiple_decorations() {
    let raw = "\x01abc1234\x00HEAD -> main, origin/main, tag: v1.0\x002024-01-15T10:30:00+00:00\x00Alice\x00Subject\x00";
    let entries = parse_log(raw, false, false).unwrap();
    assert_eq!(entries[0].decorations, vec!["HEAD -> main", "origin/main", "tag: v1.0"]);
}

#[test]
fn parse_empty_input() {
    let entries = parse_log("", false, false).unwrap();
    assert!(entries.is_empty());
}

#[test]
fn parse_malformed_returns_none() {
    // Too few NUL-separated fields
    let raw = "\x01abc1234\x00only_two_fields";
    let result = parse_log(raw, false, false);
    assert!(result.is_none());
}
```

- [ ] **Step 3: Run tests to verify they pass**

Run: `cargo test --lib log::tests::parse`
Expected: All parsing tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/compressors/git/log.rs
git commit -m "$(cat <<'EOF'
feat: implement git log parsing for commit metadata

Parse NUL-delimited format output into LogEntry structs with hash,
decorations, date, author, subject, body, and optional diff/stat.
EOF
)"
```

---

### Task 7: Implement stat compression

**Files:**
- Modify: `src/compressors/git/log.rs`

- [ ] **Step 1: Write tests for stat compression**

Add to the `tests` module:

```rust
// --- stat compression tests ---

#[test]
fn compress_stat_single_file() {
    let input = " src/main.rs | 3 +++\n 1 file changed, 3 insertions(+)\n";
    let result = compress_stat(input);
    assert!(result.contains("src/main.rs | 3+"));
    assert!(result.contains("1 file changed, 3 insertions(+)"));
}

#[test]
fn compress_stat_mixed_changes() {
    let input = " src/auth.rs | 15 +++++++++------\n 2 files changed, 9 insertions(+), 6 deletions(-)\n";
    let result = compress_stat(input);
    assert!(result.contains("src/auth.rs | 9+ 6-"));
}

#[test]
fn compress_stat_deletions_only() {
    let input = " src/old.rs | 5 -----\n 1 file changed, 5 deletions(-)\n";
    let result = compress_stat(input);
    assert!(result.contains("src/old.rs | 5-"));
    assert!(!result.contains("+")); // No insertions
}

#[test]
fn compress_stat_multiple_files() {
    let input = " src/a.rs | 3 +++\n src/b.rs | 5 ++---\n 2 files changed, 4 insertions(+), 3 deletions(-)\n";
    let result = compress_stat(input);
    assert!(result.contains("src/a.rs | 3+"));
    assert!(result.contains("src/b.rs | 2+ 3-"));
    assert!(result.contains("2 files changed"));
}

#[test]
fn compress_stat_summary_passthrough() {
    let input = " 1 file changed, 1 insertion(+)\n";
    let result = compress_stat(input);
    assert_eq!(result.trim(), "1 file changed, 1 insertion(+)");
}
```

- [ ] **Step 2: Run tests to verify they pass**

Run: `cargo test --lib log::tests::compress_stat`
Expected: All stat compression tests pass (the `compress_stat` function was already implemented in Task 6).

- [ ] **Step 3: Commit**

```bash
git add src/compressors/git/log.rs
git commit -m "$(cat <<'EOF'
test: add unit tests for git log stat compression

Verify +++--- bars are compressed to N+ N- format, summary lines
pass through, and edge cases (deletions-only, mixed) work correctly.
EOF
)"
```

---

### Task 8: Implement formatting and `compress`

**Files:**
- Modify: `src/compressors/git/log.rs`

- [ ] **Step 1: Write the formatting function**

Add to `log.rs` (after `compress_stat`):

```rust
/// Format parsed log entries into compressed output.
fn format_log(entries: &[LogEntry]) -> String {
    if entries.is_empty() {
        return "(empty)\n".to_string();
    }

    let mut output = String::new();

    for entry in entries {
        // Commit header: * hash (decorations) date [author] subject
        output.push_str("* ");
        output.push_str(&entry.hash);

        if !entry.decorations.is_empty() {
            output.push_str(" (");
            output.push_str(&entry.decorations.join(", "));
            output.push(')');
        }

        output.push(' ');
        output.push_str(&entry.date);
        output.push_str(" [");
        output.push_str(&entry.author);
        output.push_str("] ");
        output.push_str(&entry.subject);
        output.push('\n');

        // Body (indented with 2 spaces)
        if let Some(body) = &entry.body {
            for line in body.lines() {
                output.push_str("  ");
                output.push_str(line);
                output.push('\n');
            }
        }

        // Stat (indented with 2 spaces)
        if let Some(stat) = &entry.stat {
            for line in stat.lines() {
                output.push_str("  ");
                output.push_str(line);
                output.push('\n');
            }
        }

        // Diff (blank line separator, not indented)
        if let Some(files) = &entry.diff {
            output.push('\n');
            for file in files {
                output.push_str(&diff_parser::format_file(file));
            }
        }
    }

    output
}
```

- [ ] **Step 2: Implement `compress`**

Replace the `todo!()` in `compress`:

```rust
fn compress(&self, stdout: &str, _stderr: &str, exit_code: i32) -> Option<String> {
    if exit_code != 0 {
        return None;
    }
    if stdout.trim().is_empty() {
        return Some("(empty)\n".to_string());
    }

    // Detect whether -p or --stat output is present
    let has_patch = stdout.contains("\ndiff --git ");
    let has_stat = stdout.contains(" | ") && (stdout.contains("file changed") || stdout.contains("files changed"));

    let entries = parse_log(stdout, has_patch, has_stat)?;
    if entries.is_empty() {
        return Some("(empty)\n".to_string());
    }

    let mut output = format_log(&entries);

    // Truncation notice when exactly 20 commits returned
    if entries.len() == 20 {
        output.push_str("(showing 20 commits, use -n to see more)\n");
    }

    Some(output)
}
```

- [ ] **Step 3: Write formatting tests**

Add to the `tests` module:

```rust
// --- formatting tests ---

#[test]
fn format_standard_commit() {
    let raw = "\x01a1b2c3f\x00HEAD -> main\x002024-01-15T10:30:00+00:00\x00John Smith\x00Add auth\x00";
    let entries = parse_log(raw, false, false).unwrap();
    let output = format_log(&entries);
    assert_eq!(output, "* a1b2c3f (HEAD -> main) 2024-01-15 [John Smith] Add auth\n");
}

#[test]
fn format_commit_no_decorations() {
    let raw = "\x01a1b2c3f\x00\x002024-01-15T10:30:00+00:00\x00Alice\x00Fix bug\x00";
    let entries = parse_log(raw, false, false).unwrap();
    let output = format_log(&entries);
    assert_eq!(output, "* a1b2c3f 2024-01-15 [Alice] Fix bug\n");
}

#[test]
fn format_commit_with_body() {
    let raw = "\x01a1b2c3f\x00\x002024-01-15T10:30:00+00:00\x00Alice\x00Short subject\x00Body line 1.\nBody line 2.\n";
    let entries = parse_log(raw, false, false).unwrap();
    let output = format_log(&entries);
    assert!(output.contains("* a1b2c3f 2024-01-15 [Alice] Short subject\n"));
    assert!(output.contains("  Body line 1.\n"));
    assert!(output.contains("  Body line 2.\n"));
}

#[test]
fn format_empty_log() {
    let entries: Vec<LogEntry> = Vec::new();
    let output = format_log(&entries);
    assert_eq!(output, "(empty)\n");
}

#[test]
fn format_multiple_commits() {
    let raw = "\x01aaa\x00main\x002024-01-15T10:00:00+00:00\x00Alice\x00First\x00\x01bbb\x00\x002024-01-14T10:00:00+00:00\x00Bob\x00Second\x00";
    let entries = parse_log(raw, false, false).unwrap();
    let output = format_log(&entries);
    assert!(output.contains("* aaa (main) 2024-01-15 [Alice] First\n"));
    assert!(output.contains("* bbb 2024-01-14 [Bob] Second\n"));
}

// --- compress tests ---

#[test]
fn compress_nonzero_exit_returns_none() {
    let result = GitLogCompressor.compress("anything", "error", 128);
    assert_eq!(result, None);
}

#[test]
fn compress_empty_output() {
    let result = GitLogCompressor.compress("", "", 0).unwrap();
    assert_eq!(result, "(empty)\n");
}

#[test]
fn compress_whitespace_only_output() {
    let result = GitLogCompressor.compress("  \n\n  ", "", 0).unwrap();
    assert_eq!(result, "(empty)\n");
}

#[test]
fn compress_truncation_notice() {
    // Build 20 commits
    let mut raw = String::new();
    for i in 0..20 {
        raw.push_str(&format!(
            "\x01hash{:02}\x00\x002024-01-{:02}T10:00:00+00:00\x00Alice\x00Commit {}\x00",
            i, (i % 28) + 1, i
        ));
    }
    let result = GitLogCompressor.compress(&raw, "", 0).unwrap();
    assert!(result.contains("(showing 20 commits, use -n to see more)"));
}

#[test]
fn compress_no_truncation_notice_under_20() {
    let raw = "\x01abc\x00\x002024-01-15T10:00:00+00:00\x00Alice\x00Only one\x00";
    let result = GitLogCompressor.compress(&raw, "", 0).unwrap();
    assert!(!result.contains("showing"));
}
```

- [ ] **Step 4: Run all tests**

Run: `cargo test --lib log::tests`
Expected: All tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/compressors/git/log.rs
git commit -m "$(cat <<'EOF'
feat: implement compress and formatting for GitLogCompressor

Format commits as single-line entries with optional body, stat,
and diff. Truncation notice when exactly 20 commits returned.
EOF
)"
```

---

### Task 9: Register `GitLogCompressor` in dispatcher

**Files:**
- Modify: `src/compressors/git/mod.rs`

- [ ] **Step 1: Add module declaration and register compressor**

Update `src/compressors/git/mod.rs`:

```rust
pub mod diff;
pub mod diff_parser;
pub mod log;
pub mod status;

use super::Compressor;

/// Find a compressor for the given git subcommand args.
pub fn find_compressor(args: &[String]) -> Option<Box<dyn Compressor>> {
    let compressors: Vec<Box<dyn Compressor>> = vec![
        Box::new(diff::GitDiffCompressor),
        Box::new(log::GitLogCompressor),
        Box::new(status::GitStatusCompressor),
    ];

    compressors
        .into_iter()
        .find(|compressor| compressor.can_compress(args))
}
```

- [ ] **Step 2: Build and verify compilation**

Run: `cargo build`
Expected: Compiles with no errors.

- [ ] **Step 3: Run all tests**

Run: `cargo test`
Expected: All existing tests pass, plus all new log tests.

- [ ] **Step 4: Update passthrough test**

In `tests/passthrough.rs`, update the test `passthrough_for_unknown_subcommand` (lines 26-43). This test uses `git log --oneline` as an example of an unknown subcommand — but now `git log` has a compressor. However, `--oneline` is a skip flag, so it should still passthrough. Update the comment:

```rust
#[test]
fn passthrough_for_skip_flags() {
    let repo = common::create_temp_repo();

    // git log --oneline has a compressor but --oneline is a skip flag — should passthrough
    let output = Command::new(common::binary_path())
        .args(["git", "log", "--oneline"])
        .env("TOKEN_SAVER", "1")
        .current_dir(repo.path())
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("init"),
        "Expected git log output with commit message, got: {}",
        stdout
    );
}
```

Also add a new test for a truly unknown subcommand:

```rust
#[test]
fn passthrough_for_unknown_subcommand() {
    let repo = common::create_temp_repo();

    // git shortlog has no compressor — should passthrough
    let output = Command::new(common::binary_path())
        .args(["git", "shortlog", "-1"])
        .env("TOKEN_SAVER", "1")
        .current_dir(repo.path())
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Test") || stdout.contains("init"),
        "Expected git shortlog output, got: {}",
        stdout
    );
}
```

- [ ] **Step 5: Run all tests**

Run: `cargo test`
Expected: All tests pass.

- [ ] **Step 6: Commit**

```bash
git add src/compressors/git/mod.rs src/compressors/git/log.rs tests/passthrough.rs
git commit -m "$(cat <<'EOF'
feat: register GitLogCompressor in dispatcher

Wire up the log compressor in the git subcommand dispatcher.
Update passthrough tests for the new compressor.
EOF
)"
```

---

### Task 10: Add integration test scenarios

**Files:**
- Create: `tests/common/git_log.rs`
- Create: `tests/git_log.rs`
- Modify: `tests/common/mod.rs`

- [ ] **Step 1: Create scenario definitions**

Create `tests/common/git_log.rs`:

```rust
use std::path::Path;
use std::process::Command;

use super::{Assertion, Scenario};

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
                // Should NOT contain verbose default format elements
                Assertion::NotContains("Author:"),
                Assertion::NotContains("Date:"),
                Assertion::NotContains("commit "),
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
                // Should have compressed stat format
                Assertion::NotContains("+++"),
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
            assertions: vec![
                Assertion::Contains("(empty)"),
            ],
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
        .args(["commit", "-m", "commit with body\n\nThis is the detailed body.\nWith multiple lines."])
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
```

- [ ] **Step 2: Register module in `tests/common/mod.rs`**

Add `pub mod git_log;` after the existing module declarations at the top of `tests/common/mod.rs`:

```rust
pub mod git_diff;
pub mod git_log;
pub mod git_status;
```

- [ ] **Step 3: Create integration test file**

Create `tests/git_log.rs`:

```rust
mod common;

#[test]
fn compressed_basic_log() {
    common::run_test(&common::git_log::scenarios()[0]);
}

#[test]
fn compressed_log_with_body() {
    common::run_test(&common::git_log::scenarios()[1]);
}

#[test]
fn compressed_log_with_patch() {
    common::run_test(&common::git_log::scenarios()[2]);
}

#[test]
fn compressed_log_with_stat() {
    common::run_test(&common::git_log::scenarios()[3]);
}

#[test]
fn compressed_log_with_n() {
    common::run_test(&common::git_log::scenarios()[4]);
}

#[test]
fn compressed_log_empty_result() {
    common::run_test(&common::git_log::scenarios()[5]);
}
```

- [ ] **Step 4: Run integration tests**

Run: `cargo test --test git_log`
Expected: All integration tests pass.

- [ ] **Step 5: Commit**

```bash
git add tests/common/git_log.rs tests/common/mod.rs tests/git_log.rs
git commit -m "$(cat <<'EOF'
test: add integration test scenarios for git log compressor

Cover basic log, commit body, patches, stat, -n flag, and empty results.
EOF
)"
```

---

### Task 11: Add token comparison and passthrough tests

**Files:**
- Modify: `tests/compare.rs`
- Modify: `tests/passthrough.rs`

- [ ] **Step 1: Add git log comparison test**

Add to `tests/compare.rs`:

```rust
#[test]
#[ignore]
fn compare_git_log() {
    let scenarios = common::git_log::scenarios();
    let mut results = Vec::new();

    for scenario in &scenarios {
        let (raw, comp) = common::run_compare(scenario);
        results.push(ScenarioResult {
            name: scenario.name.to_string(),
            raw_tokens: raw,
            compressed_tokens: comp,
        });
    }

    common::print_summary(&results);
}
```

- [ ] **Step 2: Add git log passthrough tests**

Add to `tests/passthrough.rs`:

```rust
#[test]
fn passthrough_log_oneline() {
    let repo = common::create_temp_repo();

    let output = Command::new(common::binary_path())
        .args(["git", "log", "--oneline"])
        .env("TOKEN_SAVER", "1")
        .current_dir(repo.path())
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    // --oneline is a skip flag — should get raw git output
    assert!(
        stdout.contains("init"),
        "Expected raw git log --oneline output, got: {}",
        stdout
    );
    // Should NOT contain our compressed format markers
    assert!(
        !stdout.contains("[Test]"),
        "Should not contain compressed author format: {}",
        stdout
    );
}

#[test]
fn passthrough_log_custom_format() {
    let repo = common::create_temp_repo();

    let output = Command::new(common::binary_path())
        .args(["git", "log", "--format=%H %s"])
        .env("TOKEN_SAVER", "1")
        .current_dir(repo.path())
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    // Custom format is a skip flag — should get raw git output
    assert!(
        stdout.contains("init"),
        "Expected raw git log output, got: {}",
        stdout
    );
}

#[test]
fn passthrough_log_graph() {
    let repo = common::create_temp_repo();

    let output = Command::new(common::binary_path())
        .args(["git", "log", "--graph", "--oneline"])
        .env("TOKEN_SAVER", "1")
        .current_dir(repo.path())
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("init"),
        "Expected raw git log --graph output, got: {}",
        stdout
    );
}
```

- [ ] **Step 3: Run all tests**

Run: `cargo test`
Expected: All tests pass.

- [ ] **Step 4: Run token comparison (visual check)**

Run: `cargo test --test compare compare_git_log -- --ignored --nocapture`
Expected: Table showing raw vs compressed token counts with savings percentages.

- [ ] **Step 5: Commit**

```bash
git add tests/compare.rs tests/passthrough.rs
git commit -m "$(cat <<'EOF'
test: add token comparison and passthrough tests for git log

Compare raw vs compressed output with token counts. Verify --oneline,
--graph, and custom --format passthrough correctly.
EOF
)"
```

---

### Task 12: Run full verification and fix issues

**Files:**
- Any files that need fixes

- [ ] **Step 1: Run full test suite**

Run: `cargo test`
Expected: All tests pass (unit + integration + passthrough).

- [ ] **Step 2: Run clippy**

Run: `cargo clippy`
Expected: No warnings.

- [ ] **Step 3: Run formatter**

Run: `cargo fmt`
Expected: No changes (or apply formatting fixes).

- [ ] **Step 4: Run token comparison for all commands**

Run: `cargo test --test compare -- --ignored --nocapture`
Expected: Summary tables for git status, git diff, and git log showing token savings.

- [ ] **Step 5: Manual smoke test**

Run: `TOKEN_SAVER=1 cargo run -- git log`
Expected: Compressed log output with `* hash date [author] subject` format.

Run: `TOKEN_SAVER=1 cargo run -- git log -p -n 3`
Expected: Compressed log with embedded compressed diffs.

Run: `TOKEN_SAVER=1 cargo run -- git log --oneline`
Expected: Raw git log --oneline output (passthrough).

- [ ] **Step 6: Fix any issues found**

If any tests fail or clippy/fmt report issues, fix them and re-run.

- [ ] **Step 7: Commit any fixes**

```bash
git add -A
git commit -m "fix: address issues found during final verification"
```

Only create this commit if there were fixes needed. If everything passed, skip this step.
