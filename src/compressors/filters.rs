/// Directories/files that are noise for LLM agents. Shared by find and grep compressors.
const NOISE_DIRS: &[&str] = &[".git", "__pycache__"];

/// Returns true if the path is inside a noise directory or matches a noise file pattern.
/// Works for both full paths (`src/.git/config`) and bare names (`.git`).
pub fn should_filter(path: &str) -> bool {
    for dir in NOISE_DIRS {
        if path == *dir
            || path.starts_with(&format!("{}/", dir))
            || path.contains(&format!("/{}/", dir))
            || path.ends_with(&format!("/{}", dir))
        {
            return true;
        }
    }

    if path == ".DS_Store" || path.ends_with("/.DS_Store") {
        return true;
    }

    if path.ends_with(".pyc") {
        return true;
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters_git_root() {
        assert!(should_filter(".git"));
        assert!(should_filter(".git/config"));
        assert!(should_filter(".git/objects/abc123"));
    }

    #[test]
    fn filters_nested_git() {
        assert!(should_filter("vendor/repo/.git"));
        assert!(should_filter("vendor/repo/.git/config"));
    }

    #[test]
    fn filters_pycache() {
        assert!(should_filter("__pycache__"));
        assert!(should_filter("__pycache__/foo.pyc"));
        assert!(should_filter("src/__pycache__/bar.pyc"));
        assert!(should_filter("app/__pycache__"));
    }

    #[test]
    fn filters_ds_store() {
        assert!(should_filter(".DS_Store"));
        assert!(should_filter("subdir/.DS_Store"));
    }

    #[test]
    fn filters_pyc() {
        assert!(should_filter("src/foo.pyc"));
    }

    #[test]
    fn keeps_normal_paths() {
        assert!(!should_filter("src/main.rs"));
        assert!(!should_filter("Cargo.toml"));
        assert!(!should_filter("src/git/handler.rs"));
        assert!(!should_filter("node_modules/foo.js"));
    }
}
