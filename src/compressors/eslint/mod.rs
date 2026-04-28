use crate::compressors::Compressor;
use serde::Deserialize;

const MAX_PROBLEMS_PER_FILE: usize = 50;
const MAX_PROBLEMS_TOTAL: usize = 200;

#[derive(Deserialize)]
struct EslintFileResult {
    #[serde(rename = "filePath")]
    file_path: String,
    messages: Vec<EslintMessage>,
    #[serde(rename = "errorCount")]
    error_count: u32,
    #[serde(rename = "warningCount")]
    warning_count: u32,
    #[serde(rename = "fixableErrorCount")]
    fixable_error_count: u32,
    #[serde(rename = "fixableWarningCount")]
    fixable_warning_count: u32,
}

#[derive(Deserialize)]
struct EslintMessage {
    #[serde(rename = "ruleId")]
    rule_id: Option<String>,
    severity: u8,
    message: String,
    line: u32,
    column: u32,
    #[serde(default)]
    fatal: bool,
}

struct FileOutput {
    path: String,
    problems: Vec<Problem>,
}

struct Problem {
    line: u32,
    column: u32,
    severity: &'static str,
    message: String,
    rule_id: String,
}

pub(crate) struct EslintCompressor;

impl Compressor for EslintCompressor {
    fn can_compress(&self, args: &[String]) -> bool {
        !has_skip_flag(args)
    }

    fn normalized_args(&self, original_args: &[String]) -> Vec<String> {
        let mut result = Vec::new();
        let mut i = 0;
        while i < original_args.len() {
            let arg = &original_args[i];
            if arg == "--format" || arg == "-f" {
                i += 2;
                continue;
            }
            if arg.starts_with("--format=") || arg.starts_with("-f=") {
                i += 1;
                continue;
            }
            result.push(arg.clone());
            i += 1;
        }
        result.push("--format".to_string());
        result.push("json".to_string());
        result
    }

    fn compress(&self, stdout: &str, stderr: &str, exit_code: i32) -> Option<String> {
        compress_eslint(stdout, stderr, exit_code)
    }
}

/// Returns true if the given args contain a flag that means we should not compress.
pub(crate) fn has_skip_flag(args: &[String]) -> bool {
    for arg in args {
        let a = arg.as_str();
        if a == "--fix"
            || a == "--fix-dry-run"
            || a == "--init"
            || a == "--debug"
            || a == "--print-config"
            || a == "--format"
            || a == "-f"
            || a.starts_with("--format=")
            || a.starts_with("-f=")
        {
            return true;
        }
    }
    false
}

/// Returns a compressor for eslint if args are compressible.
pub fn find_compressor(args: &[String]) -> Option<Box<dyn Compressor>> {
    let compressor = EslintCompressor;
    if compressor.can_compress(args) {
        Some(Box::new(compressor))
    } else {
        None
    }
}

pub(crate) fn compress_eslint(stdout: &str, _stderr: &str, exit_code: i32) -> Option<String> {
    let cwd = std::env::current_dir().ok().map(|p| {
        let mut s = p.to_string_lossy().to_string();
        if !s.ends_with('/') {
            s.push('/');
        }
        s
    });
    compress_eslint_with_cwd(stdout, exit_code, cwd)
}

fn compress_eslint_with_cwd(stdout: &str, exit_code: i32, cwd: Option<String>) -> Option<String> {
    // Exit code 2 = config/fatal error — passthrough
    if exit_code == 2 {
        return None;
    }

    // Empty stdout = clean run
    if stdout.trim().is_empty() {
        return Some(String::new());
    }

    // Parse JSON
    let results: Vec<EslintFileResult> = serde_json::from_str(stdout).ok()?;

    let mut fatal_entries: Vec<(String, String)> = Vec::new();
    let mut file_groups: Vec<FileOutput> = Vec::new();
    let mut total_errors: u32 = 0;
    let mut total_warnings: u32 = 0;
    let mut total_fixable: u32 = 0;

    for file_result in &results {
        let relative_path = relativize_path(&file_result.file_path, &cwd);

        total_errors += file_result.error_count;
        total_warnings += file_result.warning_count;
        total_fixable += file_result.fixable_error_count + file_result.fixable_warning_count;

        let mut fatals: Vec<&EslintMessage> = Vec::new();
        let mut normals: Vec<&EslintMessage> = Vec::new();

        for msg in &file_result.messages {
            if msg.fatal {
                fatals.push(msg);
            } else {
                normals.push(msg);
            }
        }

        for f in &fatals {
            fatal_entries.push((relative_path.clone(), f.message.clone()));
        }

        if !normals.is_empty() {
            let mut sorted = normals;
            sorted.sort_by_key(|m| std::cmp::Reverse(m.severity));

            let problems: Vec<Problem> = sorted
                .iter()
                .map(|m| Problem {
                    line: m.line,
                    column: m.column,
                    severity: if m.severity == 2 { "error" } else { "warn" },
                    message: m.message.clone(),
                    rule_id: m.rule_id.clone().unwrap_or_default(),
                })
                .collect();

            file_groups.push(FileOutput {
                path: relative_path,
                problems,
            });
        }
    }

    // Sort files alphabetically
    file_groups.sort_by(|a, b| a.path.cmp(&b.path));

    // No problems at all
    if fatal_entries.is_empty() && file_groups.is_empty() {
        return Some(String::new());
    }

    render_output(
        &fatal_entries,
        &file_groups,
        total_errors,
        total_warnings,
        total_fixable,
    )
}

fn relativize_path(absolute: &str, cwd: &Option<String>) -> String {
    if let Some(prefix) = cwd
        && let Some(stripped) = absolute.strip_prefix(prefix.as_str())
    {
        return stripped.to_string();
    }
    absolute.to_string()
}

fn render_output(
    fatal_entries: &[(String, String)],
    file_groups: &[FileOutput],
    total_errors: u32,
    total_warnings: u32,
    total_fixable: u32,
) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();

    // FATAL section
    if !fatal_entries.is_empty() {
        let mut lines = vec!["FATAL:".to_string()];
        for (path, message) in fatal_entries {
            lines.push(format!("  {}: {}", path, message));
        }
        parts.push(lines.join("\n"));
    }

    // File groups with dual caps
    let mut total_emitted: usize = 0;
    let mut skipped_problems: usize = 0;
    let mut skipped_files: usize = 0;
    let mut capped = false;

    for file_group in file_groups {
        if capped {
            skipped_problems += file_group.problems.len();
            skipped_files += 1;
            continue;
        }

        let mut file_lines: Vec<String> = Vec::new();
        file_lines.push(file_group.path.clone());

        let loc_strings: Vec<String> = file_group
            .problems
            .iter()
            .map(|p| format!("{}:{}", p.line, p.column))
            .collect();
        let max_loc_width = loc_strings.iter().map(|s| s.len()).max().unwrap_or(0);

        let mut file_remaining: usize = 0;

        for (i, problem) in file_group.problems.iter().enumerate() {
            if total_emitted >= MAX_PROBLEMS_TOTAL {
                file_remaining = file_group.problems.len() - i;
                capped = true;
                break;
            }
            if i >= MAX_PROBLEMS_PER_FILE {
                file_remaining = file_group.problems.len() - i;
                break;
            }

            let padded_loc = format!("{:>width$}", &loc_strings[i], width = max_loc_width);
            file_lines.push(format!(
                "  {}  {}  {}  {}",
                padded_loc, problem.severity, problem.message, problem.rule_id
            ));

            total_emitted += 1;
        }

        if file_remaining > 0 {
            file_lines.push(format!(
                "  ... and {} more problems in this file",
                file_remaining
            ));
        }

        parts.push(file_lines.join("\n"));
    }

    // Total overflow for entirely skipped files
    if skipped_files > 0 {
        let file_label = if skipped_files == 1 { "file" } else { "files" };
        parts.push(format!(
            "... and {} more problems across {} {}",
            skipped_problems, skipped_files, file_label
        ));
    }

    // Summary footer
    let total = total_errors + total_warnings;
    if total > 0 {
        let problem_label = if total == 1 { "problem" } else { "problems" };
        let error_label = if total_errors == 1 { "error" } else { "errors" };
        let warning_label = if total_warnings == 1 {
            "warning"
        } else {
            "warnings"
        };
        parts.push(format!(
            "{} {} ({} {}, {} {})",
            total, problem_label, total_errors, error_label, total_warnings, warning_label
        ));
    }

    // Fixable footer
    if total_fixable > 0 {
        parts.push(format!("{} fixable with --fix", total_fixable));
    }

    Some(parts.join("\n\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(val: &str) -> String {
        val.to_string()
    }

    fn args(strs: &[&str]) -> Vec<String> {
        strs.iter().map(|s| s.to_string()).collect()
    }

    /// Compress with a fake CWD of "/project/"
    fn compress(json: &str) -> Option<String> {
        compress_eslint_with_cwd(json, 1, Some("/project/".to_string()))
    }

    fn compress_with_exit(json: &str, exit_code: i32) -> Option<String> {
        compress_eslint_with_cwd(json, exit_code, Some("/project/".to_string()))
    }

    fn make_msg(
        line: u32,
        col: u32,
        severity: u8,
        message: &str,
        rule_id: Option<&str>,
        fatal: bool,
    ) -> serde_json::Value {
        let mut msg = serde_json::json!({
            "severity": severity,
            "message": message,
            "line": line,
            "column": col,
        });
        if let Some(rid) = rule_id {
            msg["ruleId"] = serde_json::json!(rid);
        } else {
            msg["ruleId"] = serde_json::Value::Null;
        }
        if fatal {
            msg["fatal"] = serde_json::json!(true);
        }
        msg
    }

    fn make_file(
        path: &str,
        messages: Vec<serde_json::Value>,
        errors: u32,
        warnings: u32,
        fixable_errors: u32,
        fixable_warnings: u32,
    ) -> serde_json::Value {
        serde_json::json!({
            "filePath": path,
            "messages": messages,
            "errorCount": errors,
            "warningCount": warnings,
            "fixableErrorCount": fixable_errors,
            "fixableWarningCount": fixable_warnings,
            "usedDeprecatedRules": [],
            "suppressedMessages": []
        })
    }

    #[test]
    fn test_can_compress_normal_args() {
        assert!(EslintCompressor.can_compress(&args(&["src/"])));
    }

    #[test]
    fn test_can_compress_skip_fix() {
        assert!(!EslintCompressor.can_compress(&args(&["--fix"])));
    }

    #[test]
    fn test_can_compress_skip_fix_dry_run() {
        assert!(!EslintCompressor.can_compress(&args(&["--fix-dry-run"])));
    }

    #[test]
    fn test_can_compress_skip_format() {
        assert!(!EslintCompressor.can_compress(&args(&["--format"])));
    }

    #[test]
    fn test_can_compress_skip_format_eq() {
        assert!(!EslintCompressor.can_compress(&args(&["--format=compact"])));
    }

    #[test]
    fn test_can_compress_skip_f() {
        assert!(!EslintCompressor.can_compress(&args(&["-f"])));
    }

    #[test]
    fn test_can_compress_skip_init() {
        assert!(!EslintCompressor.can_compress(&args(&["--init"])));
    }

    #[test]
    fn test_can_compress_skip_debug() {
        assert!(!EslintCompressor.can_compress(&args(&["--debug"])));
    }

    #[test]
    fn test_can_compress_skip_print_config() {
        assert!(!EslintCompressor.can_compress(&args(&["--print-config"])));
    }

    #[test]
    fn test_normalized_args_appends_format_json() {
        let input = args(&["src/", "--quiet"]);
        let result = EslintCompressor.normalized_args(&input);
        assert_eq!(result, args(&["src/", "--quiet", "--format", "json"]));
    }

    #[test]
    fn test_normalized_args_strips_existing_format() {
        let input = args(&["--format", "stylish", "src/"]);
        let result = EslintCompressor.normalized_args(&input);
        assert_eq!(result, args(&["src/", "--format", "json"]));
    }

    #[test]
    fn test_exit_code_2_returns_none() {
        assert_eq!(compress_with_exit("[]", 2), None);
    }

    #[test]
    fn test_empty_stdout_returns_empty() {
        assert_eq!(compress_with_exit("", 0), Some(s("")));
    }

    #[test]
    fn test_invalid_json_returns_none() {
        assert_eq!(compress("not json"), None);
    }

    #[test]
    fn test_empty_results_returns_empty() {
        assert_eq!(compress("[]"), Some(s("")));
    }

    #[test]
    fn test_clean_file_returns_empty() {
        let json =
            serde_json::json!([make_file("/project/src/clean.ts", vec![], 0, 0, 0, 0)]).to_string();
        assert_eq!(compress(&json), Some(s("")));
    }

    #[test]
    fn test_single_file_errors_and_warnings() {
        let msgs = vec![
            make_msg(1, 1, 2, "Unexpected var", Some("no-var"), false),
            make_msg(2, 5, 2, "Missing semicolon", Some("semi"), false),
            make_msg(3, 10, 1, "Unused variable", Some("no-unused-vars"), false),
        ];
        let json =
            serde_json::json!([make_file("/project/src/main.ts", msgs, 2, 1, 0, 0)]).to_string();
        let result = compress(&json).unwrap();

        assert!(
            result.contains("src/main.ts"),
            "should contain relativized path"
        );
        // Errors before warning: check that error lines appear before warn line
        let error_pos = result.find("error").unwrap();
        let warn_pos = result.find("warn").unwrap();
        assert!(error_pos < warn_pos, "errors should appear before warnings");
        assert!(result.contains("no-var"), "should contain rule ID");
        assert!(result.contains("semi"), "should contain rule ID");
        assert!(result.contains("no-unused-vars"), "should contain rule ID");
        assert!(
            result.contains("3 problems (2 errors, 1 warning)"),
            "summary should be correct"
        );
    }

    #[test]
    fn test_multiple_files_alphabetical() {
        let msg_b = make_msg(1, 1, 2, "error in b", Some("rule"), false);
        let msg_a = make_msg(1, 1, 2, "error in a", Some("rule"), false);
        let json = serde_json::json!([
            make_file("/project/b.ts", vec![msg_b], 1, 0, 0, 0),
            make_file("/project/a.ts", vec![msg_a], 1, 0, 0, 0),
        ])
        .to_string();
        let result = compress(&json).unwrap();

        let a_pos = result.find("a.ts").unwrap();
        let b_pos = result.find("b.ts").unwrap();
        assert!(a_pos < b_pos, "a.ts should appear before b.ts");
    }

    #[test]
    fn test_fatal_section() {
        let fatal_msg = make_msg(1, 1, 2, "Parsing error: Unexpected token", None, true);
        let json = serde_json::json!([make_file(
            "/project/src/bad.ts",
            vec![fatal_msg],
            1,
            0,
            0,
            0
        )])
        .to_string();
        let result = compress(&json).unwrap();

        assert!(
            result.starts_with("FATAL:"),
            "output should start with FATAL:"
        );
        assert!(result.contains("src/bad.ts"), "should contain file path");
        assert!(
            result.contains("Parsing error: Unexpected token"),
            "should contain error message"
        );
    }

    #[test]
    fn test_fatal_and_normal_files() {
        let fatal_msg = make_msg(1, 1, 2, "Parsing error", None, true);
        let normal_msg = make_msg(5, 3, 2, "no-console violation", Some("no-console"), false);
        let json = serde_json::json!([
            make_file("/project/src/bad.ts", vec![fatal_msg], 1, 0, 0, 0),
            make_file("/project/src/normal.ts", vec![normal_msg], 1, 0, 0, 0),
        ])
        .to_string();
        let result = compress(&json).unwrap();

        let fatal_pos = result.find("FATAL:").unwrap();
        let normal_pos = result.find("src/normal.ts").unwrap();
        assert!(
            fatal_pos < normal_pos,
            "FATAL section should appear before normal file group"
        );
    }

    #[test]
    fn test_per_file_cap() {
        let msgs: Vec<serde_json::Value> = (1..=60)
            .map(|i| make_msg(i, 1, 2, &format!("error {}", i), Some("no-undef"), false))
            .collect();
        let json =
            serde_json::json!([make_file("/project/src/big.ts", msgs, 60, 0, 0, 0)]).to_string();
        let result = compress(&json).unwrap();

        assert!(
            result.contains("... and 10 more problems in this file"),
            "should show per-file cap message"
        );
        // Count problem lines (lines that contain "  " + loc + "  error")
        let problem_lines = result.lines().filter(|l| l.contains("  error  ")).count();
        assert_eq!(problem_lines, 50, "should emit exactly 50 problems");
    }

    #[test]
    fn test_total_cap() {
        // 5 files x 60 problems each = 300 total
        // Files 1-4: emit 50 each (200 total), each gets "... and 10 more in this file"
        // File 5: skipped entirely -> "... and 60 more problems across 1 file"
        let msgs: Vec<serde_json::Value> = (1..=60)
            .map(|i| make_msg(i, 1, 2, &format!("error {}", i), Some("no-undef"), false))
            .collect();
        let files: Vec<serde_json::Value> = (1..=5)
            .map(|f| {
                make_file(
                    &format!("/project/src/file{}.ts", f),
                    msgs.clone(),
                    60,
                    0,
                    0,
                    0,
                )
            })
            .collect();
        let json = serde_json::json!(files).to_string();
        let result = compress(&json).unwrap();

        assert!(
            result.contains("... and 60 more problems across 1 file"),
            "should show total cap message for skipped file; got: {}",
            result
        );
    }

    #[test]
    fn test_fixable_footer() {
        let msg = make_msg(1, 1, 2, "Missing semicolon", Some("semi"), false);
        let json =
            serde_json::json!([make_file("/project/src/a.ts", vec![msg], 1, 0, 1, 0)]).to_string();
        let result = compress(&json).unwrap();

        assert!(
            result.contains("fixable with --fix"),
            "should contain fixable footer"
        );
    }

    #[test]
    fn test_no_fixable_footer_when_zero() {
        let msg = make_msg(1, 1, 2, "no-console", Some("no-console"), false);
        let json =
            serde_json::json!([make_file("/project/src/a.ts", vec![msg], 1, 0, 0, 0)]).to_string();
        let result = compress(&json).unwrap();

        assert!(
            !result.contains("fixable"),
            "should not contain fixable footer when count is 0"
        );
    }

    #[test]
    fn test_singular_problem() {
        let msg = make_msg(1, 1, 2, "no-console", Some("no-console"), false);
        let json =
            serde_json::json!([make_file("/project/src/a.ts", vec![msg], 1, 0, 0, 0)]).to_string();
        let result = compress(&json).unwrap();

        assert!(
            result.contains("1 problem (1 error, 0 warnings)"),
            "should use singular 'problem' and 'error'; got: {}",
            result
        );
    }

    #[test]
    fn test_path_relativization() {
        let msg = make_msg(1, 1, 2, "error", Some("rule"), false);
        let json = serde_json::json!([make_file("/project/src/main.ts", vec![msg], 1, 0, 0, 0)])
            .to_string();
        let result = compress(&json).unwrap();

        assert!(
            result.contains("src/main.ts"),
            "path should be relativized to src/main.ts"
        );
        assert!(
            !result.contains("/project/src/main.ts"),
            "absolute path should not appear"
        );
    }

    #[test]
    fn test_path_no_cwd_match() {
        let msg = make_msg(1, 1, 2, "error", Some("rule"), false);
        let json =
            serde_json::json!([make_file("/other/src/main.ts", vec![msg], 1, 0, 0, 0)]).to_string();
        let result = compress_eslint_with_cwd(&json, 1, Some("/project/".to_string())).unwrap();

        assert!(
            result.contains("/other/src/main.ts"),
            "full path should be preserved when cwd doesn't match"
        );
    }

    #[test]
    fn test_warnings_only_file() {
        let msg = make_msg(1, 1, 1, "console statement", Some("no-console"), false);
        let json =
            serde_json::json!([make_file("/project/src/a.ts", vec![msg], 0, 1, 0, 0)]).to_string();
        let result = compress(&json).unwrap();

        assert!(result.contains("src/a.ts"), "should contain file path");
        assert!(result.contains("warn"), "should contain warn label");
    }

    #[test]
    fn test_right_aligned_loc() {
        let msgs = vec![
            make_msg(1, 1, 2, "short loc", Some("rule"), false),
            make_msg(100, 20, 2, "long loc", Some("rule"), false),
        ];
        let json =
            serde_json::json!([make_file("/project/src/a.ts", msgs, 2, 0, 0, 0)]).to_string();
        let result = compress(&json).unwrap();

        // "100:20" is 6 chars, "1:1" is 3 chars — padded to 6
        // So "1:1" should appear as "   1:1" (right-aligned to width 6)
        assert!(
            result.contains("   1:1"),
            "short loc should be right-padded to match long loc width; got:\n{}",
            result
        );
        assert!(
            result.contains("100:20"),
            "long loc should appear as-is; got:\n{}",
            result
        );
    }
}
