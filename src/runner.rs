use std::env;
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

    for dir in env::split_paths(&path_var) {
        let candidate = dir.join(command_name);
        if !candidate.is_file() {
            continue;
        }

        // Skip only if this candidate IS our own binary, to avoid recursing
        // into token-saver. Resolve symlinks so a wrapper symlink is caught.
        if let (Some(me), Ok(resolved)) = (&self_canonical, candidate.canonicalize())
            && &resolved == me
        {
            continue;
        }

        return Some(candidate);
    }
    None
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
    fn find_binary_skips_only_self_exe() {
        // If token-saver's own exe path IS the binary we resolve (the legacy
        // wrapper-symlink recursion case), that exact path must be skipped —
        // we never return our own binary as the "real" one.
        let Some(git) = find_real_binary("git", Path::new("/nonexistent")) else {
            return; // no git on this machine
        };
        if let Some(found) = find_real_binary("git", &git) {
            assert_ne!(found, git, "must not return the skipped self exe");
        }
    }

    #[test]
    fn find_binary_in_self_exe_dir_is_not_skipped() {
        // A real tool that shares token-saver's directory must still be found.
        // Resolve git, then pretend token-saver lives in the same dir under a
        // different name: the sibling git must still resolve.
        let Some(git) = find_real_binary("git", Path::new("/nonexistent")) else {
            return; // no git on this machine
        };
        let fake_self = git.parent().unwrap().join("token-saver");
        let found = find_real_binary("git", &fake_self);
        assert_eq!(
            found.as_deref(),
            Some(git.as_path()),
            "sharing a directory with token-saver must not exclude a real tool"
        );
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
