use std::collections::BTreeMap;
use std::collections::HashMap;

use serde::Deserialize;
use serde_json::Value;

use crate::compressors::Compressor;
use crate::compressors::report::{
    Group, Item, MidTotalOverflow, Noun, ReportConfig, relativize_path, render_groups,
};

const MAX_FAILURES_PER_SUITE: usize = 10;
const MAX_FAILURES_TOTAL: usize = 20;
const MAX_ERROR_LINES: usize = 15;

// ── Serde structs ─────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct JestResult {
    success: bool,
    #[serde(rename = "numPassedTests", default)]
    num_passed_tests: u64,
    #[serde(rename = "numFailedTests", default)]
    num_failed_tests: u64,
    #[serde(rename = "numPendingTests", default)]
    num_pending_tests: u64,
    #[serde(rename = "numTodoTests", default)]
    num_todo_tests: u64,
    #[serde(rename = "numTotalTestSuites", default)]
    num_total_test_suites: u64,
    #[serde(rename = "testResults", default)]
    test_results: Vec<JestTestSuiteResult>,
    #[serde(rename = "coverageMap")]
    coverage_map: Option<HashMap<String, Value>>,
}

#[derive(Deserialize)]
struct JestTestSuiteResult {
    /// Absolute path to the test file.
    name: String,
    status: String,
    #[serde(default)]
    message: String,
    #[serde(rename = "assertionResults", default)]
    assertion_results: Vec<JestAssertionResult>,
}

#[derive(Deserialize)]
struct JestAssertionResult {
    #[serde(rename = "ancestorTitles", default)]
    ancestor_titles: Vec<String>,
    title: String,
    status: String,
    #[serde(rename = "failureMessages", default)]
    failure_messages: Vec<String>,
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Returns true when an arg in `args` means we should skip compression and pass through.
pub(crate) fn has_skip_flag(args: &[String]) -> bool {
    for arg in args {
        let a = arg.as_str();
        if a == "--watch"
            || a == "--watchAll"
            || a == "--init"
            || a == "--help"
            || a == "-h"
            || a == "--version"
            || a == "--showConfig"
            || a == "--listReporters"
            || a == "--clearCache"
            || a == "--json"
            || a == "--outputFile"
            || a == "--reporters"
            || a == "--testResultsProcessor"
            || a.starts_with("--outputFile=")
            || a.starts_with("--reporters=")
            || a.starts_with("--testResultsProcessor=")
        {
            return true;
        }
    }
    false
}

pub(crate) struct JestCompressor;

impl Compressor for JestCompressor {
    fn can_compress(&self, args: &[String]) -> bool {
        !has_skip_flag(args)
    }

    fn normalized_args(&self, original_args: &[String]) -> Vec<String> {
        let mut result: Vec<String> = original_args
            .iter()
            .filter(|a| {
                let s = a.as_str();
                s != "--json"
                    && s != "--color"
                    && s != "--colors"
                    && s != "--no-color"
                    && !s.starts_with("--color=")
            })
            .cloned()
            .collect();
        result.push("--json".to_string());
        result.push("--no-color".to_string());
        result
    }

    fn compress(&self, stdout: &str, stderr: &str, exit_code: i32) -> Option<String> {
        compress_jest(stdout, stderr, exit_code)
    }

    fn side_effects(&self) -> bool {
        // running the suite is expensive (and may touch snapshots); never re-run
        true
    }
}

/// Returns a compressor for jest if args are compressible.
pub fn find_compressor(args: &[String]) -> Option<Box<dyn Compressor>> {
    if has_skip_flag(args) {
        return None;
    }
    Some(Box::new(JestCompressor))
}

// ── Internal compress logic ───────────────────────────────────────────────────

pub(crate) fn compress_jest(stdout: &str, _stderr: &str, exit_code: i32) -> Option<String> {
    compress_jest_with_cwd(stdout, exit_code, get_cwd())
}

fn get_cwd() -> Option<String> {
    std::env::current_dir().ok().map(|p| {
        let mut s = p.to_string_lossy().to_string();
        if !s.ends_with('/') {
            s.push('/');
        }
        s
    })
}

fn compress_jest_with_cwd(stdout: &str, exit_code: i32, cwd: Option<String>) -> Option<String> {
    // Only handle normal success/failure exit codes.
    if exit_code != 0 && exit_code != 1 {
        return None;
    }

    let result: JestResult = serde_json::from_str(stdout).ok()?;

    let mut parts: Vec<String> = Vec::new();

    // ── FAIL blocks (shared capped-report renderer) ───────────────────────────
    // Each failed suite is a group; one failed assertion (or one synthesized
    // suite-level message) is one item toward the caps.
    let failed_groups: Vec<Group> = result
        .test_results
        .iter()
        .filter(|s| s.status == "failed")
        .map(|suite| {
            let header = format!("FAIL {}", relativize_path(&suite.name, &cwd));

            let failed_assertions: Vec<&JestAssertionResult> = suite
                .assertion_results
                .iter()
                .filter(|a| a.status == "failed")
                .collect();

            let items: Vec<Item> = if failed_assertions.is_empty() {
                // suite-level failure: one synthesized item; empty block when
                // there is no message (header-only, still counts toward caps)
                let block = if suite.message.is_empty() {
                    String::new()
                } else {
                    truncate_error(&suite.message, MAX_ERROR_LINES)
                        .lines()
                        .map(|l| format!("  {}", l))
                        .collect::<Vec<_>>()
                        .join("\n")
                };
                vec![Item::new(block)]
            } else {
                failed_assertions
                    .iter()
                    .map(|assertion| {
                        let mut lines = vec![format!("  \u{2717} {}", build_test_name(assertion))];
                        for raw_msg in &assertion.failure_messages {
                            let truncated = truncate_error(raw_msg, MAX_ERROR_LINES);
                            for line in truncated.lines() {
                                lines.push(format!("    {}", line));
                            }
                        }
                        Item::new(lines.join("\n"))
                    })
                    .collect()
            };

            Group {
                header,
                items,
                path: None,
            }
        })
        .collect();

    let report = render_groups(
        failed_groups,
        &ReportConfig {
            max_items_per_group: Some(MAX_FAILURES_PER_SUITE),
            max_items_total: MAX_FAILURES_TOTAL,
            items_already_emitted: 0,
            item_noun: Noun::new("failure", "failures"),
            group_noun: Noun::new("suite", "suites"),
            mid_total: MidTotalOverflow::AttributeToTotal,
            enter_first_overflow_group: false,
        },
    );
    parts.extend(report.groups.into_iter().map(|g| g.block));
    if let Some(line) = report.total_overflow {
        parts.push(line);
    }

    // ── Suite list ────────────────────────────────────────────────────────────
    let all_suite_paths: Vec<String> = result
        .test_results
        .iter()
        .map(|s| relativize_path(&s.name, &cwd))
        .collect();

    if !all_suite_paths.is_empty() {
        let groups = group_by_directory(&all_suite_paths);
        let suite_block = render_inline_groups(&groups);
        parts.push(format!("suites:\n{}", suite_block));
    }

    // ── Coverage table ────────────────────────────────────────────────────────
    // gate on data presence, not a CLI flag: config `collectCoverage` or
    // `--coverage=true` populates coverageMap without the bare flag
    if let Some(coverage_map) = &result.coverage_map {
        let table = render_coverage_table(coverage_map, &cwd);
        if !table.is_empty() {
            parts.push(format!("coverage:\n{}", table));
        }
    }

    // ── Summary line ─────────────────────────────────────────────────────────
    parts.push(build_summary(&result));

    Some(parts.join("\n\n"))
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn build_test_name(assertion: &JestAssertionResult) -> String {
    if assertion.ancestor_titles.is_empty() {
        assertion.title.clone()
    } else {
        format!(
            "{} > {}",
            assertion.ancestor_titles.join(" > "),
            assertion.title
        )
    }
}

/// Returns true when an `at` stack frame points at jest/node internals
/// rather than user code (node_modules, `node:` builtins, internal/).
fn is_internal_frame(trimmed_at_line: &str) -> bool {
    trimmed_at_line.contains("node_modules/")
        || trimmed_at_line.contains("(node:")
        || trimmed_at_line.contains("at node:")
        || trimmed_at_line.contains("internal/")
}

/// Strips internal stack frames from a jest failure message.
///
/// Jest `--json` `failureMessages` include full stack traces with 10-20 lines
/// of jest/node internals. The human-readable format hides these. We keep:
/// - All non-`at` lines (assertion error, Expected/Received, blank lines)
/// - The first `at` line that points to user code (the test file location)
/// - Drop all other `at` frames (jest-circus, node internals, etc.)
///
/// When no frame points at user code (e.g. an error thrown entirely inside a
/// dependency) we fall back to the first frame so location is never lost.
fn strip_stack_trace(message: &str) -> String {
    let lines: Vec<&str> = message.lines().collect();

    // index of the at-frame to keep: first user-code frame, else first frame
    let at_indices: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| l.trim_start().starts_with("at "))
        .map(|(i, _)| i)
        .collect();

    let keep_at = at_indices
        .iter()
        .copied()
        .find(|&i| !is_internal_frame(lines[i].trim_start()))
        .or_else(|| at_indices.first().copied());

    let mut result_lines: Vec<&str> = Vec::new();
    for (i, &line) in lines.iter().enumerate() {
        if line.trim_start().starts_with("at ") {
            if Some(i) == keep_at {
                result_lines.push(line);
            }
            // drop every other `at` frame
        } else {
            result_lines.push(line);
        }
    }

    result_lines.join("\n")
}

fn truncate_error(message: &str, max_lines: usize) -> String {
    let stripped = strip_stack_trace(message);
    let lines: Vec<&str> = stripped.lines().collect();
    if lines.len() <= max_lines {
        return stripped;
    }
    let remaining = lines.len() - max_lines;
    let mut result: String = lines[..max_lines].join("\n");
    result.push_str(&format!("\n... ({} more lines)", remaining));
    result
}

/// Groups file paths by their parent directory (trailing `/`).
/// Files without a parent directory use `""` as key.
/// Filenames within each group are sorted alphabetically.
fn group_by_directory(paths: &[String]) -> BTreeMap<String, Vec<String>> {
    let mut groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for path in paths {
        if let Some(slash_pos) = path.rfind('/') {
            let dir = &path[..=slash_pos];
            let file = &path[slash_pos + 1..];
            groups
                .entry(dir.to_string())
                .or_default()
                .push(file.to_string());
        } else {
            groups.entry(String::new()).or_default().push(path.clone());
        }
    }
    for files in groups.values_mut() {
        files.sort();
    }
    groups
}

/// Renders the suite list in inline format with directory-aligned columns.
///
/// Example:
/// ```text
///   src/api/        auth.test.js, users.test.js
///   src/components/  Button.test.js
/// ```
fn render_inline_groups(groups: &BTreeMap<String, Vec<String>>) -> String {
    // Compute maximum directory label width for alignment.
    let max_dir_width = groups.keys().map(|d| d.len()).max().unwrap_or(0);

    let mut lines: Vec<String> = Vec::new();
    for (dir, files) in groups {
        let files_str = files.join(", ");
        if dir.is_empty() {
            lines.push(format!(
                "  {:width$}  {}",
                "",
                files_str,
                width = max_dir_width
            ));
        } else {
            lines.push(format!(
                "  {:width$}  {}",
                dir,
                files_str,
                width = max_dir_width
            ));
        }
    }
    lines.join("\n")
}

// ── Coverage ──────────────────────────────────────────────────────────────────

struct FileCoverage {
    path: String,
    stmts_pct: Option<f64>,
    branch_pct: Option<f64>,
    funcs_pct: Option<f64>,
}

/// Percentage covered, or `None` when the metric is undefined (no entities).
/// Undefined must not render as a misleading 100%.
fn pct(covered: u64, total: u64) -> Option<f64> {
    if total == 0 {
        None
    } else {
        Some((covered as f64 / total as f64) * 100.0)
    }
}

fn count_object_map(map: &Value) -> (u64, u64) {
    let obj = match map.as_object() {
        Some(o) => o,
        None => return (0, 0),
    };
    let total = obj.len() as u64;
    let covered = obj
        .values()
        .filter(|v| v.as_u64().map(|n| n > 0).unwrap_or(false))
        .count() as u64;
    (covered, total)
}

fn count_branch_map(b: &Value) -> (u64, u64) {
    let obj = match b.as_object() {
        Some(o) => o,
        None => return (0, 0),
    };
    let mut total: u64 = 0;
    let mut covered: u64 = 0;
    for arr in obj.values() {
        if let Some(counts) = arr.as_array() {
            for c in counts {
                total += 1;
                if c.as_u64().map(|n| n > 0).unwrap_or(false) {
                    covered += 1;
                }
            }
        }
    }
    (covered, total)
}

fn compute_file_coverage(path: String, data: &Value, cwd: &Option<String>) -> FileCoverage {
    let rel_path = relativize_path(&path, cwd);

    let (stmts_cov, stmts_tot) = count_object_map(&data["s"]);
    let (branch_cov, branch_tot) = count_branch_map(&data["b"]);
    let (funcs_cov, funcs_tot) = count_object_map(&data["f"]);

    FileCoverage {
        path: rel_path,
        stmts_pct: pct(stmts_cov, stmts_tot),
        branch_pct: pct(branch_cov, branch_tot),
        funcs_pct: pct(funcs_cov, funcs_tot),
    }
}

fn render_coverage_table(coverage_map: &HashMap<String, Value>, cwd: &Option<String>) -> String {
    let mut file_coverages: Vec<FileCoverage> = coverage_map
        .iter()
        .map(|(path, data)| compute_file_coverage(path.clone(), data, cwd))
        .collect();
    file_coverages.sort_by(|a, b| a.path.cmp(&b.path));

    // Only include files that are not fully covered.
    // an undefined metric (None) does not by itself mark a file incomplete
    let below_full = |p: &Option<f64>| matches!(p, Some(v) if *v < 100.0);
    let incomplete: Vec<&FileCoverage> = file_coverages
        .iter()
        .filter(|f| {
            below_full(&f.stmts_pct) || below_full(&f.branch_pct) || below_full(&f.funcs_pct)
        })
        .collect();

    // Compute "All" row totals directly from coverage_map.
    let mut all_stmts_cov: u64 = 0;
    let mut all_stmts_tot: u64 = 0;
    let mut all_branch_cov: u64 = 0;
    let mut all_branch_tot: u64 = 0;
    let mut all_funcs_cov: u64 = 0;
    let mut all_funcs_tot: u64 = 0;
    for data in coverage_map.values() {
        let (sc, st) = count_object_map(&data["s"]);
        let (bc, bt) = count_branch_map(&data["b"]);
        let (fc, ft) = count_object_map(&data["f"]);
        all_stmts_cov += sc;
        all_stmts_tot += st;
        all_branch_cov += bc;
        all_branch_tot += bt;
        all_funcs_cov += fc;
        all_funcs_tot += ft;
    }
    let all_row = FileCoverage {
        path: "All".to_string(),
        stmts_pct: pct(all_stmts_cov, all_stmts_tot),
        branch_pct: pct(all_branch_cov, all_branch_tot),
        funcs_pct: pct(all_funcs_cov, all_funcs_tot),
    };

    if incomplete.is_empty() && coverage_map.is_empty() {
        return String::new();
    }

    // Compute column widths.
    let header_file = "File";
    let max_file_width = incomplete
        .iter()
        .map(|f| f.path.len())
        .chain(std::iter::once(all_row.path.len()))
        .chain(std::iter::once(header_file.len()))
        .max()
        .unwrap_or(4);

    // None -> "-" (undefined metric); never round a sub-100 value up to "100%"
    let format_pct = |p: Option<f64>| match p {
        None => "-".to_string(),
        Some(v) if (99.0..100.0).contains(&v) => "99%".to_string(),
        Some(v) => format!("{:.0}%", v),
    };

    let mut lines: Vec<String> = Vec::new();

    // Header.
    lines.push(format!(
        "  {:<width$}  {:>6}  {:>6}  {:>5}",
        header_file,
        "Stmts",
        "Branch",
        "Funcs",
        width = max_file_width
    ));

    for fc in &incomplete {
        lines.push(format!(
            "  {:<width$}  {:>6}  {:>6}  {:>5}",
            fc.path,
            format_pct(fc.stmts_pct),
            format_pct(fc.branch_pct),
            format_pct(fc.funcs_pct),
            width = max_file_width
        ));
    }

    // "All" row always shown.
    lines.push(format!(
        "  {:<width$}  {:>6}  {:>6}  {:>5}",
        all_row.path,
        format_pct(all_row.stmts_pct),
        format_pct(all_row.branch_pct),
        format_pct(all_row.funcs_pct),
        width = max_file_width
    ));

    lines.join("\n")
}

// ── Summary ───────────────────────────────────────────────────────────────────

fn build_summary(result: &JestResult) -> String {
    let skipped = result.num_pending_tests + result.num_todo_tests;
    let suites = result.num_total_test_suites;

    let suite_label = if suites == 1 { "suite" } else { "suites" };

    if result.num_failed_tests > 0 || !result.success {
        // Show failed count using num_failed_tests (even if success=false due to suite error).
        let failed = result.num_failed_tests;
        let passed = result.num_passed_tests;

        let mut s = format!("{} failed, {} passed", failed, passed);
        if skipped > 0 {
            s.push_str(&format!(", {} skipped", skipped));
        }
        s.push_str(&format!(" ({} {})", suites, suite_label));
        s
    } else {
        let passed = result.num_passed_tests;
        let mut s = format!("{} passed", passed);
        if skipped > 0 {
            s.push_str(&format!(", {} skipped", skipped));
        }
        s.push_str(&format!(" ({} {})", suites, suite_label));
        s
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn args(strs: &[&str]) -> Vec<String> {
        strs.iter().map(|s| s.to_string()).collect()
    }

    /// Compress with a fake CWD of "/project/".
    /// `_has_coverage` retained so existing call sites read clearly; coverage is
    /// now gated on data presence, not a flag.
    fn compress(json: &str, exit_code: i32, _has_coverage: bool) -> Option<String> {
        compress_jest_with_cwd(json, exit_code, Some("/project/".to_string()))
    }

    // ── Helper builders ───────────────────────────────────────────────────────

    fn make_assertion(
        ancestors: &[&str],
        title: &str,
        status: &str,
        failures: &[&str],
    ) -> serde_json::Value {
        serde_json::json!({
            "ancestorTitles": ancestors,
            "title": title,
            "status": status,
            "failureMessages": failures,
        })
    }

    fn make_suite(
        name: &str,
        status: &str,
        assertions: Vec<serde_json::Value>,
    ) -> serde_json::Value {
        serde_json::json!({
            "name": name,
            "status": status,
            "message": "",
            "assertionResults": assertions,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn make_jest_json(
        success: bool,
        passed_tests: u64,
        failed_tests: u64,
        pending_tests: u64,
        todo_tests: u64,
        passed_suites: u64,
        failed_suites: u64,
        total_suites: u64,
        suites: Vec<serde_json::Value>,
    ) -> String {
        serde_json::json!({
            "success": success,
            "numPassedTests": passed_tests,
            "numFailedTests": failed_tests,
            "numPendingTests": pending_tests,
            "numTodoTests": todo_tests,
            "numPassedTestSuites": passed_suites,
            "numFailedTestSuites": failed_suites,
            "numTotalTestSuites": total_suites,
            "testResults": suites,
        })
        .to_string()
    }

    // ── has_skip_flag ─────────────────────────────────────────────────────────

    #[test]
    fn test_has_skip_flag_watch() {
        assert!(has_skip_flag(&args(&["--watch"])));
    }

    #[test]
    fn test_has_skip_flag_watch_all() {
        assert!(has_skip_flag(&args(&["--watchAll"])));
    }

    #[test]
    fn test_has_skip_flag_init() {
        assert!(has_skip_flag(&args(&["--init"])));
    }

    #[test]
    fn test_has_skip_flag_help() {
        assert!(has_skip_flag(&args(&["--help"])));
    }

    #[test]
    fn test_has_skip_flag_help_short() {
        assert!(has_skip_flag(&args(&["-h"])));
    }

    #[test]
    fn test_has_skip_flag_version() {
        assert!(has_skip_flag(&args(&["--version"])));
    }

    #[test]
    fn test_has_skip_flag_show_config() {
        assert!(has_skip_flag(&args(&["--showConfig"])));
    }

    #[test]
    fn test_has_skip_flag_list_reporters() {
        assert!(has_skip_flag(&args(&["--listReporters"])));
    }

    #[test]
    fn test_has_skip_flag_clear_cache() {
        assert!(has_skip_flag(&args(&["--clearCache"])));
    }

    #[test]
    fn test_has_skip_flag_json() {
        assert!(has_skip_flag(&args(&["--json"])));
    }

    #[test]
    fn test_has_skip_flag_output_file() {
        assert!(has_skip_flag(&args(&["--outputFile"])));
    }

    #[test]
    fn test_has_skip_flag_output_file_equals() {
        assert!(has_skip_flag(&args(&["--outputFile=results.json"])));
    }

    #[test]
    fn test_has_skip_flag_reporters() {
        assert!(has_skip_flag(&args(&["--reporters"])));
    }

    #[test]
    fn test_has_skip_flag_reporters_equals() {
        assert!(has_skip_flag(&args(&["--reporters=junit"])));
    }

    #[test]
    fn test_has_skip_flag_test_results_processor() {
        assert!(has_skip_flag(&args(&["--testResultsProcessor"])));
    }

    #[test]
    fn test_has_skip_flag_test_results_processor_equals() {
        assert!(has_skip_flag(&args(&[
            "--testResultsProcessor=./processor.js"
        ])));
    }

    #[test]
    fn test_has_skip_flag_normal_args() {
        assert!(!has_skip_flag(&args(&["--coverage", "src/"])));
    }

    // ── normalized_args ───────────────────────────────────────────────────────

    #[test]
    fn test_normalized_args_appends_json_and_no_color() {
        let c = JestCompressor;
        let result = c.normalized_args(&args(&["src/"]));
        assert_eq!(result, args(&["src/", "--json", "--no-color"]));
    }

    #[test]
    fn test_normalized_args_strips_existing_json() {
        let c = JestCompressor;
        let result = c.normalized_args(&args(&["--json", "src/"]));
        assert_eq!(result, args(&["src/", "--json", "--no-color"]));
    }

    #[test]
    fn test_normalized_args_strips_color_flags() {
        let c = JestCompressor;
        let result = c.normalized_args(&args(&["--color", "--colors", "--color=always", "src/"]));
        assert_eq!(result, args(&["src/", "--json", "--no-color"]));
    }

    #[test]
    fn test_normalized_args_strips_no_color_then_readds() {
        let c = JestCompressor;
        let result = c.normalized_args(&args(&["--no-color", "src/"]));
        assert_eq!(result, args(&["src/", "--json", "--no-color"]));
    }

    // ── compress: basic cases ─────────────────────────────────────────────────

    #[test]
    fn test_compress_exit_code_2_returns_none() {
        let json = make_jest_json(true, 5, 0, 0, 0, 1, 0, 1, vec![]);
        assert_eq!(compress(&json, 2, false), None);
    }

    #[test]
    fn test_compress_invalid_json_returns_none() {
        assert_eq!(compress("not json", 0, false), None);
    }

    #[test]
    fn test_compress_all_pass() {
        let suite = make_suite(
            "/project/src/utils/math.test.js",
            "passed",
            vec![
                make_assertion(&["add"], "should add", "passed", &[]),
                make_assertion(&["add"], "should subtract", "passed", &[]),
            ],
        );
        let json = make_jest_json(true, 2, 0, 0, 0, 1, 0, 1, vec![suite]);
        let result = compress(&json, 0, false).unwrap();

        // No FAIL blocks.
        assert!(!result.contains("FAIL "), "should have no FAIL blocks");

        // Suite list present.
        assert!(result.contains("suites:"), "should contain suite list");
        assert!(
            result.contains("src/utils/"),
            "should contain directory group"
        );
        assert!(result.contains("math.test.js"), "should contain filename");

        // Summary.
        assert!(result.contains("2 passed (1 suite)"), "got: {}", result);
    }

    #[test]
    fn test_compress_failures() {
        let assertions = vec![
            make_assertion(
                &["add"],
                "should handle negative numbers",
                "failed",
                &["Expected: -1\nReceived: 1\nat src/utils/math.test.js:15"],
            ),
            make_assertion(&["add"], "should add positives", "passed", &[]),
        ];
        let suite = make_suite("/project/src/utils/math.test.js", "failed", assertions);
        let json = make_jest_json(false, 1, 1, 0, 0, 0, 1, 1, vec![suite]);
        let result = compress(&json, 1, false).unwrap();

        assert!(
            result.contains("FAIL src/utils/math.test.js"),
            "should have FAIL block; got: {}",
            result
        );
        assert!(
            result.contains("\u{2717} add > should handle negative numbers"),
            "should show failing test name"
        );
        assert!(
            result.contains("Expected: -1"),
            "should show failure message"
        );
        assert!(result.contains("suites:"), "should have suite list");
        assert!(
            result.contains("1 failed, 1 passed (1 suite)"),
            "got: {}",
            result
        );
    }

    #[test]
    fn test_compress_error_truncation() {
        let long_msg: String = (1..=20)
            .map(|i| format!("line {}", i))
            .collect::<Vec<_>>()
            .join("\n");
        let assertions = vec![make_assertion(
            &["render"],
            "should match snapshot",
            "failed",
            &[&long_msg],
        )];
        let suite = make_suite("/project/src/components/App.test.js", "failed", assertions);
        let json = make_jest_json(false, 0, 1, 0, 0, 0, 1, 1, vec![suite]);
        let result = compress(&json, 1, false).unwrap();

        assert!(
            result.contains("... (5 more lines)"),
            "should truncate to 15 lines; got: {}",
            result
        );
    }

    #[test]
    fn test_compress_per_suite_cap() {
        let assertions: Vec<serde_json::Value> = (1..=15)
            .map(|i| {
                make_assertion(
                    &["suite"],
                    &format!("test {}", i),
                    "failed",
                    &[&format!("Error {}", i)],
                )
            })
            .collect();
        let suite = make_suite("/project/src/api/auth.test.js", "failed", assertions);
        let json = make_jest_json(false, 0, 15, 0, 0, 0, 1, 1, vec![suite]);
        let result = compress(&json, 1, false).unwrap();

        assert!(
            result.contains("... and 5 more failures in this suite"),
            "should cap at 10 per suite; got: {}",
            result
        );
    }

    #[test]
    fn test_compress_total_cap() {
        // 3 suites × 8 failures = 24 total. Cap is 20.
        // Suite 1: 8 emitted (total=8), suite 2: 8 emitted (total=16),
        // suite 3: 4 emitted before cap (total=20), then overflow.
        let make_failing_suite = |name: &str, count: usize| {
            let assertions: Vec<serde_json::Value> = (1..=count)
                .map(|i| make_assertion(&["s"], &format!("t{}", i), "failed", &["err"]))
                .collect();
            make_suite(name, "failed", assertions)
        };

        let suites = vec![
            make_failing_suite("/project/src/a.test.js", 8),
            make_failing_suite("/project/src/b.test.js", 8),
            make_failing_suite("/project/src/c.test.js", 8),
        ];
        let json = make_jest_json(false, 0, 24, 0, 0, 0, 3, 3, suites);
        let result = compress(&json, 1, false).unwrap();

        assert!(
            result.contains("... and") && result.contains("more failures across"),
            "should show global overflow; got: {}",
            result
        );
    }

    #[test]
    fn test_compress_skipped_in_summary() {
        let suite = make_suite(
            "/project/src/a.test.js",
            "passed",
            vec![
                make_assertion(&[], "test 1", "passed", &[]),
                make_assertion(&[], "test 2", "pending", &[]),
                make_assertion(&[], "test 3", "todo", &[]),
            ],
        );
        let json = make_jest_json(true, 1, 0, 1, 1, 1, 0, 1, vec![suite]);
        let result = compress(&json, 0, false).unwrap();

        assert!(
            result.contains("2 skipped"),
            "should include skipped count; got: {}",
            result
        );
    }

    #[test]
    fn test_compress_with_coverage() {
        let suite = make_suite(
            "/project/src/utils/math.js",
            "passed",
            vec![make_assertion(&[], "test", "passed", &[])],
        );
        let json_val = serde_json::json!({
            "success": true,
            "numPassedTests": 1,
            "numFailedTests": 0,
            "numPendingTests": 0,
            "numTodoTests": 0,
            "numPassedTestSuites": 1,
            "numFailedTestSuites": 0,
            "numTotalTestSuites": 1,
            "testResults": [suite],
            "coverageMap": {
                "/project/src/utils/math.js": {
                    "s": { "0": 1, "1": 0, "2": 1 },
                    "b": { "0": [1, 0] },
                    "f": { "0": 1, "1": 0 },
                    "statementMap": {},
                    "branchMap": {},
                    "fnMap": {}
                }
            }
        });
        let result =
            compress_jest_with_cwd(&json_val.to_string(), 0, Some("/project/".to_string()))
                .unwrap();

        assert!(result.contains("coverage:"), "should have coverage section");
        assert!(result.contains("Stmts"), "should have coverage header");
        assert!(
            result.contains("src/utils/math.js"),
            "should show file path"
        );
        assert!(result.contains("All"), "should have All row");
    }

    #[test]
    fn test_compress_coverage_all_100_shows_only_all_row() {
        let json_val = serde_json::json!({
            "success": true,
            "numPassedTests": 1,
            "numFailedTests": 0,
            "numPendingTests": 0,
            "numTodoTests": 0,
            "numPassedTestSuites": 1,
            "numFailedTestSuites": 0,
            "numTotalTestSuites": 1,
            "testResults": [],
            "coverageMap": {
                "/project/src/fully_covered.js": {
                    "s": { "0": 1, "1": 1 },
                    "b": { "0": [1, 1] },
                    "f": { "0": 1 },
                    "statementMap": {},
                    "branchMap": {},
                    "fnMap": {}
                }
            }
        });
        let result =
            compress_jest_with_cwd(&json_val.to_string(), 0, Some("/project/".to_string()))
                .unwrap();

        // Fully covered file should NOT appear as a row.
        assert!(
            !result.contains("src/fully_covered.js"),
            "100% covered file should be hidden; got: {}",
            result
        );
        // But "All" row still appears.
        assert!(result.contains("All"), "All row should still be present");
    }

    #[test]
    fn test_compress_coverage_section_from_data_without_flag() {
        // coverageMap present (config collectCoverage / --coverage=true) but the
        // bare CLI flag absent: section must still render (gated on data)
        let json_val = serde_json::json!({
            "success": true,
            "numPassedTests": 1,
            "numFailedTests": 0,
            "numPendingTests": 0,
            "numTodoTests": 0,
            "numPassedTestSuites": 1,
            "numFailedTestSuites": 0,
            "numTotalTestSuites": 1,
            "testResults": [],
            "coverageMap": {
                "/project/src/math.js": {
                    "s": { "0": 0 },
                    "b": {},
                    "f": {},
                    "statementMap": {},
                    "branchMap": {},
                    "fnMap": {}
                }
            }
        });
        let result =
            compress_jest_with_cwd(&json_val.to_string(), 0, Some("/project/".to_string()))
                .unwrap();

        assert!(
            result.contains("coverage:"),
            "coverage section should appear from data; got: {}",
            result
        );
        // undefined branch/funcs metrics (empty maps) render "-", not "100%"
        assert!(
            result.contains('-'),
            "undefined metric should render '-'; got: {}",
            result
        );
        assert!(
            !result.contains("100%"),
            "undefined metric must not render as 100%; got: {}",
            result
        );
    }

    #[test]
    fn test_compress_no_coverage_section_when_map_absent() {
        // no coverageMap field at all: no coverage section
        let suite = make_suite(
            "/project/src/a.test.js",
            "passed",
            vec![make_assertion(&[], "t", "passed", &[])],
        );
        let json = make_jest_json(true, 1, 0, 0, 0, 1, 0, 1, vec![suite]);
        let result = compress(&json, 0, false).unwrap();

        assert!(
            !result.contains("coverage:"),
            "coverage section should not appear without coverageMap; got: {}",
            result
        );
    }

    #[test]
    fn test_pct_undefined_when_total_zero() {
        // undefined metric must be None, never a misleading 100%
        assert_eq!(pct(0, 0), None);
        assert_eq!(pct(5, 10), Some(50.0));
    }

    #[test]
    fn test_compress_coverage_sub_100_not_rounded_up() {
        // 199/200 statements = 99.5% must render "99%", not "100%"
        let mut stmts = serde_json::Map::new();
        for i in 0..200u64 {
            // first entry uncovered (0), rest covered (1)
            stmts.insert(i.to_string(), serde_json::json!(if i == 0 { 0 } else { 1 }));
        }
        let json_val = serde_json::json!({
            "success": true,
            "numPassedTests": 1,
            "numFailedTests": 0,
            "numPendingTests": 0,
            "numTodoTests": 0,
            "numPassedTestSuites": 1,
            "numFailedTestSuites": 0,
            "numTotalTestSuites": 1,
            "testResults": [],
            "coverageMap": {
                "/project/src/big.js": {
                    "s": stmts,
                    "b": {},
                    "f": {},
                    "statementMap": {},
                    "branchMap": {},
                    "fnMap": {}
                }
            }
        });
        let result =
            compress_jest_with_cwd(&json_val.to_string(), 0, Some("/project/".to_string()))
                .unwrap();

        assert!(
            result.contains("99%"),
            "99.5% should floor to 99%; got: {}",
            result
        );
        assert!(
            !result.contains("100%"),
            "sub-100 value must not round up to 100%; got: {}",
            result
        );
    }

    #[test]
    fn test_path_relativization() {
        let suite = make_suite(
            "/project/src/utils/math.test.js",
            "passed",
            vec![make_assertion(&[], "t", "passed", &[])],
        );
        let json = make_jest_json(true, 1, 0, 0, 0, 1, 0, 1, vec![suite]);
        let result = compress(&json, 0, false).unwrap();

        // Inline suite list renders directory and filename separately.
        assert!(
            result.contains("src/utils/"),
            "should contain relativized directory; got: {}",
            result
        );
        assert!(
            result.contains("math.test.js"),
            "should contain relativized filename; got: {}",
            result
        );
        assert!(
            !result.contains("/project/src/utils/math.test.js"),
            "absolute path should not appear"
        );
    }

    #[test]
    fn test_compress_suite_level_failure_message() {
        let suite = serde_json::json!({
            "name": "/project/src/broken.test.js",
            "status": "failed",
            "message": "SyntaxError: Unexpected token\n  at broken.test.js:1",
            "assertionResults": [],
        });
        let json = make_jest_json(false, 0, 0, 0, 0, 0, 1, 1, vec![suite]);
        let result = compress(&json, 1, false).unwrap();

        assert!(
            result.contains("FAIL src/broken.test.js"),
            "should show FAIL block; got: {}",
            result
        );
        assert!(
            result.contains("SyntaxError"),
            "should show suite-level message"
        );
    }

    #[test]
    fn test_summary_singular_suite() {
        let suite = make_suite(
            "/project/src/a.test.js",
            "passed",
            vec![make_assertion(&[], "t", "passed", &[])],
        );
        let json = make_jest_json(true, 1, 0, 0, 0, 1, 0, 1, vec![suite]);
        let result = compress(&json, 0, false).unwrap();
        assert!(
            result.contains("(1 suite)"),
            "should use singular; got: {}",
            result
        );
    }

    #[test]
    fn test_strip_stack_trace_keeps_first_at_line() {
        let msg = "Error: expect(received).toBe(expected)\n\nExpected: -1\nReceived: 1\n\n    at Object.<anonymous> (src/math.test.js:8:25)\n    at Promise.then.completed (node_modules/jest-circus/build/utils.js:123)\n    at callAsyncCircusFn (node_modules/jest-circus/build/utils.js:456)\n    at _runTest (node_modules/jest-circus/build/run.js:789)";
        let stripped = strip_stack_trace(msg);

        assert!(
            stripped.contains("at Object.<anonymous> (src/math.test.js:8:25)"),
            "should keep first at line; got: {}",
            stripped
        );
        assert!(
            !stripped.contains("jest-circus"),
            "should strip internal frames; got: {}",
            stripped
        );
        assert!(stripped.contains("Expected: -1"), "should keep assertion");
        assert!(stripped.contains("Received: 1"), "should keep assertion");
    }

    #[test]
    fn test_strip_stack_trace_no_at_lines() {
        let msg = "Expected: 1\nReceived: 2";
        assert_eq!(strip_stack_trace(msg), msg);
    }

    #[test]
    fn test_strip_stack_trace_prefers_user_frame_over_internal_first() {
        // internal frame appears first; user-code frame second
        let msg = "Error: boom\n    at throwIt (node_modules/lib/index.js:5:7)\n    at Object.<anonymous> (src/math.test.js:8:25)\n    at _runTest (node_modules/jest-circus/build/run.js:789)";
        let stripped = strip_stack_trace(msg);

        assert!(
            stripped.contains("at Object.<anonymous> (src/math.test.js:8:25)"),
            "should keep first user-code frame; got: {}",
            stripped
        );
        assert!(
            !stripped.contains("node_modules"),
            "should drop all internal frames; got: {}",
            stripped
        );
    }

    #[test]
    fn test_strip_stack_trace_falls_back_to_first_when_all_internal() {
        // no user-code frame: keep the first frame so location isn't lost
        let msg = "Error: boom\n    at deep (node_modules/lib/a.js:1:1)\n    at deeper (node_modules/lib/b.js:2:2)";
        let stripped = strip_stack_trace(msg);

        assert!(
            stripped.contains("at deep (node_modules/lib/a.js:1:1)"),
            "should keep first frame as fallback; got: {}",
            stripped
        );
        assert!(
            !stripped.contains("b.js:2:2"),
            "should drop subsequent frames; got: {}",
            stripped
        );
    }

    #[test]
    fn test_compress_failure_with_stack_trace() {
        let msg = "Error: expect(received).toBe(expected)\n\nExpected: -1\nReceived: 1\n\n    at Object.<anonymous> (src/math.test.js:8:25)\n    at Promise.then.completed (node_modules/jest-circus/build/utils.js:123)\n    at callAsyncCircusFn (node_modules/jest-circus/build/utils.js:456)\n    at _runTest (node_modules/jest-circus/build/run.js:789)\n    at _runTestsForDescribeBlock (node_modules/jest-circus/build/run.js:111)\n    at run (node_modules/jest-circus/build/run.js:222)";
        let assertions = vec![make_assertion(
            &["math"],
            "negative numbers",
            "failed",
            &[msg],
        )];
        let suite = make_suite("/project/src/math.test.js", "failed", assertions);
        let json = make_jest_json(false, 0, 1, 0, 0, 0, 1, 1, vec![suite]);
        let result = compress(&json, 1, false).unwrap();

        // Should include the first at line (user code).
        assert!(
            result.contains("at Object.<anonymous>"),
            "should keep first at line; got: {}",
            result
        );
        // Should NOT include jest internals.
        assert!(
            !result.contains("jest-circus"),
            "should strip internal stack frames; got: {}",
            result
        );
    }

    #[test]
    fn test_summary_plural_suites() {
        let suite1 = make_suite(
            "/project/src/a.test.js",
            "passed",
            vec![make_assertion(&[], "t", "passed", &[])],
        );
        let suite2 = make_suite(
            "/project/src/b.test.js",
            "passed",
            vec![make_assertion(&[], "t", "passed", &[])],
        );
        let json = make_jest_json(true, 2, 0, 0, 0, 2, 0, 2, vec![suite1, suite2]);
        let result = compress(&json, 0, false).unwrap();
        assert!(
            result.contains("(2 suites)"),
            "should use plural; got: {}",
            result
        );
    }
}
