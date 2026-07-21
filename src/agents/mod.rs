//! Agent targets token-saver configures: detection, config paths, instructions.
//!
//! Scriptable agents (claude, pi, codex) get their config files written and
//! erased automatically. Manual agents (opencode, cursor) have no config-file
//! env mechanism — the wizard prints instructions instead.

pub mod claude;
pub mod codex;
pub mod pi;

use serde_json::{Map, Value};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Install scope: global writes under `$HOME`, project writes into the repo.
/// The shell hook is global in both scopes (profiles are per-user).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Global,
    Project,
}

/// Agents whose config files token-saver writes and erases automatically.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptableAgent {
    Claude,
    Pi,
    Codex,
}

/// Agents without a config-file env mechanism — manual instructions only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManualAgent {
    Opencode,
    Cursor,
}

impl ScriptableAgent {
    pub const ALL: [Self; 3] = [Self::Claude, Self::Pi, Self::Codex];

    /// Display name.
    pub fn name(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Pi => "pi",
            Self::Codex => "codex",
        }
    }

    /// Config file path under the scope root (`$HOME` or project root).
    pub fn config_path(self, scope: Scope, root: &Path) -> PathBuf {
        match (self, scope) {
            (Self::Claude, _) => root.join(".claude").join("settings.json"),
            (Self::Codex, _) => root.join(".codex").join("config.toml"),
            (Self::Pi, Scope::Global) => root.join(".pi").join("agent").join("settings.json"),
            (Self::Pi, Scope::Project) => root.join(".pi").join("settings.json"),
        }
    }

    /// Directory whose existence marks the agent as in use for this scope.
    pub fn detection_dir(self, scope: Scope) -> PathBuf {
        match (self, scope) {
            (Self::Claude, _) => PathBuf::from(".claude"),
            (Self::Codex, _) => PathBuf::from(".codex"),
            (Self::Pi, Scope::Global) => PathBuf::from(".pi").join("agent"),
            (Self::Pi, Scope::Project) => PathBuf::from(".pi"),
        }
    }

    /// True when the detection dir exists under the scope root.
    pub fn detected(self, scope: Scope, root: &Path) -> bool {
        root.join(self.detection_dir(scope)).exists()
    }

    /// True when token-saver config is already present (read-only check).
    pub fn is_configured(self, scope: Scope, root: &Path) -> bool {
        let path = self.config_path(scope, root);
        match self {
            Self::Claude => claude::is_set(&path),
            Self::Pi => pi::has_prefix(&path),
            Self::Codex => codex::is_set(&path),
        }
    }

    /// Write token-saver config. Returns Ok(true) when the file changed,
    /// Ok(false) when already configured.
    pub fn write(self, scope: Scope, root: &Path) -> io::Result<bool> {
        let path = self.config_path(scope, root);
        match self {
            Self::Claude => claude::write_env(&path),
            Self::Pi => pi::write_prefix(&path),
            Self::Codex => codex::write_env(&path),
        }
    }

    /// Erase token-saver config. Returns Ok(true) when the file changed,
    /// Ok(false) when nothing was configured.
    pub fn erase(self, scope: Scope, root: &Path) -> io::Result<bool> {
        let path = self.config_path(scope, root);
        match self {
            Self::Claude => claude::erase_env(&path),
            Self::Pi => pi::erase_prefix(&path),
            Self::Codex => codex::erase_env(&path),
        }
    }

    /// What to write by hand when the user picks "do it myself".
    pub fn manual_instructions(self, scope: Scope, root: &Path) -> String {
        let path = self.config_path(scope, root);
        match self {
            Self::Claude => format!(
                "claude — add to {}:\n    \"env\": {{ \"TOKEN_SAVER\": \"1\" }}",
                path.display()
            ),
            Self::Pi => format!(
                "pi — set \"shellCommandPrefix\" in {} to:\n{}",
                path.display(),
                pi::PREFIX_SNIPPET
            ),
            Self::Codex => format!(
                "codex — add to {}:\n    [shell_environment_policy]\n    set = {{ TOKEN_SAVER = \"1\" }}",
                path.display()
            ),
        }
    }
}

impl ManualAgent {
    pub const ALL: [Self; 2] = [Self::Opencode, Self::Cursor];

    /// Display name.
    pub fn name(self) -> &'static str {
        match self {
            Self::Opencode => "opencode",
            Self::Cursor => "cursor",
        }
    }

    /// Directory whose existence marks the agent as in use for this scope.
    pub fn detection_dir(self, scope: Scope) -> PathBuf {
        match (self, scope) {
            (Self::Opencode, Scope::Global) => PathBuf::from(".config").join("opencode"),
            (Self::Opencode, Scope::Project) => PathBuf::from(".opencode"),
            (Self::Cursor, _) => PathBuf::from(".cursor"),
        }
    }

    /// True when the detection dir exists under the scope root.
    pub fn detected(self, scope: Scope, root: &Path) -> bool {
        root.join(self.detection_dir(scope)).exists()
    }

    /// Manual setup instructions (no config-file env mechanism exists).
    pub fn manual_instructions(self) -> &'static str {
        match self {
            Self::Opencode => {
                "opencode — no config-file env mechanism; launch with the variable instead:\n    TOKEN_SAVER=1 opencode\n  and set \"shell\": \"/bin/zsh\" in opencode.json so the wrappers load"
            }
            Self::Cursor => {
                "cursor — env mechanism unverified (research tracked in COMPRESSORS.md); try launching with:\n    TOKEN_SAVER=1 cursor-agent"
            }
        }
    }
}

/// Read a JSON file whose root must be an object. Missing or empty file
/// yields Ok(None); a non-object root is an error.
pub(crate) fn read_json_object(path: &Path) -> io::Result<Option<Map<String, Value>>> {
    let raw = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    if raw.trim().is_empty() {
        return Ok(None);
    }
    let value: Value = serde_json::from_str(&raw).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("parse {}: {e}", path.display()),
        )
    })?;
    match value {
        Value::Object(obj) => Ok(Some(obj)),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{}: root is not an object", path.display()),
        )),
    }
}

/// Write a JSON object pretty-printed with trailing newline, creating parent
/// directories as needed.
pub(crate) fn write_json_object(path: &Path, obj: &Map<String, Value>) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let serialized = serde_json::to_string_pretty(obj).expect("Map always serializes");
    fs::write(path, format!("{serialized}\n"))
}
