use serde_json::{Map, Value};
use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

const COMMANDS: &[&str] = &[
    "cat", "eslint", "git", "jest", "ls", "find", "grep", "npx", "prettier", "rg", "tsc",
];

/// Dispatch entry point for `token-saver init [shell]`.
///
/// - `init` (no args): auto-detect shell, edit profile, edit `~/.claude/settings.json`.
/// - `init zsh|bash`: print the shell-function block (for `eval "$(...)"` use).
pub fn run(args: &[String]) -> i32 {
    match args.first().map(String::as_str) {
        None => auto(),
        Some(shell) => print_block(shell),
    }
}

fn print_block(shell: &str) -> i32 {
    match shell {
        "zsh" | "bash" => {
            println!("# token-saver: wrap commands for LLM output compression");
            println!("# Loads only when TOKEN_SAVER=1 — no-op otherwise.");
            println!("if [ \"$TOKEN_SAVER\" = \"1\" ]; then");
            for cmd in COMMANDS {
                println!("    {cmd}() {{ command token-saver {cmd} \"$@\"; }}");
            }
            println!("fi");
            0
        }
        "" => {
            eprintln!("token-saver init: missing shell argument (zsh|bash)");
            2
        }
        other => {
            eprintln!("token-saver init: unsupported shell '{other}' (supported: zsh, bash)");
            2
        }
    }
}

fn auto() -> i32 {
    let shell = match detect_shell() {
        Some(s) => s,
        None => {
            eprintln!(
                "token-saver init: could not detect a supported shell from $SHELL (need zsh or bash)"
            );
            eprintln!(
                "Run `token-saver init zsh` or `token-saver init bash` and add the eval line to your profile manually."
            );
            return 1;
        }
    };

    let home = match env::var_os("HOME") {
        Some(h) => PathBuf::from(h),
        None => {
            eprintln!("token-saver init: $HOME is not set");
            return 1;
        }
    };

    let profile = profile_path(&home, &shell);
    if let Err(e) = update_shell_profile(&profile, &shell) {
        eprintln!(
            "token-saver init: failed to update {}: {e}",
            profile.display()
        );
        return 1;
    }

    let settings = home.join(".claude").join("settings.json");
    if let Err(e) = update_claude_settings(&settings) {
        eprintln!(
            "token-saver init: failed to update {}: {e}",
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

fn update_shell_profile(path: &Path, shell: &str) -> io::Result<()> {
    let existing = fs::read_to_string(path).unwrap_or_default();
    if existing.contains("token-saver init") {
        println!(
            "Shell hook already present in {} — skipping",
            path.display()
        );
        return Ok(());
    }
    let separator = if existing.is_empty() || existing.ends_with('\n') {
        ""
    } else {
        "\n"
    };
    let block = format!(
        "{separator}\n# token-saver: enable wrappers when TOKEN_SAVER=1\neval \"$(token-saver init {shell})\"\n"
    );
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

    #[test]
    fn shell_profile_creates_when_missing() {
        let dir = tempdir().unwrap();
        let path = dir.path().join(".zshenv");
        update_shell_profile(&path, "zsh").unwrap();
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains(r#"eval "$(token-saver init zsh)""#));
    }

    #[test]
    fn shell_profile_skips_when_already_present() {
        let dir = tempdir().unwrap();
        let path = dir.path().join(".bashrc");
        let original = "# user content\neval \"$(token-saver init bash)\"\n";
        fs::write(&path, original).unwrap();
        update_shell_profile(&path, "bash").unwrap();
        let content = fs::read_to_string(&path).unwrap();
        assert_eq!(content, original);
    }

    #[test]
    fn shell_profile_appends_with_separator_when_no_trailing_newline() {
        let dir = tempdir().unwrap();
        let path = dir.path().join(".zshenv");
        fs::write(&path, "export FOO=bar").unwrap();
        update_shell_profile(&path, "zsh").unwrap();
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.starts_with("export FOO=bar\n"));
        assert!(content.contains(r#"eval "$(token-saver init zsh)""#));
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
