use crate::agents::{Scope, ScriptableAgent};
use crate::shell_hook::is_token_saver_hook;
use crate::wizard;
use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Dispatch entry point for `token-saver uninstall`.
///
/// Reverses what `token-saver install` did:
/// - strips token-saver lines from `~/.zshenv` and `~/.bashrc`
/// - erases agent configs (claude/pi/codex), global and in the current repo
/// - removes the legacy `~/.token-saver/bin` binary from the install.sh era
///
/// The package-manager-installed binary itself is removed via
/// `cargo uninstall token-saver` / `brew uninstall token-saver`.
pub fn run(args: &[String]) -> i32 {
    if let Some(extra) = args.first() {
        eprintln!("token-saver uninstall: unexpected argument '{extra}'");
        return 2;
    }
    auto()
}

fn auto() -> i32 {
    let home = match env::var_os("HOME") {
        Some(h) => PathBuf::from(h),
        None => {
            eprintln!("token-saver uninstall: $HOME is not set");
            return 1;
        }
    };

    let mut errors = 0;

    for profile in [home.join(".zshenv"), home.join(".bashrc")] {
        match clean_shell_profile(&profile) {
            Ok(true) => println!("Cleaned token-saver lines from {}", profile.display()),
            Ok(false) => {}
            Err(e) => {
                eprintln!(
                    "token-saver uninstall: failed to clean {}: {e}",
                    profile.display()
                );
                errors += 1;
            }
        }
    }

    for agent in ScriptableAgent::ALL {
        erase_agent(agent, Scope::Global, &home, &mut errors);
    }

    let root = wizard::project_root();
    if root != home {
        for agent in ScriptableAgent::ALL {
            erase_agent(agent, Scope::Project, &root, &mut errors);
        }
    }

    remove_legacy_binary(&home, &mut errors);

    if errors > 0 {
        return 1;
    }

    println!();
    println!("Reload your shell to drop the wrappers (e.g. `source ~/.zshenv`).");
    0
}

/// Erase one agent target's config, printing removals. Non-fatal: errors
/// are reported and counted, cleanup continues.
fn erase_agent(agent: ScriptableAgent, scope: Scope, root: &Path, errors: &mut i32) {
    let path = agent.config_path(scope, root);
    match agent.erase(scope, root) {
        Ok(true) => println!("Removed token-saver config from {}", path.display()),
        Ok(false) => {}
        Err(e) => {
            eprintln!(
                "token-saver uninstall: failed to clean {}: {e}",
                path.display()
            );
            *errors += 1;
        }
    }
}

/// Remove the binary left by the legacy scripts/install.sh era. New installs
/// are owned by the package manager (cargo/brew), not by us.
fn remove_legacy_binary(home: &Path, errors: &mut i32) {
    let binary = home.join(".token-saver").join("bin").join("token-saver");
    match fs::remove_file(&binary) {
        Ok(()) => println!("Removed legacy binary {}", binary.display()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => {
            eprintln!(
                "token-saver uninstall: failed to remove {}: {e}",
                binary.display()
            );
            *errors += 1;
        }
    }
    // tidy up if the dirs are now empty; non-empty is fine, ignore failures
    let _ = fs::remove_dir(home.join(".token-saver").join("bin"));
    let _ = fs::remove_dir(home.join(".token-saver"));
}

/// Remove every token-saver-related line from a shell profile.
///
/// Returns `Ok(true)` if the file changed, `Ok(false)` if nothing matched
/// (or the file is missing). Lines we strip:
/// - any `# token-saver: ...` comment (current and legacy forms)
/// - any token-saver `eval` hook line (see [`is_token_saver_hook`]) — covers
///   the canonical quoted-path form
///   `eval "$('/path/to/token-saver' install zsh)"` and all legacy forms
///   (bare `token-saver install`, and `init` instead of `install` from
///   pre-rename installs). Shared with `install` so the two never diverge.
/// - any line referencing `.token-saver/bin` (the PATH export from install.sh)
/// - the legacy multi-line `if [ "$TOKEN_SAVER" = "1" ]; then ... fi` block
///   that older install.sh versions inlined into the profile
fn clean_shell_profile(path: &Path) -> io::Result<bool> {
    let raw = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(e),
    };

    let cleaned = match strip_token_saver_lines(&raw) {
        Some(s) => s,
        None => return Ok(false),
    };

    fs::write(path, cleaned)?;
    Ok(true)
}

fn strip_token_saver_lines(content: &str) -> Option<String> {
    let mut output: Vec<&str> = Vec::with_capacity(content.lines().count());
    let mut changed = false;
    let mut iter = content.lines();

    while let Some(line) = iter.next() {
        let trimmed = line.trim_start();

        // Legacy inlined block: drop everything from the marker comment
        // through the matching `fi`.
        if trimmed.starts_with("# token-saver: wrap commands") {
            changed = true;
            for inner in iter.by_ref() {
                if inner.trim() == "fi" {
                    break;
                }
            }
            continue;
        }

        if trimmed.starts_with("# token-saver:")
            || is_token_saver_hook(trimmed)
            || trimmed.contains(".token-saver/bin")
        {
            changed = true;
            continue;
        }

        output.push(line);
    }

    if !changed {
        return None;
    }

    while output.last().map(|l| l.is_empty()).unwrap_or(false) {
        output.pop();
    }

    if output.is_empty() {
        Some(String::new())
    } else {
        let mut s = output.join("\n");
        s.push('\n');
        Some(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    const HOOK_LINE: &str = "eval \"$('/opt/homebrew/bin/token-saver' init zsh)\"";
    const LEGACY_HOOK_LINE: &str = "eval \"$(token-saver init zsh)\"";

    #[test]
    fn strip_removes_current_form_with_comment() {
        let input = format!(
            "export FOO=bar\n\n# token-saver: enable wrappers when TOKEN_SAVER=1\n{HOOK_LINE}\n"
        );
        let out = strip_token_saver_lines(&input).expect("changed");
        assert_eq!(out, "export FOO=bar\n");
    }

    #[test]
    fn strip_removes_legacy_bare_form() {
        let input = format!("# user content\n{LEGACY_HOOK_LINE}\n");
        let out = strip_token_saver_lines(&input).expect("changed");
        assert_eq!(out, "# user content\n");
    }

    #[test]
    fn strip_removes_install_sh_path_export_and_eval_line() {
        let input = format!(
            "alias ll='ls -l'\n\nexport PATH=\"$HOME/.token-saver/bin:$PATH\"\n{LEGACY_HOOK_LINE}\n"
        );
        let out = strip_token_saver_lines(&input).expect("changed");
        assert_eq!(out, "alias ll='ls -l'\n");
    }

    #[test]
    fn strip_removes_legacy_multiline_block() {
        let input = "export FOO=bar\n# token-saver: wrap commands for LLM output compression\nif [ \"$TOKEN_SAVER\" = \"1\" ]; then\n    git() { /path/token-saver git \"$@\"; }\n    ls() { /path/token-saver ls \"$@\"; }\nfi\nexport BAR=baz\n";
        let out = strip_token_saver_lines(input).expect("changed");
        assert_eq!(out, "export FOO=bar\nexport BAR=baz\n");
    }

    #[test]
    fn strip_returns_none_when_nothing_matches() {
        let input = "export FOO=bar\nalias g=git\n";
        assert!(strip_token_saver_lines(input).is_none());
    }

    #[test]
    fn strip_handles_empty_input() {
        assert!(strip_token_saver_lines("").is_none());
    }

    #[test]
    fn strip_collapses_trailing_blank_lines_after_removal() {
        let input = format!("export FOO=bar\n\n{LEGACY_HOOK_LINE}\n");
        let out = strip_token_saver_lines(&input).expect("changed");
        assert_eq!(out, "export FOO=bar\n");
    }

    #[test]
    fn strip_yields_empty_string_when_only_token_saver_lines_present() {
        let input = format!("# token-saver: enable wrappers when TOKEN_SAVER=1\n{HOOK_LINE}\n");
        let out = strip_token_saver_lines(&input).expect("changed");
        assert_eq!(out, "");
    }

    #[test]
    fn clean_shell_profile_returns_false_for_missing_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("does-not-exist");
        assert!(!clean_shell_profile(&path).unwrap());
    }

    #[test]
    fn clean_shell_profile_writes_back_on_change() {
        let dir = tempdir().unwrap();
        let path = dir.path().join(".zshenv");
        fs::write(
            &path,
            format!(
                "export FOO=1\n# token-saver: enable wrappers when TOKEN_SAVER=1\n{HOOK_LINE}\n"
            ),
        )
        .unwrap();
        assert!(clean_shell_profile(&path).unwrap());
        assert_eq!(fs::read_to_string(&path).unwrap(), "export FOO=1\n");
    }
}
