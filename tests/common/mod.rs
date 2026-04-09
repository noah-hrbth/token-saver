#![allow(dead_code)]

pub mod cat;
pub mod eslint;
pub mod find;
pub mod git_branch;
pub mod git_diff;
pub mod git_log;
pub mod git_show;
pub mod git_status;
pub mod grep;
pub mod jest;
pub mod ls;
pub mod prettier;

use std::path::Path;
use std::process::Command;

/// Get the path to the compiled token-saver binary.
pub fn binary_path() -> String {
    env!("CARGO_BIN_EXE_token-saver").to_string()
}

/// Create a temporary git repo with an initial commit (README.md).
pub fn create_temp_repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path();

    Command::new("git")
        .args(["init"])
        .current_dir(path)
        .output()
        .unwrap();
    Command::new("git")
        .args(["config", "user.email", "test@test.com"])
        .current_dir(path)
        .output()
        .unwrap();
    Command::new("git")
        .args(["config", "user.name", "Test"])
        .current_dir(path)
        .output()
        .unwrap();

    std::fs::write(path.join("README.md"), "init").unwrap();
    Command::new("git")
        .args(["add", "README.md"])
        .current_dir(path)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "init"])
        .current_dir(path)
        .output()
        .unwrap();

    dir
}

/// An assertion to check against compressed output.
pub enum Assertion {
    Contains(&'static str),
    NotContains(&'static str),
}

/// A test scenario: setup function + expected behavior.
pub struct Scenario {
    pub name: &'static str,
    pub command: &'static str,
    pub args: &'static [&'static str],
    pub setup: fn(&Path),
    pub assertions: Vec<Assertion>,
}

/// Run a scenario with TOKEN_SAVER=1 and verify assertions.
pub fn run_test(scenario: &Scenario) {
    run_test_with_exit_codes(scenario, &[0]);
}

/// Like `run_test`, but accepts any of the given exit codes as success.
/// Useful for commands like eslint that return 1 when lint problems are found.
pub fn run_test_with_exit_codes(scenario: &Scenario, expected: &[i32]) {
    let repo = create_temp_repo();
    (scenario.setup)(repo.path());

    let args: Vec<&str> = std::iter::once(scenario.command)
        .chain(scenario.args.iter().copied())
        .collect();

    let output = Command::new(binary_path())
        .args(&args)
        .env("TOKEN_SAVER", "1")
        .current_dir(repo.path())
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let exit_code = output.status.code().unwrap_or(-1);

    assert!(
        expected.contains(&exit_code),
        "Scenario '{}': expected exit code in {:?}, got {}\nstdout: {}\nstderr: {}",
        scenario.name,
        expected,
        exit_code,
        stdout,
        String::from_utf8_lossy(&output.stderr),
    );

    for assertion in &scenario.assertions {
        match assertion {
            Assertion::Contains(s) => {
                assert!(
                    stdout.contains(s),
                    "Scenario '{}': expected '{}' in:\n{}",
                    scenario.name,
                    s,
                    stdout
                );
            }
            Assertion::NotContains(s) => {
                assert!(
                    !stdout.contains(s),
                    "Scenario '{}': did not expect '{}' in:\n{}",
                    scenario.name,
                    s,
                    stdout
                );
            }
        }
    }
}

/// Run a scenario in compare mode: raw vs compressed, with token counts.
/// Returns (raw_tokens, compressed_tokens).
pub fn run_compare(scenario: &Scenario) -> (usize, usize) {
    let repo = create_temp_repo();
    (scenario.setup)(repo.path());

    let args: Vec<&str> = std::iter::once(scenario.command)
        .chain(scenario.args.iter().copied())
        .collect();

    let raw = Command::new(binary_path())
        .args(&args)
        .env_remove("TOKEN_SAVER")
        .current_dir(repo.path())
        .output()
        .unwrap();
    // Combine stdout+stderr for raw output — agents see both streams.
    // Some commands (e.g. prettier) write diagnostics to stderr.
    let raw_out = String::from_utf8_lossy(&raw.stdout);
    let raw_err = String::from_utf8_lossy(&raw.stderr);
    let raw_stdout = if raw_err.is_empty() {
        raw_out.to_string()
    } else if raw_out.is_empty() {
        raw_err.to_string()
    } else {
        format!("{}\n{}", raw_out.trim_end(), raw_err)
    };

    let comp = Command::new(binary_path())
        .args(&args)
        .env("TOKEN_SAVER", "1")
        .current_dir(repo.path())
        .output()
        .unwrap();
    let comp_stdout = String::from_utf8_lossy(&comp.stdout).to_string();

    let raw_tokens = estimate_tokens(&raw_stdout);
    let comp_tokens = estimate_tokens(&comp_stdout);

    // Visual output
    let dim = "\x1b[2m";
    let bold = "\x1b[1m";
    let red = "\x1b[0;31m";
    let green = "\x1b[0;32m";
    let cyan = "\x1b[0;36m";
    let reset = "\x1b[0m";

    let sep: String = std::iter::repeat_n('─', 70).collect();

    println!("\n{dim}{sep}{reset}");
    println!("{bold}{cyan} SCENARIO: {}{reset}", scenario.name);
    println!("{dim}{sep}{reset}");

    println!(
        "\n{dim}  $ {} {}{reset}",
        scenario.command,
        scenario.args.join(" ")
    );

    println!(
        "\n{bold}{red}▸ Raw output{reset}  {dim}({} tokens, {} chars){reset}",
        raw_tokens,
        raw_stdout.len()
    );
    for line in raw_stdout.lines() {
        println!("  {}", line);
    }

    println!(
        "\n{bold}{green}▸ Compressed output{reset}  {dim}({} tokens, {} chars){reset}",
        comp_tokens,
        comp_stdout.len()
    );
    for line in comp_stdout.lines() {
        println!("  {}", line);
    }

    if raw_tokens > 0 {
        let saved = raw_tokens as isize - comp_tokens as isize;
        let pct = saved * 100 / raw_tokens as isize;

        if saved > 0 {
            println!(
                "\n  {green}{bold}↓ {} tokens saved ({}% reduction){reset}",
                saved, pct
            );
        } else if saved == 0 {
            println!("\n  {bold}→ No token savings{reset}");
        } else {
            println!(
                "\n  {red}{bold}↑ {} tokens added (compressed is larger){reset}",
                -saved
            );
        }
    }

    (raw_tokens, comp_tokens)
}

/// Result from a single scenario comparison.
pub struct ScenarioResult {
    pub name: String,
    pub raw_tokens: usize,
    pub compressed_tokens: usize,
}

/// Print a summary table of all scenarios plus totals.
pub fn print_summary(results: &[ScenarioResult]) {
    let dim = "\x1b[2m";
    let bold = "\x1b[1m";
    let red = "\x1b[0;31m";
    let green = "\x1b[0;32m";
    let cyan = "\x1b[0;36m";
    let reset = "\x1b[0m";

    let sep: String = std::iter::repeat_n('─', 70).collect();

    println!("\n{dim}{sep}{reset}");
    println!("{bold}{cyan} SUMMARY — {} scenarios{reset}", results.len());
    println!("{dim}{sep}{reset}");

    // Table header
    println!(
        "\n  {bold}{:<38} {:>5}  {:>5}  {:>10}{reset}",
        "Scenario", "Raw", "Comp", "Saved"
    );
    let rule = "──────────────────────────────────────";
    println!("  {dim}{rule} ─────  ─────  ──────────{reset}");

    // Per-scenario rows
    let mut total_raw = 0usize;
    let mut total_comp = 0usize;

    for r in results {
        total_raw += r.raw_tokens;
        total_comp += r.compressed_tokens;

        let saved = r.raw_tokens as isize - r.compressed_tokens as isize;
        let pct = if r.raw_tokens > 0 {
            saved * 100 / r.raw_tokens as isize
        } else {
            0
        };
        let saved_col = format!("{} ({}%)", saved, pct);

        // Truncate long scenario names
        let name: String = if r.name.len() > 36 {
            format!("{}...", &r.name[..33])
        } else {
            r.name.clone()
        };

        println!(
            "  {:<38} {red}{:>5}{reset}  {green}{:>5}{reset}  {bold}{:>10}{reset}",
            name, r.raw_tokens, r.compressed_tokens, saved_col
        );
    }

    // Totals
    let total_saved = total_raw as isize - total_comp as isize;
    let total_pct = if total_raw > 0 {
        total_saved * 100 / total_raw as isize
    } else {
        0
    };
    let total_saved_col = format!("{} ({}%)", total_saved, total_pct);

    println!("  {dim}{rule} ─────  ─────  ──────────{reset}");
    println!(
        "  {bold}{:<38} {red}{:>5}{reset}  {green}{bold}{:>5}{reset}  {bold}{:>10}{reset}",
        "TOTAL", total_raw, total_comp, total_saved_col
    );
    println!();
}

/// Estimate token count using OpenAI's cl100k_base BPE tokenizer.
/// ~70% vocabulary overlap with Claude's tokenizer — accurate enough
/// for relative comparisons between raw and compressed output.
pub fn estimate_tokens(text: &str) -> usize {
    use tiktoken_rs::cl100k_base;

    let bpe = cl100k_base().expect("failed to load cl100k_base tokenizer");
    bpe.encode_with_special_tokens(text).len()
}
