//! codex agent target — sets `shell_environment_policy` TOKEN_SAVER in
//! config.toml. Uses toml_edit so existing content, comments, and formatting
//! survive the edit.

use std::fs;
use std::io;
use std::path::Path;
use toml_edit::{DocumentMut, Item, Table, Value};

const POLICY: &str = "shell_environment_policy";
const SET: &str = "set";
const KEY: &str = "TOKEN_SAVER";

/// Set `shell_environment_policy.set.TOKEN_SAVER = "1"`, creating the tables
/// as needed. Returns Ok(true) when the file changed, Ok(false) when already
/// set.
pub fn write_env(path: &Path) -> io::Result<bool> {
    let raw = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) if e.kind() == io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e),
    };
    let mut doc = parse(&raw, path)?;

    let policy_item = doc[POLICY].or_insert(Item::Table(Table::new()));
    let policy = policy_item
        .as_table_like_mut()
        .ok_or_else(|| invalid(path, POLICY))?;
    if policy.get(SET).is_none() {
        policy.insert(SET, Item::Table(Table::new()));
    }
    let set = policy
        .get_mut(SET)
        .and_then(Item::as_table_like_mut)
        .ok_or_else(|| invalid(path, SET))?;

    if set.get(KEY).and_then(Item::as_str) == Some("1") {
        return Ok(false);
    }

    set.insert(KEY, Item::Value(Value::from("1")));
    write_doc(path, &doc)?;
    Ok(true)
}

/// Remove `shell_environment_policy.set.TOKEN_SAVER`, dropping the `set` and
/// policy tables if they become empty. Returns Ok(true) when the file
/// changed, Ok(false) when nothing to remove.
pub fn erase_env(path: &Path) -> io::Result<bool> {
    let raw = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(e),
    };
    let mut doc = parse(&raw, path)?;

    let policy = match doc.get_mut(POLICY).and_then(Item::as_table_like_mut) {
        Some(p) => p,
        None => return Ok(false),
    };
    let set = match policy.get_mut(SET).and_then(Item::as_table_like_mut) {
        Some(s) => s,
        None => return Ok(false),
    };
    if set.remove(KEY).is_none() {
        return Ok(false);
    }
    if set.is_empty() {
        policy.remove(SET);
    }
    let policy_empty = policy.is_empty();
    if policy_empty {
        doc.as_table_mut().remove(POLICY);
    }

    write_doc(path, &doc)?;
    Ok(true)
}

fn parse(raw: &str, path: &Path) -> io::Result<DocumentMut> {
    raw.parse::<DocumentMut>().map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("parse {}: {e}", path.display()),
        )
    })
}

fn invalid(path: &Path, key: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!("{}: `{key}` is not a table", path.display()),
    )
}

fn write_doc(path: &Path, doc: &DocumentMut) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, doc.to_string())
}

/// True when `shell_environment_policy.set.TOKEN_SAVER` is already "1"
/// (read-only check).
pub fn is_set(path: &Path) -> bool {
    let Ok(raw) = fs::read_to_string(path) else {
        return false;
    };
    let Ok(doc) = raw.parse::<DocumentMut>() else {
        return false;
    };
    doc.get(POLICY)
        .and_then(|p| p.get(SET))
        .and_then(|s| s.get(KEY))
        .and_then(Item::as_str)
        == Some("1")
}

#[cfg(test)]
mod tests {
    use super::*;

    use tempfile::tempdir;

    #[test]
    fn is_set_reflects_state() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        assert!(!is_set(&path));
        write_env(&path).unwrap();
        assert!(is_set(&path));
        erase_env(&path).unwrap();
        assert!(!is_set(&path));
    }

    #[test]
    fn write_creates_file_with_policy() {
        let dir = tempdir().unwrap();
        let path = dir.path().join(".codex/config.toml");
        assert!(write_env(&path).unwrap());
        let content = fs::read_to_string(&path).unwrap();
        let doc = content.parse::<DocumentMut>().unwrap();
        assert_eq!(
            doc["shell_environment_policy"]["set"]["TOKEN_SAVER"].as_str(),
            Some("1")
        );
    }

    #[test]
    fn write_preserves_existing_content_and_comments() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let original = "# my codex config\nmodel = \"gpt-5\"\n\n[shell_environment_policy]\ninherit = \"all\"\n";
        fs::write(&path, original).unwrap();
        assert!(write_env(&path).unwrap());
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("# my codex config"));
        assert!(content.contains("model = \"gpt-5\""));
        assert!(content.contains("inherit = \"all\""));
        let doc = content.parse::<DocumentMut>().unwrap();
        assert_eq!(
            doc["shell_environment_policy"]["set"]["TOKEN_SAVER"].as_str(),
            Some("1")
        );
    }

    #[test]
    fn write_merges_into_existing_inline_set() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(
            &path,
            "[shell_environment_policy]\nset = { OTHER = \"x\" }\n",
        )
        .unwrap();
        assert!(write_env(&path).unwrap());
        let doc = fs::read_to_string(&path)
            .unwrap()
            .parse::<DocumentMut>()
            .unwrap();
        assert_eq!(
            doc["shell_environment_policy"]["set"]["OTHER"].as_str(),
            Some("x")
        );
        assert_eq!(
            doc["shell_environment_policy"]["set"]["TOKEN_SAVER"].as_str(),
            Some("1")
        );
    }

    #[test]
    fn write_idempotent_when_already_set() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        assert!(write_env(&path).unwrap());
        let after_first = fs::read_to_string(&path).unwrap();
        assert!(!write_env(&path).unwrap());
        assert_eq!(fs::read_to_string(&path).unwrap(), after_first);
    }

    #[test]
    fn write_rejects_non_table_policy() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(&path, "shell_environment_policy = \"oops\"\n").unwrap();
        let err = write_env(&path).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn erase_removes_key_and_empty_tables() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        write_env(&path).unwrap();
        assert!(erase_env(&path).unwrap());
        let content = fs::read_to_string(&path).unwrap();
        let doc = content.parse::<DocumentMut>().unwrap();
        assert!(doc.get("shell_environment_policy").is_none());
    }

    #[test]
    fn erase_preserves_other_policy_keys() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let original = "model = \"gpt-5\"\n\n[shell_environment_policy]\ninherit = \"all\"\n";
        fs::write(&path, original).unwrap();
        write_env(&path).unwrap();
        assert!(erase_env(&path).unwrap());
        let doc = fs::read_to_string(&path)
            .unwrap()
            .parse::<DocumentMut>()
            .unwrap();
        assert_eq!(doc["model"].as_str(), Some("gpt-5"));
        assert_eq!(
            doc["shell_environment_policy"]["inherit"].as_str(),
            Some("all")
        );
        assert!(doc["shell_environment_policy"].get("set").is_none());
    }

    #[test]
    fn erase_returns_false_when_nothing_configured() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let original = "model = \"gpt-5\"\n";
        fs::write(&path, original).unwrap();
        assert!(!erase_env(&path).unwrap());
        assert_eq!(fs::read_to_string(&path).unwrap(), original);
    }

    #[test]
    fn erase_returns_false_for_missing_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        assert!(!erase_env(&path).unwrap());
    }
}
