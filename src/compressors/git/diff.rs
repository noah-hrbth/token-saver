use crate::compressors::Compressor;

pub struct GitDiffCompressor;

const SKIP_FLAGS: &[&str] = &[
    "--stat",
    "--name-only",
    "--name-status",
    "--raw",
    "--word-diff",
    "--check",
    "--summary",
    "--shortstat",
    "--numstat",
    "--submodule",
    "--color",
    "--color=always",
    "--ext-diff",
];

impl Compressor for GitDiffCompressor {
    /// Returns true when the first arg is exactly "diff" (not diff-tree, etc.)
    /// and no skip flags are present in the remaining args.
    fn can_compress(&self, args: &[String]) -> bool {
        if args.first().map(|s| s.as_str()) != Some("diff") {
            return false;
        }

        let tail = &args[1..];
        !tail.iter().any(|arg| SKIP_FLAGS.contains(&arg.as_str()))
    }

    fn normalized_args(&self, original_args: &[String]) -> Vec<String> {
        let mut result = vec![
            "diff".to_string(),
            "--unified=1".to_string(),
            "--diff-algorithm=histogram".to_string(),
            "--no-ext-diff".to_string(),
            "--no-color".to_string(),
        ];
        result.extend(original_args[1..].iter().cloned());
        result
    }

    /// Stub — returns None (no compression yet).
    fn compress(&self, _stdout: &str, _stderr: &str, _exit_code: i32) -> Option<String> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(strs: &[&str]) -> Vec<String> {
        strs.iter().map(|s| s.to_string()).collect()
    }

    // --- positive cases ---

    #[test]
    fn can_compress_bare_diff() {
        assert!(GitDiffCompressor.can_compress(&args(&["diff"])));
    }

    #[test]
    fn can_compress_diff_staged() {
        assert!(GitDiffCompressor.can_compress(&args(&["diff", "--staged"])));
    }

    #[test]
    fn can_compress_diff_cached() {
        assert!(GitDiffCompressor.can_compress(&args(&["diff", "--cached"])));
    }

    #[test]
    fn can_compress_diff_commits() {
        assert!(GitDiffCompressor.can_compress(&args(&["diff", "HEAD~3"])));
    }

    #[test]
    fn can_compress_diff_branches() {
        assert!(GitDiffCompressor.can_compress(&args(&["diff", "main..feature"])));
    }

    // --- skip flags ---

    #[test]
    fn skip_diff_stat() {
        assert!(!GitDiffCompressor.can_compress(&args(&["diff", "--stat"])));
    }

    #[test]
    fn skip_diff_name_only() {
        assert!(!GitDiffCompressor.can_compress(&args(&["diff", "--name-only"])));
    }

    #[test]
    fn skip_diff_name_status() {
        assert!(!GitDiffCompressor.can_compress(&args(&["diff", "--name-status"])));
    }

    #[test]
    fn skip_diff_raw() {
        assert!(!GitDiffCompressor.can_compress(&args(&["diff", "--raw"])));
    }

    #[test]
    fn skip_diff_word_diff() {
        assert!(!GitDiffCompressor.can_compress(&args(&["diff", "--word-diff"])));
    }

    #[test]
    fn skip_diff_check() {
        assert!(!GitDiffCompressor.can_compress(&args(&["diff", "--check"])));
    }

    #[test]
    fn skip_diff_summary() {
        assert!(!GitDiffCompressor.can_compress(&args(&["diff", "--summary"])));
    }

    #[test]
    fn skip_diff_shortstat() {
        assert!(!GitDiffCompressor.can_compress(&args(&["diff", "--shortstat"])));
    }

    #[test]
    fn skip_diff_numstat() {
        assert!(!GitDiffCompressor.can_compress(&args(&["diff", "--numstat"])));
    }

    #[test]
    fn skip_diff_submodule() {
        assert!(!GitDiffCompressor.can_compress(&args(&["diff", "--submodule"])));
    }

    #[test]
    fn skip_diff_color() {
        assert!(!GitDiffCompressor.can_compress(&args(&["diff", "--color"])));
    }

    #[test]
    fn skip_diff_color_always() {
        assert!(!GitDiffCompressor.can_compress(&args(&["diff", "--color=always"])));
    }

    #[test]
    fn skip_diff_ext_diff() {
        assert!(!GitDiffCompressor.can_compress(&args(&["diff", "--ext-diff"])));
    }

    // --- normalized_args ---

    #[test]
    fn normalized_args_bare_diff() {
        let args: Vec<String> = vec!["diff".into()];
        let result = GitDiffCompressor.normalized_args(&args);
        assert_eq!(result, vec![
            "diff", "--unified=1", "--diff-algorithm=histogram",
            "--no-ext-diff", "--no-color",
        ]);
    }

    #[test]
    fn normalized_args_with_staged() {
        let args: Vec<String> = vec!["diff".into(), "--staged".into()];
        let result = GitDiffCompressor.normalized_args(&args);
        assert_eq!(result, vec![
            "diff", "--unified=1", "--diff-algorithm=histogram",
            "--no-ext-diff", "--no-color", "--staged",
        ]);
    }

    #[test]
    fn normalized_args_with_commits() {
        let args: Vec<String> = vec!["diff".into(), "HEAD~3".into()];
        let result = GitDiffCompressor.normalized_args(&args);
        assert_eq!(result, vec![
            "diff", "--unified=1", "--diff-algorithm=histogram",
            "--no-ext-diff", "--no-color", "HEAD~3",
        ]);
    }

    #[test]
    fn normalized_args_user_override_unified() {
        let args: Vec<String> = vec!["diff".into(), "--unified=3".into()];
        let result = GitDiffCompressor.normalized_args(&args);
        assert_eq!(result, vec![
            "diff", "--unified=1", "--diff-algorithm=histogram",
            "--no-ext-diff", "--no-color", "--unified=3",
        ]);
    }

    // --- non-diff commands ---

    #[test]
    fn skip_non_diff_commands() {
        assert!(!GitDiffCompressor.can_compress(&args(&["status"])));
        assert!(!GitDiffCompressor.can_compress(&args(&["log"])));
        assert!(!GitDiffCompressor.can_compress(&args(&["diff-tree"])));
        assert!(!GitDiffCompressor.can_compress(&args(&["diff-files"])));
        assert!(!GitDiffCompressor.can_compress(&args(&["diff-index"])));
        assert!(!GitDiffCompressor.can_compress(&args(&[])));
    }
}
