use crate::compressors::Compressor;

/// Maximum number of lines before truncation.
const MAX_LINES: usize = 1000;

/// Lines longer than this threshold are treated as minified content.
const MINIFIED_THRESHOLD: usize = 2000;

/// Number of characters to show as preview for minified lines.
const MINIFIED_PREVIEW_LEN: usize = 200;

/// Number of bytes to scan for NUL bytes when detecting binary content.
const BINARY_CHECK_LEN: usize = 8192;

pub struct CatCompressor;

impl Compressor for CatCompressor {
    fn can_compress(&self, args: &[String]) -> bool {
        has_file_args(args)
    }

    fn normalized_args(&self, original_args: &[String]) -> Vec<String> {
        original_args.to_vec()
    }

    fn compress(&self, stdout: &str, stderr: &str, exit_code: i32) -> Option<String> {
        if exit_code != 0 {
            return None;
        }

        if stdout.is_empty() {
            return Some(String::new());
        }

        // Binary detection: check first 8KB for NUL bytes
        if is_binary(stdout) {
            return Some(format!("(binary content, {} bytes)", stdout.len()));
        }

        let mut output = String::new();
        let lines: Vec<&str> = stdout.lines().collect();

        for line in lines.iter().take(MAX_LINES) {
            if line.len() > MINIFIED_THRESHOLD {
                let preview: String = line.chars().take(MINIFIED_PREVIEW_LEN).collect();
                if !output.is_empty() {
                    output.push('\n');
                }
                output.push_str(&format!(
                    "{}... (line is {} chars, likely minified)",
                    preview,
                    line.len()
                ));
            } else {
                if !output.is_empty() {
                    output.push('\n');
                }
                output.push_str(line);
            }
        }

        let processed = lines.len().min(MAX_LINES);
        let remaining = lines.len() - processed;
        if remaining > 0 {
            output.push_str(&format!("\n... {} more lines", remaining));
        }

        // Append stderr if present (e.g. permission errors on some files)
        if !stderr.is_empty() {
            output.push_str("\nerrors:");
            for line in stderr.lines() {
                output.push_str(&format!("\n  {}", line));
            }
        }

        Some(output)
    }
}

/// Returns true if any argument looks like a file path (not a flag).
/// A bare `-` is treated as stdin, not a file.
fn has_file_args(args: &[String]) -> bool {
    args.iter().any(|arg| !arg.starts_with('-'))
}

/// Check the first `BINARY_CHECK_LEN` bytes of stdout for NUL bytes.
fn is_binary(stdout: &str) -> bool {
    let check_len = stdout.len().min(BINARY_CHECK_LEN);
    stdout.as_bytes()[..check_len].contains(&0)
}

/// Find a compressor for the given cat args.
pub fn find_compressor(args: &[String]) -> Option<Box<dyn Compressor>> {
    let compressor = CatCompressor;
    if compressor.can_compress(args) {
        Some(Box::new(compressor))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn compress(input: &str) -> Option<String> {
        CatCompressor.compress(input, "", 0)
    }

    // --- can_compress ---

    #[test]
    fn test_can_compress_with_file() {
        assert!(CatCompressor.can_compress(&["file.txt".into()]));
    }

    #[test]
    fn test_can_compress_with_flag_and_file() {
        assert!(CatCompressor.can_compress(&["-n".into(), "file.txt".into()]));
    }

    #[test]
    fn test_can_compress_flags_only() {
        assert!(!CatCompressor.can_compress(&["-n".into()]));
    }

    #[test]
    fn test_can_compress_empty_args() {
        assert!(!CatCompressor.can_compress(&[]));
    }

    #[test]
    fn test_can_compress_stdin_dash() {
        assert!(!CatCompressor.can_compress(&["-".into()]));
    }

    #[test]
    fn test_can_compress_stdin_dash_with_flag() {
        assert!(!CatCompressor.can_compress(&["-n".into(), "-".into()]));
    }

    #[test]
    fn test_can_compress_multiple_files() {
        assert!(CatCompressor.can_compress(&["a.txt".into(), "b.txt".into()]));
    }

    // --- normalized_args ---

    #[test]
    fn test_normalized_args_passthrough() {
        let args: Vec<String> = vec!["-n".into(), "file.txt".into()];
        assert_eq!(CatCompressor.normalized_args(&args), args);
    }

    // --- compress: basic ---

    #[test]
    fn test_compress_basic() {
        let result = compress("hello\nworld\n");
        assert_eq!(result, Some("hello\nworld".to_string()));
    }

    #[test]
    fn test_compress_preserves_content() {
        let input = "line 1\nline 2\nline 3";
        let result = compress(input);
        assert_eq!(result, Some("line 1\nline 2\nline 3".to_string()));
    }

    // --- compress: empty ---

    #[test]
    fn test_compress_empty() {
        let result = compress("");
        assert_eq!(result, Some(String::new()));
    }

    // --- compress: non-zero exit ---

    #[test]
    fn test_compress_nonzero_exit() {
        let result = CatCompressor.compress("content", "cat: no such file", 1);
        assert_eq!(result, None);
    }

    // --- compress: binary ---

    #[test]
    fn test_compress_binary_nul_bytes() {
        let input = "ELF\x00\x01\x02binary content";
        let result = compress(input);
        let s = result.unwrap();
        assert!(s.starts_with("(binary content, "));
        assert!(s.ends_with(" bytes)"));
    }

    #[test]
    fn test_compress_no_false_binary() {
        let input = "normal text\nno binary here\n";
        let result = compress(input);
        let s = result.unwrap();
        assert!(!s.contains("binary content"));
    }

    // --- compress: minified lines ---

    #[test]
    fn test_compress_minified_line() {
        let long_line: String = "a".repeat(5000);
        let result = compress(&long_line);
        let s = result.unwrap();
        assert!(s.contains("likely minified"));
        assert!(s.contains("5000 chars"));
        // Preview should be 200 chars of 'a' + the notice
        assert!(s.starts_with(&"a".repeat(200)));
    }

    #[test]
    fn test_compress_just_under_minified_threshold() {
        let line: String = "b".repeat(2000);
        let result = compress(&line);
        let s = result.unwrap();
        assert!(!s.contains("likely minified"));
        assert_eq!(s, line);
    }

    #[test]
    fn test_compress_exactly_at_minified_threshold() {
        let line: String = "c".repeat(2001);
        let result = compress(&line);
        let s = result.unwrap();
        assert!(s.contains("likely minified"));
        assert!(s.contains("2001 chars"));
    }

    // --- compress: line cap ---

    #[test]
    fn test_compress_at_cap() {
        let input: String = (0..1000).map(|i| format!("line {}\n", i)).collect();
        let result = compress(&input);
        let s = result.unwrap();
        assert!(!s.contains("more lines"));
        assert!(s.contains("line 0"));
        assert!(s.contains("line 999"));
    }

    #[test]
    fn test_compress_over_cap() {
        let input: String = (0..1500).map(|i| format!("line {}\n", i)).collect();
        let result = compress(&input);
        let s = result.unwrap();
        assert!(s.contains("... 500 more lines"));
        assert!(s.contains("line 0"));
        assert!(s.contains("line 999"));
        assert!(!s.contains("line 1000"));
    }

    // --- compress: stderr ---

    #[test]
    fn test_compress_stderr_appended() {
        let result = CatCompressor.compress("hello\n", "cat: error reading file\n", 0);
        let s = result.unwrap();
        assert!(s.contains("hello"));
        assert!(s.contains("errors:"));
        assert!(s.contains("  cat: error reading file"));
    }

    // --- compress: mixed minified and normal ---

    #[test]
    fn test_compress_mixed_minified_and_normal() {
        let long_line: String = "x".repeat(3000);
        let input = format!("normal line\n{}\nanother normal line\n", long_line);
        let result = compress(&input);
        let s = result.unwrap();
        assert!(s.contains("normal line"));
        assert!(s.contains("another normal line"));
        assert!(s.contains("likely minified"));
        assert!(s.contains("3000 chars"));
    }
}
