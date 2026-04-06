use std::collections::BTreeMap;

use crate::compressors::Compressor;

const MAX_CHECK_FILES: usize = 200;

#[derive(Debug, PartialEq)]
pub(crate) enum PrettierMode {
    Check,
    Write,
}

pub(crate) struct PrettierCompressor {
    pub(crate) mode: PrettierMode,
}

/// Returns true if any arg is a flag that means we should not compress.
pub(crate) fn has_skip_flag(args: &[String]) -> bool {
    for arg in args {
        let a = arg.as_str();
        if a == "--find-config-path"
            || a == "--file-info"
            || a == "--support-info"
            || a == "--debug-check"
            || a == "--debug-print-doc"
            || a == "--help"
            || a == "-h"
            || a == "--version"
            || a == "-v"
            || a == "--list-different"
            || a == "-l"
        {
            return true;
        }
    }
    false
}

/// Returns `Write` if `--write` or `-w` is present, `Check` if `--check` is present,
/// or `None` if neither mode flag is found. `--write` wins when both are present.
fn detect_mode(args: &[String]) -> Option<PrettierMode> {
    let mut found_write = false;
    let mut found_check = false;
    for arg in args {
        let a = arg.as_str();
        if a == "--write" || a == "-w" {
            found_write = true;
        } else if a == "--check" {
            found_check = true;
        }
    }
    if found_write {
        Some(PrettierMode::Write)
    } else if found_check {
        Some(PrettierMode::Check)
    } else {
        None
    }
}

/// Returns a compressor for prettier if args are compressible.
pub fn find_compressor(args: &[String]) -> Option<Box<dyn Compressor>> {
    if has_skip_flag(args) {
        return None;
    }
    let mode = detect_mode(args)?;
    Some(Box::new(PrettierCompressor { mode }))
}

impl Compressor for PrettierCompressor {
    fn can_compress(&self, args: &[String]) -> bool {
        !has_skip_flag(args) && detect_mode(args).is_some()
    }

    fn normalized_args(&self, original_args: &[String]) -> Vec<String> {
        let mut result: Vec<String> = original_args
            .iter()
            .filter(|a| {
                let s = a.as_str();
                s != "--color" && s != "--no-color" && !s.starts_with("--color=")
            })
            .cloned()
            .collect();
        result.push("--no-color".to_string());
        result
    }

    fn compress(&self, stdout: &str, stderr: &str, exit_code: i32) -> Option<String> {
        compress_prettier(stdout, stderr, exit_code, &self.mode)
    }
}

pub(crate) fn compress_prettier(
    stdout: &str,
    stderr: &str,
    exit_code: i32,
    mode: &PrettierMode,
) -> Option<String> {
    if exit_code >= 2 {
        return None;
    }
    match mode {
        PrettierMode::Check => compress_check(stdout, stderr, exit_code),
        PrettierMode::Write => compress_write(stdout, stderr, exit_code),
    }
}

/// Groups file paths by their parent directory.
/// Keys are directory paths with trailing `/`; files without a directory use `""`.
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

/// Renders grouped files: directory header followed by indented filenames.
/// Root-level files (empty key) are rendered without header or indent.
fn render_grouped_files(groups: &BTreeMap<String, Vec<String>>) -> String {
    let mut lines: Vec<String> = Vec::new();
    for (dir, files) in groups {
        if dir.is_empty() {
            for file in files {
                lines.push(file.clone());
            }
        } else {
            lines.push(dir.clone());
            for file in files {
                lines.push(format!("  {}", file));
            }
        }
    }
    lines.join("\n")
}

fn compress_check(stdout: &str, stderr: &str, exit_code: i32) -> Option<String> {
    let combined = format!("{}\n{}", stdout, stderr);

    if combined.trim().is_empty() && exit_code == 0 {
        return Some(String::new());
    }

    let mut unformatted: Vec<String> = Vec::new();
    let mut errors: Vec<(Option<String>, String)> = Vec::new();

    for line in combined.lines() {
        if line.is_empty()
            || line == "Checking formatting..."
            || line == "All matched files use Prettier code style!"
        {
            continue;
        }

        if let Some(text) = line.strip_prefix("[warn] ") {
            if text.starts_with("Code style issues") {
                continue;
            }
            unformatted.push(text.to_string());
        } else if let Some(text) = line.strip_prefix("[error] ") {
            if let Some((path, msg)) = text.split_once(": ") {
                errors.push((Some(path.to_string()), msg.to_string()));
            } else {
                errors.push((None, text.to_string()));
            }
        }
    }

    if exit_code == 0 && unformatted.is_empty() && errors.is_empty() {
        return Some("All matched files use Prettier code style!".to_string());
    }

    let total_files = unformatted.len();
    let mut parts: Vec<String> = Vec::new();

    if !unformatted.is_empty() {
        let capped = &unformatted[..total_files.min(MAX_CHECK_FILES)];
        let groups = group_by_directory(capped);
        parts.push(render_grouped_files(&groups));

        if total_files > MAX_CHECK_FILES {
            let remaining = total_files - MAX_CHECK_FILES;
            let label = if remaining == 1 { "file" } else { "files" };
            parts.push(format!("... and {} more {}", remaining, label));
        }
    }

    if !errors.is_empty() {
        let mut error_lines = vec!["ERRORS:".to_string()];
        for (path, msg) in &errors {
            if let Some(p) = path {
                error_lines.push(format!("  {}: {}", p, msg));
            } else {
                error_lines.push(format!("  {}", msg));
            }
        }
        parts.push(error_lines.join("\n"));
    }

    if total_files > 0 {
        if total_files == 1 {
            parts.push("1 file needs formatting".to_string());
        } else {
            parts.push(format!("{} files need formatting", total_files));
        }
    }

    Some(parts.join("\n\n"))
}

fn compress_write(stdout: &str, stderr: &str, exit_code: i32) -> Option<String> {
    let combined = format!("{}\n{}", stdout, stderr);

    if combined.trim().is_empty() && exit_code == 0 {
        return Some(String::new());
    }

    let mut formatted_count: usize = 0;
    let mut errors: Vec<(Option<String>, String)> = Vec::new();

    for line in combined.lines() {
        if line.is_empty() {
            continue;
        }
        if let Some(text) = line.strip_prefix("[error] ") {
            if let Some((path, msg)) = text.split_once(": ") {
                errors.push((Some(path.to_string()), msg.to_string()));
            } else {
                errors.push((None, text.to_string()));
            }
        } else if !line.starts_with('[') {
            // Prettier --write outputs one line per formatted file: "<path> <timing>"
            // (e.g. "src/a.ts 47ms"). Count these as formatted files.
            formatted_count += 1;
        }
    }

    if formatted_count == 0 && errors.is_empty() {
        return Some(String::new());
    }

    let mut parts: Vec<String> = Vec::new();

    if formatted_count > 0 {
        let label = if formatted_count == 1 {
            "file"
        } else {
            "files"
        };
        parts.push(format!("Formatted {} {}", formatted_count, label));
    }

    if !errors.is_empty() {
        let mut error_lines = vec!["ERRORS:".to_string()];
        for (path, msg) in &errors {
            if let Some(p) = path {
                error_lines.push(format!("  {}: {}", p, msg));
            } else {
                error_lines.push(format!("  {}", msg));
            }
        }
        parts.push(error_lines.join("\n"));

        let n = errors.len();
        let label = if n == 1 { "file" } else { "files" };
        parts.push(format!("{} {} could not be formatted", n, label));
    }

    Some(parts.join("\n\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(strs: &[&str]) -> Vec<String> {
        strs.iter().map(|s| s.to_string()).collect()
    }

    fn check(stdout: &str, stderr: &str, exit_code: i32) -> Option<String> {
        compress_prettier(stdout, stderr, exit_code, &PrettierMode::Check)
    }

    fn write_mode(stdout: &str, stderr: &str, exit_code: i32) -> Option<String> {
        compress_prettier(stdout, stderr, exit_code, &PrettierMode::Write)
    }

    // --- has_skip_flag / can_compress / detect_mode ---

    #[test]
    fn test_can_compress_check() {
        assert!(
            PrettierCompressor {
                mode: PrettierMode::Check
            }
            .can_compress(&args(&["--check", "src/"]))
        );
    }

    #[test]
    fn test_can_compress_write() {
        assert!(
            PrettierCompressor {
                mode: PrettierMode::Write
            }
            .can_compress(&args(&["--write", "src/"]))
        );
    }

    #[test]
    fn test_can_compress_bare() {
        assert!(
            !PrettierCompressor {
                mode: PrettierMode::Check
            }
            .can_compress(&args(&["src/"]))
        );
    }

    #[test]
    fn test_can_compress_skip_help() {
        assert!(
            !PrettierCompressor {
                mode: PrettierMode::Check
            }
            .can_compress(&args(&["--help"]))
        );
    }

    #[test]
    fn test_can_compress_skip_h() {
        assert!(
            !PrettierCompressor {
                mode: PrettierMode::Check
            }
            .can_compress(&args(&["-h"]))
        );
    }

    #[test]
    fn test_can_compress_skip_version() {
        assert!(
            !PrettierCompressor {
                mode: PrettierMode::Check
            }
            .can_compress(&args(&["--version"]))
        );
    }

    #[test]
    fn test_can_compress_skip_v() {
        assert!(
            !PrettierCompressor {
                mode: PrettierMode::Check
            }
            .can_compress(&args(&["-v"]))
        );
    }

    #[test]
    fn test_can_compress_skip_list_different() {
        assert!(
            !PrettierCompressor {
                mode: PrettierMode::Check
            }
            .can_compress(&args(&["--list-different", "src/"]))
        );
    }

    #[test]
    fn test_can_compress_skip_l() {
        assert!(
            !PrettierCompressor {
                mode: PrettierMode::Check
            }
            .can_compress(&args(&["-l", "src/"]))
        );
    }

    #[test]
    fn test_can_compress_skip_find_config_path() {
        assert!(
            !PrettierCompressor {
                mode: PrettierMode::Check
            }
            .can_compress(&args(&["--find-config-path", "src/"]))
        );
    }

    #[test]
    fn test_can_compress_skip_file_info() {
        assert!(
            !PrettierCompressor {
                mode: PrettierMode::Check
            }
            .can_compress(&args(&["--file-info", "src/"]))
        );
    }

    #[test]
    fn test_can_compress_skip_support_info() {
        assert!(
            !PrettierCompressor {
                mode: PrettierMode::Check
            }
            .can_compress(&args(&["--support-info"]))
        );
    }

    #[test]
    fn test_can_compress_skip_debug_check() {
        assert!(
            !PrettierCompressor {
                mode: PrettierMode::Check
            }
            .can_compress(&args(&["--debug-check", "src/"]))
        );
    }

    #[test]
    fn test_can_compress_skip_debug_print_doc() {
        assert!(
            !PrettierCompressor {
                mode: PrettierMode::Check
            }
            .can_compress(&args(&["--debug-print-doc", "src/"]))
        );
    }

    #[test]
    fn test_detect_mode_write_wins() {
        assert_eq!(
            detect_mode(&args(&["--write", "--check", "src/"])),
            Some(PrettierMode::Write)
        );
    }

    #[test]
    fn test_detect_mode_check() {
        assert_eq!(
            detect_mode(&args(&["--check", "src/"])),
            Some(PrettierMode::Check)
        );
    }

    #[test]
    fn test_detect_mode_write() {
        assert_eq!(
            detect_mode(&args(&["--write", "src/"])),
            Some(PrettierMode::Write)
        );
    }

    #[test]
    fn test_detect_mode_w_short() {
        assert_eq!(
            detect_mode(&args(&["-w", "src/"])),
            Some(PrettierMode::Write)
        );
    }

    #[test]
    fn test_detect_mode_bare() {
        assert_eq!(detect_mode(&args(&["src/"])), None);
    }

    // --- normalized_args ---

    #[test]
    fn test_normalized_strips_color() {
        let c = PrettierCompressor {
            mode: PrettierMode::Check,
        };
        let result = c.normalized_args(&args(&["--check", "--color", "src/"]));
        assert_eq!(result, args(&["--check", "src/", "--no-color"]));
    }

    #[test]
    fn test_normalized_strips_no_color() {
        let c = PrettierCompressor {
            mode: PrettierMode::Check,
        };
        let result = c.normalized_args(&args(&["--check", "--no-color", "src/"]));
        assert_eq!(result, args(&["--check", "src/", "--no-color"]));
    }

    #[test]
    fn test_normalized_strips_color_equals() {
        let c = PrettierCompressor {
            mode: PrettierMode::Check,
        };
        let result = c.normalized_args(&args(&["--check", "--color=always", "src/"]));
        assert_eq!(result, args(&["--check", "src/", "--no-color"]));
    }

    #[test]
    fn test_normalized_passes_through_other_flags() {
        let c = PrettierCompressor {
            mode: PrettierMode::Check,
        };
        let result = c.normalized_args(&args(&["--check", "--single-quote", "src/"]));
        assert_eq!(
            result,
            args(&["--check", "--single-quote", "src/", "--no-color"])
        );
    }

    // --- compress_check ---

    #[test]
    fn test_check_exit_0_empty() {
        assert_eq!(check("", "", 0), Some(String::new()));
    }

    #[test]
    fn test_check_exit_0_clean_message() {
        assert_eq!(
            check(
                "",
                "Checking formatting...\nAll matched files use Prettier code style!",
                0
            ),
            Some("All matched files use Prettier code style!".to_string())
        );
    }

    #[test]
    fn test_check_single_warn_file() {
        let stderr = "Checking formatting...\n[warn] src/a.ts\n[warn] Code style issues found in 1 file. Run Prettier with --write to fix.";
        let result = check("", stderr, 1).unwrap();
        assert!(result.contains("src/"), "should contain directory header");
        assert!(result.contains("  a.ts"), "should contain indented file");
        assert!(
            result.contains("1 file needs formatting"),
            "should say '1 file needs formatting'; got: {}",
            result
        );
    }

    #[test]
    fn test_check_multiple_warn_files() {
        let stderr = "Checking formatting...\n[warn] src/a.ts\n[warn] src/b.ts\n[warn] src/c.ts\n[warn] Code style issues found in 3 files.";
        let result = check("", stderr, 1).unwrap();
        assert!(result.contains("src/"), "should contain directory header");
        assert!(result.contains("  a.ts"));
        assert!(result.contains("  b.ts"));
        assert!(result.contains("  c.ts"));
        assert!(
            result.contains("3 files need formatting"),
            "got: {}",
            result
        );
    }

    #[test]
    fn test_check_cap_at_200() {
        let mut stderr = "Checking formatting...\n".to_string();
        for i in 0..210 {
            stderr.push_str(&format!("[warn] src/file{}.ts\n", i));
        }
        stderr.push_str("[warn] Code style issues found in 210 files.");
        let result = check("", &stderr, 1).unwrap();
        assert!(
            result.contains("... and 10 more files"),
            "should show overflow; got: {}",
            result
        );
        assert!(
            result.contains("210 files need formatting"),
            "got: {}",
            result
        );
    }

    #[test]
    fn test_check_errors() {
        let stderr = "[error] src/bad.ts: SyntaxError: Unexpected token";
        let result = check("", stderr, 1).unwrap();
        assert!(
            result.contains("ERRORS:"),
            "should contain ERRORS section; got: {}",
            result
        );
        assert!(result.contains("src/bad.ts"), "should contain file path");
        assert!(
            result.contains("SyntaxError: Unexpected token"),
            "should contain message"
        );
    }

    #[test]
    fn test_check_warns_and_errors() {
        let stderr = "Checking formatting...\n[warn] src/a.ts\n[error] src/bad.ts: SyntaxError: Unexpected token\n[warn] Code style issues found in 1 file.";
        let result = check("", stderr, 1).unwrap();
        assert!(result.contains("  a.ts"), "should contain warn file");
        assert!(result.contains("ERRORS:"), "should contain ERRORS section");
        assert!(
            result.contains("1 file needs formatting"),
            "got: {}",
            result
        );
    }

    #[test]
    fn test_check_exit_2() {
        assert_eq!(check("", "", 2), None);
    }

    #[test]
    fn test_check_stdout_v2() {
        let stdout =
            "Checking formatting...\n[warn] src/a.ts\n[warn] Code style issues found in 1 file.";
        let result = check(stdout, "", 1).unwrap();
        assert!(
            result.contains("  a.ts"),
            "should parse [warn] lines from stdout"
        );
    }

    #[test]
    fn test_check_stderr_v3() {
        let stderr =
            "Checking formatting...\n[warn] src/a.ts\n[warn] Code style issues found in 1 file.";
        let result = check("", stderr, 1).unwrap();
        assert!(
            result.contains("  a.ts"),
            "should parse [warn] lines from stderr"
        );
    }

    #[test]
    fn test_check_preamble_stripped() {
        let stderr =
            "Checking formatting...\n[warn] src/a.ts\n[warn] Code style issues found in 1 file.";
        let result = check("", stderr, 1).unwrap();
        assert!(
            !result.contains("Checking formatting..."),
            "preamble should be stripped"
        );
    }

    #[test]
    fn test_check_footer_stripped() {
        let stderr = "Checking formatting...\n[warn] src/a.ts\n[warn] Code style issues found in 1 file. Run Prettier with --write to fix.";
        let result = check("", stderr, 1).unwrap();
        assert!(
            !result.contains("Code style issues found"),
            "footer should be stripped"
        );
    }

    #[test]
    fn test_check_singular_file() {
        let stderr = "[warn] src/a.ts\n[warn] Code style issues found in 1 file.";
        let result = check("", stderr, 1).unwrap();
        assert!(
            result.contains("1 file needs formatting"),
            "should use singular; got: {}",
            result
        );
    }

    // --- compress_write ---

    #[test]
    fn test_write_exit_0_empty() {
        assert_eq!(write_mode("", "", 0), Some(String::new()));
    }

    #[test]
    fn test_write_formatted_files() {
        let stdout = "src/a.ts 47ms\nsrc/b.ts 12ms\n";
        let result = write_mode(stdout, "", 0).unwrap();
        assert_eq!(result, "Formatted 2 files");
    }

    #[test]
    fn test_write_singular_file() {
        let stdout = "src/a.ts 47ms\n";
        let result = write_mode(stdout, "", 0).unwrap();
        assert_eq!(result, "Formatted 1 file");
    }

    #[test]
    fn test_write_errors() {
        let stderr = "[error] src/bad.ts: SyntaxError: Unexpected token";
        let result = write_mode("", stderr, 1).unwrap();
        assert!(
            result.contains("ERRORS:"),
            "should contain ERRORS section; got: {}",
            result
        );
        assert!(result.contains("src/bad.ts"));
        assert!(result.contains("SyntaxError: Unexpected token"));
        assert!(
            result.contains("1 file could not be formatted"),
            "got: {}",
            result
        );
    }

    #[test]
    fn test_write_formatted_and_errors() {
        let stdout = "src/a.ts 47ms\n";
        let stderr = "[error] src/bad.ts: SyntaxError: Unexpected token";
        let result = write_mode(stdout, stderr, 1).unwrap();
        assert!(result.contains("Formatted 1 file"), "got: {}", result);
        assert!(result.contains("ERRORS:"), "got: {}", result);
        assert!(
            result.contains("1 file could not be formatted"),
            "got: {}",
            result
        );
    }

    #[test]
    fn test_write_exit_2() {
        assert_eq!(write_mode("", "", 2), None);
    }

    // --- group_by_directory / render_grouped_files ---

    #[test]
    fn test_group_by_directory_multiple_dirs() {
        let paths: Vec<String> = vec![
            "src/components/Button.js".into(),
            "src/components/Modal.js".into(),
            "src/utils/format.js".into(),
        ];
        let groups = group_by_directory(&paths);
        assert_eq!(groups.len(), 2);
        assert_eq!(
            groups["src/components/"],
            vec!["Button.js", "Modal.js"] // sorted
        );
        assert_eq!(groups["src/utils/"], vec!["format.js"]);
    }

    #[test]
    fn test_group_by_directory_root_files() {
        let paths: Vec<String> = vec!["ugly.js".into(), "bad.css".into()];
        let groups = group_by_directory(&paths);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[""], vec!["bad.css", "ugly.js"]); // sorted
    }

    #[test]
    fn test_group_by_directory_sorts_filenames() {
        let paths: Vec<String> = vec!["src/c.ts".into(), "src/a.ts".into(), "src/b.ts".into()];
        let groups = group_by_directory(&paths);
        assert_eq!(groups["src/"], vec!["a.ts", "b.ts", "c.ts"]);
    }

    #[test]
    fn test_render_grouped_files_with_dirs() {
        let paths: Vec<String> = vec![
            "src/components/Button.js".into(),
            "src/components/Modal.js".into(),
            "src/utils/format.js".into(),
        ];
        let groups = group_by_directory(&paths);
        let rendered = render_grouped_files(&groups);
        assert_eq!(
            rendered,
            "src/components/\n  Button.js\n  Modal.js\nsrc/utils/\n  format.js"
        );
    }

    #[test]
    fn test_render_grouped_files_root_only() {
        let paths: Vec<String> = vec!["ugly.js".into(), "bad.css".into()];
        let groups = group_by_directory(&paths);
        let rendered = render_grouped_files(&groups);
        assert_eq!(rendered, "bad.css\nugly.js");
    }

    #[test]
    fn test_render_grouped_files_mixed_root_and_dirs() {
        let paths: Vec<String> = vec!["root.js".into(), "src/a.ts".into()];
        let groups = group_by_directory(&paths);
        let rendered = render_grouped_files(&groups);
        // empty key sorts first, so root files come before directory groups
        assert_eq!(rendered, "root.js\nsrc/\n  a.ts");
    }

    #[test]
    fn test_check_groups_nested_dirs() {
        let stderr = "[warn] src/components/Button.js\n[warn] src/components/Modal.js\n[warn] src/utils/format.js\n[warn] Code style issues found in 3 files.";
        let result = check("", stderr, 1).unwrap();
        assert!(result.contains("src/components/"), "should have dir header");
        assert!(result.contains("src/utils/"), "should have dir header");
        assert!(result.contains("  Button.js"), "should have indented file");
        assert!(result.contains("  Modal.js"), "should have indented file");
        assert!(result.contains("  format.js"), "should have indented file");
        assert!(result.contains("3 files need formatting"));
    }
}
