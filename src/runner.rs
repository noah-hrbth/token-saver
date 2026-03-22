use std::env;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// Find the real binary for `command_name` by walking PATH,
/// skipping any directory that matches `skip_dir`.
pub fn find_real_binary(command_name: &str, skip_dir: &Path) -> Option<PathBuf> {
    let path_var = env::var("PATH").ok()?;
    let skip_canonical = skip_dir.canonicalize().ok();

    for dir in env::split_paths(&path_var) {
        // Skip our own bin directory
        if let Some(ref skip) = skip_canonical {
            if let Ok(canonical) = dir.canonicalize() {
                if &canonical == skip {
                    continue;
                }
            }
        }

        let candidate = dir.join(command_name);
        if candidate.is_file() {
            return Some(candidate);
        }
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
    fn find_git_skipping_nonexistent_dir() {
        // Should find git even when skip_dir is some random path
        let result = find_real_binary("git", Path::new("/nonexistent/path"));
        assert!(result.is_some(), "git should be found in PATH");
        let path = result.unwrap();
        assert!(path.exists());
        assert!(path.to_string_lossy().contains("git"));
    }

    #[test]
    fn find_binary_skips_specified_dir() {
        // Find git's actual directory, then ask to skip it — should find nothing
        // (unless git is installed in multiple places)
        let first = find_real_binary("git", Path::new("/nonexistent"));
        if let Some(first_path) = first {
            let skip = first_path.parent().unwrap();
            // We can't guarantee git is installed in only one place,
            // but we can verify the returned path (if any) is NOT in skip_dir
            if let Some(second_path) = find_real_binary("git", skip) {
                assert_ne!(second_path.parent().unwrap(), skip);
            }
        }
    }

    #[test]
    fn find_nonexistent_binary() {
        let result = find_real_binary("this_binary_does_not_exist_xyz", Path::new("/nonexistent"));
        assert!(result.is_none());
    }

    #[test]
    fn execute_captured_runs_echo() {
        // Use 'echo' as a simple test — it exists on all unix systems
        let echo = find_real_binary("echo", Path::new("/nonexistent")).unwrap();
        let output = execute_captured(&echo, &["hello".to_string()]).unwrap();
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert_eq!(stdout.trim(), "hello");
    }
}
