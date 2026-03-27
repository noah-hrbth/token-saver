pub mod diff;
pub mod diff_parser;
pub mod log;
pub mod status;

use super::Compressor;

/// Find a compressor for the given git subcommand args.
pub fn find_compressor(args: &[String]) -> Option<Box<dyn Compressor>> {
    let compressors: Vec<Box<dyn Compressor>> = vec![
        Box::new(diff::GitDiffCompressor),
        Box::new(log::GitLogCompressor),
        Box::new(status::GitStatusCompressor),
    ];

    compressors
        .into_iter()
        .find(|compressor| compressor.can_compress(args))
}
