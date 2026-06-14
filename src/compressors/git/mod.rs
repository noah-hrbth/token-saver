pub mod branch;
pub mod commit_parser;
pub mod diff;
pub mod diff_parser;
pub mod log;
pub mod show;
pub mod status;

use super::Compressor;

/// Find a compressor for the given git subcommand args.
pub fn find_compressor(args: &[String]) -> Option<Box<dyn Compressor>> {
    if branch::matches(args) {
        return Some(Box::new(branch::GitBranchCompressor {
            verbose: branch::is_verbose(args),
        }));
    }
    if diff::GitDiffCompressor.can_compress(args) {
        return Some(Box::new(diff::GitDiffCompressor));
    }
    let log_compressor = log::GitLogCompressor {
        user_limit: log::user_specified_count(args),
    };
    if log_compressor.can_compress(args) {
        return Some(Box::new(log_compressor));
    }
    if show::GitShowCompressor.can_compress(args) {
        return Some(Box::new(show::GitShowCompressor));
    }
    if status::GitStatusCompressor.can_compress(args) {
        return Some(Box::new(status::GitStatusCompressor));
    }
    None
}
