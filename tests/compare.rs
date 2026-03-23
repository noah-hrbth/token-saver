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
