//! pi agent target — configures `shellCommandPrefix` in settings.json.
//!
//! pi runs its bash tool as non-interactive `bash -c`, which sources no
//! profile, so the shell hook never loads there. The prefix snippet activates
//! token-saver inside every pi bash call instead.

use super::{read_json_object, write_json_object};
use serde_json::Value;
use std::io;
use std::path::Path;

/// Begin marker of the delimited region — idempotence and erase anchor.
pub const PREFIX_MARKER: &str = "# token-saver:begin";

/// End marker of the delimited region.
const END_MARKER: &str = "# token-saver:end";

/// Ambiguous begin marker written by the first delimited-region release.
const LEGACY_DELIMITED_PREFIX_MARKER: &str = "# token-saver";

/// Prefix prepended to every pi bash command. Guarded so machines without
/// the token-saver binary stay silent. Delimited by `PREFIX_MARKER` and
/// `END_MARKER` so write/erase can operate on the region instead of an
/// exact body match.
pub const PREFIX_SNIPPET: &str = "# token-saver:begin\nif command -v token-saver >/dev/null 2>&1; then\n    export TOKEN_SAVER=1\n    eval \"$(token-saver install bash)\"\nfi\n# token-saver:end";

/// Undelimited snippet body written by releases before the delimited-region
/// fix. Kept so write/erase can still find and migrate it — dropping this
/// would strand anyone who installed with the old shape.
const LEGACY_PREFIX_SNIPPET: &str = "# token-saver\nif command -v token-saver >/dev/null 2>&1; then\n    export TOKEN_SAVER=1\n    eval \"$(token-saver install bash)\"\nfi";

/// Locate one exact-line delimited region `[start, end)`.
fn find_delimited_region(s: &str, begin_marker: &str) -> Option<(usize, usize)> {
    let mut latest_begin = None;
    let mut offset = 0;

    for segment in s.split_inclusive('\n') {
        let without_newline = segment.strip_suffix('\n').unwrap_or(segment);
        let line = without_newline
            .strip_suffix('\r')
            .unwrap_or(without_newline);
        if line == begin_marker {
            latest_begin = Some(offset);
        } else if line == END_MARKER
            && let Some(start) = latest_begin
        {
            return Some((start, offset + without_newline.len()));
        }
        offset += segment.len();
    }
    None
}

/// Locate a current or legacy delimited region. When a legacy-looking user
/// line precedes a current region, prefer the unambiguous current marker.
fn find_region(s: &str) -> Option<(usize, usize)> {
    let current = find_delimited_region(s, PREFIX_MARKER);
    let legacy = find_delimited_region(s, LEGACY_DELIMITED_PREFIX_MARKER);
    match (current, legacy) {
        (Some(current), Some(legacy)) if current.1 == legacy.1 => Some(current),
        (Some(current), Some(legacy)) if current.0 < legacy.0 => Some(current),
        (Some(_), Some(legacy)) => Some(legacy),
        (Some(current), None) => Some(current),
        (None, Some(legacy)) => Some(legacy),
        (None, None) => None,
    }
}

/// Locate one region to replace or erase, including the exact undelimited body
/// written before end markers existed.
fn find_existing_region(s: &str) -> Option<(usize, usize)> {
    let delimited = find_region(s);
    let undelimited = s
        .find(LEGACY_PREFIX_SNIPPET)
        .map(|start| (start, start + LEGACY_PREFIX_SNIPPET.len()));
    match (delimited, undelimited) {
        (Some(delimited), Some(undelimited)) if delimited.0 < undelimited.0 => Some(delimited),
        (Some(delimited), Some(undelimited)) if delimited.0 == undelimited.0 => {
            Some((delimited.0, delimited.1.max(undelimited.1)))
        }
        (Some(_), Some(undelimited)) => Some(undelimited),
        (Some(delimited), None) => Some(delimited),
        (None, Some(undelimited)) => Some(undelimited),
        (None, None) => None,
    }
}

/// Locate every non-overlapping managed region.
fn find_existing_regions(s: &str) -> Vec<(usize, usize)> {
    let mut regions = Vec::new();
    let mut offset = 0;
    while offset < s.len() {
        let Some((start, end)) = find_existing_region(&s[offset..]) else {
            break;
        };
        regions.push((offset + start, offset + end));
        offset += end;
    }
    regions
}

/// Cut `[start, end)` out of `s`, collapsing the seam newline so removing a
/// region between two user sections doesn't leave a blank line behind.
fn strip_region(s: &str, start: usize, end: usize) -> String {
    let mut before = &s[..start];
    let mut after = &s[end..];
    if after.starts_with('\n') && (before.is_empty() || before.ends_with('\n')) {
        after = &after[1..];
    } else if after.is_empty() && before.ends_with('\n') {
        before = &before[..before.len() - 1];
    }
    format!("{before}{after}")
}

/// Write (or upgrade) the token-saver delimited region in
/// `shellCommandPrefix`, preserving any existing user prefix. Returns
/// Ok(true) when the file changed, Ok(false) when the region is already
/// byte-identical to the current snippet.
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
            let regions = find_existing_regions(existing);
            if regions.is_empty() {
                if existing.trim().is_empty() {
                    PREFIX_SNIPPET.to_string()
                } else {
                    format!("{existing}\n{PREFIX_SNIPPET}")
                }
            } else {
                let mut updated = existing.to_string();
                for &(start, end) in regions.iter().skip(1).rev() {
                    updated = strip_region(&updated, start, end);
                }
                let (start, end) = regions[0];
                updated.replace_range(start..end, PREFIX_SNIPPET);
                updated
            }
        }
    };

    if obj.get("shellCommandPrefix").and_then(Value::as_str) == Some(combined.as_str()) {
        return Ok(false);
    }

    obj.insert("shellCommandPrefix".to_string(), Value::String(combined));
    write_json_object(path, &obj)?;
    Ok(true)
}

/// Remove the token-saver region (delimited or legacy undelimited),
/// preserving any user prefix around it. Drops the `shellCommandPrefix` key
/// if nothing but whitespace remains. Returns Ok(true) when the file
/// changed, Ok(false) when no region was found.
pub fn erase_prefix(path: &Path) -> io::Result<bool> {
    let mut obj = match read_json_object(path)? {
        Some(o) => o,
        None => return Ok(false),
    };

    let existing = match obj.get("shellCommandPrefix").and_then(Value::as_str) {
        Some(s) => s,
        None => return Ok(false),
    };

    let regions = find_existing_regions(existing);
    if regions.is_empty() {
        return Ok(false);
    }

    let mut cleaned = existing.to_string();
    for &(start, end) in regions.iter().rev() {
        cleaned = strip_region(&cleaned, start, end);
    }
    if cleaned.trim().is_empty() {
        obj.remove("shellCommandPrefix");
    } else {
        obj.insert("shellCommandPrefix".to_string(), Value::String(cleaned));
    }

    write_json_object(path, &obj)?;
    Ok(true)
}

/// True when a managed current or legacy snippet is present in
/// `shellCommandPrefix` (read-only check).
pub fn has_prefix(path: &Path) -> bool {
    let Ok(Some(obj)) = read_json_object(path) else {
        return false;
    };
    obj.get("shellCommandPrefix")
        .and_then(Value::as_str)
        .is_some_and(|s| find_existing_region(s).is_some())
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
    fn write_replaces_drifted_delimited_region_in_place() {
        // begin/end markers present but body differs (e.g. an older release's snippet) — must refresh
        let dir = tempdir().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(
            &path,
            r#"{ "shellCommandPrefix": "before\n# token-saver\nexport TOKEN_SAVER=1\n# token-saver:end\nafter" }"#,
        )
        .unwrap();
        assert!(write_prefix(&path).unwrap());
        let value: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        let prefix = value["shellCommandPrefix"].as_str().unwrap();
        assert_eq!(prefix, format!("before\n{PREFIX_SNIPPET}\nafter"));
    }

    #[test]
    fn write_upgrades_legacy_undelimited_snippet() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let legacy = serde_json::json!({ "shellCommandPrefix": LEGACY_PREFIX_SNIPPET });
        fs::write(&path, serde_json::to_string(&legacy).unwrap()).unwrap();
        assert!(write_prefix(&path).unwrap());
        let value: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(value["shellCommandPrefix"], PREFIX_SNIPPET);
        // now delimited, so it stays idempotent going forward
        assert!(!write_prefix(&path).unwrap());
    }

    #[test]
    fn write_preserves_marker_like_user_lines_across_reinstall() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let original = "echo before\n# token-saver\necho after";
        let value = serde_json::json!({ "shellCommandPrefix": original });
        fs::write(&path, serde_json::to_string(&value).unwrap()).unwrap();

        assert!(write_prefix(&path).unwrap());
        assert!(!write_prefix(&path).unwrap());

        let value: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        let prefix = value["shellCommandPrefix"].as_str().unwrap();
        assert!(prefix.starts_with(original));
        assert_eq!(prefix.matches(PREFIX_MARKER).count(), 1);
    }

    #[test]
    fn write_collapses_duplicate_regions_and_erase_removes_all() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let duplicate = format!("before\n{PREFIX_SNIPPET}\n{PREFIX_SNIPPET}\nafter");
        let value = serde_json::json!({ "shellCommandPrefix": duplicate });
        fs::write(&path, serde_json::to_string(&value).unwrap()).unwrap();

        assert!(write_prefix(&path).unwrap());
        let value: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(
            value["shellCommandPrefix"]
                .as_str()
                .unwrap()
                .matches(PREFIX_MARKER)
                .count(),
            1
        );

        assert!(erase_prefix(&path).unwrap());
        let value: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(value["shellCommandPrefix"], "before\nafter");
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
    fn erase_preserves_content_on_both_sides_of_region() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let existing = serde_json::json!({
            "shellCommandPrefix": format!("before\n{PREFIX_SNIPPET}\nafter")
        });
        fs::write(&path, serde_json::to_string(&existing).unwrap()).unwrap();
        assert!(erase_prefix(&path).unwrap());
        let value: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(value["shellCommandPrefix"], "before\nafter");
    }

    #[test]
    fn erase_preserves_outer_whitespace() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let existing = serde_json::json!({
            "shellCommandPrefix": format!("  before\n{PREFIX_SNIPPET}\nafter  ")
        });
        fs::write(&path, serde_json::to_string(&existing).unwrap()).unwrap();
        assert!(erase_prefix(&path).unwrap());
        let value: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(value["shellCommandPrefix"], "  before\nafter  ");
    }

    #[test]
    fn write_erase_round_trip_preserves_trailing_newlines() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let original = "echo user\n\n";
        let existing = serde_json::json!({ "shellCommandPrefix": original });
        fs::write(&path, serde_json::to_string(&existing).unwrap()).unwrap();

        assert!(write_prefix(&path).unwrap());
        assert!(erase_prefix(&path).unwrap());

        let value: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(value["shellCommandPrefix"], original);
    }

    #[test]
    fn erase_collapses_crlf_region_seams() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let prefix = PREFIX_SNIPPET.replace('\n', "\r\n");
        let existing = serde_json::json!({
            "shellCommandPrefix": format!("before\r\n{prefix}\r\nafter")
        });
        fs::write(&path, serde_json::to_string(&existing).unwrap()).unwrap();

        assert!(erase_prefix(&path).unwrap());

        let value: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(value["shellCommandPrefix"], "before\r\nafter");
    }

    #[test]
    fn erase_removes_drifted_delimited_region() {
        // begin/end markers present but body differs — region must still be erasable
        let dir = tempdir().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(
            &path,
            "{ \"shellCommandPrefix\": \"# token-saver\\nexport TOKEN_SAVER=1\\n# token-saver:end\" }",
        )
        .unwrap();
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
    fn erase_removes_legacy_undelimited_snippet() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let legacy = serde_json::json!({ "shellCommandPrefix": LEGACY_PREFIX_SNIPPET });
        fs::write(&path, serde_json::to_string(&legacy).unwrap()).unwrap();
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
