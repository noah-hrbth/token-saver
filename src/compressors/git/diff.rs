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

    /// Stub — returns None (no compression yet).
    fn compress(&self, _stdout: &str, _stderr: &str, _exit_code: i32) -> Option<String> {
        None
    }
}

// --- Data model ---

#[derive(Debug, PartialEq)]
enum FileStatus {
    Normal,
    New,
    Deleted,
    Renamed,
    ModeChanged,
    Binary,
}

#[derive(Debug)]
struct DiffFile {
    path: String,
    status: FileStatus,
    old_path: Option<String>,
    old_mode: Option<String>,
    new_mode: Option<String>,
    hunks: Vec<Hunk>,
}

#[derive(Debug)]
struct Hunk {
    old_start: u32,
    new_start: u32,
    function_context: Option<String>,
    lines: Vec<DiffLine>,
}

#[derive(Debug, PartialEq)]
enum DiffLine {
    Context(String),
    Added(String),
    Removed(String),
}

// --- Parsing ---

/// Parse raw unified diff output into structured DiffFile entries.
fn parse_diff(raw: &str) -> Vec<DiffFile> {
    let mut files = Vec::new();

    // Split on "diff --git " boundaries. First segment is empty/preamble — skip it.
    let chunks: Vec<&str> = raw.split("\ndiff --git ").collect();

    for (i, chunk) in chunks.iter().enumerate() {
        let chunk = if i == 0 {
            match chunk.strip_prefix("diff --git ") {
                Some(c) => c,
                None => continue,
            }
        } else {
            chunk
        };

        files.push(parse_file_chunk(chunk));
    }

    files
}

/// Parse a single file's diff chunk (everything after "diff --git ").
fn parse_file_chunk(chunk: &str) -> DiffFile {
    let lines: Vec<&str> = chunk.lines().collect();

    let mut path = String::new();
    let mut status = FileStatus::Normal;
    let mut old_path: Option<String> = None;
    let mut old_mode: Option<String> = None;
    let mut new_mode: Option<String> = None;
    let mut hunk_start_idx = None;

    // First line is "a/path b/path" — extract path from b/ side
    if let Some(first) = lines.first() {
        if let Some(b_part) = first.split(" b/").last() {
            path = b_part.to_string();
        }
    }

    for (idx, line) in lines.iter().enumerate() {
        if line.starts_with("new file mode") {
            status = FileStatus::New;
        } else if line.starts_with("deleted file mode") {
            status = FileStatus::Deleted;
        } else if let Some(from) = line.strip_prefix("rename from ") {
            old_path = Some(from.to_string());
            status = FileStatus::Renamed;
        } else if let Some(to) = line.strip_prefix("rename to ") {
            path = to.to_string();
        } else if let Some(mode) = line.strip_prefix("old mode ") {
            old_mode = Some(mode.to_string());
        } else if let Some(mode) = line.strip_prefix("new mode ") {
            new_mode = Some(mode.to_string());
            if status == FileStatus::Normal {
                status = FileStatus::ModeChanged;
            }
        } else if line.starts_with("Binary files ") {
            status = FileStatus::Binary;
        } else if let Some(p) = line.strip_prefix("+++ b/") {
            path = p.to_string();
        } else if line.starts_with("@@ ") && hunk_start_idx.is_none() {
            hunk_start_idx = Some(idx);
        }
    }

    let hunks = match hunk_start_idx {
        Some(start) => parse_hunks(&lines[start..]),
        None => Vec::new(),
    };

    DiffFile { path, status, old_path, old_mode, new_mode, hunks }
}

/// Parse hunk headers and content lines.
fn parse_hunks(lines: &[&str]) -> Vec<Hunk> {
    let mut hunks = Vec::new();
    let mut current_hunk: Option<Hunk> = None;

    for line in lines {
        if line.starts_with("@@ ") {
            if let Some(hunk) = current_hunk.take() {
                hunks.push(hunk);
            }
            current_hunk = Some(parse_hunk_header(line));
        } else if line.starts_with("\\ ") {
            // "\ No newline at end of file" — discard
            continue;
        } else if let Some(ref mut hunk) = current_hunk {
            if let Some(rest) = line.strip_prefix('+') {
                hunk.lines.push(DiffLine::Added(rest.to_string()));
            } else if let Some(rest) = line.strip_prefix('-') {
                hunk.lines.push(DiffLine::Removed(rest.to_string()));
            } else if let Some(rest) = line.strip_prefix(' ') {
                hunk.lines.push(DiffLine::Context(rest.to_string()));
            } else if line.is_empty() {
                hunk.lines.push(DiffLine::Context(String::new()));
            }
        }
    }

    if let Some(hunk) = current_hunk {
        hunks.push(hunk);
    }

    hunks
}

/// Parse "@@ -old,count +new,count @@ function_context" into a Hunk.
fn parse_hunk_header(line: &str) -> Hunk {
    let mut old_start = 0u32;
    let mut new_start = 0u32;
    let mut function_context = None;

    let content = line.strip_prefix("@@ ").unwrap_or(line);

    if let Some(end_idx) = content.find(" @@") {
        let range_part = &content[..end_idx];
        let after = content[end_idx + 3..].trim();
        if !after.is_empty() {
            function_context = Some(after.to_string());
        }

        for part in range_part.split_whitespace() {
            if let Some(old) = part.strip_prefix('-') {
                old_start = old.split(',').next().unwrap_or("0").parse().unwrap_or(0);
            } else if let Some(new) = part.strip_prefix('+') {
                new_start = new.split(',').next().unwrap_or("0").parse().unwrap_or(0);
            }
        }
    }

    Hunk { old_start, new_start, function_context, lines: Vec::new() }
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
        assert_eq!(result, vec![
            "diff", "--unified=1", "--diff-algorithm=histogram",
            "--no-ext-diff", "--no-color",
        ]);
    }

    #[test]
    fn normalized_args_with_staged() {
        let args: Vec<String> = vec!["diff".into(), "--staged".into()];
        let result = GitDiffCompressor.normalized_args(&args);
        assert_eq!(result, vec![
            "diff", "--unified=1", "--diff-algorithm=histogram",
            "--no-ext-diff", "--no-color", "--staged",
        ]);
    }

    #[test]
    fn normalized_args_with_commits() {
        let args: Vec<String> = vec!["diff".into(), "HEAD~3".into()];
        let result = GitDiffCompressor.normalized_args(&args);
        assert_eq!(result, vec![
            "diff", "--unified=1", "--diff-algorithm=histogram",
            "--no-ext-diff", "--no-color", "HEAD~3",
        ]);
    }

    #[test]
    fn normalized_args_user_override_unified() {
        let args: Vec<String> = vec!["diff".into(), "--unified=3".into()];
        let result = GitDiffCompressor.normalized_args(&args);
        assert_eq!(result, vec![
            "diff", "--unified=1", "--diff-algorithm=histogram",
            "--no-ext-diff", "--no-color", "--unified=3",
        ]);
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
        assert_eq!(hunk.lines, vec![
            DiffLine::Context("fn main() {".to_string()),
            DiffLine::Removed("    old_line();".to_string()),
            DiffLine::Added("    new_line();".to_string()),
            DiffLine::Added("    extra_line();".to_string()),
            DiffLine::Context("}".to_string()),
        ]);
    }

    #[test]
    fn parse_multiple_hunks() {
        let raw = "diff --git a/src/main.rs b/src/main.rs\nindex abc..def 100644\n--- a/src/main.rs\n+++ b/src/main.rs\n@@ -1,2 +1,3 @@ fn first\n fn first() {\n+    // added\n }\n@@ -10,2 +11,3 @@ fn second\n fn second() {\n+    // also added\n }\n";
        let files = parse_diff(raw);
        assert_eq!(files[0].hunks.len(), 2);
        assert_eq!(files[0].hunks[0].old_start, 1);
        assert_eq!(files[0].hunks[0].function_context, Some("fn first".to_string()));
        assert_eq!(files[0].hunks[1].old_start, 10);
        assert_eq!(files[0].hunks[1].function_context, Some("fn second".to_string()));
    }

    #[test]
    fn parse_no_newline_at_end_stripped() {
        let raw = "diff --git a/file.txt b/file.txt\nindex abc..def 100644\n--- a/file.txt\n+++ b/file.txt\n@@ -1 +1 @@\n-old\n\\ No newline at end of file\n+new\n\\ No newline at end of file\n";
        let files = parse_diff(raw);
        let hunk = &files[0].hunks[0];
        assert_eq!(hunk.lines, vec![
            DiffLine::Removed("old".to_string()),
            DiffLine::Added("new".to_string()),
        ]);
    }
}
