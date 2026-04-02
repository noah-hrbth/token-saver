use super::commit_parser;
use super::diff_parser;
use crate::compressors::Compressor;

pub struct GitShowCompressor;

/// Flags that cause passthrough — output format or display mode chosen by the agent.
const SKIP_FLAGS: &[&str] = &[
    "--stat",
    "--name-only",
    "--name-status",
    "--raw",
    "--shortstat",
    "--numstat",
    "--summary",
    "--word-diff",
    "--color",
    "--color=always",
];

/// Known git --pretty presets that we can compress.
const COMPRESS_PRESETS: &[&str] = &["short", "medium", "full", "fuller"];

/// Known git --pretty presets that trigger passthrough.
const SKIP_PRESETS: &[&str] = &["oneline", "reference", "email", "raw", "mboxrd"];

fn should_skip_format_arg(arg: &str) -> bool {
    if let Some(value) = arg.strip_prefix("--format=") {
        return !COMPRESS_PRESETS.contains(&value);
    }
    if let Some(value) = arg.strip_prefix("--pretty=") {
        if SKIP_PRESETS.contains(&value) {
            return true;
        }
        if COMPRESS_PRESETS.contains(&value) {
            return false;
        }
        // Unknown preset treated as custom format → passthrough
        return true;
    }
    false
}

impl Compressor for GitShowCompressor {
    /// Returns true when first arg is exactly "show" and no skip conditions apply.
    fn can_compress(&self, args: &[String]) -> bool {
        if args.first().map(|s| s.as_str()) != Some("show") {
            return false;
        }

        let tail = &args[1..];

        for arg in tail {
            // Blob reference: positional arg containing ':'
            if !arg.starts_with('-') && arg.contains(':') {
                return false;
            }
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

        let has_no_patch = tail.iter().any(|a| a == "--no-patch");

        let mut result = vec![
            "show".to_string(),
            "--format=%x01%h%x00%D%x00%aI%x00%an%x00%s%x00%b".to_string(),
            "--no-color".to_string(),
        ];

        if !has_no_patch {
            result.push("--unified=1".to_string());
            result.push("--no-ext-diff".to_string());
            result.push("--diff-algorithm=histogram".to_string());
        }

        for arg in tail {
            // Strip patch aliases (diff flags already added unconditionally above)
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

        // Split on \x01: everything before the first \x01 may be a tag header.
        let chunks: Vec<&str> = stdout.split('\x01').collect();

        // chunks[0] is the preamble (tag header or empty), chunks[1..] are commit chunks.
        let preamble = chunks[0];
        let commit_chunks = &chunks[1..];

        if commit_chunks.is_empty() || commit_chunks.iter().all(|c| c.trim().is_empty()) {
            return Some("(empty)\n".to_string());
        }

        let mut output = String::new();

        // Prepend tag header when present
        if let Some(tag) = parse_tag_header(preamble) {
            output.push_str(&format_tag_header(&tag));
        }

        let mut first_commit = true;
        for chunk in commit_chunks {
            if chunk.trim().is_empty() {
                continue;
            }

            // Blank line between consecutive commits
            if !first_commit {
                output.push('\n');
            }
            first_commit = false;

            // Split off diff section
            let (meta, diff_files): (&str, Option<Vec<diff_parser::DiffFile>>) =
                match chunk.find("\ndiff --git ") {
                    Some(idx) => {
                        let diff_text = &chunk[idx + 1..];
                        let files = diff_parser::parse_diff(diff_text);
                        (
                            &chunk[..idx],
                            if files.is_empty() { None } else { Some(files) },
                        )
                    }
                    None => (chunk, None),
                };

            let fields = commit_parser::parse_commit_fields(meta)?;

            output.push_str(&commit_parser::format_commit_oneline(&fields));

            if let Some(ref body) = fields.body {
                output.push_str(&commit_parser::format_commit_body(body));
            }

            // Stat summary + diff
            if let Some(ref files) = diff_files {
                output.push('\n');
                if files.len() >= 2 {
                    output.push_str(&diff_parser::stat_summary(files));
                    output.push_str("\n\n");
                }
                for file in files {
                    output.push_str(&diff_parser::format_file(file));
                }
            }
        }

        Some(output)
    }
}

// ---------------------------------------------------------------------------
// Tag header
// ---------------------------------------------------------------------------

/// Parsed fields from an annotated git tag header.
pub struct TagHeader {
    pub name: String,
    pub tagger: String,
    pub date: String,
    pub annotation: String,
}

/// Parse the tag object header emitted by `git show <annotated-tag>`.
///
/// The header precedes the first `\x01` commit marker in the output and looks like:
/// ```text
/// tag v1.0
/// Tagger: Name <email>
/// Date:   Mon Jan 15 10:00:00 2024 +0000
///
///     Annotation message here.
/// ```
///
/// Returns `None` when the preamble does not contain a recognisable tag header.
pub fn parse_tag_header(raw: &str) -> Option<TagHeader> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    let mut name = None;
    let mut tagger = None;
    let mut raw_date = None;
    let mut annotation_lines: Vec<&str> = Vec::new();
    let mut in_annotation = false;

    for line in trimmed.lines() {
        if let Some(tag_name) = line.strip_prefix("tag ") {
            name = Some(tag_name.trim().to_string());
        } else if let Some(tagger_raw) = line.strip_prefix("Tagger: ") {
            // Extract name before '<'
            let tagger_name = tagger_raw
                .split('<')
                .next()
                .unwrap_or(tagger_raw)
                .trim()
                .to_string();
            tagger = Some(tagger_name);
        } else if let Some(date_raw) = line.strip_prefix("Date:") {
            raw_date = Some(date_raw.trim().to_string());
        } else if line.is_empty() && name.is_some() {
            // Blank line after headers marks start of annotation body
            in_annotation = true;
        } else if in_annotation {
            // Annotation lines are indented by 4 spaces in git output
            let content = line.strip_prefix("    ").unwrap_or(line);
            annotation_lines.push(content);
        }
    }

    let name = name?;

    let date = raw_date.map(|d| parse_git_date(&d)).unwrap_or_default();
    let tagger = tagger.unwrap_or_default();
    let annotation = annotation_lines.join("\n").trim().to_string();

    Some(TagHeader {
        name,
        tagger,
        date,
        annotation,
    })
}

/// Format a tag header as a single summary line with optional continuation.
///
/// Output: `tag: <name> [<tagger>] <date> "<first annotation line>"\n`
/// Additional annotation lines are indented by 2 spaces.
pub fn format_tag_header(tag: &TagHeader) -> String {
    let mut output = String::new();

    let mut lines = tag.annotation.lines();
    let first_line = lines.next().unwrap_or("");

    output.push_str(&format!(
        "tag: {} [{}] {} \"{}\"\n",
        tag.name, tag.tagger, tag.date, first_line
    ));

    for line in lines {
        output.push_str(&format!("  {}\n", line));
    }

    output
}

/// Parse a human-readable git date string to YYYY-MM-DD.
///
/// Supports the format `Mon Jan 15 10:00:00 2024 +0000` produced by git's `Date:` field.
/// Falls back to returning the raw string when parsing fails.
fn parse_git_date(raw: &str) -> String {
    // Format: "Mon Jan 15 10:00:00 2024 +0000"
    let parts: Vec<&str> = raw.split_whitespace().collect();
    if parts.len() < 5 {
        return raw.to_string();
    }

    let month_str = parts[1];
    let day_str = parts[2];
    let year_str = parts[4];

    let month = match month_str {
        "Jan" => "01",
        "Feb" => "02",
        "Mar" => "03",
        "Apr" => "04",
        "May" => "05",
        "Jun" => "06",
        "Jul" => "07",
        "Aug" => "08",
        "Sep" => "09",
        "Oct" => "10",
        "Nov" => "11",
        "Dec" => "12",
        _ => return raw.to_string(),
    };

    let day: u32 = match day_str.parse() {
        Ok(d) => d,
        Err(_) => return raw.to_string(),
    };

    format!("{}-{}-{:02}", year_str, month, day)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn args(strs: &[&str]) -> Vec<String> {
        strs.iter().map(|s| s.to_string()).collect()
    }

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

    // --- can_compress ---

    #[test]
    fn can_compress_bare_show() {
        assert!(GitShowCompressor.can_compress(&args(&["show"])));
    }

    #[test]
    fn can_compress_with_ref() {
        assert!(GitShowCompressor.can_compress(&args(&["show", "HEAD"])));
    }

    #[test]
    fn can_compress_multiple_refs() {
        assert!(GitShowCompressor.can_compress(&args(&["show", "HEAD", "HEAD~1"])));
    }

    #[test]
    fn can_compress_no_patch() {
        assert!(GitShowCompressor.can_compress(&args(&["show", "--no-patch", "HEAD"])));
    }

    #[test]
    fn can_compress_pretty_medium() {
        assert!(GitShowCompressor.can_compress(&args(&["show", "--pretty=medium"])));
    }

    #[test]
    fn can_compress_pretty_full() {
        assert!(GitShowCompressor.can_compress(&args(&["show", "--pretty=full"])));
    }

    #[test]
    fn can_compress_pretty_short() {
        assert!(GitShowCompressor.can_compress(&args(&["show", "--pretty=short"])));
    }

    #[test]
    fn can_compress_pretty_fuller() {
        assert!(GitShowCompressor.can_compress(&args(&["show", "--pretty=fuller"])));
    }

    #[test]
    fn skip_blob_ref_colon() {
        assert!(!GitShowCompressor.can_compress(&args(&["show", "HEAD:src/main.rs"])));
    }

    #[test]
    fn skip_blob_ref_tag_colon() {
        assert!(!GitShowCompressor.can_compress(&args(&["show", "v1.0:README.md"])));
    }

    #[test]
    fn skip_stat_flag() {
        assert!(!GitShowCompressor.can_compress(&args(&["show", "--stat"])));
    }

    #[test]
    fn skip_name_only() {
        assert!(!GitShowCompressor.can_compress(&args(&["show", "--name-only"])));
    }

    #[test]
    fn skip_name_status() {
        assert!(!GitShowCompressor.can_compress(&args(&["show", "--name-status"])));
    }

    #[test]
    fn skip_raw_flag() {
        assert!(!GitShowCompressor.can_compress(&args(&["show", "--raw"])));
    }

    #[test]
    fn skip_shortstat() {
        assert!(!GitShowCompressor.can_compress(&args(&["show", "--shortstat"])));
    }

    #[test]
    fn skip_numstat() {
        assert!(!GitShowCompressor.can_compress(&args(&["show", "--numstat"])));
    }

    #[test]
    fn skip_summary() {
        assert!(!GitShowCompressor.can_compress(&args(&["show", "--summary"])));
    }

    #[test]
    fn skip_word_diff() {
        assert!(!GitShowCompressor.can_compress(&args(&["show", "--word-diff"])));
    }

    #[test]
    fn skip_color_flag() {
        assert!(!GitShowCompressor.can_compress(&args(&["show", "--color"])));
    }

    #[test]
    fn skip_color_always() {
        assert!(!GitShowCompressor.can_compress(&args(&["show", "--color=always"])));
    }

    #[test]
    fn skip_custom_format() {
        assert!(!GitShowCompressor.can_compress(&args(&["show", "--format=%H %s"])));
    }

    #[test]
    fn skip_pretty_oneline() {
        assert!(!GitShowCompressor.can_compress(&args(&["show", "--pretty=oneline"])));
    }

    #[test]
    fn skip_pretty_reference() {
        assert!(!GitShowCompressor.can_compress(&args(&["show", "--pretty=reference"])));
    }

    #[test]
    fn skip_pretty_email() {
        assert!(!GitShowCompressor.can_compress(&args(&["show", "--pretty=email"])));
    }

    #[test]
    fn skip_pretty_raw() {
        assert!(!GitShowCompressor.can_compress(&args(&["show", "--pretty=raw"])));
    }

    #[test]
    fn skip_pretty_mboxrd() {
        assert!(!GitShowCompressor.can_compress(&args(&["show", "--pretty=mboxrd"])));
    }

    #[test]
    fn non_show_status() {
        assert!(!GitShowCompressor.can_compress(&args(&["status"])));
    }

    #[test]
    fn non_show_diff() {
        assert!(!GitShowCompressor.can_compress(&args(&["diff"])));
    }

    #[test]
    fn non_show_log() {
        assert!(!GitShowCompressor.can_compress(&args(&["log"])));
    }

    #[test]
    fn empty_args() {
        assert!(!GitShowCompressor.can_compress(&args(&[])));
    }

    // --- normalized_args ---

    #[test]
    fn bare_show_has_required_flags() {
        let result = GitShowCompressor.normalized_args(&args(&["show"]));
        assert_eq!(result[0], "show");
        assert!(result.iter().any(|a| a.starts_with("--format=")));
        assert!(result.contains(&"--no-color".to_string()));
        assert!(result.contains(&"--unified=1".to_string()));
        assert!(result.contains(&"--no-ext-diff".to_string()));
        assert!(result.contains(&"--diff-algorithm=histogram".to_string()));
    }

    #[test]
    fn no_patch_skips_diff_flags() {
        let result = GitShowCompressor.normalized_args(&args(&["show", "--no-patch", "HEAD"]));
        assert!(!result.contains(&"--unified=1".to_string()));
        assert!(!result.contains(&"--no-ext-diff".to_string()));
        assert!(!result.contains(&"--diff-algorithm=histogram".to_string()));
        // --no-patch is preserved
        assert!(result.contains(&"--no-patch".to_string()));
    }

    #[test]
    fn preserves_ref_args() {
        let result = GitShowCompressor.normalized_args(&args(&["show", "HEAD~3"]));
        assert!(result.contains(&"HEAD~3".to_string()));
    }

    #[test]
    fn strips_patch_alias() {
        let result = GitShowCompressor.normalized_args(&args(&["show", "-p", "HEAD"]));
        assert!(!result.contains(&"-p".to_string()));
    }

    #[test]
    fn strips_patch_long() {
        let result = GitShowCompressor.normalized_args(&args(&["show", "--patch"]));
        assert!(!result.contains(&"--patch".to_string()));
    }

    #[test]
    fn strips_u_alias() {
        let result = GitShowCompressor.normalized_args(&args(&["show", "-u"]));
        assert!(!result.contains(&"-u".to_string()));
    }

    #[test]
    fn strips_format_arg() {
        let result = GitShowCompressor.normalized_args(&args(&["show", "--format=medium"]));
        assert!(!result.iter().any(|a| a.starts_with("--format=medium")));
        // Our own --format= is still present
        assert!(result.iter().any(|a| a.starts_with("--format=%x01")));
    }

    #[test]
    fn strips_pretty_arg() {
        let result = GitShowCompressor.normalized_args(&args(&["show", "--pretty=full"]));
        assert!(!result.iter().any(|a| a == "--pretty=full"));
    }

    #[test]
    fn strips_color_arg() {
        let result = GitShowCompressor.normalized_args(&args(&["show", "--color"]));
        assert!(!result.contains(&"--color".to_string()));
    }

    #[test]
    fn strips_color_auto() {
        let result = GitShowCompressor.normalized_args(&args(&["show", "--color=auto"]));
        assert!(!result.iter().any(|a| a.starts_with("--color=")));
    }

    #[test]
    fn preserves_path_limiter() {
        let result =
            GitShowCompressor.normalized_args(&args(&["show", "HEAD", "--", "src/main.rs"]));
        assert!(result.contains(&"--".to_string()));
        assert!(result.contains(&"src/main.rs".to_string()));
    }

    // --- compress ---

    #[test]
    fn compress_nonzero_exit_returns_none() {
        assert_eq!(GitShowCompressor.compress("anything", "error", 128), None);
    }

    #[test]
    fn compress_empty_output() {
        assert_eq!(
            GitShowCompressor.compress("", "", 0),
            Some("(empty)\n".to_string())
        );
    }

    #[test]
    fn compress_whitespace_only() {
        assert_eq!(
            GitShowCompressor.compress("  \n\n  ", "", 0),
            Some("(empty)\n".to_string())
        );
    }

    #[test]
    fn compress_malformed_returns_none() {
        // Has \x01 but chunk is malformed (fewer than 6 fields)
        let raw = "\x01hash\x00decs\x00date";
        assert_eq!(GitShowCompressor.compress(raw, "", 0), None);
    }

    #[test]
    fn compress_single_commit() {
        let chunk = make_chunk(
            "a1b2c3f",
            "HEAD -> main",
            "2024-01-15T10:00:00+00:00",
            "Alice",
            "Add feature",
            "",
        );
        let raw = format!("\x01{}", chunk);
        let result = GitShowCompressor.compress(&raw, "", 0).unwrap();
        assert!(result.contains("a1b2c3f"));
        assert!(result.contains("HEAD -> main"));
        assert!(result.contains("2024-01-15"));
        assert!(result.contains("Alice"));
        assert!(result.contains("Add feature"));
    }

    #[test]
    fn compress_commit_with_body() {
        let chunk = make_chunk(
            "a1b2c3f",
            "",
            "2024-01-15T10:00:00+00:00",
            "Alice",
            "Subject",
            "This is the body.\nWith multiple lines.",
        );
        let raw = format!("\x01{}", chunk);
        let result = GitShowCompressor.compress(&raw, "", 0).unwrap();
        assert!(result.contains("  This is the body.\n"));
        assert!(result.contains("  With multiple lines.\n"));
    }

    #[test]
    fn compress_multiple_refs() {
        let c1 = make_chunk(
            "aaa1111",
            "",
            "2024-01-15T10:00:00+00:00",
            "Alice",
            "First",
            "",
        );
        let c2 = make_chunk(
            "bbb2222",
            "",
            "2024-01-14T10:00:00+00:00",
            "Bob",
            "Second",
            "",
        );
        let raw = format!("\x01{}\x01{}", c1, c2);
        let result = GitShowCompressor.compress(&raw, "", 0).unwrap();
        assert!(result.contains("aaa1111"));
        assert!(result.contains("bbb2222"));
        assert!(result.contains("First"));
        assert!(result.contains("Second"));
    }

    #[test]
    fn compress_commit_with_diff() {
        let diff_text = "diff --git a/src/main.rs b/src/main.rs\nindex abc..def 100644\n--- a/src/main.rs\n+++ b/src/main.rs\n@@ -1,3 +1,4 @@\n fn main() {\n+    println!(\"hello\");\n }\n";
        let chunk = make_chunk(
            "a1b2c3f",
            "",
            "2024-01-15T10:00:00+00:00",
            "Alice",
            "Add print",
            "",
        );
        let raw = format!("\x01{}\n{}", chunk, diff_text);
        let result = GitShowCompressor.compress(&raw, "", 0).unwrap();
        assert!(result.contains("a1b2c3f"));
        assert!(result.contains("src/main.rs"));
    }

    #[test]
    fn compress_multi_file_diff_has_stat_summary() {
        let diff_text = concat!(
            "diff --git a/src/a.rs b/src/a.rs\n",
            "index abc..def 100644\n",
            "--- a/src/a.rs\n",
            "+++ b/src/a.rs\n",
            "@@ -1,2 +1,3 @@\n",
            " fn a() {\n",
            "+    // changed\n",
            " }\n",
            "diff --git a/src/b.rs b/src/b.rs\n",
            "index abc..def 100644\n",
            "--- a/src/b.rs\n",
            "+++ b/src/b.rs\n",
            "@@ -1,2 +1,3 @@\n",
            " fn b() {\n",
            "+    // also changed\n",
            " }\n"
        );
        let chunk = make_chunk(
            "a1b2c3f",
            "",
            "2024-01-15T10:00:00+00:00",
            "Alice",
            "Change both",
            "",
        );
        let raw = format!("\x01{}\n{}", chunk, diff_text);
        let result = GitShowCompressor.compress(&raw, "", 0).unwrap();
        // Should have a stat summary for multi-file diff
        assert!(
            result.contains("files changed"),
            "Expected stat summary in:\n{}",
            result
        );
    }

    #[test]
    fn compress_single_file_diff_no_stat_summary() {
        let diff_text = concat!(
            "diff --git a/src/main.rs b/src/main.rs\n",
            "index abc..def 100644\n",
            "--- a/src/main.rs\n",
            "+++ b/src/main.rs\n",
            "@@ -1,2 +1,3 @@\n",
            " fn main() {\n",
            "+    // changed\n",
            " }\n"
        );
        let chunk = make_chunk(
            "a1b2c3f",
            "",
            "2024-01-15T10:00:00+00:00",
            "Alice",
            "Change one",
            "",
        );
        let raw = format!("\x01{}\n{}", chunk, diff_text);
        let result = GitShowCompressor.compress(&raw, "", 0).unwrap();
        // Single-file diff should NOT include a stat summary
        assert!(
            !result.contains("files changed"),
            "Should not have stat summary for single file:\n{}",
            result
        );
    }

    // --- parse_tag_header / format_tag_header ---

    #[test]
    fn parse_tag_header_basic() {
        let raw = "tag v1.0\nTagger: Alice <alice@example.com>\nDate:   Mon Jan 15 10:00:00 2024 +0000\n\n    Initial release\n";
        let tag = parse_tag_header(raw).unwrap();
        assert_eq!(tag.name, "v1.0");
        assert_eq!(tag.tagger, "Alice");
        assert_eq!(tag.date, "2024-01-15");
        assert_eq!(tag.annotation, "Initial release");
    }

    #[test]
    fn parse_tag_header_empty_returns_none() {
        assert!(parse_tag_header("").is_none());
        assert!(parse_tag_header("   \n  ").is_none());
    }

    #[test]
    fn parse_tag_header_no_tag_line_returns_none() {
        // Looks like a commit header, not a tag header
        let raw = "commit abc1234\nAuthor: Alice\nDate: Mon Jan 15 2024\n\n    Fix bug\n";
        assert!(parse_tag_header(raw).is_none());
    }

    #[test]
    fn format_tag_header_single_line_annotation() {
        let tag = TagHeader {
            name: "v1.0".to_string(),
            tagger: "Alice".to_string(),
            date: "2024-01-15".to_string(),
            annotation: "Initial release".to_string(),
        };
        let result = format_tag_header(&tag);
        assert_eq!(result, "tag: v1.0 [Alice] 2024-01-15 \"Initial release\"\n");
    }

    #[test]
    fn format_tag_header_multi_line_annotation() {
        let tag = TagHeader {
            name: "v2.0".to_string(),
            tagger: "Bob".to_string(),
            date: "2024-06-01".to_string(),
            annotation: "Major release\nBreaking changes\nSee CHANGELOG".to_string(),
        };
        let result = format_tag_header(&tag);
        assert!(result.starts_with("tag: v2.0 [Bob] 2024-06-01 \"Major release\"\n"));
        assert!(result.contains("  Breaking changes\n"));
        assert!(result.contains("  See CHANGELOG\n"));
    }

    #[test]
    fn compress_annotated_tag_with_commit() {
        let tag_header = "tag v1.0\nTagger: Alice <alice@example.com>\nDate:   Mon Jan 15 10:00:00 2024 +0000\n\n    Initial release\n\n";
        let chunk = make_chunk(
            "a1b2c3f",
            "tag: v1.0",
            "2024-01-15T10:00:00+00:00",
            "Alice",
            "Add feature",
            "",
        );
        let raw = format!("{}\x01{}", tag_header, chunk);
        let result = GitShowCompressor.compress(&raw, "", 0).unwrap();
        assert!(result.contains("tag: v1.0"));
        assert!(result.contains("Alice"));
        assert!(result.contains("Initial release"));
        assert!(result.contains("a1b2c3f"));
    }

    #[test]
    fn compress_annotated_tag_multi_line_annotation() {
        let tag_header = "tag v2.0\nTagger: Bob <bob@example.com>\nDate:   Fri Jun 01 12:00:00 2024 +0000\n\n    Major release\n    Breaking changes here\n\n";
        let chunk = make_chunk(
            "bbb2222",
            "tag: v2.0",
            "2024-06-01T12:00:00+00:00",
            "Bob",
            "Bump version",
            "",
        );
        let raw = format!("{}\x01{}", tag_header, chunk);
        let result = GitShowCompressor.compress(&raw, "", 0).unwrap();
        assert!(result.contains("tag: v2.0"));
        assert!(result.contains("Major release"));
        assert!(result.contains("  Breaking changes here"));
    }

    // --- parse_git_date ---

    #[test]
    fn parse_git_date_standard() {
        assert_eq!(
            parse_git_date("Mon Jan 15 10:00:00 2024 +0000"),
            "2024-01-15"
        );
    }

    #[test]
    fn parse_git_date_single_digit_day() {
        assert_eq!(
            parse_git_date("Thu Jun  1 12:00:00 2024 +0000"),
            "2024-06-01"
        );
    }

    #[test]
    fn parse_git_date_malformed_passthrough() {
        assert_eq!(parse_git_date("not a date"), "not a date");
    }
}
