use crate::shell_hook::{HOOK_MARKER, is_token_saver_hook};
use serde_json::{Map, Value};
use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

const COMMANDS: &[&str] = &[
    "cat", "eslint", "git", "jest", "ls", "find", "grep", "npx", "prettier", "rg", "tsc",
];

/// Dispatch entry point for `token-saver install [shell]`.
///
/// - `install` (no args): auto-detect shell, edit profile, edit `~/.claude/settings.json`.
/// - `install zsh|bash`: print the shell-function block (for `eval "$(...)"` use).
pub fn run(args: &[String]) -> i32 {
    let binary = current_binary_path();
    match args.first().map(String::as_str) {
        None => auto(&binary),
        Some(shell) => print_block(shell, &binary),
    }
}

/// Resolve the path we should write into shell config.
///
/// Prefers a PATH-resolved location (e.g. `/opt/homebrew/bin/token-saver`) so the
/// recorded path remains valid across `brew upgrade`. Falls back to
/// `env::current_exe()` when not on PATH.
fn current_binary_path() -> String {
    if let Some(p) = find_in_path("token-saver")
        && let Some(s) = p.to_str()
    {
        return s.to_string();
    }
    env::current_exe()
        .ok()
        .and_then(|p| p.to_str().map(String::from))
        .unwrap_or_else(|| "token-saver".to_string())
}

fn find_in_path(name: &str) -> Option<PathBuf> {
    let path_var = env::var_os("PATH")?;
    for dir in env::split_paths(&path_var) {
        let candidate = dir.join(name);
        if is_executable(&candidate) {
            return Some(candidate);
        }
    }
    None
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    fs::metadata(path)
        .map(|m| m.is_file() && (m.permissions().mode() & 0o111) != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    fs::metadata(path).map(|m| m.is_file()).unwrap_or(false)
}

/// POSIX-safe single-quote escaping. Wraps in single quotes; a literal `'`
/// becomes `'\''` (close, escaped quote, reopen).
fn shell_single_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

fn print_block(shell: &str, binary: &str) -> i32 {
    match shell {
        "zsh" | "bash" => {
            let bin = shell_single_quote(binary);
            println!("# token-saver: wrap commands for LLM output compression");
            println!("# Loads only when TOKEN_SAVER=1 — no-op otherwise.");
            println!("if [ \"$TOKEN_SAVER\" = \"1\" ]; then");
            for cmd in COMMANDS {
                println!("    {cmd}() {{ {bin} {cmd} \"$@\"; }}");
            }
            println!("fi");
            0
        }
        "" => {
            eprintln!("token-saver install: missing shell argument (zsh|bash)");
            2
        }
        other => {
            eprintln!("token-saver install: unsupported shell '{other}' (supported: zsh, bash)");
            2
        }
    }
}

fn auto(binary: &str) -> i32 {
    let shell = match detect_shell() {
        Some(s) => s,
        None => {
            eprintln!(
                "token-saver install: could not detect a supported shell from $SHELL (need zsh or bash)"
            );
            eprintln!(
                "Run `token-saver install zsh` or `token-saver install bash` and add the eval line to your profile manually."
            );
            return 1;
        }
    };

    let home = match env::var_os("HOME") {
        Some(h) => PathBuf::from(h),
        None => {
            eprintln!("token-saver install: $HOME is not set");
            return 1;
        }
    };

    let profile = profile_path(&home, &shell);
    if let Err(e) = update_shell_profile(&profile, &shell, binary) {
        eprintln!(
            "token-saver install: failed to update {}: {e}",
            profile.display()
        );
        return 1;
    }

    let settings = home.join(".claude").join("settings.json");
    if let Err(e) = update_claude_settings(&settings) {
        eprintln!(
            "token-saver install: failed to update {}: {e}",
            settings.display()
        );
        return 1;
    }

    println!();
    println!("Done. Reload your shell to pick up the wrappers:");
    println!("    source {}", profile.display());
    0
}

fn detect_shell() -> Option<String> {
    let shell = env::var("SHELL").ok()?;
    let name = Path::new(&shell).file_name()?.to_string_lossy().to_string();
    match name.as_str() {
        "zsh" | "bash" => Some(name),
        _ => None,
    }
}

fn profile_path(home: &Path, shell: &str) -> PathBuf {
    match shell {
        "zsh" => home.join(".zshenv"),
        "bash" => home.join(".bashrc"),
        _ => unreachable!("detect_shell only returns zsh|bash"),
    }
}

fn update_shell_profile(path: &Path, shell: &str, binary: &str) -> io::Result<()> {
    let existing = fs::read_to_string(path).unwrap_or_default();
    let quoted = shell_single_quote(binary);
    let canonical = format!(r#"eval "$({quoted} install {shell})""#);

    let hook_count = existing
        .lines()
        .filter(|l| is_token_saver_hook(l.trim_start()))
        .count();
    // legacy install.sh inlined an `if...fi` block instead of an eval hook
    let has_legacy_block = existing
        .lines()
        .any(|l| l.trim_start().starts_with("# token-saver: wrap commands"));

    // Exactly one hook, no legacy block, and it is already canonical — nothing
    // to do. (Kept strict so this stays a true no-op and converges.)
    if hook_count == 1 && !has_legacy_block && existing.lines().any(|l| l.trim_start() == canonical)
    {
        println!(
            "Shell hook already present in {} — skipping",
            path.display()
        );
        return Ok(());
    }

    // Any hook or legacy block present (possibly stale path, legacy `init`, a
    // legacy inlined `if...fi` block, or a duplicate from an earlier buggy
    // run): rewrite the first hook in place to the marker comment + canonical
    // line and drop every other token-saver hook, marker comment, and the full
    // legacy block body. Path-agnostic so it migrates across `brew upgrade` /
    // reinstall too, and produces the same shape as a fresh install.
    if hook_count >= 1 || has_legacy_block {
        let mut out: Vec<&str> = Vec::with_capacity(existing.lines().count());
        let mut canonical_emitted = false;
        let mut iter = existing.lines();
        while let Some(line) = iter.next() {
            let trimmed = line.trim_start();
            // legacy inlined block: drop marker through matching `fi`
            if trimmed.starts_with("# token-saver: wrap commands") {
                for inner in iter.by_ref() {
                    if inner.trim() == "fi" {
                        break;
                    }
                }
                if !canonical_emitted {
                    out.push(HOOK_MARKER);
                    out.push(&canonical);
                    canonical_emitted = true;
                }
                continue;
            }
            if is_token_saver_hook(trimmed) {
                if !canonical_emitted {
                    out.push(HOOK_MARKER);
                    out.push(&canonical);
                    canonical_emitted = true;
                }
                continue;
            }
            if trimmed.starts_with("# token-saver:") {
                continue;
            }
            out.push(line);
        }
        let mut updated = out.join("\n");
        updated.push('\n');
        fs::write(path, updated)?;
        println!(
            "Upgraded shell hook in {} to the canonical install form",
            path.display()
        );
        return Ok(());
    }

    let separator = if existing.is_empty() || existing.ends_with('\n') {
        ""
    } else {
        "\n"
    };
    let block = format!("{separator}\n{HOOK_MARKER}\n{canonical}\n");
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    file.write_all(block.as_bytes())?;
    println!("Added shell hook to {}", path.display());
    Ok(())
}

fn update_claude_settings(path: &Path) -> io::Result<()> {
    let raw = fs::read_to_string(path).unwrap_or_default();
    let mut value: Value = if raw.trim().is_empty() {
        Value::Object(Map::new())
    } else {
        serde_json::from_str(&raw).map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("parse settings.json: {e}"),
            )
        })?
    };

    let obj = value.as_object_mut().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "settings.json root is not an object",
        )
    })?;
    let env_entry = obj
        .entry("env".to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    let env_obj = env_entry.as_object_mut().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "settings.json `env` is not an object",
        )
    })?;

    if env_obj.get("TOKEN_SAVER").and_then(Value::as_str) == Some("1") {
        println!(
            "TOKEN_SAVER=1 already present in {} — skipping",
            path.display()
        );
        return Ok(());
    }

    env_obj.insert("TOKEN_SAVER".to_string(), Value::String("1".to_string()));

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let serialized = serde_json::to_string_pretty(&value).expect("Value always serializes");
    fs::write(path, format!("{serialized}\n"))?;
    println!("Added TOKEN_SAVER=1 to {}", path.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    const BIN: &str = "/opt/homebrew/bin/token-saver";

    #[test]
    fn shell_quoting_wraps_in_single_quotes() {
        assert_eq!(shell_single_quote("foo"), "'foo'");
        assert_eq!(
            shell_single_quote("/path with space/bin"),
            "'/path with space/bin'"
        );
    }

    #[test]
    fn shell_quoting_escapes_embedded_single_quote() {
        assert_eq!(shell_single_quote("foo'bar"), "'foo'\\''bar'");
    }

    #[test]
    fn shell_profile_collapses_multiple_stale_hooks_into_one() {
        let dir = tempdir().unwrap();
        let path = dir.path().join(".zshenv");
        // Two differing token-saver hooks + a stray marker comment, with user
        // content interleaved (simulates an earlier buggy double-install).
        let original = "# user content\n\
            # token-saver: enable wrappers when TOKEN_SAVER=1\n\
            eval \"$(token-saver init zsh)\"\n\
            export FOO=bar\n\
            eval \"$('/old/path/token-saver' install zsh)\"\n";
        fs::write(&path, original).unwrap();
        update_shell_profile(&path, "zsh", BIN).unwrap();
        let content = fs::read_to_string(&path).unwrap();

        // exactly one hook line, and it is canonical
        let canonical = r#"eval "$('/opt/homebrew/bin/token-saver' install zsh)""#;
        assert_eq!(content.matches("eval ").count(), 1);
        assert!(content.contains(canonical));
        // exactly one marker comment, restored above the hook
        assert_eq!(content.matches(HOOK_MARKER).count(), 1);
        assert!(content.contains(&format!("{HOOK_MARKER}\n{canonical}")));
        // user content preserved
        assert!(content.contains("# user content"));
        assert!(content.contains("export FOO=bar"));

        // re-running on the upgraded shape is a true no-op
        update_shell_profile(&path, "zsh", BIN).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), content);
    }

    #[test]
    fn shell_profile_strips_legacy_inlined_block_with_no_eval_hook() {
        // legacy install.sh inlined the whole if...fi block, no eval hook
        let dir = tempdir().unwrap();
        let path = dir.path().join(".zshenv");
        let original = "export FOO=bar\n\
            # token-saver: wrap commands for LLM output compression\n\
            # Loads only when TOKEN_SAVER=1 — no-op otherwise.\n\
            if [ \"$TOKEN_SAVER\" = \"1\" ]; then\n\
            \x20   git() { /old/path/token-saver git \"$@\"; }\n\
            \x20   ls() { /old/path/token-saver ls \"$@\"; }\n\
            fi\n\
            export BAR=baz\n";
        fs::write(&path, original).unwrap();
        update_shell_profile(&path, "zsh", BIN).unwrap();
        let content = fs::read_to_string(&path).unwrap();

        let canonical = r#"eval "$('/opt/homebrew/bin/token-saver' install zsh)""#;
        assert!(content.contains(canonical));
        // legacy block body fully gone, no orphaned wrappers
        assert!(!content.contains("if [ \"$TOKEN_SAVER\" = \"1\" ]; then"));
        assert!(!content.contains("/old/path/token-saver"));
        assert!(!content.contains("fi\n"));
        // exactly one marker comment, user content preserved
        assert_eq!(content.matches(HOOK_MARKER).count(), 1);
        assert!(content.contains("export FOO=bar"));
        assert!(content.contains("export BAR=baz"));
    }

    #[test]
    fn shell_profile_strips_legacy_block_when_eval_hook_also_present() {
        // legacy if...fi block coexisting with a stale eval hook
        let dir = tempdir().unwrap();
        let path = dir.path().join(".zshenv");
        let original = "# token-saver: wrap commands for LLM output compression\n\
            if [ \"$TOKEN_SAVER\" = \"1\" ]; then\n\
            \x20   git() { /old/path/token-saver git \"$@\"; }\n\
            fi\n\
            export FOO=bar\n\
            eval \"$('/old/path/token-saver' install zsh)\"\n";
        fs::write(&path, original).unwrap();
        update_shell_profile(&path, "zsh", BIN).unwrap();
        let content = fs::read_to_string(&path).unwrap();

        let canonical = r#"eval "$('/opt/homebrew/bin/token-saver' install zsh)""#;
        assert_eq!(content.matches("eval ").count(), 1);
        assert!(content.contains(canonical));
        // legacy block body fully removed
        assert!(!content.contains("if [ \"$TOKEN_SAVER\" = \"1\" ]; then"));
        assert!(!content.contains("/old/path/token-saver"));
        assert_eq!(content.matches(HOOK_MARKER).count(), 1);
        assert!(content.contains("export FOO=bar"));
    }

    #[test]
    fn shell_profile_creates_when_missing_with_absolute_path() {
        let dir = tempdir().unwrap();
        let path = dir.path().join(".zshenv");
        update_shell_profile(&path, "zsh", BIN).unwrap();
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains(r#"eval "$('/opt/homebrew/bin/token-saver' install zsh)""#));
    }

    #[test]
    fn shell_profile_upgrades_legacy_bare_init_form() {
        let dir = tempdir().unwrap();
        let path = dir.path().join(".zshenv");
        let original = "# user content\neval \"$(token-saver init zsh)\"\n";
        fs::write(&path, original).unwrap();
        update_shell_profile(&path, "zsh", BIN).unwrap();
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains(r#"eval "$('/opt/homebrew/bin/token-saver' install zsh)""#));
        assert!(!content.contains(r#"eval "$(token-saver init zsh)""#));
        assert!(content.starts_with("# user content\n"));
    }

    #[test]
    fn shell_profile_upgrades_absolute_init_form() {
        let dir = tempdir().unwrap();
        let path = dir.path().join(".zshenv");
        let original = "# user content\neval \"$('/opt/homebrew/bin/token-saver' init zsh)\"\n";
        fs::write(&path, original).unwrap();
        update_shell_profile(&path, "zsh", BIN).unwrap();
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains(r#"eval "$('/opt/homebrew/bin/token-saver' install zsh)""#));
        assert!(!content.contains(r#"eval "$('/opt/homebrew/bin/token-saver' init zsh)""#));
        assert!(content.starts_with("# user content\n"));
    }

    #[test]
    fn shell_profile_upgrades_bare_install_form() {
        let dir = tempdir().unwrap();
        let path = dir.path().join(".zshenv");
        let original = "# user content\neval \"$(token-saver install zsh)\"\n";
        fs::write(&path, original).unwrap();
        update_shell_profile(&path, "zsh", BIN).unwrap();
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains(r#"eval "$('/opt/homebrew/bin/token-saver' install zsh)""#));
        assert!(!content.contains(r#"eval "$(token-saver install zsh)""#));
        assert!(content.starts_with("# user content\n"));
    }

    #[test]
    fn shell_profile_skips_when_already_canonical() {
        let dir = tempdir().unwrap();
        let path = dir.path().join(".bashrc");
        let original = "eval \"$('/opt/homebrew/bin/token-saver' install bash)\"\n";
        fs::write(&path, original).unwrap();
        update_shell_profile(&path, "bash", BIN).unwrap();
        let content = fs::read_to_string(&path).unwrap();
        assert_eq!(content, original);
    }

    #[test]
    fn shell_profile_appends_with_separator_when_no_trailing_newline() {
        let dir = tempdir().unwrap();
        let path = dir.path().join(".zshenv");
        fs::write(&path, "export FOO=bar").unwrap();
        update_shell_profile(&path, "zsh", BIN).unwrap();
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.starts_with("export FOO=bar\n"));
        assert!(content.contains("eval \"$('/opt/homebrew/bin/token-saver' install zsh)\""));
    }

    #[test]
    fn claude_settings_creates_directory_and_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join(".claude/settings.json");
        update_claude_settings(&path).unwrap();
        let value: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(value["env"]["TOKEN_SAVER"], "1");
    }

    #[test]
    fn claude_settings_preserves_other_keys_and_env_entries() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(
            &path,
            r#"{ "model": "sonnet", "env": { "OTHER": "value" } }"#,
        )
        .unwrap();
        update_claude_settings(&path).unwrap();
        let value: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(value["model"], "sonnet");
        assert_eq!(value["env"]["OTHER"], "value");
        assert_eq!(value["env"]["TOKEN_SAVER"], "1");
    }

    #[test]
    fn claude_settings_idempotent_when_already_set() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let original = "{\n  \"env\": {\n    \"TOKEN_SAVER\": \"1\"\n  }\n}\n";
        fs::write(&path, original).unwrap();
        update_claude_settings(&path).unwrap();
        let after = fs::read_to_string(&path).unwrap();
        assert_eq!(after, original);
    }

    #[test]
    fn claude_settings_handles_empty_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(&path, "").unwrap();
        update_claude_settings(&path).unwrap();
        let value: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(value["env"]["TOKEN_SAVER"], "1");
    }

    #[test]
    fn claude_settings_rejects_non_object_root() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(&path, "[1, 2, 3]").unwrap();
        let err = update_claude_settings(&path).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }
}
