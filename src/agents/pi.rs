//! pi agent target — configures `shellCommandPrefix` in settings.json.
//!
//! pi runs its bash tool as non-interactive `bash -c`, which sources no
//! profile, so the shell hook never loads there. The prefix snippet activates
//! token-saver inside every pi bash call instead.

use super::{read_json_object, write_json_object};
use serde_json::Value;
use std::io;
use std::path::Path;

/// Marker comment inside the snippet — idempotence and erase anchor.
pub const PREFIX_MARKER: &str = "# token-saver";

/// Prefix prepended to every pi bash command. Guarded so machines without
/// the token-saver binary stay silent.
pub const PREFIX_SNIPPET: &str = "# token-saver\nif command -v token-saver >/dev/null 2>&1; then\n    export TOKEN_SAVER=1\n    eval \"$(token-saver install bash)\"\nfi";

/// Append the token-saver snippet to `shellCommandPrefix`, preserving any
/// existing user prefix. Returns Ok(true) when the file changed, Ok(false)
/// when the snippet is already present.
pub fn write_prefix(path: &Path) -> io::Result<bool> {
    let mut obj = read_json_object(path)?.unwrap_or_default();

    // reject non-string prefix before computing anything
    if let Some(v) = obj.get("shellCommandPrefix")
        && !v.is_string()
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{}: `shellCommandPrefix` is not a string", path.display()),
        ));
    }

    let combined = match obj.get("shellCommandPrefix").and_then(Value::as_str) {
        None => PREFIX_SNIPPET.to_string(),
        Some(existing) => {
            if existing.contains(PREFIX_MARKER) {
                return Ok(false);
            }
            if existing.trim().is_empty() {
                PREFIX_SNIPPET.to_string()
            } else {
                format!("{}\n{}", existing.trim_end_matches('\n'), PREFIX_SNIPPET)
            }
        }
    };

    obj.insert("shellCommandPrefix".to_string(), Value::String(combined));
    write_json_object(path, &obj)?;
    Ok(true)
}

/// Remove the token-saver snippet, preserving any user prefix. Drops the
/// `shellCommandPrefix` key if nothing remains. Returns Ok(true) when the
/// file changed, Ok(false) when the exact snippet was absent (including
/// marker-only drift from an older release).
pub fn erase_prefix(path: &Path) -> io::Result<bool> {
    let mut obj = match read_json_object(path)? {
        Some(o) => o,
        None => return Ok(false),
    };

    let existing = match obj.get("shellCommandPrefix").and_then(Value::as_str) {
        Some(s) if s.contains(PREFIX_MARKER) => s,
        _ => return Ok(false),
    };

    let cleaned_full = existing.replace(PREFIX_SNIPPET, "");
    // marker present but our exact snippet not found (e.g. version drift) — leave user content untouched
    if cleaned_full == existing {
        return Ok(false);
    }
    let cleaned = cleaned_full.trim();
    if cleaned.is_empty() {
        obj.remove("shellCommandPrefix");
    } else {
        obj.insert(
            "shellCommandPrefix".to_string(),
            Value::String(cleaned.to_string()),
        );
    }

    write_json_object(path, &obj)?;
    Ok(true)
}

/// True when the token-saver snippet is already in `shellCommandPrefix`
/// (read-only check).
pub fn has_prefix(path: &Path) -> bool {
    let Ok(Some(obj)) = read_json_object(path) else {
        return false;
    };
    obj.get("shellCommandPrefix")
        .and_then(Value::as_str)
        .is_some_and(|s| s.contains(PREFIX_MARKER))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn has_prefix_reflects_state() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("settings.json");
        assert!(!has_prefix(&path));
        write_prefix(&path).unwrap();
        assert!(has_prefix(&path));
        erase_prefix(&path).unwrap();
        assert!(!has_prefix(&path));
    }

    #[test]
    fn write_creates_file_with_snippet() {
        let dir = tempdir().unwrap();
        let path = dir.path().join(".pi/settings.json");
        assert!(write_prefix(&path).unwrap());
        let value: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(value["shellCommandPrefix"], PREFIX_SNIPPET);
    }

    #[test]
    fn write_appends_to_existing_user_prefix() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(
            &path,
            r#"{ "shellCommandPrefix": "shopt -s expand_aliases" }"#,
        )
        .unwrap();
        assert!(write_prefix(&path).unwrap());
        let value: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        let prefix = value["shellCommandPrefix"].as_str().unwrap();
        assert!(prefix.starts_with("shopt -s expand_aliases\n"));
        assert!(prefix.contains(PREFIX_SNIPPET));
    }

    #[test]
    fn write_idempotent_when_snippet_present() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("settings.json");
        assert!(write_prefix(&path).unwrap());
        let after_first = fs::read_to_string(&path).unwrap();
        assert!(!write_prefix(&path).unwrap());
        assert_eq!(fs::read_to_string(&path).unwrap(), after_first);
    }

    #[test]
    fn write_preserves_other_keys() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(&path, r#"{ "shellPath": "/bin/zsh" }"#).unwrap();
        assert!(write_prefix(&path).unwrap());
        let value: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(value["shellPath"], "/bin/zsh");
    }

    #[test]
    fn write_rejects_non_string_prefix() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(&path, r#"{ "shellCommandPrefix": 42 }"#).unwrap();
        let err = write_prefix(&path).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn erase_removes_snippet_and_key_when_only_content() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("settings.json");
        write_prefix(&path).unwrap();
        assert!(erase_prefix(&path).unwrap());
        let value: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert!(
            value
                .as_object()
                .unwrap()
                .get("shellCommandPrefix")
                .is_none()
        );
    }

    #[test]
    fn erase_preserves_user_prefix() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(
            &path,
            r#"{ "shellCommandPrefix": "shopt -s expand_aliases" }"#,
        )
        .unwrap();
        write_prefix(&path).unwrap();
        assert!(erase_prefix(&path).unwrap());
        let value: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(value["shellCommandPrefix"], "shopt -s expand_aliases");
    }

    #[test]
    fn erase_returns_false_on_snippet_drift() {
        // marker present but exact snippet differs (simulated older release) — leave untouched
        let dir = tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let stale = "{ \"shellCommandPrefix\": \"# token-saver\\nexport TOKEN_SAVER=1\" }";
        fs::write(&path, stale).unwrap();
        assert!(!erase_prefix(&path).unwrap());
        assert_eq!(fs::read_to_string(&path).unwrap(), stale);
    }

    #[test]
    fn erase_returns_false_when_absent() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(&path, r#"{ "shellPath": "/bin/zsh" }"#).unwrap();
        assert!(!erase_prefix(&path).unwrap());
    }

    #[test]
    fn erase_returns_false_for_missing_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("settings.json");
        assert!(!erase_prefix(&path).unwrap());
    }
}
