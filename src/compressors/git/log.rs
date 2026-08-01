use super::commit_parser::{self, CommitFields};
use super::diff_parser;
use crate::compressors::Compressor;

#[derive(Default)]
pub struct GitLogCompressor {
    /// True when the user passed an explicit commit count (-n/-N/--max-count).
    /// We then neither inject our own cap nor claim truncation, so the exact set
    /// the user asked for is returned (no off-by-one drop at count == cap+1).
    pub user_limit: bool,
}

/// True when the user passed an explicit commit-count flag (`-n N`, `-nN`,
/// `--max-count[=N]`, or `-<digits>`). Used to avoid injecting our own cap and to
/// suppress the truncation notice — we only claim "more" for caps WE added.
pub(crate) fn user_specified_count(args: &[String]) -> bool {
    args.iter().skip(1).any(|a| {
        a == "-n"
            || a.starts_with("-n")
            || a.starts_with("--max-count")
            || (a.len() > 1 && a.starts_with('-') && a[1..].chars().all(|c| c.is_ascii_digit()))
    })
}

/// Flags that cause passthrough — agent chose a specific output format or display mode.
/// The output-shape flags (`--name-only` etc.) emit per-commit data we cannot place in
/// our format slots, so they must pass through rather than be parsed as body text.
const SKIP_FLAGS: &[&str] = &[
    "--oneline",
    "--graph",
    "--color",
    "--color=always",
    "--name-only",
    "--name-status",
    "--numstat",
    "--shortstat",
    "--raw",
];

/// Default cap on commits shown. We request one extra to detect genuine truncation.
const DEFAULT_LIMIT: usize = 20;

/// Known git --pretty presets that we can compress (verbose formats we improve upon).
const COMPRESS_PRESETS: &[&str] = &["short", "medium", "full", "fuller"];

/// Known git --pretty presets that trigger passthrough (specialized or already compact).
const SKIP_PRESETS: &[&str] = &["oneline", "reference", "email", "raw", "mboxrd"];

fn should_skip_format_arg(arg: &str) -> bool {
    // --format=<value> — skip unless the value is a compress preset
    if let Some(value) = arg.strip_prefix("--format=") {
        // Custom format string → passthrough
        return !COMPRESS_PRESETS.contains(&value);
    }

    // --pretty=<value> or --pretty= — same logic
    if let Some(value) = arg.strip_prefix("--pretty=") {
        if SKIP_PRESETS.contains(&value) {
            return true;
        }
        if COMPRESS_PRESETS.contains(&value) {
            return false;
        }
        // Unknown value that isn't a recognized preset → custom format → passthrough
        return true;
    }

    false
}

impl Compressor for GitLogCompressor {
    /// Returns true when first arg is exactly "log" and no skip flag is present.
    fn can_compress(&self, args: &[String]) -> bool {
        if args.first().map(|s| s.as_str()) != Some("log") {
            return false;
        }

        let tail = &args[1..];

        for arg in tail {
            if SKIP_FLAGS.contains(&arg.as_str()) {
                return false;
            }
            if should_skip_format_arg(arg) {
                return false;
            }
        }

        true
    }

    fn normalized_args(&self, original_args: &[String]) -> Vec<String> {
        let tail = &original_args[1..];

        let has_patch = tail
            .iter()
            .any(|a| a == "-p" || a == "--patch" || a == "-u");
        let has_count = user_specified_count(original_args);

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

        // request one extra so compress can detect genuine truncation vs exactly-at-cap
        if !has_count {
            result.push("-n".to_string());
            result.push((DEFAULT_LIMIT + 1).to_string());
        }

        for arg in tail {
            if arg == "-p" || arg == "--patch" || arg == "-u" {
                continue;
            }
            if arg.starts_with("--format=") || arg.starts_with("--pretty=") {
                continue;
            }
            if arg == "--color" || arg.starts_with("--color=") {
                continue;
            }
            result.push(arg.clone());
        }

        result
    }

    fn compress(&self, stdout: &str, _stderr: &str, exit_code: i32) -> Option<String> {
        if exit_code != 0 {
            return None;
        }

        if stdout.trim().is_empty() {
            return Some("(empty)\n".to_string());
        }

        let has_patch = output_has_patch(stdout);
        let has_stat = stdout.contains(" | ")
            && (stdout.contains("file changed") || stdout.contains("files changed"));

        let entries = parse_log(stdout, has_patch, has_stat)?;

        if entries.is_empty() {
            return Some("(empty)\n".to_string());
        }

        // truncation: normalized_args requests exactly DEFAULT_LIMIT+1, so seeing the
        // sentinel count proves more commits exist beyond our injected cap. A user-supplied
        // -n returns a different count and is left untouched (never over-claim or drop).
        let truncated = !self.user_limit && entries.len() == DEFAULT_LIMIT + 1;
        let shown = if truncated {
            &entries[..DEFAULT_LIMIT]
        } else {
            &entries[..]
        };

        let mut output = format_log(shown);

        if truncated {
            output.push_str(&format!(
                "(showing {} commits, use -n to see more)\n",
                DEFAULT_LIMIT
            ));
        }

        Some(output)
    }
}

/// Detect whether the output contains a real patch section.
///
/// We cannot see args from `compress`, so we sniff structurally: a genuine diff
/// begins with `diff --git ` followed by a canonical diff-header line (`index `,
/// `--- `, mode/rename markers, `Binary files `). A commit body that merely mentions
/// "diff --git " is not followed by those, so it is NOT treated as a patch.
fn output_has_patch(stdout: &str) -> bool {
    let lines: Vec<&str> = stdout.lines().collect();
    for (i, line) in lines.iter().enumerate() {
        if !line.starts_with("diff --git ") {
            continue;
        }
        // next non-blank line must look like a diff header
        if let Some(next) = lines[i + 1..].iter().find(|l| !l.trim().is_empty())
            && is_diff_header_line(next)
        {
            return true;
        }
    }
    false
}

/// Canonical lines that immediately follow a real `diff --git ` line.
fn is_diff_header_line(line: &str) -> bool {
    line.starts_with("index ")
        || line.starts_with("--- ")
        || line.starts_with("old mode ")
        || line.starts_with("new file mode ")
        || line.starts_with("deleted file mode ")
        || line.starts_with("similarity index ")
        || line.starts_with("rename from ")
        || line.starts_with("Binary files ")
}

// ---------------------------------------------------------------------------
// Data model
// ---------------------------------------------------------------------------

struct LogEntry {
    fields: CommitFields,
    diff: Option<Vec<diff_parser::DiffFile>>,
    stat: Option<String>,
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

/// Parse raw git log output (produced by our format string) into a list of entries.
///
/// Returns `None` only on unrecoverable parse failure. An empty vec is valid.
fn parse_log(raw: &str, has_patch: bool, has_stat: bool) -> Option<Vec<LogEntry>> {
    // Commits are delimited by \x01. The format string starts each commit with \x01,
    // so splitting produces a leading empty element — skip it.
    let chunks: Vec<&str> = raw.split('\x01').collect();

    let mut entries = Vec::new();
    for chunk in chunks {
        if chunk.trim().is_empty() {
            continue;
        }
        let entry = parse_log_entry(chunk, has_patch, has_stat)?;
        entries.push(entry);
    }

    Some(entries)
}

/// Find the byte offset of the `\n` preceding the first genuine `diff --git ` header
/// inside a chunk, so `chunk[idx + 1..]` begins at the diff. A body line mentioning
/// "diff --git " is skipped because it is not followed by a diff-header line.
fn find_diff_start(chunk: &str) -> Option<usize> {
    let mut search_from = 0;
    while let Some(rel) = chunk[search_from..].find("\ndiff --git ") {
        let nl_idx = search_from + rel;
        let after = &chunk[nl_idx + 1..];
        // next non-blank line after the "diff --git" line must be a diff header
        let next_line = after.lines().nth(1);
        if next_line.map(is_diff_header_line).unwrap_or(false) {
            return Some(nl_idx);
        }
        search_from = nl_idx + 1;
    }
    None
}

/// Parse a single commit chunk (everything after the leading \x01).
fn parse_log_entry(chunk: &str, has_patch: bool, has_stat: bool) -> Option<LogEntry> {
    // Split off diff section at the first genuine "diff --git " header
    let (meta_and_stat, diff) = if has_patch {
        match find_diff_start(chunk) {
            Some(idx) => {
                let diff_text = &chunk[idx + 1..]; // keep the leading \n stripped
                let parsed = diff_parser::parse_diff(diff_text);
                (
                    &chunk[..idx],
                    if parsed.is_empty() {
                        None
                    } else {
                        Some(parsed)
                    },
                )
            }
            None => (chunk, None),
        }
    } else {
        (chunk, None)
    };

    // Split off stat section
    let (format_part, stat) = if has_stat {
        split_stat(meta_and_stat)
    } else {
        (meta_and_stat, None)
    };

    let fields = commit_parser::parse_commit_fields(format_part)?;
    let stat_compressed = stat.map(diff_parser::compress_stat);

    Some(LogEntry {
        fields,
        diff,
        stat: stat_compressed,
    })
}

/// Split a `--stat` block from the end of the format+stat region.
///
/// A git stat block is a CONTIGUOUS TRAILING block: zero or more
/// `<path> | <count> <bar>` lines followed by exactly one summary line
/// (`N file(s) changed, ...`). We detect it by anchoring on the trailing summary
/// line, then walking backwards over stat-shaped lines. This keeps body lines that
/// merely contain ` | ` (e.g. markdown tables) inside the body.
/// Returns `(format_portion, Some(stat_text))` or `(full_text, None)`.
fn split_stat(text: &str) -> (&str, Option<&str>) {
    // stat appears after the body field; only inspect content after the last \x00
    let last_nul = match text.rfind('\x00') {
        Some(idx) => idx,
        None => return (text, None),
    };
    let body_start = last_nul + 1;
    let after_nul = &text[body_start..];

    let lines: Vec<&str> = after_nul.lines().collect();

    // last non-blank line must be a git stat summary, else there is no stat block
    let summary_idx = match lines.iter().rposition(|l| !l.trim().is_empty()) {
        Some(idx) if is_stat_summary_line(lines[idx]) => idx,
        _ => return (text, None),
    };

    // walk backwards over contiguous stat-shaped lines preceding the summary
    let mut block_start = summary_idx;
    while block_start > 0 && is_stat_file_line(lines[block_start - 1]) {
        block_start -= 1;
    }

    // byte offset where the stat block begins within `text`
    let mut stat_text_start = body_start;
    for line in &lines[..block_start] {
        stat_text_start += line.len() + 1; // +1 for newline
    }

    let format_part = &text[..stat_text_start];
    let stat_part = &text[stat_text_start..];

    (format_part, Some(stat_part))
}

/// A git stat summary line: `N file(s) changed, ...` (leading whitespace allowed).
fn is_stat_summary_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with(|c: char| c.is_ascii_digit())
        && (line.contains("file changed") || line.contains("files changed"))
}

/// A git stat file line: `<path> | <count><bar>` or `<path> | Bin ...`.
/// The segment after ` | ` must start with a digit or `Bin` (binary marker).
fn is_stat_file_line(line: &str) -> bool {
    let Some(pipe_idx) = line.find(" | ") else {
        return false;
    };
    let after = line[pipe_idx + 3..].trim_start();
    after.starts_with(|c: char| c.is_ascii_digit()) || after.starts_with("Bin")
}

// ---------------------------------------------------------------------------
// Formatting
// ---------------------------------------------------------------------------

/// Format a list of log entries into compressed output.
fn format_log(entries: &[LogEntry]) -> String {
    if entries.is_empty() {
        return "(empty)\n".to_string();
    }

    let mut output = String::new();

    for entry in entries {
        output.push_str(&commit_parser::format_commit_oneline(&entry.fields));

        // Body (indented 2 spaces)
        if let Some(ref body) = entry.fields.body {
            output.push_str(&commit_parser::format_commit_body(body));
        }

        // Stat (indented 2 spaces)
        if let Some(ref stat) = entry.stat {
            for line in stat.lines() {
                output.push_str(&format!("  {}\n", line));
            }
        }

        // Diff (blank line separator, not indented)
        if let Some(ref files) = entry.diff {
            output.push('\n');
            for file in files {
                output.push_str(&diff_parser::format_file(file));
            }
        }
    }

    output
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::super::commit_parser::CommitFields;
    use super::*;

    fn args(strs: &[&str]) -> Vec<String> {
        strs.iter().map(|s| s.to_string()).collect()
    }

    // --- Task 4: can_compress ---

    #[test]
    fn can_compress_bare_log() {
        assert!(GitLogCompressor::default().can_compress(&args(&["log"])));
    }

    #[test]
    fn can_compress_with_n() {
        assert!(GitLogCompressor::default().can_compress(&args(&["log", "-n", "5"])));
    }

    #[test]
    fn can_compress_with_author() {
        assert!(GitLogCompressor::default().can_compress(&args(&["log", "--author=Alice"])));
    }

    #[test]
    fn can_compress_with_patch() {
        assert!(GitLogCompressor::default().can_compress(&args(&["log", "-p"])));
    }

    #[test]
    fn can_compress_with_stat() {
        assert!(GitLogCompressor::default().can_compress(&args(&["log", "--stat"])));
    }

    #[test]
    fn can_compress_with_since() {
        assert!(GitLogCompressor::default().can_compress(&args(&["log", "--since=2024-01-01"])));
    }

    #[test]
    fn can_compress_pretty_medium() {
        assert!(GitLogCompressor::default().can_compress(&args(&["log", "--pretty=medium"])));
    }

    #[test]
    fn can_compress_pretty_full() {
        assert!(GitLogCompressor::default().can_compress(&args(&["log", "--pretty=full"])));
    }

    #[test]
    fn skip_oneline() {
        assert!(!GitLogCompressor::default().can_compress(&args(&["log", "--oneline"])));
    }

    #[test]
    fn skip_graph() {
        assert!(!GitLogCompressor::default().can_compress(&args(&["log", "--graph"])));
    }

    #[test]
    fn skip_color() {
        assert!(!GitLogCompressor::default().can_compress(&args(&["log", "--color"])));
    }

    #[test]
    fn skip_color_always() {
        assert!(!GitLogCompressor::default().can_compress(&args(&["log", "--color=always"])));
    }

    #[test]
    fn skip_format_custom() {
        assert!(!GitLogCompressor::default().can_compress(&args(&["log", "--format=%H %s"])));
    }

    #[test]
    fn skip_pretty_custom() {
        assert!(!GitLogCompressor::default().can_compress(&args(&["log", "--pretty=%H %an"])));
    }

    #[test]
    fn skip_pretty_oneline() {
        assert!(!GitLogCompressor::default().can_compress(&args(&["log", "--pretty=oneline"])));
    }

    #[test]
    fn skip_pretty_reference() {
        assert!(!GitLogCompressor::default().can_compress(&args(&["log", "--pretty=reference"])));
    }

    #[test]
    fn skip_pretty_email() {
        assert!(!GitLogCompressor::default().can_compress(&args(&["log", "--pretty=email"])));
    }

    #[test]
    fn skip_pretty_raw() {
        assert!(!GitLogCompressor::default().can_compress(&args(&["log", "--pretty=raw"])));
    }

    #[test]
    fn skip_pretty_mboxrd() {
        assert!(!GitLogCompressor::default().can_compress(&args(&["log", "--pretty=mboxrd"])));
    }

    #[test]
    fn non_log_status() {
        assert!(!GitLogCompressor::default().can_compress(&args(&["status"])));
    }

    #[test]
    fn non_log_diff() {
        assert!(!GitLogCompressor::default().can_compress(&args(&["diff"])));
    }

    #[test]
    fn non_log_log_tree() {
        // "log-tree" is not "log"
        assert!(!GitLogCompressor::default().can_compress(&args(&["log-tree"])));
    }

    #[test]
    fn non_log_empty_args() {
        assert!(!GitLogCompressor::default().can_compress(&args(&[])));
    }

    // --- Task 5: normalized_args ---

    #[test]
    fn bare_log_contains_required_flags() {
        let result = GitLogCompressor::default().normalized_args(&args(&["log"]));
        assert_eq!(result[0], "log");
        assert!(result.iter().any(|a| a.starts_with("--format=")));
        assert!(result.contains(&"--no-color".to_string()));
        assert!(result.contains(&"-n".to_string()));
        // cap is DEFAULT_LIMIT+1 so compress can detect genuine truncation
        assert!(result.contains(&(DEFAULT_LIMIT + 1).to_string()));
    }

    #[test]
    fn injects_default_cap() {
        let result = GitLogCompressor::default().normalized_args(&args(&["log"]));
        let n_idx = result.iter().position(|a| a == "-n").unwrap();
        assert_eq!(result[n_idx + 1], (DEFAULT_LIMIT + 1).to_string());
    }

    #[test]
    fn preserves_user_n() {
        let result = GitLogCompressor::default().normalized_args(&args(&["log", "-n", "5"]));
        let n_count = result.iter().filter(|a| a.as_str() == "-n").count();
        assert_eq!(n_count, 1, "Should have exactly one -n");
        let n_idx = result.iter().position(|a| a == "-n").unwrap();
        assert_eq!(result[n_idx + 1], "5");
    }

    #[test]
    fn preserves_max_count() {
        let result = GitLogCompressor::default().normalized_args(&args(&["log", "--max-count=10"]));
        assert!(result.contains(&"--max-count=10".to_string()));
        assert!(!result.contains(&"-n".to_string()));
    }

    #[test]
    fn with_patch_adds_diff_flags() {
        let result = GitLogCompressor::default().normalized_args(&args(&["log", "-p"]));
        assert!(result.contains(&"-p".to_string()));
        assert!(result.contains(&"--unified=1".to_string()));
        assert!(result.contains(&"--no-ext-diff".to_string()));
        assert!(result.contains(&"--diff-algorithm=histogram".to_string()));
    }

    #[test]
    fn patch_alias_adds_diff_flags() {
        let result = GitLogCompressor::default().normalized_args(&args(&["log", "--patch"]));
        assert!(result.contains(&"-p".to_string()));
        assert!(result.contains(&"--unified=1".to_string()));
    }

    #[test]
    fn preserves_filters() {
        let result = GitLogCompressor::default().normalized_args(&args(&[
            "log",
            "--author=Alice",
            "--since=2024-01-01",
        ]));
        assert!(result.contains(&"--author=Alice".to_string()));
        assert!(result.contains(&"--since=2024-01-01".to_string()));
    }

    #[test]
    fn preserves_stat() {
        let result = GitLogCompressor::default().normalized_args(&args(&["log", "--stat"]));
        assert!(result.contains(&"--stat".to_string()));
    }

    #[test]
    fn preserves_range() {
        let result = GitLogCompressor::default().normalized_args(&args(&["log", "HEAD~5..HEAD"]));
        assert!(result.contains(&"HEAD~5..HEAD".to_string()));
    }

    #[test]
    fn strips_pretty_preset() {
        let result =
            GitLogCompressor::default().normalized_args(&args(&["log", "--pretty=medium"]));
        assert!(!result.iter().any(|a| a.starts_with("--pretty=")));
    }

    #[test]
    fn numeric_shorthand_count() {
        let result = GitLogCompressor::default().normalized_args(&args(&["log", "-5"]));
        assert!(result.contains(&"-5".to_string()));
        // user count present → no default cap injected
        assert!(!result.contains(&"-n".to_string()));
        assert!(!result.contains(&(DEFAULT_LIMIT + 1).to_string()));
    }

    // --- Task 6: parsing ---

    fn make_chunk(
        hash: &str,
        decs: &str,
        date: &str,
        author: &str,
        subject: &str,
        body: &str,
    ) -> String {
        format!(
            "{}\x00{}\x00{}\x00{}\x00{}\x00{}",
            hash, decs, date, author, subject, body
        )
    }

    #[test]
    fn parse_single_commit() {
        let raw = format!(
            "\x01{}",
            make_chunk(
                "a1b2c3f",
                "HEAD -> main",
                "2024-01-15T10:00:00+00:00",
                "Alice",
                "Add feature",
                ""
            )
        );
        let entries = parse_log(&raw, false, false).unwrap();
        assert_eq!(entries.len(), 1);
        let e = &entries[0].fields;
        assert_eq!(e.hash, "a1b2c3f");
        assert_eq!(e.decorations, vec!["HEAD -> main"]);
        assert_eq!(e.date, "2024-01-15");
        assert_eq!(e.author, "Alice");
        assert_eq!(e.subject, "Add feature");
        assert!(e.body.is_none());
    }

    #[test]
    fn parse_multiple_commits() {
        let c1 = make_chunk(
            "aaa1111",
            "",
            "2024-01-15T10:00:00+00:00",
            "Alice",
            "First commit",
            "",
        );
        let c2 = make_chunk(
            "bbb2222",
            "",
            "2024-01-14T10:00:00+00:00",
            "Bob",
            "Second commit",
            "",
        );
        let raw = format!("\x01{}\x01{}", c1, c2);
        let entries = parse_log(&raw, false, false).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].fields.subject, "First commit");
        assert_eq!(entries[1].fields.subject, "Second commit");
        assert!(entries[0].fields.decorations.is_empty());
    }

    #[test]
    fn parse_commit_with_body() {
        let body = "This is the body.\nWith multiple lines.";
        let raw = format!(
            "\x01{}",
            make_chunk(
                "a1b2c3f",
                "",
                "2024-01-15T10:00:00+00:00",
                "Alice",
                "Subject",
                body
            )
        );
        let entries = parse_log(&raw, false, false).unwrap();
        assert_eq!(entries[0].fields.body, Some(body.to_string()));
    }

    #[test]
    fn parse_commit_empty_body_trimmed() {
        let raw = format!(
            "\x01{}",
            make_chunk(
                "a1b2c3f",
                "",
                "2024-01-15T10:00:00+00:00",
                "Alice",
                "Subject",
                "   \n  "
            )
        );
        let entries = parse_log(&raw, false, false).unwrap();
        assert!(entries[0].fields.body.is_none());
    }

    #[test]
    fn parse_commit_multiple_decorations() {
        let raw = format!(
            "\x01{}",
            make_chunk(
                "a1b2c3f",
                "HEAD -> main, origin/main, tag: v1.0",
                "2024-01-15T10:00:00+00:00",
                "Alice",
                "Subject",
                ""
            )
        );
        let entries = parse_log(&raw, false, false).unwrap();
        assert_eq!(
            entries[0].fields.decorations,
            vec!["HEAD -> main", "origin/main", "tag: v1.0"]
        );
    }

    #[test]
    fn parse_empty_input() {
        let entries = parse_log("", false, false).unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn parse_malformed_returns_none() {
        // Only 3 fields (need 6)
        let raw = "\x01hash\x00decs\x00date";
        assert!(parse_log(raw, false, false).is_none());
    }

    // --- Task 7: compress_stat (moved to diff_parser, tested there) ---

    #[test]
    fn compress_stat_integration() {
        let raw = " src/a.rs | 3 +++\n src/b.rs | 2 --\n 2 files changed, 3 insertions(+), 2 deletions(-)\n";
        let result = diff_parser::compress_stat(raw);
        assert!(result.contains("src/a.rs | 3+"));
        assert!(result.contains("src/b.rs | 2-"));
        assert!(result.contains("2 files changed"));
    }

    // --- Task 8: format_log and compress ---

    #[test]
    fn format_standard_commit() {
        let entry = LogEntry {
            fields: CommitFields {
                hash: "a1b2c3f".to_string(),
                decorations: vec!["HEAD -> main".to_string()],
                date: "2024-01-15".to_string(),
                author: "John Smith".to_string(),
                subject: "Add auth".to_string(),
                body: None,
            },
            diff: None,
            stat: None,
        };
        let result = format_log(&[entry]);
        assert_eq!(
            result,
            "* a1b2c3f (HEAD -> main) 2024-01-15 [John Smith] Add auth\n"
        );
    }

    #[test]
    fn format_commit_no_decorations() {
        let entry = LogEntry {
            fields: CommitFields {
                hash: "a1b2c3f".to_string(),
                decorations: vec![],
                date: "2024-01-15".to_string(),
                author: "John Smith".to_string(),
                subject: "Fix bug".to_string(),
                body: None,
            },
            diff: None,
            stat: None,
        };
        let result = format_log(&[entry]);
        assert!(!result.contains('('));
        assert_eq!(result, "* a1b2c3f 2024-01-15 [John Smith] Fix bug\n");
    }

    #[test]
    fn format_commit_with_body() {
        let entry = LogEntry {
            fields: CommitFields {
                hash: "a1b2c3f".to_string(),
                decorations: vec![],
                date: "2024-01-15".to_string(),
                author: "Alice".to_string(),
                subject: "Update docs".to_string(),
                body: Some("Added README.\nFixed typos.".to_string()),
            },
            diff: None,
            stat: None,
        };
        let result = format_log(&[entry]);
        assert!(result.contains("  Added README.\n"));
        assert!(result.contains("  Fixed typos.\n"));
    }

    #[test]
    fn format_empty_log() {
        let result = format_log(&[]);
        assert_eq!(result, "(empty)\n");
    }

    #[test]
    fn format_multiple_commits() {
        let e1 = LogEntry {
            fields: CommitFields {
                hash: "aaa1111".to_string(),
                decorations: vec![],
                date: "2024-01-15".to_string(),
                author: "Alice".to_string(),
                subject: "First".to_string(),
                body: None,
            },
            diff: None,
            stat: None,
        };
        let e2 = LogEntry {
            fields: CommitFields {
                hash: "bbb2222".to_string(),
                decorations: vec![],
                date: "2024-01-14".to_string(),
                author: "Bob".to_string(),
                subject: "Second".to_string(),
                body: None,
            },
            diff: None,
            stat: None,
        };
        let result = format_log(&[e1, e2]);
        assert!(result.contains("* aaa1111"));
        assert!(result.contains("* bbb2222"));
    }

    #[test]
    fn compress_nonzero_exit_returns_none() {
        assert_eq!(
            GitLogCompressor::default().compress("anything", "fatal: error", 128),
            None
        );
    }

    #[test]
    fn compress_empty_output() {
        assert_eq!(
            GitLogCompressor::default().compress("", "", 0),
            Some("(empty)\n".to_string())
        );
    }

    #[test]
    fn compress_whitespace_only_output() {
        assert_eq!(
            GitLogCompressor::default().compress("  \n\n  ", "", 0),
            Some("(empty)\n".to_string())
        );
    }

    fn n_commits(n: usize) -> String {
        (0..n)
            .map(|i| {
                format!(
                    "\x01{:07x}\x00\x002024-01-{:02}T10:00:00+00:00\x00Author{}\x00Subject {}\x00",
                    i,
                    (i % 28) + 1,
                    i,
                    i
                )
            })
            .collect()
    }

    // --- Task 3: truncation notice only when genuinely truncated ---

    #[test]
    fn compress_truncation_notice_when_over_limit() {
        // 21 entries (limit+1) → genuinely more than shown
        let chunks = n_commits(DEFAULT_LIMIT + 1);
        let result = GitLogCompressor::default()
            .compress(&chunks, "", 0)
            .unwrap();
        assert!(
            result.contains("(showing 20 commits, use -n to see more)"),
            "Expected truncation notice in:\n{}",
            result
        );
        // only DEFAULT_LIMIT commits rendered, the extra is dropped
        let shown = result.matches("* ").count();
        assert_eq!(shown, DEFAULT_LIMIT, "should render exactly 20 commits");
    }

    #[test]
    fn compress_no_truncation_notice_at_exactly_limit() {
        // exactly 20 entries → NOT truncated, must not over-claim
        let chunks = n_commits(DEFAULT_LIMIT);
        let result = GitLogCompressor::default()
            .compress(&chunks, "", 0)
            .unwrap();
        assert!(
            !result.contains("showing 20 commits"),
            "exactly 20 commits must not trigger truncation notice:\n{}",
            result
        );
    }

    #[test]
    fn compress_user_count_above_limit_not_truncated() {
        // user asked for more (count != sentinel) → render all, never drop or over-claim
        let chunks = n_commits(25);
        let result = GitLogCompressor::default()
            .compress(&chunks, "", 0)
            .unwrap();
        assert!(
            !result.contains("showing 20 commits"),
            "user-requested 25 commits must not be flagged as truncated:\n{}",
            result
        );
        assert_eq!(
            result.matches("* ").count(),
            25,
            "all 25 commits must render"
        );
    }

    #[test]
    fn compress_user_count_exactly_at_sentinel_not_truncated() {
        // user asked for exactly DEFAULT_LIMIT+1 (e.g. `git log -n 21`): we inject
        // no cap, so 21 entries means the user got what they asked for — render all,
        // never drop the 21st or claim truncation. user_limit must defeat the sentinel.
        let chunks = n_commits(DEFAULT_LIMIT + 1);
        let result = GitLogCompressor { user_limit: true }
            .compress(&chunks, "", 0)
            .unwrap();
        assert!(
            !result.contains("showing 20 commits"),
            "user-requested 21 commits must not be flagged as truncated:\n{}",
            result
        );
        assert_eq!(
            result.matches("* ").count(),
            DEFAULT_LIMIT + 1,
            "all 21 commits must render"
        );
    }

    #[test]
    fn user_specified_count_detects_forms() {
        assert!(user_specified_count(&args(&["log", "-n", "21"])));
        assert!(user_specified_count(&args(&["log", "--max-count=21"])));
        assert!(user_specified_count(&args(&["log", "-21"])));
        assert!(!user_specified_count(&args(&["log"])));
        assert!(!user_specified_count(&args(&["log", "--stat"])));
        // bare "-" is not a count
        assert!(!user_specified_count(&args(&["log", "-"])));
    }

    #[test]
    fn compress_no_truncation_notice_under_20() {
        let chunks = n_commits(5);
        let result = GitLogCompressor::default()
            .compress(&chunks, "", 0)
            .unwrap();
        assert!(
            !result.contains("showing 20 commits"),
            "Should not have truncation notice for 5 commits"
        );
    }

    // --- Task 1: output-shape flags decline (passthrough) ---

    #[test]
    fn skip_name_only() {
        assert!(!GitLogCompressor::default().can_compress(&args(&["log", "--name-only"])));
    }

    #[test]
    fn skip_name_status() {
        assert!(!GitLogCompressor::default().can_compress(&args(&["log", "--name-status"])));
    }

    #[test]
    fn skip_numstat() {
        assert!(!GitLogCompressor::default().can_compress(&args(&["log", "--numstat"])));
    }

    #[test]
    fn skip_shortstat() {
        assert!(!GitLogCompressor::default().can_compress(&args(&["log", "--shortstat"])));
    }

    #[test]
    fn skip_raw() {
        assert!(!GitLogCompressor::default().can_compress(&args(&["log", "--raw"])));
    }

    // --- Task 2: has_patch derived from real diff structure, not body content ---

    #[test]
    fn body_mentioning_diff_git_is_not_patch() {
        // body contains a literal "diff --git " line but no real diff follows
        let raw = "\x01a1b2c3f\x00\x002024-01-15T10:00:00+00:00\x00Alice\x00Subject\x00Discussed the diff --git format in review.\nNext line of body.".to_string();
        assert!(
            !output_has_patch(&raw),
            "body mention of 'diff --git' must not be treated as patch"
        );
        // body must survive intact, not be truncated/rendered as diff
        let result = GitLogCompressor::default().compress(&raw, "", 0).unwrap();
        assert!(result.contains("diff --git format in review"));
        assert!(result.contains("Next line of body"));
    }

    #[test]
    fn real_patch_is_detected() {
        let raw = "\x01a1b2c3f\x00\x002024-01-15T10:00:00+00:00\x00Alice\x00Subject\x00\ndiff --git a/x.rs b/x.rs\nindex abc..def 100644\n--- a/x.rs\n+++ b/x.rs\n@@ -1 +1 @@\n-old\n+new\n".to_string();
        assert!(
            output_has_patch(&raw),
            "a genuine diff followed by 'index ' must be detected as patch"
        );
    }

    // --- Task 4: split_stat anchors on trailing summary, not stray " | " ---

    #[test]
    fn body_with_markdown_table_kept_in_body() {
        // body has a markdown table (lines with " | ") but NO trailing stat summary
        let body = "See table:\n| col a | col b |\n| ----- | ----- |\n| 1 | 2 |";
        let raw = format!(
            "\x01a1b2c3f\x00\x002024-01-15T10:00:00+00:00\x00Alice\x00Subject\x00{}",
            body
        );
        // no stat summary present → split_stat must return None
        let (format_part, stat) = split_stat(&raw);
        assert!(stat.is_none(), "markdown table must not be parsed as stat");
        assert_eq!(format_part, raw);
    }

    #[test]
    fn trailing_stat_block_split_off() {
        // genuine --stat: file line + summary at the very end
        let raw = "a1b2c3f\x00\x002024-01-15T10:00:00+00:00\x00Alice\x00Subject\x00 src/a.rs | 3 +++\n 1 file changed, 3 insertions(+)\n".to_string();
        let (format_part, stat) = split_stat(&raw);
        let stat = stat.expect("trailing stat block must be detected");
        assert!(stat.contains("src/a.rs | 3 +++"));
        assert!(stat.contains("1 file changed"));
        assert!(!format_part.contains("src/a.rs"));
    }

    #[test]
    fn body_table_before_real_stat_not_mangled() {
        // body has a markdown table, then a genuine stat block trails it
        let raw = "a1b2c3f\x00\x002024-01-15T10:00:00+00:00\x00Alice\x00Subject\x00See:\n| a | b |\n src/a.rs | 2 +-\n 1 file changed, 1 insertion(+), 1 deletion(-)\n".to_string();
        let (format_part, stat) = split_stat(&raw);
        let stat = stat.expect("trailing stat block must be detected");
        // table line stays in body, only the contiguous trailing stat is split
        assert!(format_part.contains("| a | b |"));
        assert!(stat.contains("src/a.rs | 2 +-"));
        assert!(stat.contains("1 file changed"));
        assert!(!stat.contains("| a | b |"));
    }
}
