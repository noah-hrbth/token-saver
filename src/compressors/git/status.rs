use crate::compressors::Compressor;

pub struct GitStatusCompressor;

impl Compressor for GitStatusCompressor {
    fn can_compress(&self, args: &[String]) -> bool {
        args.first().map(|s| s.as_str()) == Some("status")
    }

    fn normalized_args(&self, _original_args: &[String]) -> Vec<String> {
        vec![
            "status".to_string(),
            "--porcelain=v2".to_string(),
            "--branch".to_string(),
            "-z".to_string(),
        ]
    }

    fn compress(&self, _stdout: &str, _stderr: &str, _exit_code: i32) -> Option<String> {
        todo!()
    }
}
