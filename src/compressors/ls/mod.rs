use crate::compressors::Compressor;

pub struct LsCompressor;

impl Compressor for LsCompressor {
    fn can_compress(&self, args: &[String]) -> bool {
        let mut has_l = false;

        for arg in args {
            if !arg.starts_with('-') {
                continue; // path argument, skip
            }
            let flags = arg.trim_start_matches('-');
            if flags.contains('R') {
                return false; // recursive — skip
            }
            if flags.contains('l') {
                has_l = true;
            }
        }

        has_l
    }

    fn normalized_args(&self, original_args: &[String]) -> Vec<String> {
        let mut paths: Vec<String> = Vec::new();
        let mut after_double_dash = false;

        for arg in original_args {
            if after_double_dash {
                paths.push(arg.clone());
            } else if arg == "--" {
                after_double_dash = true;
            } else if !arg.starts_with('-') {
                paths.push(arg.clone());
            }
        }

        let mut result = vec!["-la".to_string(), "--".to_string()];
        result.extend(paths);
        result
    }

    fn compress(&self, _stdout: &str, _stderr: &str, _exit_code: i32) -> Option<String> {
        todo!()
    }
}

/// Find a compressor for the given ls args.
/// Returns None if args don't contain `-l` or contain skip flags.
pub fn find_compressor(args: &[String]) -> Option<Box<dyn Compressor>> {
    let compressor = LsCompressor;
    if compressor.can_compress(args) {
        Some(Box::new(compressor))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn can_compress_l_flag() {
        let c = LsCompressor;
        assert!(c.can_compress(&["-l".into()]));
    }

    #[test]
    fn can_compress_la_flag() {
        let c = LsCompressor;
        assert!(c.can_compress(&["-la".into()]));
    }

    #[test]
    fn can_compress_al_flag() {
        let c = LsCompressor;
        assert!(c.can_compress(&["-al".into()]));
    }

    #[test]
    fn can_compress_lah_flag() {
        let c = LsCompressor;
        assert!(c.can_compress(&["-lah".into()]));
    }

    #[test]
    fn can_compress_l_with_path() {
        let c = LsCompressor;
        assert!(c.can_compress(&["-l".into(), "/tmp".into()]));
    }

    #[test]
    fn skip_bare_ls() {
        let c = LsCompressor;
        assert!(!c.can_compress(&[]));
    }

    #[test]
    fn skip_no_l_flag() {
        let c = LsCompressor;
        assert!(!c.can_compress(&["-a".into()]));
        assert!(!c.can_compress(&["src".into()]));
    }

    #[test]
    fn skip_recursive() {
        let c = LsCompressor;
        assert!(!c.can_compress(&["-lR".into()]));
        assert!(!c.can_compress(&["-l".into(), "-R".into()]));
    }

    #[test]
    fn normalized_args_bare_l() {
        let c = LsCompressor;
        assert_eq!(c.normalized_args(&["-l".into()]), vec!["-la", "--"]);
    }

    #[test]
    fn normalized_args_strips_extra_flags() {
        let c = LsCompressor;
        assert_eq!(c.normalized_args(&["-lh".into()]), vec!["-la", "--"]);
    }

    #[test]
    fn normalized_args_preserves_paths() {
        let c = LsCompressor;
        assert_eq!(
            c.normalized_args(&["-la".into(), "src".into(), "tests".into()]),
            vec!["-la", "--", "src", "tests"]
        );
    }

    #[test]
    fn normalized_args_path_after_double_dash() {
        let c = LsCompressor;
        assert_eq!(
            c.normalized_args(&["-l".into(), "--".into(), "-weird-name".into()]),
            vec!["-la", "--", "-weird-name"]
        );
    }
}
