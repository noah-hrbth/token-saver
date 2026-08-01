//! Interactive install wizard — runs on `token-saver install` when stdin is
//! a TTY. Three steps: shell hook (always global), install scope, agent
//! selection. Non-TTY callers fall back to the silent path in `install`.

use crate::agents::{
    ManualAgent, Scope, ScriptableAgent, codex_project_trust_note, pi_project_trust_note,
};
use crate::install;
use dialoguer::theme::ColorfulTheme;
use dialoguer::{MultiSelect, Select};
use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

/// What the wizard did — drives the closing summary.
struct Summary {
    shell: String,
    scope: String,
    agents: Vec<String>,
}

impl Summary {
    /// Print the recap block of everything the wizard did.
    fn print(&self) {
        println!();
        println!("Summary");
        println!("  shell hook   {}", self.shell);
        println!("  scope        {}", self.scope);
        for line in &self.agents {
            println!("  {line}");
        }
    }
}

/// Run the 3-step install wizard. Returns the process exit code.
pub fn run(binary: &str) -> i32 {
    let theme = ColorfulTheme::default();

    let home = match env::var_os("HOME") {
        Some(h) => PathBuf::from(h),
        None => {
            eprintln!("token-saver install: $HOME is not set");
            return 1;
        }
    };

    println!();
    println!("token-saver install — setup wizard");

    println!();
    println!("Step 1/3 — shell hook");
    let shell_outcome = match step_shell(&theme, &home, binary) {
        Ok(o) => o,
        Err(code) => return code,
    };
    println!("Done");

    println!();
    println!("Step 2/3 — scope");
    let (scope, root) = match step_scope(&theme, &home) {
        Ok(s) => s,
        Err(code) => return code,
    };
    println!("Done");

    println!();
    println!("Step 3/3 — agents");
    if scope == Scope::Global {
        println!(
            "Note: detection covers only default locations under ~ — custom installs are not detected."
        );
    }
    let (agent_outcomes, agents_failed) = match step_agents(&theme, scope, &root, &home) {
        Ok(o) => o,
        Err(code) => return code,
    };
    println!("Done");

    let scope_label = match scope {
        Scope::Global => "global — config under ~".to_string(),
        Scope::Project => format!("project — {}", root.display()),
    };
    Summary {
        shell: shell_outcome,
        scope: scope_label,
        agents: agent_outcomes,
    }
    .print();

    println!();
    println!("Reload your shell to pick up the wrappers (e.g. `source ~/.zshenv`).");
    if agents_failed {
        println!("Some agents failed to configure — see errors above.");
        1
    } else {
        println!("Done! Ready to go!");
        0
    }
}

/// Step 1: offer to write the shell hook, or print it for manual setup.
/// An existing hook is called out before prompting.
fn step_shell(theme: &ColorfulTheme, home: &Path, binary: &str) -> Result<String, i32> {
    let Some(shell) = install::detect_shell() else {
        println!();
        println!("Could not detect a supported shell from $SHELL (need zsh or bash).");
        println!("Add this to your shell profile manually:");
        println!("    eval \"$(token-saver install <zsh|bash>)\"");
        return Ok("unsupported shell — manual setup printed".to_string());
    };
    let profile = install::profile_path(home, &shell);
    let shown = display_path(&profile, home);
    let already = install::profile_has_hook(&profile);

    let yes_label = if already {
        format!("Yes — refresh the hook in {shown} (already installed)")
    } else {
        format!("Yes — add the hook to {shown}")
    };
    let items = [yes_label, "No — I'll do it myself".to_string()];
    let choice = Select::with_theme(theme)
        .with_prompt("Set up the shell hook?")
        .items(&items)
        .default(0)
        .interact()
        .map_err(prompt_failed)?;

    if choice == 0 {
        install::update_shell_profile(&profile, &shell, binary).map_err(|e| {
            eprintln!(
                "token-saver install: failed to update {}: {e}",
                profile.display()
            );
            1
        })?;
        return Ok(if already {
            format!("refreshed → {shown}")
        } else {
            format!("added → {shown}")
        });
    }

    println!();
    println!("Add this to {shown}:");
    println!("    eval \"$(token-saver install {shell})\"");
    Ok(format!("manual — eval line printed for {shown}"))
}

/// Step 2: global vs project scope. Returns the scope and its config root
/// (`$HOME` or the resolved project root).
fn step_scope(theme: &ColorfulTheme, home: &Path) -> Result<(Scope, PathBuf), i32> {
    let choice = Select::with_theme(theme)
        .with_prompt("Install scope?")
        .items([
            "Global — all projects (config under ~)",
            "Project — this repository only",
        ])
        .default(0)
        .interact()
        .map_err(prompt_failed)?;

    if choice == 0 {
        return Ok((Scope::Global, home.to_path_buf()));
    }

    let root = project_root().map_err(|e| {
        eprintln!("token-saver install: project scope requires a Git repository: {e}");
        1
    })?;
    println!("Using project root: {}", root.display());
    Ok((Scope::Project, root))
}

/// Step 3: pick agents via checkboxes (scriptable pre-checked when detected
/// or already configured) or print manual instructions for everything.
///
/// Returns the summary lines plus whether any agent write failed. A failed
/// agent is reported and recorded but does not abort the remaining picks —
/// the wizard still reaches its closing summary.
fn step_agents(
    theme: &ColorfulTheme,
    scope: Scope,
    root: &Path,
    home: &Path,
) -> Result<(Vec<String>, bool), i32> {
    let choice = Select::with_theme(theme)
        .with_prompt("Configure agents?")
        .items(["Select agents", "None — I'll do it myself"])
        .default(0)
        .interact()
        .map_err(prompt_failed)?;

    if choice == 1 {
        print_manual_instructions(scope, root);
        return Ok((
            vec!["manual instructions printed for all agents".to_string()],
            false,
        ));
    }

    let mut items: Vec<String> = Vec::new();
    let mut defaults: Vec<bool> = Vec::new();
    for agent in ScriptableAgent::ALL {
        let configured = agent.is_configured(scope, root);
        let detected = agent.detected(scope, root);
        items.push(agent_label(agent.name(), configured, detected));
        defaults.push(detected || configured);
    }
    for agent in ManualAgent::ALL {
        let detected = agent.detected(scope, root);
        items.push(if detected {
            format!("{} (manual — prints instructions)", agent.name())
        } else {
            format!(
                "{} (not detected; manual — prints instructions)",
                agent.name()
            )
        });
        defaults.push(false);
    }

    let picked = MultiSelect::with_theme(theme)
        .with_prompt("Agents to configure")
        .items(&items)
        .defaults(&defaults)
        .interact()
        .map_err(prompt_failed)?;

    if picked.is_empty() {
        return Ok((vec!["agents       none selected".to_string()], false));
    }

    let mut outcomes = Vec::new();
    let mut failed = false;
    for idx in picked {
        match resolve_pick(idx) {
            AgentPick::Scriptable(agent) => {
                let path = display_path(&agent.config_path(scope, root), home);
                match agent.write(scope, root) {
                    Ok(true) => {
                        println!("Configured {} ({path})", agent.name());
                        print_project_trust_note(agent, scope, root);
                        outcomes.push(format!("{:<13} configured → {path}", agent.name()));
                    }
                    Ok(false) => {
                        println!("{} already configured — skipping", agent.name());
                        print_project_trust_note(agent, scope, root);
                        outcomes.push(format!("{:<13} already configured", agent.name()));
                    }
                    Err(e) => {
                        eprintln!(
                            "token-saver install: failed to configure {}: {e}",
                            agent.name()
                        );
                        outcomes.push(format!("{:<13} failed — {e}", agent.name()));
                        failed = true;
                    }
                }
            }
            AgentPick::Manual(agent) => {
                println!();
                println!("{}", agent.manual_instructions());
                outcomes.push(format!("{:<13} manual instructions printed", agent.name()));
            }
        }
    }
    Ok((outcomes, failed))
}

/// Print project-trust caveats after writes and already-configured results.
fn print_project_trust_note(agent: ScriptableAgent, scope: Scope, root: &Path) {
    if scope != Scope::Project {
        return;
    }
    match agent {
        ScriptableAgent::Pi => println!("  note: {}", pi_project_trust_note()),
        ScriptableAgent::Codex => println!("  note: {}", codex_project_trust_note(root)),
        ScriptableAgent::Claude => {}
    }
}

/// A checkbox index resolved to its concrete agent target.
enum AgentPick {
    Scriptable(ScriptableAgent),
    Manual(ManualAgent),
}

/// Map a MultiSelect index to its agent. Items are listed scriptable-first
/// (`ScriptableAgent::ALL`) then manual (`ManualAgent::ALL`), so the index
/// space is `[0, scriptable_count)` for scriptable and the remainder manual.
fn resolve_pick(idx: usize) -> AgentPick {
    let scriptable_count = ScriptableAgent::ALL.len();
    if idx < scriptable_count {
        AgentPick::Scriptable(ScriptableAgent::ALL[idx])
    } else {
        AgentPick::Manual(ManualAgent::ALL[idx - scriptable_count])
    }
}

/// Checkbox label for a scriptable agent, reflecting existing state.
fn agent_label(name: &str, configured: bool, detected: bool) -> String {
    if configured {
        format!("{name} (already configured)")
    } else if detected {
        name.to_string()
    } else {
        format!("{name} (not detected — will create)")
    }
}

/// Print manual setup instructions for every agent.
fn print_manual_instructions(scope: Scope, root: &Path) {
    for agent in ScriptableAgent::ALL {
        println!();
        println!("{}", agent.manual_instructions(scope, root));
    }
    for agent in ManualAgent::ALL {
        println!();
        println!("{}", agent.manual_instructions());
    }
}

/// Display a path with `~` substituted for the home directory.
fn display_path(path: &Path, home: &Path) -> String {
    match path.strip_prefix(home) {
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => path.display().to_string(),
    }
}

/// Resolve the current Git worktree root for project-scoped configuration.
pub(crate) fn project_root() -> std::io::Result<PathBuf> {
    project_root_at(&env::current_dir()?)
}

fn project_root_at(cwd: &Path) -> std::io::Result<PathBuf> {
    let out = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(cwd)
        .output()?;
    if !out.status.success() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("{} is not inside a Git worktree", cwd.display()),
        ));
    }
    let stdout = String::from_utf8(out.stdout).map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("git returned a non-UTF-8 worktree path: {e}"),
        )
    })?;
    let root = stdout.strip_suffix('\n').unwrap_or(&stdout);
    let root = root.strip_suffix('\r').unwrap_or(root);
    if root.is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "git returned an empty worktree path",
        ));
    }
    Ok(PathBuf::from(root))
}

fn prompt_failed(e: dialoguer::Error) -> i32 {
    eprintln!("token-saver install: prompt failed: {e}");
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_path_shortens_home_prefix() {
        let home = Path::new("/Users/x");
        assert_eq!(display_path(&home.join(".zshenv"), home), "~/.zshenv");
    }

    #[test]
    fn display_path_keeps_paths_outside_home() {
        let home = Path::new("/Users/x");
        assert_eq!(
            display_path(Path::new("/repo/.pi/settings.json"), home),
            "/repo/.pi/settings.json"
        );
    }

    #[test]
    fn project_root_rejects_directory_outside_a_repository() {
        let directory = tempfile::tempdir().unwrap();
        let error = project_root_at(directory.path()).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
    }

    #[test]
    fn project_root_preserves_trailing_path_whitespace() {
        let container = tempfile::tempdir().unwrap();
        let repository = container.path().join("repo ");
        std::fs::create_dir(&repository).unwrap();
        let output = Command::new("git")
            .args(["init", "--quiet"])
            .current_dir(&repository)
            .output()
            .unwrap();
        assert!(output.status.success(), "git init failed: {output:?}");

        let root = project_root_at(&repository).unwrap();

        assert_eq!(root, std::fs::canonicalize(repository).unwrap());
    }

    #[test]
    fn resolve_pick_maps_indices_to_targets() {
        // items are scriptable-first (claude, pi, codex) then manual (opencode, cursor)
        assert!(matches!(
            resolve_pick(0),
            AgentPick::Scriptable(ScriptableAgent::Claude)
        ));
        assert!(matches!(
            resolve_pick(2),
            AgentPick::Scriptable(ScriptableAgent::Codex)
        ));
        assert!(matches!(
            resolve_pick(3),
            AgentPick::Manual(ManualAgent::Opencode)
        ));
        assert!(matches!(
            resolve_pick(4),
            AgentPick::Manual(ManualAgent::Cursor)
        ));
    }

    #[test]
    fn agent_label_reflects_state() {
        assert_eq!(
            agent_label("claude", true, true),
            "claude (already configured)"
        );
        assert_eq!(agent_label("claude", false, true), "claude");
        assert_eq!(
            agent_label("claude", false, false),
            "claude (not detected — will create)"
        );
    }
}
