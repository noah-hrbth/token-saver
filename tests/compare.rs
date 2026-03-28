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
