pub mod diff;
pub mod status;

use super::Compressor;

/// Find a compressor for the given git subcommand args.
pub fn find_compressor(args: &[String]) -> Option<Box<dyn Compressor>> {
    let compressors: Vec<Box<dyn Compressor>> = vec![
        Box::new(diff::GitDiffCompressor),
        Box::new(status::GitStatusCompressor),
    ];

    for compressor in compressors {
        if compressor.can_compress(args) {
            return Some(compressor);
        }
    }
    None
}
