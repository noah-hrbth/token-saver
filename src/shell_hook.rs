//! Shared shell-profile hook detection.
//!
//! Both `install` (dedup/upgrade) and `uninstall` (strip) must agree on what
//! counts as a token-saver `eval` hook line. Keeping the predicate here — used
//! by both — makes the sync invariant structural rather than a comment.

/// Marker comment written above the freshly-installed hook line.
pub const HOOK_MARKER: &str = "# token-saver: enable wrappers when TOKEN_SAVER=1";

/// True if `trimmed` (a leading-whitespace-stripped line) is a token-saver
/// `eval` hook, regardless of which binary path it embeds or whether it uses
/// the old `init` or new `install` subcommand.
pub fn is_token_saver_hook(trimmed: &str) -> bool {
    trimmed.starts_with("eval ")
        && trimmed.contains("token-saver")
        && (trimmed.contains(" init") || trimmed.contains(" install"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_all_generated_forms() {
        assert!(is_token_saver_hook(r#"eval "$(token-saver init zsh)""#));
        assert!(is_token_saver_hook(r#"eval "$(token-saver install zsh)""#));
        assert!(is_token_saver_hook(
            r#"eval "$('/abs/token-saver' install zsh)""#
        ));
        // no shell arg (manually-crafted profile)
        assert!(is_token_saver_hook(r#"eval "$(token-saver init)""#));
    }

    #[test]
    fn rejects_unrelated_eval_lines() {
        assert!(!is_token_saver_hook(r#"eval "$(starship init zsh)""#));
        // references token-saver but is not an eval hook
        assert!(!is_token_saver_hook(
            r#"export PATH="$HOME/.token-saver/bin:$PATH""#
        ));
    }
}
