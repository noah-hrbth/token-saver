// TODO: Semantic compression — Layer 5 techniques for v2
// #23: Detect moved blocks — large delete + identical add elsewhere → "(moved from line X)".
//      High token savings for refactors, high implementation complexity.
// #24: Collapse large uniform additions — e.g. 30 consecutive `+use ...` lines →
//      `+use ... (28 more imports)`. High savings when triggered, medium complexity.
// #26: Factor common path prefix — when all files share a deep prefix like
//      `src/compressors/git/`, show it once. Low-medium savings, low complexity.

use super::diff_parser::{DiffFile, DiffLine, FileStatus, Hunk, parse_diff};
use crate::compressors::Compressor;

pub struct GitDiffCompressor;

const SKIP_FLAGS: &[&str] = &[
    "--stat",
    "--name-only",
    "--name-status",
    "--raw",
    "--word-diff",
    "--check",
    "--summary",
    "--shortstat",
    "--numstat",
    "--submodule",
    "--color",
    "--color=always",
    "--ext-diff",
];

impl Compressor for GitDiffCompressor {
    /// Returns true when the first arg is exactly "diff" (not diff-tree, etc.)
    /// and no skip flags are present in the remaining args.
    fn can_compress(&self, args: &[String]) -> bool {
        if args.first().map(|s| s.as_str()) != Some("diff") {
            return false;
        }

        let tail = &args[1..];
        !tail.iter().any(|arg| SKIP_FLAGS.contains(&arg.as_str()))
    }

    fn normalized_args(&self, original_args: &[String]) -> Vec<String> {
        let mut result = vec![
            "diff".to_string(),
            "--unified=1".to_string(),
            "--diff-algorithm=histogram".to_string(),
            "--no-ext-diff".to_string(),
            "--no-color".to_string(),
        ];
        result.extend(original_args[1..].iter().cloned());
        result
    }

    fn compress(&self, stdout: &str, _stderr: &str, exit_code: i32) -> Option<String> {
        if exit_code != 0 {
            return None;
        }
        if stdout.trim().is_empty() {
            return Some(String::new());
        }

        let files = parse_diff(stdout);
        if files.is_empty() {
            return None;
        }

        let mut output = String::new();

        if files.len() >= 2 {
            output.push_str(&stat_summary(&files));
            output.push_str("\n\n");
        }

        for file in &files {
            output.push_str(&format_file(file));
        }

        Some(output)
    }
}

// --- Formatting ---

/// Format a single DiffFile into compressed output.
fn format_file(file: &DiffFile) -> String {
    let mut output = String::new();

    match file.status {
        FileStatus::New => output.push_str(&format!("{} (new)\n", file.path)),
        FileStatus::Deleted => output.push_str(&format!("{} (deleted)\n", file.path)),
        FileStatus::Renamed => {
            let old = file.old_path.as_deref().unwrap_or("?");
            output.push_str(&format!("{} \u{2192} {}\n", old, file.path));
        }
        FileStatus::ModeChanged => {
            let old = file.old_mode.as_deref().unwrap_or("?");
            let new = file.new_mode.as_deref().unwrap_or("?");
            output.push_str(&format!("{} (mode {} \u{2192} {})\n", file.path, old, new));
        }
        FileStatus::Binary => {
            output.push_str(&format!("{} (binary, changed)\n", file.path));
        }
        FileStatus::Normal => output.push_str(&format!("{}\n", file.path)),
    }

    for hunk in &file.hunks {
        output.push_str(&format_hunk(hunk));
    }

    output
}

/// Format a hunk: compressed header + content lines.
fn format_hunk(hunk: &Hunk) -> String {
    let mut output = String::new();

    // Hunk header — line numbers without counts
    let old_part = if hunk.old_start > 0 {
        format!("-{}", hunk.old_start)
    } else {
        String::new()
    };
    let new_part = if hunk.new_start > 0 {
        format!("+{}", hunk.new_start)
    } else {
        String::new()
    };

    match &hunk.function_context {
        Some(ctx) => output.push_str(&format!("@@ {} {} @@ {}\n", old_part, new_part, ctx)),
        None => output.push_str(&format!("@@ {} {}\n", old_part, new_part)),
    }

    // Whitespace-only collapse
    if is_whitespace_only_hunk(hunk) {
        output.push_str("(whitespace changes)\n");
        return output;
    }

    for line in &hunk.lines {
        match line {
            DiffLine::Context(s) => output.push_str(&format!(" {}\n", s)),
            DiffLine::Added(s) => output.push_str(&format!("+{}\n", s)),
            DiffLine::Removed(s) => output.push_str(&format!("-{}\n", s)),
        }
    }

    output
}

/// Check if a hunk only contains whitespace changes.
/// After trimming whitespace, the multiset of removed lines equals the multiset of added lines.
fn is_whitespace_only_hunk(hunk: &Hunk) -> bool {
    let mut removed: Vec<String> = Vec::new();
    let mut added: Vec<String> = Vec::new();

    for line in &hunk.lines {
        match line {
            DiffLine::Removed(s) => removed.push(s.split_whitespace().collect()),
            DiffLine::Added(s) => added.push(s.split_whitespace().collect()),
            DiffLine::Context(_) => {}
        }
    }

    if removed.is_empty() && added.is_empty() {
        return false;
    }

    removed.sort();
    added.sort();
    removed == added
}

/// Build stat summary line for multi-file diffs.
fn stat_summary(files: &[DiffFile]) -> String {
    let mut insertions = 0usize;
    let mut deletions = 0usize;

    for file in files {
        for hunk in &file.hunks {
            for line in &hunk.lines {
                match line {
                    DiffLine::Added(_) => insertions += 1,
                    DiffLine::Removed(_) => deletions += 1,
                    DiffLine::Context(_) => {}
                }
            }
        }
    }

    let files_part = if files.len() == 1 {
        "1 file changed".to_string()
    } else {
        format!("{} files changed", files.len())
    };

    let ins_part = if insertions == 1 {
        "1 insertion(+)".to_string()
    } else {
        format!("{} insertions(+)", insertions)
    };

    let del_part = if deletions == 1 {
        "1 deletion(-)".to_string()
    } else {
        format!("{} deletions(-)", deletions)
    };

    format!("{}, {}, {}", files_part, ins_part, del_part)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(strs: &[&str]) -> Vec<String> {
        strs.iter().map(|s| s.to_string()).collect()
    }

    // --- positive cases ---

    #[test]
    fn can_compress_bare_diff() {
        assert!(GitDiffCompressor.can_compress(&args(&["diff"])));
    }

    #[test]
    fn can_compress_diff_staged() {
        assert!(GitDiffCompressor.can_compress(&args(&["diff", "--staged"])));
    }

    #[test]
    fn can_compress_diff_cached() {
        assert!(GitDiffCompressor.can_compress(&args(&["diff", "--cached"])));
    }

    #[test]
    fn can_compress_diff_commits() {
        assert!(GitDiffCompressor.can_compress(&args(&["diff", "HEAD~3"])));
    }

    #[test]
    fn can_compress_diff_branches() {
        assert!(GitDiffCompressor.can_compress(&args(&["diff", "main..feature"])));
    }

    // --- skip flags ---

    #[test]
    fn skip_diff_stat() {
        assert!(!GitDiffCompressor.can_compress(&args(&["diff", "--stat"])));
    }

    #[test]
    fn skip_diff_name_only() {
        assert!(!GitDiffCompressor.can_compress(&args(&["diff", "--name-only"])));
    }

    #[test]
    fn skip_diff_name_status() {
        assert!(!GitDiffCompressor.can_compress(&args(&["diff", "--name-status"])));
    }

    #[test]
    fn skip_diff_raw() {
        assert!(!GitDiffCompressor.can_compress(&args(&["diff", "--raw"])));
    }

    #[test]
    fn skip_diff_word_diff() {
        assert!(!GitDiffCompressor.can_compress(&args(&["diff", "--word-diff"])));
    }

    #[test]
    fn skip_diff_check() {
        assert!(!GitDiffCompressor.can_compress(&args(&["diff", "--check"])));
    }

    #[test]
    fn skip_diff_summary() {
        assert!(!GitDiffCompressor.can_compress(&args(&["diff", "--summary"])));
    }

    #[test]
    fn skip_diff_shortstat() {
        assert!(!GitDiffCompressor.can_compress(&args(&["diff", "--shortstat"])));
    }

    #[test]
    fn skip_diff_numstat() {
        assert!(!GitDiffCompressor.can_compress(&args(&["diff", "--numstat"])));
    }

    #[test]
    fn skip_diff_submodule() {
        assert!(!GitDiffCompressor.can_compress(&args(&["diff", "--submodule"])));
    }

    #[test]
    fn skip_diff_color() {
        assert!(!GitDiffCompressor.can_compress(&args(&["diff", "--color"])));
    }

    #[test]
    fn skip_diff_color_always() {
        assert!(!GitDiffCompressor.can_compress(&args(&["diff", "--color=always"])));
    }

    #[test]
    fn skip_diff_ext_diff() {
        assert!(!GitDiffCompressor.can_compress(&args(&["diff", "--ext-diff"])));
    }

    // --- normalized_args ---

    #[test]
    fn normalized_args_bare_diff() {
        let args: Vec<String> = vec!["diff".into()];
        let result = GitDiffCompressor.normalized_args(&args);
        assert_eq!(
            result,
            vec![
                "diff",
                "--unified=1",
                "--diff-algorithm=histogram",
                "--no-ext-diff",
                "--no-color",
            ]
        );
    }

    #[test]
    fn normalized_args_with_staged() {
        let args: Vec<String> = vec!["diff".into(), "--staged".into()];
        let result = GitDiffCompressor.normalized_args(&args);
        assert_eq!(
            result,
            vec![
                "diff",
                "--unified=1",
                "--diff-algorithm=histogram",
                "--no-ext-diff",
                "--no-color",
                "--staged",
            ]
        );
    }

    #[test]
    fn normalized_args_with_commits() {
        let args: Vec<String> = vec!["diff".into(), "HEAD~3".into()];
        let result = GitDiffCompressor.normalized_args(&args);
        assert_eq!(
            result,
            vec![
                "diff",
                "--unified=1",
                "--diff-algorithm=histogram",
                "--no-ext-diff",
                "--no-color",
                "HEAD~3",
            ]
        );
    }

    #[test]
    fn normalized_args_user_override_unified() {
        let args: Vec<String> = vec!["diff".into(), "--unified=3".into()];
        let result = GitDiffCompressor.normalized_args(&args);
        assert_eq!(
            result,
            vec![
                "diff",
                "--unified=1",
                "--diff-algorithm=histogram",
                "--no-ext-diff",
                "--no-color",
                "--unified=3",
            ]
        );
    }

    // --- non-diff commands ---

    #[test]
    fn skip_non_diff_commands() {
        assert!(!GitDiffCompressor.can_compress(&args(&["status"])));
        assert!(!GitDiffCompressor.can_compress(&args(&["log"])));
        assert!(!GitDiffCompressor.can_compress(&args(&["diff-tree"])));
        assert!(!GitDiffCompressor.can_compress(&args(&["diff-files"])));
        assert!(!GitDiffCompressor.can_compress(&args(&["diff-index"])));
        assert!(!GitDiffCompressor.can_compress(&args(&[])));
    }

    // --- parsing tests ---

    #[test]
    fn parse_normal_file_header() {
        let raw = "diff --git a/src/main.rs b/src/main.rs\nindex abc1234..def5678 100644\n--- a/src/main.rs\n+++ b/src/main.rs\n@@ -1,3 +1,4 @@\n fn main() {\n+    println!(\"hello\");\n }\n";
        let files = parse_diff(raw);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "src/main.rs");
        assert_eq!(files[0].status, FileStatus::Normal);
        assert_eq!(files[0].hunks.len(), 1);
    }

    #[test]
    fn parse_new_file_header() {
        let raw = "diff --git a/src/new.rs b/src/new.rs\nnew file mode 100644\nindex 0000000..abc1234\n--- /dev/null\n+++ b/src/new.rs\n@@ -0,0 +1,3 @@\n+fn new_function() {\n+    // new\n+}\n";
        let files = parse_diff(raw);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "src/new.rs");
        assert_eq!(files[0].status, FileStatus::New);
    }

    #[test]
    fn parse_deleted_file_header() {
        let raw = "diff --git a/src/old.rs b/src/old.rs\ndeleted file mode 100644\nindex abc1234..0000000\n--- a/src/old.rs\n+++ /dev/null\n@@ -1,3 +0,0 @@\n-fn old_function() {\n-    // old\n-}\n";
        let files = parse_diff(raw);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "src/old.rs");
        assert_eq!(files[0].status, FileStatus::Deleted);
    }

    #[test]
    fn parse_renamed_file_header() {
        let raw = "diff --git a/old_name.rs b/new_name.rs\nsimilarity index 95%\nrename from old_name.rs\nrename to new_name.rs\nindex abc1234..def5678 100644\n--- a/old_name.rs\n+++ b/new_name.rs\n@@ -1,3 +1,3 @@\n-fn old() {}\n+fn new() {}\n";
        let files = parse_diff(raw);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "new_name.rs");
        assert_eq!(files[0].old_path, Some("old_name.rs".to_string()));
        assert_eq!(files[0].status, FileStatus::Renamed);
    }

    #[test]
    fn parse_mode_change_header() {
        let raw = "diff --git a/script.sh b/script.sh\nold mode 100644\nnew mode 100755\n";
        let files = parse_diff(raw);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "script.sh");
        assert_eq!(files[0].status, FileStatus::ModeChanged);
        assert_eq!(files[0].old_mode, Some("100644".to_string()));
        assert_eq!(files[0].new_mode, Some("100755".to_string()));
    }

    #[test]
    fn parse_binary_file() {
        let raw = "diff --git a/image.png b/image.png\nindex abc1234..def5678 100644\nBinary files a/image.png and b/image.png differ\n";
        let files = parse_diff(raw);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "image.png");
        assert_eq!(files[0].status, FileStatus::Binary);
        assert!(files[0].hunks.is_empty());
    }

    #[test]
    fn parse_multiple_files() {
        let raw = "diff --git a/src/a.rs b/src/a.rs\nindex abc..def 100644\n--- a/src/a.rs\n+++ b/src/a.rs\n@@ -1,2 +1,3 @@\n fn a() {\n+    // changed\n }\ndiff --git a/src/b.rs b/src/b.rs\nindex abc..def 100644\n--- a/src/b.rs\n+++ b/src/b.rs\n@@ -1,2 +1,3 @@\n fn b() {\n+    // also changed\n }\n";
        let files = parse_diff(raw);
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].path, "src/a.rs");
        assert_eq!(files[1].path, "src/b.rs");
    }

    #[test]
    fn parse_hunk_content_lines() {
        let raw = "diff --git a/src/main.rs b/src/main.rs\nindex abc..def 100644\n--- a/src/main.rs\n+++ b/src/main.rs\n@@ -1,3 +1,4 @@ fn main\n fn main() {\n-    old_line();\n+    new_line();\n+    extra_line();\n }\n";
        let files = parse_diff(raw);
        let hunk = &files[0].hunks[0];
        assert_eq!(hunk.function_context, Some("fn main".to_string()));
        assert_eq!(hunk.old_start, 1);
        assert_eq!(hunk.new_start, 1);
        assert_eq!(
            hunk.lines,
            vec![
                DiffLine::Context("fn main() {".to_string()),
                DiffLine::Removed("    old_line();".to_string()),
                DiffLine::Added("    new_line();".to_string()),
                DiffLine::Added("    extra_line();".to_string()),
                DiffLine::Context("}".to_string()),
            ]
        );
    }

    #[test]
    fn parse_multiple_hunks() {
        let raw = "diff --git a/src/main.rs b/src/main.rs\nindex abc..def 100644\n--- a/src/main.rs\n+++ b/src/main.rs\n@@ -1,2 +1,3 @@ fn first\n fn first() {\n+    // added\n }\n@@ -10,2 +11,3 @@ fn second\n fn second() {\n+    // also added\n }\n";
        let files = parse_diff(raw);
        assert_eq!(files[0].hunks.len(), 2);
        assert_eq!(files[0].hunks[0].old_start, 1);
        assert_eq!(
            files[0].hunks[0].function_context,
            Some("fn first".to_string())
        );
        assert_eq!(files[0].hunks[1].old_start, 10);
        assert_eq!(
            files[0].hunks[1].function_context,
            Some("fn second".to_string())
        );
    }

    // --- compress / formatting tests ---

    #[test]
    fn compress_single_file() {
        let input = "diff --git a/src/main.rs b/src/main.rs\nindex abc..def 100644\n--- a/src/main.rs\n+++ b/src/main.rs\n@@ -1,3 +1,4 @@ fn main\n fn main() {\n+    println!(\"hello\");\n }\n";
        let result = GitDiffCompressor.compress(input, "", 0);
        let output = result.unwrap();
        assert!(output.starts_with("src/main.rs\n"));
        assert!(output.contains("@@ -1 +1 @@ fn main\n"));
        assert!(output.contains("+    println!(\"hello\");\n"));
    }

    #[test]
    fn compress_new_file() {
        let input = "diff --git a/src/new.rs b/src/new.rs\nnew file mode 100644\nindex 0000000..abc1234\n--- /dev/null\n+++ b/src/new.rs\n@@ -0,0 +1,2 @@\n+fn new_fn() {\n+}\n";
        let result = GitDiffCompressor.compress(input, "", 0).unwrap();
        assert!(result.starts_with("src/new.rs (new)\n"));
        assert!(result.contains("+fn new_fn() {\n"));
    }

    #[test]
    fn compress_deleted_file() {
        let input = "diff --git a/src/old.rs b/src/old.rs\ndeleted file mode 100644\nindex abc1234..0000000\n--- a/src/old.rs\n+++ /dev/null\n@@ -1,2 +0,0 @@\n-fn old_fn() {\n-}\n";
        let result = GitDiffCompressor.compress(input, "", 0).unwrap();
        assert!(result.contains("src/old.rs (deleted)\n"));
        assert!(result.contains("-fn old_fn() {\n"));
    }

    #[test]
    fn compress_renamed_file() {
        let input = "diff --git a/old.rs b/new.rs\nsimilarity index 95%\nrename from old.rs\nrename to new.rs\nindex abc..def 100644\n--- a/old.rs\n+++ b/new.rs\n@@ -1,2 +1,2 @@\n-fn old() {}\n+fn new() {}\n";
        let result = GitDiffCompressor.compress(input, "", 0).unwrap();
        assert!(result.starts_with("old.rs \u{2192} new.rs\n"));
        assert!(result.contains("-fn old() {}"));
        assert!(result.contains("+fn new() {}"));
    }

    #[test]
    fn compress_mode_change() {
        let input = "diff --git a/script.sh b/script.sh\nold mode 100644\nnew mode 100755\n";
        let result = GitDiffCompressor.compress(input, "", 0).unwrap();
        assert!(result.contains("script.sh (mode 100644 \u{2192} 100755)"));
    }

    #[test]
    fn compress_binary_file() {
        let input = "diff --git a/image.png b/image.png\nindex abc..def 100644\nBinary files a/image.png and b/image.png differ\n";
        let result = GitDiffCompressor.compress(input, "", 0).unwrap();
        assert!(result.contains("image.png (binary, changed)"));
    }

    #[test]
    fn compress_hunk_no_function_context() {
        let input = "diff --git a/file.txt b/file.txt\nindex abc..def 100644\n--- a/file.txt\n+++ b/file.txt\n@@ -1,2 +1,3 @@\n line1\n+line2\n line3\n";
        let result = GitDiffCompressor.compress(input, "", 0).unwrap();
        assert!(
            result.contains("@@ -1 +1\n"),
            "Expected hunk header without function context in:\n{}",
            result
        );
    }

    // --- stat summary tests ---

    #[test]
    fn compress_multifile_has_stat_summary() {
        let input = "diff --git a/src/a.rs b/src/a.rs\nindex abc..def 100644\n--- a/src/a.rs\n+++ b/src/a.rs\n@@ -1,2 +1,3 @@\n fn a() {\n+    // changed\n }\ndiff --git a/src/b.rs b/src/b.rs\nindex abc..def 100644\n--- a/src/b.rs\n+++ b/src/b.rs\n@@ -1,3 +1,2 @@\n fn b() {\n-    // removed\n }\n";
        let result = GitDiffCompressor.compress(input, "", 0).unwrap();
        assert!(
            result.starts_with("2 files changed, 1 insertion(+), 1 deletion(-)\n\n"),
            "Got:\n{}",
            result
        );
    }

    #[test]
    fn compress_single_file_no_stat_summary() {
        let input = "diff --git a/src/a.rs b/src/a.rs\nindex abc..def 100644\n--- a/src/a.rs\n+++ b/src/a.rs\n@@ -1,2 +1,3 @@\n fn a() {\n+    // changed\n }\n";
        let result = GitDiffCompressor.compress(input, "", 0).unwrap();
        assert!(
            result.starts_with("src/a.rs\n"),
            "Single file should not have stat summary. Got:\n{}",
            result
        );
    }

    // --- whitespace collapse tests ---

    #[test]
    fn compress_whitespace_only_hunk() {
        let input = "diff --git a/src/main.rs b/src/main.rs\nindex abc..def 100644\n--- a/src/main.rs\n+++ b/src/main.rs\n@@ -1,3 +1,3 @@ fn main\n-    old_line();\n-    another();\n+        old_line();\n+        another();\n";
        let result = GitDiffCompressor.compress(input, "", 0).unwrap();
        assert!(
            result.contains("(whitespace changes)"),
            "Expected whitespace collapse in:\n{}",
            result
        );
        assert!(
            !result.contains("-    old_line"),
            "Should not contain original lines:\n{}",
            result
        );
    }

    #[test]
    fn compress_non_whitespace_hunk_preserved() {
        let input = "diff --git a/src/main.rs b/src/main.rs\nindex abc..def 100644\n--- a/src/main.rs\n+++ b/src/main.rs\n@@ -1,2 +1,2 @@ fn main\n-    old_line();\n+    new_line();\n";
        let result = GitDiffCompressor.compress(input, "", 0).unwrap();
        assert!(
            result.contains("-    old_line();"),
            "Non-whitespace hunk should be preserved:\n{}",
            result
        );
        assert!(
            !result.contains("(whitespace changes)"),
            "Should not be collapsed:\n{}",
            result
        );
    }

    // --- error handling tests ---

    #[test]
    fn compress_nonzero_exit_returns_none() {
        let result = GitDiffCompressor.compress("anything", "fatal: error", 128);
        assert_eq!(result, None);
    }

    #[test]
    fn compress_empty_diff_returns_empty_string() {
        let result = GitDiffCompressor.compress("", "", 0);
        assert_eq!(result, Some(String::new()));
    }

    #[test]
    fn compress_whitespace_only_input_returns_empty_string() {
        let result = GitDiffCompressor.compress("  \n\n  ", "", 0);
        assert_eq!(result, Some(String::new()));
    }

    #[test]
    fn compress_garbage_input_returns_none() {
        let result = GitDiffCompressor.compress("not a diff at all", "", 0);
        assert_eq!(result, None);
    }

    // --- parsing edge case tests ---

    #[test]
    fn parse_no_newline_at_end_stripped() {
        let raw = "diff --git a/file.txt b/file.txt\nindex abc..def 100644\n--- a/file.txt\n+++ b/file.txt\n@@ -1 +1 @@\n-old\n\\ No newline at end of file\n+new\n\\ No newline at end of file\n";
        let files = parse_diff(raw);
        let hunk = &files[0].hunks[0];
        assert_eq!(
            hunk.lines,
            vec![
                DiffLine::Removed("old".to_string()),
                DiffLine::Added("new".to_string()),
            ]
        );
    }
}
