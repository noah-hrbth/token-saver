use std::collections::BTreeMap;
use std::collections::HashMap;

use serde::Deserialize;
use serde_json::Value;

use crate::compressors::Compressor;

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

/// Returns true when `--coverage` or `--collectCoverage` is present in `args`.
pub(crate) fn has_coverage_flag(args: &[String]) -> bool {
    args.iter()
        .any(|a| a == "--coverage" || a == "--collectCoverage")
}

pub(crate) struct JestCompressor {
    pub(crate) has_coverage: bool,
}

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
        compress_jest(stdout, stderr, exit_code, self.has_coverage)
    }
}

/// Returns a compressor for jest if args are compressible.
pub fn find_compressor(args: &[String]) -> Option<Box<dyn Compressor>> {
    if has_skip_flag(args) {
        return None;
    }
    Some(Box::new(JestCompressor {
        has_coverage: has_coverage_flag(args),
    }))
}

// ── Internal compress logic ───────────────────────────────────────────────────

pub(crate) fn compress_jest(
    stdout: &str,
    _stderr: &str,
    exit_code: i32,
    has_coverage: bool,
) -> Option<String> {
    compress_jest_with_cwd(stdout, exit_code, has_coverage, get_cwd())
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

fn compress_jest_with_cwd(
    stdout: &str,
    exit_code: i32,
    has_coverage: bool,
    cwd: Option<String>,
) -> Option<String> {
    // Only handle normal success/failure exit codes.
    if exit_code != 0 && exit_code != 1 {
        return None;
    }

    let result: JestResult = serde_json::from_str(stdout).ok()?;

    let mut parts: Vec<String> = Vec::new();

    // ── FAIL blocks ───────────────────────────────────────────────────────────
    let mut total_failures_emitted: usize = 0;
    let mut overflow_suites: usize = 0;
    let mut overflow_failures: u64 = 0;

    for suite in &result.test_results {
        if suite.status != "failed" {
            continue;
        }

        let suite_path = relativize_path(&suite.name, &cwd);

        // Collect failed assertions (or synthesize one from suite message).
        let failed_assertions: Vec<&JestAssertionResult> = suite
            .assertion_results
            .iter()
            .filter(|a| a.status == "failed")
            .collect();

        if total_failures_emitted >= MAX_FAILURES_TOTAL {
            overflow_suites += 1;
            overflow_failures += if failed_assertions.is_empty() {
                1
            } else {
                failed_assertions.len() as u64
            };
            continue;
        }

        let mut block_lines: Vec<String> = Vec::new();
        block_lines.push(format!("FAIL {}", suite_path));

        if failed_assertions.is_empty() {
            // Suite-level failure (e.g. syntax error) — emit message if present.
            if !suite.message.is_empty() {
                let truncated = truncate_error(&suite.message, MAX_ERROR_LINES);
                for line in truncated.lines() {
                    block_lines.push(format!("  {}", line));
                }
            }
            total_failures_emitted += 1;
        } else {
            let suite_total = failed_assertions.len();

            for (suite_emitted, assertion) in failed_assertions.iter().enumerate() {
                if total_failures_emitted >= MAX_FAILURES_TOTAL {
                    // Overflow the rest of this suite.
                    let remaining = suite_total - suite_emitted;
                    overflow_suites += 1;
                    overflow_failures += remaining as u64;
                    break;
                }
                if suite_emitted >= MAX_FAILURES_PER_SUITE {
                    let remaining = suite_total - suite_emitted;
                    block_lines.push(format!(
                        "  ... and {} more failures in this suite",
                        remaining
                    ));
                    break;
                }

                let full_name = build_test_name(assertion);
                block_lines.push(format!("  \u{2717} {}", full_name));

                for raw_msg in &assertion.failure_messages {
                    let truncated = truncate_error(raw_msg, MAX_ERROR_LINES);
                    for line in truncated.lines() {
                        block_lines.push(format!("    {}", line));
                    }
                }

                total_failures_emitted += 1;
            }
        }

        parts.push(block_lines.join("\n"));
    }

    // Global overflow note.
    if overflow_suites > 0 {
        parts.push(format!(
            "... and {} more failures across {} suite{}",
            overflow_failures,
            overflow_suites,
            if overflow_suites == 1 { "" } else { "s" }
        ));
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
    if has_coverage && let Some(coverage_map) = &result.coverage_map {
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

fn relativize_path(absolute: &str, cwd: &Option<String>) -> String {
    if let Some(prefix) = cwd
        && let Some(stripped) = absolute.strip_prefix(prefix.as_str())
    {
        return stripped.to_string();
    }
    absolute.to_string()
}

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

/// Strips internal stack frames from a jest failure message.
///
/// Jest `--json` `failureMessages` include full stack traces with 10-20 lines
/// of jest/node internals. The human-readable format hides these. We keep:
/// - All non-`at` lines (assertion error, Expected/Received, blank lines)
/// - The first `at` line that points to user code (the test file location)
/// - Drop all subsequent `at` frames (jest-circus, node internals, etc.)
fn strip_stack_trace(message: &str) -> String {
    let mut result_lines: Vec<&str> = Vec::new();
    let mut found_first_at = false;

    for line in message.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("at ") {
            if !found_first_at {
                result_lines.push(line);
                found_first_at = true;
            }
            // Drop all subsequent `at` frames.
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
    stmts_pct: f64,
    branch_pct: f64,
    funcs_pct: f64,
}

fn pct(covered: u64, total: u64) -> f64 {
    if total == 0 {
        100.0
    } else {
        (covered as f64 / total as f64) * 100.0
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
    let incomplete: Vec<&FileCoverage> = file_coverages
        .iter()
        .filter(|f| f.stmts_pct < 100.0 || f.branch_pct < 100.0 || f.funcs_pct < 100.0)
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

    let format_pct = |p: f64| format!("{:.0}%", p);

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
    fn compress(json: &str, exit_code: i32, has_coverage: bool) -> Option<String> {
        compress_jest_with_cwd(json, exit_code, has_coverage, Some("/project/".to_string()))
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

    // ── has_coverage_flag ─────────────────────────────────────────────────────

    #[test]
    fn test_has_coverage_flag_coverage() {
        assert!(has_coverage_flag(&args(&["--coverage"])));
    }

    #[test]
    fn test_has_coverage_flag_collect_coverage() {
        assert!(has_coverage_flag(&args(&["--collectCoverage"])));
    }

    #[test]
    fn test_has_coverage_flag_absent() {
        assert!(!has_coverage_flag(&args(&["src/"])));
    }

    // ── normalized_args ───────────────────────────────────────────────────────

    #[test]
    fn test_normalized_args_appends_json_and_no_color() {
        let c = JestCompressor {
            has_coverage: false,
        };
        let result = c.normalized_args(&args(&["src/"]));
        assert_eq!(result, args(&["src/", "--json", "--no-color"]));
    }

    #[test]
    fn test_normalized_args_strips_existing_json() {
        let c = JestCompressor {
            has_coverage: false,
        };
        let result = c.normalized_args(&args(&["--json", "src/"]));
        assert_eq!(result, args(&["src/", "--json", "--no-color"]));
    }

    #[test]
    fn test_normalized_args_strips_color_flags() {
        let c = JestCompressor {
            has_coverage: false,
        };
        let result = c.normalized_args(&args(&["--color", "--colors", "--color=always", "src/"]));
        assert_eq!(result, args(&["src/", "--json", "--no-color"]));
    }

    #[test]
    fn test_normalized_args_strips_no_color_then_readds() {
        let c = JestCompressor {
            has_coverage: false,
        };
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
        let result = compress_jest_with_cwd(
            &json_val.to_string(),
            0,
            true,
            Some("/project/".to_string()),
        )
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
        let result = compress_jest_with_cwd(
            &json_val.to_string(),
            0,
            true,
            Some("/project/".to_string()),
        )
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
    fn test_compress_no_coverage_section_without_flag() {
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
        let result = compress_jest_with_cwd(
            &json_val.to_string(),
            0,
            false,
            Some("/project/".to_string()),
        )
        .unwrap();

        assert!(
            !result.contains("coverage:"),
            "coverage section should not appear without flag; got: {}",
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
