pub mod find;
pub mod git;
pub mod ls;

/// Trait for command output compressors.
/// Each compressor knows how to parse a specific command's output
/// and return a compressed, LLM-friendly version.
pub trait Compressor {
    /// Can this compressor handle the given args?
    /// For git, args would be e.g. ["status", "-sb"].
    fn can_compress(&self, args: &[String]) -> bool;

    /// Normalized args to pass to the real binary for machine-parseable output.
    /// e.g., ["status", "-sb"] -> ["status", "--porcelain=v2", "--branch", "-z"]
    fn normalized_args(&self, original_args: &[String]) -> Vec<String>;

    /// Parse raw output and return compressed version.
    /// Returns None on parse failure (caller falls back to raw output).
    /// exit_code lets the compressor decide whether to skip compression on errors.
    fn compress(&self, stdout: &str, stderr: &str, exit_code: i32) -> Option<String>;
}

/// Look up a compressor for the given command and args.
/// Returns None if no compressor is registered for this command/args combo.
pub fn find_compressor(command: &str, args: &[String]) -> Option<Box<dyn Compressor>> {
    match command {
        "find" => find::find_compressor(args),
        "git" => git::find_compressor(args),
        "ls" => ls::find_compressor(args),
        _ => None,
    }
}
