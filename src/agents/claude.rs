//! claude agent target — sets `env.TOKEN_SAVER` in settings.json.

use super::{read_json_object, write_json_object};
use serde_json::{Map, Value};
use std::io;
use std::path::Path;

/// Set `env.TOKEN_SAVER=1` in claude's settings.json, preserving other keys.
/// Returns Ok(true) when the file changed, Ok(false) when already set.
pub fn write_env(path: &Path) -> io::Result<bool> {
    let mut obj = read_json_object(path)?.unwrap_or_default();

    let env_entry = obj
        .entry("env".to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    let env_obj = env_entry.as_object_mut().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{}: `env` is not an object", path.display()),
        )
    })?;

    if env_obj.get("TOKEN_SAVER").and_then(Value::as_str) == Some("1") {
        return Ok(false);
    }

    env_obj.insert("TOKEN_SAVER".to_string(), Value::String("1".to_string()));
    write_json_object(path, &obj)?;
    Ok(true)
}

/// Remove `env.TOKEN_SAVER`, dropping the `env` key if it becomes empty.
/// Returns Ok(true) when the file changed, Ok(false) when nothing to remove.
pub fn erase_env(path: &Path) -> io::Result<bool> {
    let mut obj = match read_json_object(path)? {
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

    write_json_object(path, &obj)?;
    Ok(true)
}

/// True when `env.TOKEN_SAVER` is already "1" (read-only check).
pub fn is_set(path: &Path) -> bool {
    let Ok(Some(obj)) = read_json_object(path) else {
        return false;
    };
    obj.get("env")
        .and_then(|e| e.get("TOKEN_SAVER"))
        .and_then(Value::as_str)
        == Some("1")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn is_set_reflects_state() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("settings.json");
        assert!(!is_set(&path));
        write_env(&path).unwrap();
        assert!(is_set(&path));
        erase_env(&path).unwrap();
        assert!(!is_set(&path));
    }

    #[test]
    fn write_creates_directory_and_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join(".claude/settings.json");
        assert!(write_env(&path).unwrap());
        let value: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(value["env"]["TOKEN_SAVER"], "1");
    }

    #[test]
    fn write_preserves_other_keys_and_env_entries() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(
            &path,
            r#"{ "model": "sonnet", "env": { "OTHER": "value" } }"#,
        )
        .unwrap();
        assert!(write_env(&path).unwrap());
        let value: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(value["model"], "sonnet");
        assert_eq!(value["env"]["OTHER"], "value");
        assert_eq!(value["env"]["TOKEN_SAVER"], "1");
    }

    #[test]
    fn write_idempotent_when_already_set() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let original = "{\n  \"env\": {\n    \"TOKEN_SAVER\": \"1\"\n  }\n}\n";
        fs::write(&path, original).unwrap();
        assert!(!write_env(&path).unwrap());
        assert_eq!(fs::read_to_string(&path).unwrap(), original);
    }

    #[test]
    fn write_handles_empty_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(&path, "").unwrap();
        assert!(write_env(&path).unwrap());
        let value: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(value["env"]["TOKEN_SAVER"], "1");
    }

    #[test]
    fn write_rejects_non_object_root() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(&path, "[1, 2, 3]").unwrap();
        let err = write_env(&path).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn erase_removes_token_saver_and_preserves_rest() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(
            &path,
            r#"{ "model": "sonnet", "env": { "OTHER": "value", "TOKEN_SAVER": "1" } }"#,
        )
        .unwrap();
        assert!(erase_env(&path).unwrap());
        let value: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(value["model"], "sonnet");
        assert_eq!(value["env"]["OTHER"], "value");
        assert!(value["env"].get("TOKEN_SAVER").is_none());
    }

    #[test]
    fn erase_drops_empty_env_object() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(
            &path,
            r#"{ "model": "sonnet", "env": { "TOKEN_SAVER": "1" } }"#,
        )
        .unwrap();
        assert!(erase_env(&path).unwrap());
        let value: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(value["model"], "sonnet");
        assert!(value.as_object().unwrap().get("env").is_none());
    }

    #[test]
    fn erase_returns_false_when_key_absent() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let original = "{\n  \"model\": \"sonnet\"\n}\n";
        fs::write(&path, original).unwrap();
        assert!(!erase_env(&path).unwrap());
        assert_eq!(fs::read_to_string(&path).unwrap(), original);
    }

    #[test]
    fn erase_returns_false_for_missing_or_empty_file() {
        let dir = tempdir().unwrap();
        let missing = dir.path().join("settings.json");
        assert!(!erase_env(&missing).unwrap());
        fs::write(&missing, "").unwrap();
        assert!(!erase_env(&missing).unwrap());
    }

    #[test]
    fn erase_rejects_invalid_json() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(&path, "{ not json").unwrap();
        let err = erase_env(&path).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }
}
