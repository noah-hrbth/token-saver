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
