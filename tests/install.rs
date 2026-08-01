//! End-to-end coverage for `token-saver install` / `token-saver uninstall`.
//!
//! These drive the real binary with a throwaway `$HOME` so the shell-profile
//! rewrite and agent-config paths are exercised together. `Command::output()`
//! leaves stdin non-interactive, so `install` takes the silent `auto()` path
//! rather than the wizard.

mod common;

use std::fs;
use std::path::Path;
use std::process::Command;

/// Run a token-saver subcommand with `home` as `$HOME` and zsh as `$SHELL`.
fn run(args: &[&str], home: &Path, cwd: &Path) -> std::process::Output {
    Command::new(common::binary_path())
        .args(args)
        .env("HOME", home)
        .env("SHELL", "/bin/zsh")
        .env_remove("TOKEN_SAVER")
        .current_dir(cwd)
        .output()
        .unwrap()
}

/// Create the detection dirs that mark each agent as "in use" under `$HOME`.
fn mark_agents_detected(home: &Path) {
    fs::create_dir_all(home.join(".claude")).unwrap();
    fs::create_dir_all(home.join(".pi").join("agent")).unwrap();
    fs::create_dir_all(home.join(".codex")).unwrap();
}

#[test]
fn install_writes_shell_hook_and_detected_agent_configs() {
    let home = tempfile::tempdir().unwrap();
    mark_agents_detected(home.path());

    let out = run(&["install"], home.path(), home.path());
    assert!(out.status.success(), "install failed: {out:?}");

    let profile = fs::read_to_string(home.path().join(".zshenv")).unwrap();
    assert!(profile.contains("install zsh)"), "hook missing: {profile}");

    let claude = fs::read_to_string(home.path().join(".claude/settings.json")).unwrap();
    assert!(claude.contains("\"TOKEN_SAVER\""));

    let pi = fs::read_to_string(home.path().join(".pi/agent/settings.json")).unwrap();
    assert!(pi.contains("shellCommandPrefix"));
    assert!(
        pi.contains("# token-saver:end"),
        "region not delimited: {pi}"
    );

    let codex = fs::read_to_string(home.path().join(".codex/config.toml")).unwrap();
    assert!(codex.contains("shell_environment_policy"));
    assert!(codex.contains("TOKEN_SAVER"));
}

#[test]
fn install_skips_undetected_agents() {
    let home = tempfile::tempdir().unwrap();
    // only claude is in use
    fs::create_dir_all(home.path().join(".claude")).unwrap();

    let out = run(&["install"], home.path(), home.path());
    assert!(out.status.success(), "install failed: {out:?}");

    assert!(home.path().join(".claude/settings.json").exists());
    assert!(!home.path().join(".pi/agent/settings.json").exists());
    assert!(!home.path().join(".codex/config.toml").exists());
}

#[test]
fn install_is_idempotent() {
    let home = tempfile::tempdir().unwrap();
    mark_agents_detected(home.path());

    run(&["install"], home.path(), home.path());
    let first_profile = fs::read_to_string(home.path().join(".zshenv")).unwrap();
    let first_pi = fs::read_to_string(home.path().join(".pi/agent/settings.json")).unwrap();

    run(&["install"], home.path(), home.path());
    let second_profile = fs::read_to_string(home.path().join(".zshenv")).unwrap();
    let second_pi = fs::read_to_string(home.path().join(".pi/agent/settings.json")).unwrap();

    assert_eq!(first_profile, second_profile);
    assert_eq!(first_pi, second_pi);
    assert_eq!(second_profile.matches("install zsh)").count(), 1);
}

#[test]
fn install_preserves_unrelated_profile_content() {
    let home = tempfile::tempdir().unwrap();
    fs::create_dir_all(home.path().join(".claude")).unwrap();
    fs::write(home.path().join(".zshenv"), "export EDITOR=vim\n").unwrap();

    run(&["install"], home.path(), home.path());

    let profile = fs::read_to_string(home.path().join(".zshenv")).unwrap();
    assert!(profile.contains("export EDITOR=vim"));
    assert!(profile.contains("install zsh)"));
}

#[test]
fn install_upgrades_a_legacy_undelimited_pi_snippet() {
    // the headline regression: a settings.json written by the pre-delimiter
    // release must be upgradable and, afterwards, fully removable
    let home = tempfile::tempdir().unwrap();
    fs::create_dir_all(home.path().join(".pi").join("agent")).unwrap();
    let settings = home.path().join(".pi/agent/settings.json");

    let legacy_snippet = "# token-saver\n\
        if command -v token-saver >/dev/null 2>&1; then\n\
        \x20   export TOKEN_SAVER=1\n\
        \x20   eval \"$(token-saver install bash)\"\n\
        fi";
    let legacy = serde_json::json!({
        "shellCommandPrefix": format!("shopt -s expand_aliases\n{legacy_snippet}"),
    });
    fs::write(&settings, serde_json::to_string_pretty(&legacy).unwrap()).unwrap();

    run(&["install"], home.path(), home.path());

    let upgraded = fs::read_to_string(&settings).unwrap();
    assert!(
        upgraded.contains("# token-saver:end"),
        "not upgraded: {upgraded}"
    );
    assert!(
        upgraded.contains("shopt -s expand_aliases"),
        "user content lost: {upgraded}"
    );
    // the begin marker must not be duplicated by the upgrade
    assert_eq!(
        upgraded.matches("# token-saver:begin").count(),
        1,
        "snippet duplicated: {upgraded}"
    );

    // and the upgraded shape is no longer stranded
    run(&["uninstall"], home.path(), home.path());
    let after = fs::read_to_string(&settings).unwrap();
    assert!(!after.contains("token-saver"), "stranded: {after}");
    assert!(after.contains("shopt -s expand_aliases"));
}

#[test]
fn uninstall_reverses_install() {
    let home = tempfile::tempdir().unwrap();
    mark_agents_detected(home.path());
    fs::write(home.path().join(".zshenv"), "export EDITOR=vim\n").unwrap();

    run(&["install"], home.path(), home.path());
    let out = run(&["uninstall"], home.path(), home.path());
    assert!(out.status.success(), "uninstall failed: {out:?}");

    let profile = fs::read_to_string(home.path().join(".zshenv")).unwrap();
    assert!(
        !profile.contains("token-saver"),
        "hook left behind: {profile}"
    );
    // unrelated user content survives the round trip
    assert!(profile.contains("export EDITOR=vim"));

    let claude = fs::read_to_string(home.path().join(".claude/settings.json")).unwrap();
    assert!(!claude.contains("TOKEN_SAVER"));

    let pi = fs::read_to_string(home.path().join(".pi/agent/settings.json")).unwrap();
    assert!(!pi.contains("token-saver"), "pi prefix left behind: {pi}");

    let codex = fs::read_to_string(home.path().join(".codex/config.toml")).unwrap();
    assert!(!codex.contains("TOKEN_SAVER"));
}

#[test]
fn uninstall_outside_a_git_repo_leaves_the_cwd_untouched() {
    // regression: uninstall used to fall back to the cwd as "project root" and
    // rewrite agent config in whatever directory the user was standing in
    let home = tempfile::tempdir().unwrap();
    let elsewhere = tempfile::tempdir().unwrap();

    // precondition — the cwd must genuinely not be inside a working tree
    let probe = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(elsewhere.path())
        .output()
        .unwrap();
    assert!(
        !probe.status.success(),
        "test setup: temp dir unexpectedly inside a git repo"
    );

    let settings = elsewhere.path().join(".claude").join("settings.json");
    fs::create_dir_all(settings.parent().unwrap()).unwrap();
    let original = "{\n  \"env\": {\n    \"TOKEN_SAVER\": \"1\"\n  }\n}\n";
    fs::write(&settings, original).unwrap();

    let out = run(&["uninstall"], home.path(), elsewhere.path());
    assert!(out.status.success(), "uninstall failed: {out:?}");

    assert_eq!(fs::read_to_string(&settings).unwrap(), original);
}

#[test]
fn uninstall_cleans_project_config_inside_a_git_repo() {
    let home = tempfile::tempdir().unwrap();
    let repo = common::create_temp_repo();

    let settings = repo.path().join(".claude").join("settings.json");
    fs::create_dir_all(settings.parent().unwrap()).unwrap();
    fs::write(&settings, "{ \"env\": { \"TOKEN_SAVER\": \"1\" } }").unwrap();

    let out = run(&["uninstall"], home.path(), repo.path());
    assert!(out.status.success(), "uninstall failed: {out:?}");

    let cleaned = fs::read_to_string(&settings).unwrap();
    assert!(!cleaned.contains("TOKEN_SAVER"), "not cleaned: {cleaned}");
}

#[test]
fn uninstall_cleans_project_pi_config_when_home_is_the_repo() {
    // arrange
    let temporary_directory = tempfile::tempdir().unwrap();
    let home = temporary_directory.path().join("home ");
    fs::create_dir(&home).unwrap();
    let home = fs::canonicalize(home).unwrap();
    let init = Command::new("git")
        .args(["init", "--quiet"])
        .current_dir(&home)
        .output()
        .unwrap();
    assert!(init.status.success(), "git init failed: {init:?}");
    let settings = home.join(".pi").join("settings.json");
    fs::create_dir_all(settings.parent().unwrap()).unwrap();
    fs::write(
        &settings,
        r##"{"shellCommandPrefix":"# token-saver:begin\nexport TOKEN_SAVER=1\n# token-saver:end"}"##,
    )
    .unwrap();

    // act
    let out = run(&["uninstall"], &home, &home);

    // assert
    assert!(out.status.success(), "uninstall failed: {out:?}");
    let cleaned = fs::read_to_string(&settings).unwrap();
    assert!(!cleaned.contains("token-saver"), "not cleaned: {cleaned}");
}
