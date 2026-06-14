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

/// Parse raw unified diff output into structured DiffFile entries.
pub fn parse_diff(raw: &str) -> Vec<DiffFile> {
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

    // First line is "a/path b/path" — extract path from b/ side.
    // Quoted headers (diff --git "a/..." "b/...") are declined upstream, so the
    // unquoted ` b/` split is safe here; the explicit "+++ b/" / "rename to"
    // headers below override this for the unambiguous cases.
    if let Some(first) = lines.first()
        && let Some(b_part) = unquoted_b_path(first)
    {
        path = b_part.to_string();
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
            // keep New/Deleted (set earlier from the file-mode line) so added/
            // deleted binaries aren't downgraded to a plain "(binary, changed)"
            if status == FileStatus::Normal || status == FileStatus::ModeChanged {
                status = FileStatus::Binary;
            }
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

    DiffFile {
        path,
        status,
        old_path,
        old_mode,
        new_mode,
        hunks,
    }
}

/// Extract the b/ path from a `diff --git a/<p> b/<p>` header line (the chunk
/// is everything after "diff --git ", so `first` looks like `a/<p> b/<p>`).
///
/// For non-renames git emits identical a/ and b/ halves, so we pick the ` b/`
/// separator whose two halves are equal — this tolerates paths that themselves
/// contain " b/". Renames (differing halves) and any odd shape fall back to the
/// last " b/" segment; the explicit "+++ b/" / "rename to" headers correct those.
fn unquoted_b_path(first: &str) -> Option<&str> {
    if let Some(rest) = first.strip_prefix("a/") {
        // try every " b/" as the candidate separator; the true one splits
        // rest into equal a/ and b/ halves
        for (sep_idx, _) in rest.match_indices(" b/") {
            let a_path = &rest[..sep_idx];
            let b_path = &rest[sep_idx + 3..];
            if a_path == b_path {
                return Some(b_path);
            }
        }
    }
    first.split(" b/").last()
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

    Hunk {
        old_start,
        new_start,
        function_context,
        lines: Vec::new(),
    }
}

// --- Formatting ---

/// Format a single DiffFile into compressed output.
pub fn format_file(file: &DiffFile) -> String {
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

    // indentation-significant files (Python, YAML, Makefile) must keep
    // whitespace-only hunks — leading whitespace there changes semantics
    let allow_ws_collapse = !is_whitespace_significant_path(&file.path);
    for hunk in &file.hunks {
        output.push_str(&format_hunk(hunk, allow_ws_collapse));
    }

    output
}

/// Format a hunk: compressed header + content lines.
/// `allow_ws_collapse` gates the whitespace-only collapse; pass false for
/// files where leading whitespace is semantically significant.
fn format_hunk(hunk: &Hunk, allow_ws_collapse: bool) -> String {
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
        None => output.push_str(&format!("@@ {} {} @@\n", old_part, new_part)),
    }

    // Whitespace-only collapse (skipped for indentation-significant files)
    if allow_ws_collapse && is_whitespace_only_hunk(hunk) {
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
/// After trimming leading/trailing whitespace, the removed lines equal the added lines in order.
fn is_whitespace_only_hunk(hunk: &Hunk) -> bool {
    let mut removed: Vec<String> = Vec::new();
    let mut added: Vec<String> = Vec::new();

    for line in &hunk.lines {
        match line {
            DiffLine::Removed(s) => removed.push(s.trim().to_string()),
            DiffLine::Added(s) => added.push(s.trim().to_string()),
            DiffLine::Context(_) => {}
        }
    }

    if removed.is_empty() && added.is_empty() {
        return false;
    }

    removed == added
}

/// File types where leading whitespace is semantically significant, so
/// indentation-only hunks must NOT be collapsed to "(whitespace changes)".
fn is_whitespace_significant_path(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    if matches!(name, "Makefile" | "makefile" | "GNUmakefile") {
        return true;
    }
    let ext = name.rsplit('.').next().unwrap_or("");
    matches!(ext, "py" | "pyi" | "yaml" | "yml" | "mk")
}

/// Build stat summary line for multi-file diffs.
pub fn stat_summary(files: &[DiffFile]) -> String {
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

/// Compress raw git stat output.
///
/// Replaces `++++----` bar notation with `N+ N-` counts.
/// Summary lines (`N files changed, ...`) pass through unchanged.
pub fn compress_stat(raw: &str) -> String {
    let mut output = String::new();

    for line in raw.lines() {
        if line.trim().is_empty() {
            continue;
        }

        // Summary line: "N file(s) changed, ..."
        if line.trim_start().starts_with(|c: char| c.is_ascii_digit())
            && (line.contains("file changed") || line.contains("files changed"))
        {
            output.push_str(line.trim());
            output.push('\n');
            continue;
        }

        // File stat line: " src/foo.rs | 15 ++++++------"
        if let Some(pipe_idx) = line.find(" | ") {
            let filename = line[..pipe_idx].trim();
            let after_pipe = line[pipe_idx + 3..].trim();

            // Binary stat line: "Bin <old> -> <new> bytes" — the "->" arrow
            // contains a '-' that must NOT be counted as a deletion
            if after_pipe.starts_with("Bin") {
                output.push_str(&format!("{} | {}\n", filename, after_pipe));
                continue;
            }

            // after_pipe looks like "15 ++++------" — the leading integer is the
            // TRUE total (insertions+deletions); the bar is scaled to terminal
            // width so its char count is NOT a reliable absolute count
            let total: usize = after_pipe
                .split_whitespace()
                .next()
                .and_then(|t| t.parse().ok())
                .unwrap_or(0);

            let bar_start = after_pipe.find(['+', '-']);
            let bar = match bar_start {
                Some(idx) => &after_pipe[idx..],
                None => {
                    // No bar (e.g. "0" or mode-only) — pass through trimmed
                    output.push_str(&format!("{} | {}\n", filename, after_pipe));
                    continue;
                }
            };

            let plus = bar.chars().filter(|&c| c == '+').count();
            let minus = bar.chars().filter(|&c| c == '-').count();

            // split the true total by the bar's +/- ratio (exact when unscaled,
            // best-effort proportional when git scaled the bar to width)
            let (insertions, deletions) = match (plus, minus) {
                (0, 0) => (0, 0),
                (_p, 0) => (total, 0), // bar is all '+'
                (0, _m) => (0, total), // bar is all '-'
                (p, m) => {
                    let ins = (total * p).div_ceil(p + m);
                    (ins, total - ins)
                }
            };

            let counts = match (insertions, deletions) {
                (0, 0) => "0".to_string(),
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

#[cfg(test)]
mod tests {
    use super::*;

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
    fn parse_new_binary_file_keeps_new_status() {
        // "new file mode" then "Binary files" — must stay New, not Binary
        let raw = "diff --git a/added.png b/added.png\nnew file mode 100644\nindex 0000000..eaf36c1\nBinary files /dev/null and b/added.png differ\n";
        let files = parse_diff(raw);
        assert_eq!(files[0].path, "added.png");
        assert_eq!(files[0].status, FileStatus::New);
    }

    #[test]
    fn parse_deleted_binary_file_keeps_deleted_status() {
        // "deleted file mode" then "Binary files" — must stay Deleted, not Binary
        let raw = "diff --git a/gone.png b/gone.png\ndeleted file mode 100644\nindex eaf36c1..0000000\nBinary files a/gone.png and /dev/null differ\n";
        let files = parse_diff(raw);
        assert_eq!(files[0].path, "gone.png");
        assert_eq!(files[0].status, FileStatus::Deleted);
    }

    #[test]
    fn unquoted_b_path_handles_space_b_slash_in_path() {
        // symmetric "a/P b/P" where P contains " b/" — midpoint split keeps P whole
        // (chunk passed to parse_file_chunk has "diff --git " already stripped)
        let header = "a/dir b/file.txt b/dir b/file.txt";
        assert_eq!(unquoted_b_path(header), Some("dir b/file.txt"));
    }

    #[test]
    fn unquoted_b_path_simple() {
        assert_eq!(
            unquoted_b_path("a/src/main.rs b/src/main.rs"),
            Some("src/main.rs")
        );
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

    // --- U4-1: internal space change must NOT collapse to "(whitespace changes)" ---

    #[test]
    fn internal_space_change_not_whitespace_only() {
        // Removed has internal space; added has no internal space — real content change
        let hunk = Hunk {
            old_start: 1,
            new_start: 1,
            function_context: None,
            lines: vec![
                DiffLine::Removed("    let s = \"hello world\";".to_string()),
                DiffLine::Added("    let s = \"helloworld\";".to_string()),
            ],
        };
        assert!(
            !is_whitespace_only_hunk(&hunk),
            "internal space removal should NOT be labeled whitespace-only"
        );
    }

    // --- U4-2: reordered lines must NOT collapse to "(whitespace changes)" ---

    #[test]
    fn reordered_lines_not_whitespace_only() {
        // Same lines but swapped order — NOT whitespace-only
        let hunk = Hunk {
            old_start: 1,
            new_start: 1,
            function_context: None,
            lines: vec![
                DiffLine::Removed("use std::io;".to_string()),
                DiffLine::Removed("use std::fmt;".to_string()),
                DiffLine::Added("use std::fmt;".to_string()),
                DiffLine::Added("use std::io;".to_string()),
            ],
        };
        assert!(
            !is_whitespace_only_hunk(&hunk),
            "reordered lines should NOT be labeled whitespace-only"
        );
    }

    // --- regression guard: pure indentation change STILL collapses ---

    #[test]
    fn indentation_only_still_whitespace() {
        // Leading spaces changed, content identical — must still be whitespace-only
        let hunk = Hunk {
            old_start: 1,
            new_start: 1,
            function_context: None,
            lines: vec![
                DiffLine::Removed("    foo();".to_string()),
                DiffLine::Added("        foo();".to_string()),
            ],
        };
        assert!(
            is_whitespace_only_hunk(&hunk),
            "pure indentation change should still be labeled whitespace-only"
        );
    }

    // --- U4-4: no-context hunk header must have closing @@ ---

    #[test]
    fn no_context_hunk_has_closing_at() {
        let hunk = Hunk {
            old_start: 1,
            new_start: 1,
            function_context: None,
            lines: vec![
                DiffLine::Removed("old line".to_string()),
                DiffLine::Added("new line".to_string()),
            ],
        };
        let output = format_hunk(&hunk, true);
        assert!(
            output.contains("@@ -1 +1 @@"),
            "no-context hunk header must have closing @@, got:\n{}",
            output
        );
    }

    // --- indentation-significant files must NOT collapse whitespace-only hunks ---

    #[test]
    fn whitespace_significant_path_detection() {
        assert!(is_whitespace_significant_path("src/app.py"));
        assert!(is_whitespace_significant_path("types.pyi"));
        assert!(is_whitespace_significant_path("config/deploy.yaml"));
        assert!(is_whitespace_significant_path(".github/workflows/ci.yml"));
        assert!(is_whitespace_significant_path("Makefile"));
        assert!(is_whitespace_significant_path("build/GNUmakefile"));
        assert!(is_whitespace_significant_path("rules.mk"));
        // not whitespace-significant
        assert!(!is_whitespace_significant_path("src/main.rs"));
        assert!(!is_whitespace_significant_path("README.md"));
        assert!(!is_whitespace_significant_path("script.js"));
    }

    #[test]
    fn format_hunk_keeps_indentation_change_when_collapse_disabled() {
        // Pure indentation change that WOULD collapse, but collapse is disabled
        let hunk = Hunk {
            old_start: 1,
            new_start: 1,
            function_context: None,
            lines: vec![
                DiffLine::Removed("    do_thing()".to_string()),
                DiffLine::Added("        do_thing()".to_string()),
            ],
        };
        let output = format_hunk(&hunk, false);
        assert!(
            !output.contains("(whitespace changes)"),
            "collapse disabled — must keep raw lines, got:\n{}",
            output
        );
        assert!(
            output.contains("-    do_thing()") && output.contains("+        do_thing()"),
            "both indentation variants must be preserved, got:\n{}",
            output
        );
    }

    // --- compress_stat: real-integer counts, not scaled-bar char counts ---

    #[test]
    fn compress_stat_uses_real_total_not_bar_width() {
        // git scales the bar to ~80 cols; the leading integer is the TRUE total
        let input = " big.txt | 650 ++++++++++----------\n 1 file changed, 350 insertions(+), 300 deletions(-)\n";
        let result = compress_stat(input);
        // bar here has 10 '+' and 10 '-' -> 50/50 split of 650 = 325 each
        assert!(
            result.contains("big.txt | 325+ 325-"),
            "must split real total (650) by bar ratio, not count bar chars, got:\n{}",
            result
        );
    }

    #[test]
    fn compress_stat_all_insertions_uses_real_total() {
        let input = " new.rs | 300 +++++++++++++\n 1 file changed, 300 insertions(+)\n";
        let result = compress_stat(input);
        assert!(
            result.contains("new.rs | 300+"),
            "all-'+' bar must report real total as insertions, got:\n{}",
            result
        );
    }

    #[test]
    fn compress_stat_binary_line_not_counted_as_deletion() {
        // "Bin 1024 -> 2048 bytes" — the '->' arrow must NOT become a deletion
        let input = " image.png | Bin 1024 -> 2048 bytes\n 1 file changed, 0 insertions(+), 0 deletions(-)\n";
        let result = compress_stat(input);
        assert!(
            result.contains("image.png | Bin 1024 -> 2048 bytes"),
            "binary stat line must pass through as Bin, not counted, got:\n{}",
            result
        );
        assert!(
            !result.contains("image.png | 1-"),
            "binary line must not be reported as a deletion, got:\n{}",
            result
        );
    }
}
