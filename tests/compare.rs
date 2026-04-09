mod common;

use common::ScenarioResult;

#[test]
#[ignore]
fn compare_git_status() {
    let scenarios = common::git_status::scenarios();
    let mut results = Vec::new();

    for scenario in &scenarios {
        let (raw, comp) = common::run_compare(scenario);
        results.push(ScenarioResult {
            name: scenario.name.to_string(),
            raw_tokens: raw,
            compressed_tokens: comp,
        });
    }

    common::print_summary(&results);
}

#[test]
#[ignore]
fn compare_git_branch() {
    let scenarios = common::git_branch::scenarios();
    let mut results = Vec::new();

    for scenario in &scenarios {
        let (raw, comp) = common::run_compare(scenario);
        results.push(ScenarioResult {
            name: scenario.name.to_string(),
            raw_tokens: raw,
            compressed_tokens: comp,
        });
    }

    common::print_summary(&results);
}

#[test]
#[ignore]
fn compare_git_diff() {
    let scenarios = common::git_diff::scenarios();
    let mut results = Vec::new();

    for scenario in &scenarios {
        let (raw, comp) = common::run_compare(scenario);
        results.push(ScenarioResult {
            name: scenario.name.to_string(),
            raw_tokens: raw,
            compressed_tokens: comp,
        });
    }

    common::print_summary(&results);
}

#[test]
#[ignore]
fn compare_git_log() {
    let scenarios = common::git_log::scenarios();
    let mut results = Vec::new();

    for scenario in &scenarios {
        let (raw, comp) = common::run_compare(scenario);
        results.push(ScenarioResult {
            name: scenario.name.to_string(),
            raw_tokens: raw,
            compressed_tokens: comp,
        });
    }

    common::print_summary(&results);
}

#[test]
#[ignore]
fn compare_git_show() {
    let scenarios = common::git_show::scenarios();
    let mut results = Vec::new();

    for scenario in &scenarios {
        let (raw, comp) = common::run_compare(scenario);
        results.push(ScenarioResult {
            name: scenario.name.to_string(),
            raw_tokens: raw,
            compressed_tokens: comp,
        });
    }

    common::print_summary(&results);
}

#[test]
#[ignore]
fn compare_ls() {
    let scenarios = common::ls::scenarios();
    let mut results = Vec::new();

    for scenario in &scenarios {
        let (raw, comp) = common::run_compare(scenario);
        results.push(ScenarioResult {
            name: scenario.name.to_string(),
            raw_tokens: raw,
            compressed_tokens: comp,
        });
    }

    common::print_summary(&results);
}

#[test]
#[ignore]
fn compare_find() {
    let scenarios = common::find::scenarios();
    let mut results = Vec::new();

    for scenario in &scenarios {
        let (raw, comp) = common::run_compare(scenario);
        results.push(ScenarioResult {
            name: scenario.name.to_string(),
            raw_tokens: raw,
            compressed_tokens: comp,
        });
    }

    common::print_summary(&results);
}

#[test]
#[ignore]
fn compare_cat() {
    let scenarios = common::cat::scenarios();
    let mut results = Vec::new();

    for scenario in &scenarios {
        let (raw, comp) = common::run_compare(scenario);
        results.push(ScenarioResult {
            name: scenario.name.to_string(),
            raw_tokens: raw,
            compressed_tokens: comp,
        });
    }

    common::print_summary(&results);
}

#[test]
#[ignore]
fn compare_grep() {
    let scenarios = common::grep::scenarios();
    let mut results = Vec::new();

    for scenario in &scenarios {
        let (raw, comp) = common::run_compare(scenario);
        results.push(ScenarioResult {
            name: scenario.name.to_string(),
            raw_tokens: raw,
            compressed_tokens: comp,
        });
    }

    common::print_summary(&results);
}

#[test]
#[ignore]
fn compare_jest() {
    if !std::process::Command::new("jest")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        eprintln!("Skipping jest compare: jest not found in PATH (use compare_npx_jest instead)");
        return;
    }

    let scenarios = common::jest::scenarios();
    let mut results = Vec::new();

    for scenario in &scenarios {
        let (raw, comp) = common::run_compare(scenario);
        results.push(ScenarioResult {
            name: scenario.name.to_string(),
            raw_tokens: raw,
            compressed_tokens: comp,
        });
    }

    common::print_summary(&results);
}

#[test]
#[ignore]
fn compare_npx_jest() {
    if !std::process::Command::new("npx")
        .args(["jest", "--version"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        eprintln!("Skipping npx jest compare: npx jest not available");
        return;
    }

    let scenarios = common::jest::npx_scenarios();
    let mut results = Vec::new();

    for scenario in &scenarios {
        let (raw, comp) = common::run_compare(scenario);
        results.push(ScenarioResult {
            name: scenario.name.to_string(),
            raw_tokens: raw,
            compressed_tokens: comp,
        });
    }

    common::print_summary(&results);
}

#[test]
#[ignore]
fn compare_eslint() {
    if !std::process::Command::new("eslint")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        eprintln!("Skipping eslint compare: eslint not found in PATH");
        return;
    }

    let scenarios = common::eslint::scenarios();
    let mut results = Vec::new();

    for scenario in &scenarios {
        let (raw, comp) = common::run_compare(scenario);
        results.push(ScenarioResult {
            name: scenario.name.to_string(),
            raw_tokens: raw,
            compressed_tokens: comp,
        });
    }

    common::print_summary(&results);
}

#[test]
#[ignore]
fn compare_prettier() {
    if !std::process::Command::new("prettier")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        eprintln!("Skipping prettier compare: prettier not found in PATH");
        return;
    }

    let scenarios = common::prettier::scenarios();
    let mut results = Vec::new();

    for scenario in &scenarios {
        let (raw, comp) = common::run_compare(scenario);
        results.push(ScenarioResult {
            name: scenario.name.to_string(),
            raw_tokens: raw,
            compressed_tokens: comp,
        });
    }

    common::print_summary(&results);
}

#[test]
#[ignore]
fn compare_npx_prettier() {
    if !std::process::Command::new("npx")
        .args(["prettier", "--version"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        eprintln!("Skipping npx prettier compare: npx prettier not available");
        return;
    }

    let scenarios = common::prettier::npx_scenarios();
    let mut results = Vec::new();

    for scenario in &scenarios {
        let (raw, comp) = common::run_compare(scenario);
        results.push(ScenarioResult {
            name: scenario.name.to_string(),
            raw_tokens: raw,
            compressed_tokens: comp,
        });
    }

    common::print_summary(&results);
}
