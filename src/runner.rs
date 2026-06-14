use std::env;
use std::ffi::{OsStr, OsString};
use std::fs;
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

    // Name-only recursion fallback, used ONLY when current_exe() can't be
    // canonicalized for an exact-path compare (self_canonical is None). We must
    // NOT derive it from self_exe's file name: in a legacy symlink install
    // current_exe() can be a command-named path (file name e.g. `git`), and that
    // name would skip every real `git` on PATH. When our canonical path is known
    // the exact-path check is authoritative, so no name fallback is needed; when
    // it's unknown, the crate name is the only identity we can match.
    let self_name = if self_canonical.is_none() {
        Some(OsString::from(env!("CARGO_PKG_NAME")))
    } else {
        None
    };

    for dir in env::split_paths(&path_var) {
        let candidate = dir.join(command_name);
        if !is_executable_file(&candidate) {
            continue;
        }

        // Skip only if this candidate IS token-saver, to avoid recursing into
        // ourselves; a real tool sharing our directory must still be returned.
        if let Ok(resolved) = candidate.canonicalize()
            && is_token_saver_binary(&resolved, self_canonical.as_deref(), self_name.as_deref())
        {
            continue;
        }

        return Some(candidate);
    }
    None
}

/// True if `candidate` is a regular file the current user can execute. Mirrors
/// the shell's execvp, which skips non-executable PATH candidates and keeps
/// searching; without this token-saver could pick a candidate the shell would
/// never run. Unix-only permission check (project targets darwin/unix).
fn is_executable_file(candidate: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    let Ok(metadata) = fs::metadata(candidate) else {
        return false;
    };
    // any of user/group/other execute bits set
    metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
}

/// True if a symlink-resolved PATH candidate is token-saver's own binary, so
/// executing it would recurse back into token-saver. Matches by canonical path
/// when we know our own. The name fallback (`self_name`) is applied only when
/// it is `Some` — i.e. current_exe() couldn't be canonicalized — so a legacy
/// command-named self path never masks a same-named real tool on PATH.
fn is_token_saver_binary(
    resolved: &Path,
    self_canonical: Option<&Path>,
    self_name: Option<&OsStr>,
) -> bool {
    self_canonical == Some(resolved) || (self_name.is_some() && resolved.file_name() == self_name)
}

/// Execute a command with the given args, capturing stdout and stderr.
pub fn execute_captured(binary: &PathBuf, args: &[String]) -> std::io::Result<Output> {
    Command::new(binary).args(args).output()
}

/// Execute a command by replacing the current process (passthrough mode).
/// This function does not return on success. Generic over the arg type so the
/// caller can pass exact `OsString` argv (preserving non-UTF-8 bytes) as well as
/// `String`.
pub fn exec_passthrough<S: AsRef<OsStr>>(binary: &PathBuf, args: &[S]) -> std::io::Result<()> {
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
        assert!(is_token_saver_binary(p, Some(p), None));
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
            Some(OsStr::new("token-saver"))
        ));
    }

    #[test]
    fn is_token_saver_binary_allows_unrelated_tool() {
        let resolved = Path::new("/opt/homebrew/bin/rg");
        let me = Path::new("/opt/homebrew/bin/token-saver");
        assert!(!is_token_saver_binary(resolved, Some(me), None));
    }

    #[test]
    fn is_token_saver_binary_name_fallback_ignored_when_canonical_known() {
        // Legacy symlink install: current_exe() resolves to a command-named path
        // (e.g. `git`). self_canonical is Some, so name fallback must be off and
        // a real, distinct `git` must NOT be treated as token-saver — otherwise
        // every `git` on PATH gets skipped and we report command-not-found.
        let resolved = Path::new("/usr/bin/git");
        let me = Path::new("/opt/homebrew/Cellar/token-saver/0.4.0/bin/git");
        assert!(!is_token_saver_binary(resolved, Some(me), None));
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
    fn find_git_when_self_exe_is_command_named() {
        // Legacy symlink install where current_exe() yields a command-named path
        // (file name `git`) that no longer exists -> can't be canonicalized, so
        // self_canonical is None and only the crate-name fallback applies, which
        // does NOT match "git". The OLD code derived self_name from self_exe's
        // file name ("git") and skipped every real git -> command-not-found, even
        // when TOKEN_SAVER is unset (find_real_binary runs before that check).
        if find_real_binary("git", Path::new("/nonexistent")).is_none() {
            return; // no git on this machine
        }
        let command_named_self = Path::new("/nonexistent/legacy/symlink/git");
        let found = find_real_binary("git", command_named_self);
        assert!(
            found.is_some(),
            "git must be found even when self_exe is named like the command"
        );
    }

    #[test]
    fn is_executable_file_true_for_echo() {
        // a real binary on PATH must read as executable
        let echo = find_real_binary("echo", Path::new("/nonexistent")).unwrap();
        assert!(is_executable_file(&echo));
    }

    #[test]
    fn is_executable_file_false_for_non_executable() {
        use std::os::unix::fs::PermissionsExt;
        // Arrange: a regular file with no execute bits
        let dir = std::env::temp_dir().join("ts_runner_perm_test");
        let _ = std::fs::create_dir_all(&dir);
        let file = dir.join("not_exec");
        std::fs::write(&file, b"#!/bin/sh\n").unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
        // Act + Assert
        assert!(!is_executable_file(&file));
        let _ = std::fs::remove_file(&file);
    }

    #[test]
    fn is_executable_file_false_for_directory() {
        // directories are not runnable PATH candidates
        assert!(!is_executable_file(Path::new("/")));
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
