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

    DiffFile {
        path,
        status,
        old_path,
        old_mode,
        new_mode,
        hunks,
    }
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
}
