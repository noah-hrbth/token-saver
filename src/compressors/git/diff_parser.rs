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
