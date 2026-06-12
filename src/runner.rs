use std::env;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// Find the real binary for `command_name` by walking PATH.
///
/// `self_exe` is the path to token-saver's own executable. We skip any PATH
/// candidate that resolves to it — e.g. a legacy wrapper symlink named `git`
/// that points back at token-saver — so we never re-invoke ourselves.
///
/// We deliberately do NOT skip token-saver's whole directory: when token-saver
/// and the real tool are installed side by side (e.g. both under a Homebrew
/// prefix like `/opt/homebrew/bin`, or `rg` installed via brew next to a
/// brew-installed token-saver) the real tool lives in that same directory and
/// must still be found. Skipping the directory wholesale made such tools
/// report `command not found` even though they were installed and runnable.
pub fn find_real_binary(command_name: &str, self_exe: &Path) -> Option<PathBuf> {
    let path_var = env::var("PATH").ok()?;
    let self_canonical = self_exe.canonicalize().ok();

    // The on-disk file name token-saver's binary always has. A wrapper symlink
    // (e.g. `rg` -> token-saver) resolves to a file with this name, so matching
    // it guards against recursing into ourselves even when current_exe() is
    // unavailable for an exact-path comparison. Falls back to the crate name
    // when self_exe has no file name (e.g. an empty default path).
    let self_name = self_exe
        .file_name()
        .map(OsStr::to_os_string)
        .unwrap_or_else(|| OsString::from(env!("CARGO_PKG_NAME")));

    for dir in env::split_paths(&path_var) {
        let candidate = dir.join(command_name);
        if !candidate.is_file() {
            continue;
        }

        // Skip only if this candidate IS token-saver, to avoid recursing into
        // ourselves; a real tool sharing our directory must still be returned.
        if let Ok(resolved) = candidate.canonicalize()
            && is_token_saver_binary(&resolved, self_canonical.as_deref(), &self_name)
        {
            continue;
        }

        return Some(candidate);
    }
    None
}

/// True if a symlink-resolved PATH candidate is token-saver's own binary, so
/// executing it would recurse back into token-saver. Matches by canonical path
/// when we know our own, and always also by the binary's file name so the guard
/// still holds when current_exe() couldn't be resolved to a canonical path.
fn is_token_saver_binary(
    resolved: &Path,
    self_canonical: Option<&Path>,
    self_name: &OsStr,
) -> bool {
    self_canonical == Some(resolved) || resolved.file_name() == Some(self_name)
}

/// Execute a command with the given args, capturing stdout and stderr.
pub fn execute_captured(binary: &PathBuf, args: &[String]) -> std::io::Result<Output> {
    Command::new(binary).args(args).output()
}

/// Execute a command by replacing the current process (passthrough mode).
/// This function does not return on success.
pub fn exec_passthrough(binary: &PathBuf, args: &[String]) -> std::io::Result<()> {
    use std::os::unix::process::CommandExt;
    // exec replaces the current process — does not return on success
    let err = Command::new(binary).args(args).exec();
    Err(err)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn find_git_when_self_exe_unrelated() {
        // Should find git when our own exe is some unrelated path.
        let result = find_real_binary("git", Path::new("/nonexistent/token-saver"));
        assert!(result.is_some(), "git should be found in PATH");
        let path = result.unwrap();
        assert!(path.exists());
        assert!(path.to_string_lossy().contains("git"));
    }

    #[test]
    fn find_real_tool_is_not_skipped_with_real_self_exe() {
        // A real tool must still be found when self_exe is a real, distinct,
        // canonicalizable binary. Using current_exe() (the test binary) means
        // self_canonical is Some(..) and the file name is not "git", so the
        // identity check actually runs and returns false — guarding the
        // brew-shared-directory regression (a passthrough self_exe disabled it).
        let Some(git) = find_real_binary("git", Path::new("/nonexistent")) else {
            return; // no git on this machine
        };
        let real_self = std::env::current_exe().expect("test exe path");
        let found = find_real_binary("git", &real_self);
        assert_eq!(
            found.as_deref(),
            Some(git.as_path()),
            "a real, unrelated self exe must not exclude a real tool"
        );
    }

    #[test]
    fn is_token_saver_binary_matches_exact_path() {
        let p = Path::new("/opt/homebrew/bin/token-saver");
        assert!(is_token_saver_binary(p, Some(p), OsStr::new("token-saver")));
    }

    #[test]
    fn is_token_saver_binary_matches_by_name_when_self_path_unknown() {
        // current_exe() unavailable (self_canonical = None): a wrapper symlink
        // still resolves to a file named token-saver and must be caught, so the
        // recursion guard holds without an exact-path comparison.
        let resolved = Path::new("/opt/homebrew/Cellar/token-saver/0.4.0/bin/token-saver");
        assert!(is_token_saver_binary(
            resolved,
            None,
            OsStr::new("token-saver")
        ));
    }

    #[test]
    fn is_token_saver_binary_allows_unrelated_tool() {
        let resolved = Path::new("/opt/homebrew/bin/rg");
        let me = Path::new("/opt/homebrew/bin/token-saver");
        assert!(!is_token_saver_binary(
            resolved,
            Some(me),
            OsStr::new("token-saver")
        ));
    }

    #[test]
    fn find_nonexistent_binary() {
        let result = find_real_binary(
            "this_binary_does_not_exist_xyz",
            Path::new("/nonexistent/token-saver"),
        );
        assert!(result.is_none());
    }

    #[test]
    fn execute_captured_runs_echo() {
        // Use 'echo' as a simple test — it exists on all unix systems
        let echo = find_real_binary("echo", Path::new("/nonexistent/token-saver")).unwrap();
        let output = execute_captured(&echo, &["hello".to_string()]).unwrap();
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert_eq!(stdout.trim(), "hello");
    }
}
