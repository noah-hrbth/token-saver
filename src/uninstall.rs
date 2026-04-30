use serde_json::Value;
use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Dispatch entry point for `token-saver uninstall`.
///
/// Reverses what `token-saver init` did:
/// - strips token-saver lines from `~/.zshenv` and `~/.bashrc`
/// - removes `TOKEN_SAVER` from `~/.claude/settings.json`
///
/// The binary itself is removed by `scripts/uninstall.sh` (a process can't
/// reliably delete its own executable across platforms, and we want this
/// subcommand to be safe to call repeatedly).
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

    let settings = home.join(".claude").join("settings.json");
    match clean_claude_settings(&settings) {
        Ok(true) => println!("Removed TOKEN_SAVER from {}", settings.display()),
        Ok(false) => {}
        Err(e) => {
            eprintln!(
                "token-saver uninstall: failed to clean {}: {e}",
                settings.display()
            );
            errors += 1;
        }
    }

    if errors > 0 {
        return 1;
    }

    println!();
    println!("Reload your shell to drop the wrappers (e.g. `source ~/.zshenv`).");
    0
}

/// Remove every token-saver-related line from a shell profile.
///
/// Returns `Ok(true)` if the file changed, `Ok(false)` if nothing matched
/// (or the file is missing). Lines we strip:
/// - any `# token-saver: ...` comment (current and legacy forms)
/// - any `eval` line that references `token-saver` — covers both the
///   current quoted-path form `eval "$('/path/to/token-saver' init zsh)"`
///   and the legacy bare form `eval "$(token-saver init zsh)"`
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
            || (trimmed.starts_with("eval ") && trimmed.contains("token-saver"))
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

/// Remove `TOKEN_SAVER` from the `env` object in `~/.claude/settings.json`.
///
/// Drops the surrounding `env` key entirely if it becomes empty so we don't
/// leave dangling structure behind. Returns `Ok(false)` when the file is
/// missing, empty, or the key wasn't there.
fn clean_claude_settings(path: &Path) -> io::Result<bool> {
    let raw = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(e),
    };

    if raw.trim().is_empty() {
        return Ok(false);
    }

    let mut value: Value = serde_json::from_str(&raw).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("parse settings.json: {e}"),
        )
    })?;

    let obj = match value.as_object_mut() {
        Some(o) => o,
        None => return Ok(false),
    };

    let env_obj = match obj.get_mut("env").and_then(Value::as_object_mut) {
        Some(o) => o,
        None => return Ok(false),
    };

    if env_obj.remove("TOKEN_SAVER").is_none() {
        return Ok(false);
    }

    if env_obj.is_empty() {
        obj.remove("env");
    }

    let serialized = serde_json::to_string_pretty(&value).expect("Value always serializes");
    fs::write(path, format!("{serialized}\n"))?;
    Ok(true)
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

    #[test]
    fn clean_claude_settings_removes_token_saver_and_preserves_rest() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(
            &path,
            r#"{ "model": "sonnet", "env": { "OTHER": "value", "TOKEN_SAVER": "1" } }"#,
        )
        .unwrap();
        assert!(clean_claude_settings(&path).unwrap());
        let value: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(value["model"], "sonnet");
        assert_eq!(value["env"]["OTHER"], "value");
        assert!(value["env"].get("TOKEN_SAVER").is_none());
    }

    #[test]
    fn clean_claude_settings_drops_empty_env_object() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(
            &path,
            r#"{ "model": "sonnet", "env": { "TOKEN_SAVER": "1" } }"#,
        )
        .unwrap();
        assert!(clean_claude_settings(&path).unwrap());
        let value: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(value["model"], "sonnet");
        assert!(value.as_object().unwrap().get("env").is_none());
    }

    #[test]
    fn clean_claude_settings_returns_false_when_key_absent() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let original = "{\n  \"model\": \"sonnet\"\n}\n";
        fs::write(&path, original).unwrap();
        assert!(!clean_claude_settings(&path).unwrap());
        assert_eq!(fs::read_to_string(&path).unwrap(), original);
    }

    #[test]
    fn clean_claude_settings_returns_false_for_missing_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("settings.json");
        assert!(!clean_claude_settings(&path).unwrap());
    }

    #[test]
    fn clean_claude_settings_returns_false_for_empty_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(&path, "").unwrap();
        assert!(!clean_claude_settings(&path).unwrap());
    }

    #[test]
    fn clean_claude_settings_rejects_invalid_json() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(&path, "{ not json").unwrap();
        let err = clean_claude_settings(&path).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }
}
