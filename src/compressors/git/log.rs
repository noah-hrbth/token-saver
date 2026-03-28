use super::commit_parser::{self, CommitFields};
use super::diff_parser;
use crate::compressors::Compressor;

pub struct GitLogCompressor;

/// Flags that cause passthrough — agent chose a specific output format or display mode.
const SKIP_FLAGS: &[&str] = &["--oneline", "--graph", "--color", "--color=always"];

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

        let has_patch = stdout.contains("\ndiff --git ");
        let has_stat = stdout.contains(" | ")
            && (stdout.contains("file changed") || stdout.contains("files changed"));

        let entries = parse_log(stdout, has_patch, has_stat)?;

        if entries.is_empty() {
            return Some("(empty)\n".to_string());
        }

        let mut output = format_log(&entries);

        if entries.len() == 20 {
            output.push_str("(showing 20 commits, use -n to see more)\n");
        }

        Some(output)
    }
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
        if let Some(entry) = parse_log_entry(chunk, has_patch, has_stat) {
            entries.push(entry);
        } else {
            // Malformed chunk → give up on the whole output
            return None;
        }
    }

    Some(entries)
}

/// Parse a single commit chunk (everything after the leading \x01).
fn parse_log_entry(chunk: &str, has_patch: bool, has_stat: bool) -> Option<LogEntry> {
    // Split off diff section (starts at "\ndiff --git ")
    let (meta_and_stat, diff) = if has_patch {
        match chunk.find("\ndiff --git ") {
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
    let stat_compressed = stat.map(compress_stat);

    Some(LogEntry {
        fields,
        diff,
        stat: stat_compressed,
    })
}

/// Split stat lines from the end of the format+stat region.
///
/// The stat block appears after the format fields (after the last \x00 / body field).
/// It starts with lines containing ` | ` and ends with a summary line.
/// Returns `(format_portion, Some(stat_text))` or `(full_text, None)`.
fn split_stat(text: &str) -> (&str, Option<&str>) {
    // The stat section begins after the body field (last \x00-delimited field).
    // Find the last \x00, then look for stat lines after it.
    let last_nul = match text.rfind('\x00') {
        Some(idx) => idx,
        None => return (text, None),
    };

    let after_nul = &text[last_nul + 1..];

    // A stat block has at least one line with " | " followed by a summary.
    if !after_nul.contains(" | ") {
        return (text, None);
    }

    // Find where stat begins: first line containing " | " after the body content
    let body_and_stat = after_nul;
    let stat_start_in_after = body_and_stat
        .lines()
        .enumerate()
        .find(|(_, line)| {
            line.contains(" | ") || line.contains("file changed") || line.contains("files changed")
        })
        .map(|(i, _)| i);

    let Some(stat_line_idx) = stat_start_in_after else {
        return (text, None);
    };

    let lines: Vec<&str> = body_and_stat.lines().collect();
    let stat_lines = &lines[stat_line_idx..];

    if stat_lines.is_empty() {
        return (text, None);
    }

    // Reconstruct: format_part is everything up to where stat begins in after_nul
    let stat_text_start = {
        let mut offset = last_nul + 1;
        for line in &lines[..stat_line_idx] {
            offset += line.len() + 1; // +1 for newline
        }
        offset
    };

    let format_part = &text[..stat_text_start];
    let stat_part = &text[stat_text_start..];

    (format_part, Some(stat_part))
}

/// Compress raw git stat output.
///
/// Replaces `++++----` bar notation with `N+ N-` counts.
/// Summary lines (`N files changed, ...`) pass through unchanged.
fn compress_stat(raw: &str) -> String {
    let mut output = String::new();

    for line in raw.lines() {
        if line.trim().is_empty() {
            continue;
        }

        // Summary line: "N file(s) changed, ..."
        if line.trim_start().starts_with(|c: char| c.is_ascii_digit()) {
            output.push_str(line.trim());
            output.push('\n');
            continue;
        }

        // File stat line: " src/foo.rs | 15 ++++++------"
        if let Some(pipe_idx) = line.find(" | ") {
            let filename = line[..pipe_idx].trim();
            let after_pipe = line[pipe_idx + 3..].trim();

            // after_pipe looks like "15 +++------" or "3 +++" or "5 -----"
            let bar_start = after_pipe.find(['+', '-']);
            let bar = match bar_start {
                Some(idx) => &after_pipe[idx..],
                None => {
                    // No bar (binary or zero changes) — pass through trimmed
                    output.push_str(&format!("{} | {}\n", filename, after_pipe));
                    continue;
                }
            };

            let insertions = bar.chars().filter(|&c| c == '+').count();
            let deletions = bar.chars().filter(|&c| c == '-').count();

            let counts = match (insertions, deletions) {
                (0, 0) => String::new(),
                (ins, 0) => format!("{}+", ins),
                (0, del) => format!("{}-", del),
                (ins, del) => format!("{}+ {}-", ins, del),
            };

            output.push_str(&format!("{} | {}\n", filename, counts));
        } else {
            // Unknown line format — pass through
            output.push_str(line.trim());
            output.push('\n');
        }
    }

    output
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
        assert!(GitLogCompressor.can_compress(&args(&["log"])));
    }

    #[test]
    fn can_compress_with_n() {
        assert!(GitLogCompressor.can_compress(&args(&["log", "-n", "5"])));
    }

    #[test]
    fn can_compress_with_author() {
        assert!(GitLogCompressor.can_compress(&args(&["log", "--author=Alice"])));
    }

    #[test]
    fn can_compress_with_patch() {
        assert!(GitLogCompressor.can_compress(&args(&["log", "-p"])));
    }

    #[test]
    fn can_compress_with_stat() {
        assert!(GitLogCompressor.can_compress(&args(&["log", "--stat"])));
    }

    #[test]
    fn can_compress_with_since() {
        assert!(GitLogCompressor.can_compress(&args(&["log", "--since=2024-01-01"])));
    }

    #[test]
    fn can_compress_pretty_medium() {
        assert!(GitLogCompressor.can_compress(&args(&["log", "--pretty=medium"])));
    }

    #[test]
    fn can_compress_pretty_full() {
        assert!(GitLogCompressor.can_compress(&args(&["log", "--pretty=full"])));
    }

    #[test]
    fn skip_oneline() {
        assert!(!GitLogCompressor.can_compress(&args(&["log", "--oneline"])));
    }

    #[test]
    fn skip_graph() {
        assert!(!GitLogCompressor.can_compress(&args(&["log", "--graph"])));
    }

    #[test]
    fn skip_color() {
        assert!(!GitLogCompressor.can_compress(&args(&["log", "--color"])));
    }

    #[test]
    fn skip_color_always() {
        assert!(!GitLogCompressor.can_compress(&args(&["log", "--color=always"])));
    }

    #[test]
    fn skip_format_custom() {
        assert!(!GitLogCompressor.can_compress(&args(&["log", "--format=%H %s"])));
    }

    #[test]
    fn skip_pretty_custom() {
        assert!(!GitLogCompressor.can_compress(&args(&["log", "--pretty=%H %an"])));
    }

    #[test]
    fn skip_pretty_oneline() {
        assert!(!GitLogCompressor.can_compress(&args(&["log", "--pretty=oneline"])));
    }

    #[test]
    fn skip_pretty_reference() {
        assert!(!GitLogCompressor.can_compress(&args(&["log", "--pretty=reference"])));
    }

    #[test]
    fn skip_pretty_email() {
        assert!(!GitLogCompressor.can_compress(&args(&["log", "--pretty=email"])));
    }

    #[test]
    fn skip_pretty_raw() {
        assert!(!GitLogCompressor.can_compress(&args(&["log", "--pretty=raw"])));
    }

    #[test]
    fn skip_pretty_mboxrd() {
        assert!(!GitLogCompressor.can_compress(&args(&["log", "--pretty=mboxrd"])));
    }

    #[test]
    fn non_log_status() {
        assert!(!GitLogCompressor.can_compress(&args(&["status"])));
    }

    #[test]
    fn non_log_diff() {
        assert!(!GitLogCompressor.can_compress(&args(&["diff"])));
    }

    #[test]
    fn non_log_log_tree() {
        // "log-tree" is not "log"
        assert!(!GitLogCompressor.can_compress(&args(&["log-tree"])));
    }

    #[test]
    fn non_log_empty_args() {
        assert!(!GitLogCompressor.can_compress(&args(&[])));
    }

    // --- Task 5: normalized_args ---

    #[test]
    fn bare_log_contains_required_flags() {
        let result = GitLogCompressor.normalized_args(&args(&["log"]));
        assert_eq!(result[0], "log");
        assert!(result.iter().any(|a| a.starts_with("--format=")));
        assert!(result.contains(&"--no-color".to_string()));
        assert!(result.contains(&"-n".to_string()));
        assert!(result.contains(&"20".to_string()));
    }

    #[test]
    fn injects_default_cap() {
        let result = GitLogCompressor.normalized_args(&args(&["log"]));
        let n_idx = result.iter().position(|a| a == "-n").unwrap();
        assert_eq!(result[n_idx + 1], "20");
    }

    #[test]
    fn preserves_user_n() {
        let result = GitLogCompressor.normalized_args(&args(&["log", "-n", "5"]));
        let n_count = result.iter().filter(|a| a.as_str() == "-n").count();
        assert_eq!(n_count, 1, "Should have exactly one -n");
        let n_idx = result.iter().position(|a| a == "-n").unwrap();
        assert_eq!(result[n_idx + 1], "5");
    }

    #[test]
    fn preserves_max_count() {
        let result = GitLogCompressor.normalized_args(&args(&["log", "--max-count=10"]));
        assert!(result.contains(&"--max-count=10".to_string()));
        assert!(!result.contains(&"-n".to_string()));
    }

    #[test]
    fn with_patch_adds_diff_flags() {
        let result = GitLogCompressor.normalized_args(&args(&["log", "-p"]));
        assert!(result.contains(&"-p".to_string()));
        assert!(result.contains(&"--unified=1".to_string()));
        assert!(result.contains(&"--no-ext-diff".to_string()));
        assert!(result.contains(&"--diff-algorithm=histogram".to_string()));
    }

    #[test]
    fn patch_alias_adds_diff_flags() {
        let result = GitLogCompressor.normalized_args(&args(&["log", "--patch"]));
        assert!(result.contains(&"-p".to_string()));
        assert!(result.contains(&"--unified=1".to_string()));
    }

    #[test]
    fn preserves_filters() {
        let result = GitLogCompressor.normalized_args(&args(&[
            "log",
            "--author=Alice",
            "--since=2024-01-01",
        ]));
        assert!(result.contains(&"--author=Alice".to_string()));
        assert!(result.contains(&"--since=2024-01-01".to_string()));
    }

    #[test]
    fn preserves_stat() {
        let result = GitLogCompressor.normalized_args(&args(&["log", "--stat"]));
        assert!(result.contains(&"--stat".to_string()));
    }

    #[test]
    fn preserves_range() {
        let result = GitLogCompressor.normalized_args(&args(&["log", "HEAD~5..HEAD"]));
        assert!(result.contains(&"HEAD~5..HEAD".to_string()));
    }

    #[test]
    fn strips_pretty_preset() {
        let result = GitLogCompressor.normalized_args(&args(&["log", "--pretty=medium"]));
        assert!(!result.iter().any(|a| a.starts_with("--pretty=")));
    }

    #[test]
    fn numeric_shorthand_count() {
        let result = GitLogCompressor.normalized_args(&args(&["log", "-5"]));
        assert!(result.contains(&"-5".to_string()));
        assert!(!result.contains(&"20".to_string()));
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

    // --- Task 7: compress_stat ---

    #[test]
    fn compress_stat_single_file() {
        let raw = " src/main.rs | 3 +++\n 1 file changed, 3 insertions(+)\n";
        let result = compress_stat(raw);
        assert!(result.contains("src/main.rs | 3+"));
        assert!(result.contains("1 file changed, 3 insertions(+)"));
    }

    #[test]
    fn compress_stat_mixed_changes() {
        let raw =
            " src/auth.rs | 15 +++++++++------\n 1 file changed, 9 insertions(+), 6 deletions(-)\n";
        let result = compress_stat(raw);
        assert!(result.contains("src/auth.rs | 9+ 6-"));
    }

    #[test]
    fn compress_stat_deletions_only() {
        let raw = " src/old.rs | 5 -----\n 1 file changed, 5 deletions(-)\n";
        let result = compress_stat(raw);
        assert!(result.contains("src/old.rs | 5-"));
    }

    #[test]
    fn compress_stat_multiple_files() {
        let raw = " src/a.rs | 3 +++\n src/b.rs | 2 --\n 2 files changed, 3 insertions(+), 2 deletions(-)\n";
        let result = compress_stat(raw);
        assert!(result.contains("src/a.rs | 3+"));
        assert!(result.contains("src/b.rs | 2-"));
        assert!(result.contains("2 files changed"));
    }

    #[test]
    fn compress_stat_summary_passthrough() {
        let raw = "2 files changed, 10 insertions(+), 5 deletions(-)\n";
        let result = compress_stat(raw);
        assert!(result.contains("2 files changed, 10 insertions(+), 5 deletions(-)"));
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
            GitLogCompressor.compress("anything", "fatal: error", 128),
            None
        );
    }

    #[test]
    fn compress_empty_output() {
        assert_eq!(
            GitLogCompressor.compress("", "", 0),
            Some("(empty)\n".to_string())
        );
    }

    #[test]
    fn compress_whitespace_only_output() {
        assert_eq!(
            GitLogCompressor.compress("  \n\n  ", "", 0),
            Some("(empty)\n".to_string())
        );
    }

    #[test]
    fn compress_truncation_notice() {
        // Build exactly 20 commits
        let chunks: String = (0..20)
            .map(|i| {
                format!(
                    "\x01{:07x}\x00\x00{}\x00Author{}\x00Subject {}\x00",
                    i,
                    format!("2024-01-{:02}T10:00:00+00:00", (i % 28) + 1),
                    i,
                    i
                )
            })
            .collect();
        let result = GitLogCompressor.compress(&chunks, "", 0).unwrap();
        assert!(
            result.contains("(showing 20 commits, use -n to see more)"),
            "Expected truncation notice in:\n{}",
            result
        );
    }

    #[test]
    fn compress_no_truncation_notice_under_20() {
        let chunks: String = (0..5)
            .map(|i| {
                format!(
                    "\x01{:07x}\x00\x00{}\x00Author{}\x00Subject {}\x00",
                    i,
                    format!("2024-01-{:02}T10:00:00+00:00", i + 1),
                    i,
                    i
                )
            })
            .collect();
        let result = GitLogCompressor.compress(&chunks, "", 0).unwrap();
        assert!(
            !result.contains("showing 20 commits"),
            "Should not have truncation notice for 5 commits"
        );
    }
}
