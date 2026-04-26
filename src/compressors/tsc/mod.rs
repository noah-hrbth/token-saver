use crate::compressors::Compressor;
use std::collections::BTreeMap;

const MAX_ENTRIES_PER_FILE: usize = 30;
const MAX_ENTRIES_TOTAL: usize = 100;
const MAX_LOCATIONS_PER_GROUP: usize = 20;

struct TscDiagnostic {
    path: String,
    line: u32,
    col: u32,
    code: u32,
    message: String,
    continuations: Vec<String>,
}

struct RenderEntry {
    code: u32,
    message: String,
    continuations: Vec<String>,
    locations: Vec<(u32, u32)>,
}

fn strip_trailing_period(s: &str) -> &str {
    s.strip_suffix('.').unwrap_or(s)
}

fn render_location_str(locations: &[(u32, u32)]) -> String {
    let n = locations.len().min(MAX_LOCATIONS_PER_GROUP);
    let parts: Vec<String> = locations[..n]
        .iter()
        .map(|(ln, col)| format!("{}:{}", ln, col))
        .collect();
    let joined = parts.join(",");
    if locations.len() > MAX_LOCATIONS_PER_GROUP {
        format!(
            "{},... and {} more locations",
            joined,
            locations.len() - MAX_LOCATIONS_PER_GROUP
        )
    } else {
        joined
    }
}

fn group_diagnostics(diags: &[&TscDiagnostic]) -> Vec<RenderEntry> {
    let mut entries: Vec<RenderEntry> = Vec::new();
    for diag in diags {
        let msg = strip_trailing_period(&diag.message).to_string();
        let conts: Vec<String> = diag
            .continuations
            .iter()
            .map(|c| strip_trailing_period(c).to_string())
            .collect();
        let found = entries
            .iter_mut()
            .find(|e| e.code == diag.code && e.message == msg && e.continuations == conts);
        if let Some(entry) = found {
            entry.locations.push((diag.line, diag.col));
        } else {
            entries.push(RenderEntry {
                code: diag.code,
                message: msg,
                continuations: conts,
                locations: vec![(diag.line, diag.col)],
            });
        }
    }
    entries
}

pub(crate) struct TscCompressor;

impl Compressor for TscCompressor {
    fn can_compress(&self, args: &[String]) -> bool {
        !has_skip_flag(args)
    }

    fn normalized_args(&self, original_args: &[String]) -> Vec<String> {
        normalize_pretty(original_args)
    }

    fn compress(&self, stdout: &str, stderr: &str, exit_code: i32) -> Option<String> {
        compress_tsc(stdout, stderr, exit_code)
    }
}

/// Returns true if args contain a flag that means we should not compress.
pub(crate) fn has_skip_flag(args: &[String]) -> bool {
    for arg in args {
        let a = arg.as_str();
        if a == "--watch"
            || a == "-w"
            || a == "--build"
            || a == "-b"
            || a == "--listFiles"
            || a == "--listFilesOnly"
            || a == "--listEmittedFiles"
            || a == "--showConfig"
            || a == "--traceResolution"
            || a == "--diagnostics"
            || a == "--extendedDiagnostics"
            || a == "--generateTrace"
            || a == "--help"
            || a == "-h"
            || a == "--all"
            || a == "--version"
            || a == "-v"
            || a == "--init"
        {
            return true;
        }
    }
    false
}

/// Returns a compressor for tsc if args are compressible.
pub fn find_compressor(args: &[String]) -> Option<Box<dyn Compressor>> {
    let compressor = TscCompressor;
    if compressor.can_compress(args) {
        Some(Box::new(compressor))
    } else {
        None
    }
}

/// Inject `--pretty false` unless it's already present with value false.
fn normalize_pretty(original_args: &[String]) -> Vec<String> {
    // Check if `--pretty false` is already present in separate-arg form
    let mut i = 0;
    while i < original_args.len() {
        let arg = &original_args[i];
        if arg == "--pretty" && i + 1 < original_args.len() && original_args[i + 1] == "false" {
            // Already correct — return unchanged
            return original_args.to_vec();
        }
        i += 1;
    }

    // Strip all existing --pretty forms, then append --pretty false
    let mut result = Vec::new();
    let mut i = 0;
    while i < original_args.len() {
        let arg = &original_args[i];
        if arg == "--pretty" {
            // Skip this arg. If next arg is a value (true/false), skip it too.
            if i + 1 < original_args.len() {
                let next = &original_args[i + 1];
                if next == "true" || next == "false" {
                    i += 2;
                    continue;
                }
            }
            // Bare --pretty (means true) — just skip it
            i += 1;
            continue;
        }
        if arg.starts_with("--pretty=") {
            // --pretty=true or --pretty=false — strip it
            i += 1;
            continue;
        }
        result.push(arg.clone());
        i += 1;
    }
    result.push("--pretty".to_string());
    result.push("false".to_string());
    result
}

pub(crate) fn compress_tsc(stdout: &str, _stderr: &str, exit_code: i32) -> Option<String> {
    let cwd = std::env::current_dir().ok().map(|p| {
        let mut s = p.to_string_lossy().to_string();
        if !s.ends_with('/') {
            s.push('/');
        }
        s
    });
    compress_tsc_with_cwd(stdout, exit_code, cwd)
}

pub(crate) fn compress_tsc_with_cwd(
    stdout: &str,
    exit_code: i32,
    cwd: Option<String>,
) -> Option<String> {
    match exit_code {
        0..=2 => {}
        _ => return None,
    }

    let diagnostics = parse_tsc_output(stdout);

    // If no diagnostics parsed and stdout was non-empty and exit != 0, fall back
    if diagnostics.is_empty() && !stdout.trim().is_empty() && exit_code != 0 {
        return None;
    }

    // Clean run (exit 0 or empty diagnostics from exit 1/2 — shouldn't happen but be safe)
    if diagnostics.is_empty() {
        return Some(String::new());
    }

    render_output(&diagnostics, &cwd)
}

/// Parse tsc stdout into a list of diagnostics.
fn parse_tsc_output(stdout: &str) -> Vec<TscDiagnostic> {
    let mut diagnostics: Vec<TscDiagnostic> = Vec::new();

    for raw_line in stdout.lines() {
        // Strip trailing \r for CRLF support
        let line = raw_line.trim_end_matches('\r');

        // Check if this is a diagnostic header line (contains TSdigits:)
        if let Some(diag) = try_parse_diagnostic(line) {
            diagnostics.push(diag);
        } else if line.starts_with(|c: char| c.is_whitespace()) {
            // Continuation/related-info line
            if let Some(last) = diagnostics.last_mut() {
                last.continuations.push(line.to_string());
            }
        }
        // Otherwise: discard
    }

    diagnostics
}

/// Try to parse a line as a tsc diagnostic header.
/// Returns None if the line is not a diagnostic (or is the "Found N errors" footer).
fn try_parse_diagnostic(line: &str) -> Option<TscDiagnostic> {
    // Must contain TS followed by digits followed by :
    let ts_pos = find_ts_code(line)?;

    // Extract error code and message starting from ts_pos
    // Format: TS<digits>: <message>
    let after_ts = &line[ts_pos + 2..]; // skip "TS"
    let colon_pos = after_ts.find(':')?;
    let code_str = &after_ts[..colon_pos];
    if !code_str.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let code: u32 = code_str.parse().ok()?;
    let message = after_ts[colon_pos + 1..].trim().to_string();

    // Discard "Found N errors" footer lines
    if message.starts_with("Found ") && message.contains("error") {
        return None;
    }

    // Now determine if there's a file location: search for last valid (N,N): pattern
    let location = find_last_location(line, ts_pos);

    match location {
        Some((path_str, ln, col)) => Some(TscDiagnostic {
            path: path_str,
            line: ln,
            col,
            code,
            message,
            continuations: Vec::new(),
        }),
        None => Some(TscDiagnostic {
            path: String::new(),
            line: 0,
            col: 0,
            code,
            message,
            continuations: Vec::new(),
        }),
    }
}

/// Find the position of "TS" in the line that is followed by digits and then ":".
/// Returns the byte position of the "T" in "TS...".
fn find_ts_code(line: &str) -> Option<usize> {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i + 2 < bytes.len() {
        if bytes[i] == b'T' && bytes[i + 1] == b'S' {
            // Check that next chars are digits followed by ':'
            let rest = &line[i + 2..];
            let digits_end = rest
                .find(|c: char| !c.is_ascii_digit())
                .unwrap_or(rest.len());
            if digits_end > 0 {
                let after_digits = &rest[digits_end..];
                if after_digits.starts_with(':') {
                    return Some(i);
                }
            }
        }
        i += 1;
    }
    None
}

/// Find the last valid `(line,col):` pattern before `ts_pos` in the line.
/// Returns `(path, line, col)` or None if no valid location found.
fn find_last_location(line: &str, ts_pos: usize) -> Option<(String, u32, u32)> {
    let search_region = &line[..ts_pos];
    let mut last_valid: Option<(usize, u32, u32)> = None;

    let mut i = 0;
    while let Some(rel_pos) = search_region[i..].find('(') {
        let paren_pos = i + rel_pos;
        let rest = &search_region[paren_pos + 1..];
        if let Some(result) = try_parse_location(rest) {
            last_valid = Some((paren_pos, result.0, result.1));
        }
        i = paren_pos + 1;
    }

    let (paren_pos, ln, col) = last_valid?;
    let path = search_region[..paren_pos].trim_end().to_string();
    Some((path, ln, col))
}

/// Try to parse `N,N):` at the start of the given string (the char after `(`).
/// Returns `(line, col)` or None.
fn try_parse_location(s: &str) -> Option<(u32, u32)> {
    // Expect: digits ',' digits ')' ':'
    let comma = s.find(',')?;
    let line_str = &s[..comma];
    if !line_str.chars().all(|c| c.is_ascii_digit()) || line_str.is_empty() {
        return None;
    }
    let ln: u32 = line_str.parse().ok()?;

    let after_comma = &s[comma + 1..];
    let close = after_comma.find(')')?;
    let col_str = &after_comma[..close];
    if !col_str.chars().all(|c| c.is_ascii_digit()) || col_str.is_empty() {
        return None;
    }
    let col: u32 = col_str.parse().ok()?;

    let after_close = &after_comma[close + 1..];
    if !after_close.starts_with(':') {
        return None;
    }

    Some((ln, col))
}

fn relativize_path(path: &str, cwd: &Option<String>) -> String {
    if let Some(prefix) = cwd
        && let Some(stripped) = path.strip_prefix(prefix.as_str())
    {
        return stripped.to_string();
    }
    path.to_string()
}

fn render_output(diagnostics: &[TscDiagnostic], cwd: &Option<String>) -> Option<String> {
    let mut config_diags: Vec<&TscDiagnostic> = Vec::new();
    let mut file_map: BTreeMap<String, Vec<&TscDiagnostic>> = BTreeMap::new();

    for diag in diagnostics {
        if diag.path.is_empty() {
            config_diags.push(diag);
        } else {
            let rel = relativize_path(&diag.path, cwd);
            file_map.entry(rel).or_default().push(diag);
        }
    }

    let mut parts: Vec<String> = Vec::new();

    // CONFIG: section
    if !config_diags.is_empty() {
        let entries = group_diagnostics(&config_diags);
        let mut lines = vec!["CONFIG:".to_string()];
        for entry in &entries {
            lines.push(format!("  TS{:<5}  {}", entry.code, entry.message));
        }
        parts.push(lines.join("\n"));
    }

    // File sections with caps
    let mut total_entries_emitted: usize = 0;
    let mut skipped_source_errors: usize = 0;
    let mut skipped_files: usize = 0;
    let mut capped = false;

    for (rel_path, diags) in &file_map {
        if capped {
            skipped_source_errors += diags.len();
            skipped_files += 1;
            continue;
        }

        if total_entries_emitted >= MAX_ENTRIES_TOTAL {
            capped = true;
            skipped_source_errors += diags.len();
            skipped_files += 1;
            continue;
        }

        let entries = group_diagnostics(diags);

        let loc_strings: Vec<String> = entries
            .iter()
            .map(|e| render_location_str(&e.locations))
            .collect();
        let max_loc_width = loc_strings.iter().map(|s| s.len()).max().unwrap_or(0);

        // inline mode: single entry, single location, no continuations
        let inline = entries.len() == 1
            && entries[0].locations.len() == 1
            && entries[0].continuations.is_empty();

        if inline {
            if total_entries_emitted >= MAX_ENTRIES_TOTAL {
                capped = true;
                skipped_source_errors += diags.len();
                skipped_files += 1;
                continue;
            }
            let entry = &entries[0];
            let (ln, col) = entry.locations[0];
            parts.push(format!(
                "{}:{}:{}  TS{}  {}",
                rel_path, ln, col, entry.code, entry.message
            ));
            total_entries_emitted += 1;
            continue;
        }

        // Determine header mode
        let code_header = entries.iter().all(|e| e.code == entries[0].code);

        let file_header = if code_header {
            format!("{}  TS{}", rel_path, entries[0].code)
        } else {
            rel_path.clone()
        };

        // cont_prefix: 2 (indent) + max_loc_width + 2 (gap)
        let cont_prefix = " ".repeat(2 + max_loc_width + 2);

        let mut file_lines: Vec<String> = vec![file_header];
        let mut file_overflow_errors: usize = 0;

        for (i, entry) in entries.iter().enumerate() {
            let over_file_cap = i >= MAX_ENTRIES_PER_FILE;
            let over_total_cap = total_entries_emitted >= MAX_ENTRIES_TOTAL;

            if over_file_cap || over_total_cap {
                file_overflow_errors += entry.locations.len();
                if over_total_cap {
                    capped = true;
                }
            } else {
                let loc_str = &loc_strings[i];
                let padded_loc = format!("{:>width$}", loc_str, width = max_loc_width);
                if code_header {
                    file_lines.push(format!("  {}  {}", padded_loc, entry.message));
                } else {
                    file_lines.push(format!(
                        "  {}  TS{}  {}",
                        padded_loc, entry.code, entry.message
                    ));
                }
                for cont in &entry.continuations {
                    file_lines.push(format!("{}{}", cont_prefix, cont.trim_start()));
                }
                total_entries_emitted += 1;
            }
        }

        if file_overflow_errors > 0 {
            file_lines.push(format!(
                "  ... and {} more errors in this file",
                file_overflow_errors
            ));
        }

        parts.push(file_lines.join("\n"));
    }

    // Total overflow for entirely skipped files
    if skipped_files > 0 {
        let file_label = if skipped_files == 1 { "file" } else { "files" };
        parts.push(format!(
            "... and {} more errors across {} {}",
            skipped_source_errors, skipped_files, file_label
        ));
    }

    Some(parts.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(strs: &[&str]) -> Vec<String> {
        strs.iter().map(|s| s.to_string()).collect()
    }

    /// Compress with a fake CWD of "/project/"
    fn compress(stdout: &str, exit_code: i32) -> Option<String> {
        compress_tsc_with_cwd(stdout, exit_code, Some("/project/".to_string()))
    }

    #[test]
    fn can_compress_bare() {
        assert!(TscCompressor.can_compress(&args(&[])));
    }

    #[test]
    fn can_compress_no_emit() {
        assert!(TscCompressor.can_compress(&args(&["--noEmit"])));
    }

    #[test]
    fn can_compress_skips_watch() {
        assert!(!TscCompressor.can_compress(&args(&["--watch"])));
    }

    #[test]
    fn can_compress_skips_watch_short() {
        assert!(!TscCompressor.can_compress(&args(&["-w"])));
    }

    #[test]
    fn can_compress_skips_build() {
        assert!(!TscCompressor.can_compress(&args(&["--build"])));
    }

    #[test]
    fn can_compress_skips_build_short() {
        assert!(!TscCompressor.can_compress(&args(&["-b"])));
    }

    #[test]
    fn can_compress_skips_help() {
        assert!(!TscCompressor.can_compress(&args(&["--help"])));
        assert!(!TscCompressor.can_compress(&args(&["-h"])));
        assert!(!TscCompressor.can_compress(&args(&["--version"])));
        assert!(!TscCompressor.can_compress(&args(&["-v"])));
        assert!(!TscCompressor.can_compress(&args(&["--init"])));
    }

    #[test]
    fn normalized_args_injects_pretty() {
        let result = TscCompressor.normalized_args(&args(&[]));
        assert_eq!(result, args(&["--pretty", "false"]));
    }

    #[test]
    fn normalized_args_preserves_existing_flags() {
        let result = TscCompressor.normalized_args(&args(&["--noEmit", "--strict"]));
        assert_eq!(result, args(&["--noEmit", "--strict", "--pretty", "false"]));
    }

    #[test]
    fn normalized_args_replaces_pretty_true() {
        let result = TscCompressor.normalized_args(&args(&["--pretty", "true"]));
        assert_eq!(result, args(&["--pretty", "false"]));
    }

    #[test]
    fn normalized_args_replaces_pretty_equals_true() {
        let result = TscCompressor.normalized_args(&args(&["--pretty=true"]));
        assert_eq!(result, args(&["--pretty", "false"]));
    }

    #[test]
    fn normalized_args_idempotent() {
        let input = args(&["--pretty", "false"]);
        let result = TscCompressor.normalized_args(&input);
        assert_eq!(result, input);
    }

    #[test]
    fn compress_exit0_empty() {
        assert_eq!(compress("", 0), Some(String::new()));
    }

    #[test]
    fn compress_exit0_footer_only() {
        assert_eq!(compress("Found 0 errors.\n", 0), Some(String::new()));
    }

    #[test]
    fn compress_unparseable_nonzero() {
        assert_eq!(compress("some garbage\n", 1), None);
    }

    #[test]
    fn compress_single_file_single_error() {
        let stdout = "/project/src/api/users.ts(12,5): error TS2322: Type 'string' is not assignable to type 'number'.\n";
        let result = compress(stdout, 1).unwrap();
        assert!(
            result.contains("src/api/users.ts:12:5"),
            "should contain relativized path with inline location; got:\n{}",
            result
        );
        assert!(result.contains("12:5"), "should contain location");
        assert!(result.contains("TS2322"), "should contain error code");
        assert!(
            result.contains("Type 'string' is not assignable to type 'number'"),
            "should contain message without trailing period; got:\n{}",
            result
        );
    }

    #[test]
    fn compress_path_with_parens() {
        let stdout = "/project/path (copy)/file.ts(5,1): error TS2304: Cannot find name 'foo'.\n";
        let result = compress(stdout, 1).unwrap();
        assert!(
            result.contains("path (copy)/file.ts:5:1"),
            "path with parens should be preserved in inline format; got:\n{}",
            result
        );
        assert!(result.contains("5:1"), "should contain location");
        assert!(result.contains("TS2304"), "should contain error code");
    }

    #[test]
    fn compress_global_error() {
        let stdout = "error TS5023: Unknown compiler option 'foo'.\n";
        let result = compress(stdout, 1).unwrap();
        assert!(result.starts_with("CONFIG:"), "should start with CONFIG:");
        assert!(result.contains("TS5023"), "should contain error code");
        assert!(
            result.contains("Unknown compiler option 'foo'"),
            "should contain message without trailing period; got:\n{}",
            result
        );
    }

    #[test]
    fn compress_continuation_lines() {
        let stdout = concat!(
            "/project/src/api/users.ts(12,5): error TS2322: Type 'string' is not assignable to type 'number'.\n",
            "  Types of property 'a' are incompatible.\n",
            "    Type 'string' is not assignable to type 'number'.\n",
        );
        let result = compress(stdout, 1).unwrap();
        assert!(
            result.contains("TS2322"),
            "should contain error code; got:\n{}",
            result
        );
        assert!(
            result.contains("Type 'string' is not assignable to type 'number'"),
            "should contain message without trailing period; got:\n{}",
            result
        );
        assert!(
            result.contains("Types of property"),
            "should contain continuation"
        );
        // Continuation lines should be shifted right of the loc column
        let cont_line = result
            .lines()
            .find(|l| l.contains("Types of property"))
            .unwrap();
        // loc_width=4 "12:5", so prefix = 2+4+2 = 8 spaces
        assert!(
            cont_line.starts_with("        "),
            "continuation should be indented by prefix width; got: {:?}",
            cont_line
        );
    }

    #[test]
    fn compress_multi_file_alphabetical() {
        let stdout = concat!(
            "/project/src/z.ts(1,1): error TS2322: Error in z.\n",
            "/project/src/a.ts(1,1): error TS2322: Error in a.\n",
        );
        let result = compress(stdout, 1).unwrap();
        let a_pos = result.find("src/a.ts").unwrap();
        let z_pos = result.find("src/z.ts").unwrap();
        assert!(a_pos < z_pos, "a.ts should appear before z.ts");
    }

    #[test]
    fn compress_per_file_cap() {
        let mut stdout = String::new();
        for i in 1u32..=35 {
            stdout.push_str(&format!(
                "/project/src/big.ts({},1): error TS2322: Error {}.\n",
                i, i
            ));
        }
        let result = compress(&stdout, 1).unwrap();
        assert!(
            result.contains("... and 5 more errors in this file"),
            "should show per-file cap; got:\n{}",
            result
        );
        // All errors have unique messages, so no dedup. code-header mode fires (all TS2322).
        // TS2322 appears only in the file header, not in individual error lines.
        let shown = result
            .lines()
            .filter(|l| l.starts_with("  ") && !l.starts_with("  ..."))
            .count();
        assert_eq!(shown, 30, "should show exactly 30 errors; got:\n{}", result);
    }

    #[test]
    fn compress_total_cap() {
        let mut stdout = String::new();
        // 5 files x 25 errors = 125 total; first 4 files fill the 100-error cap exactly,
        // so the 5th file is skipped entirely → triggers "more errors across N files"
        for f in &["a", "b", "c", "d", "e"] {
            for i in 1u32..=25 {
                stdout.push_str(&format!(
                    "/project/src/{}.ts({},1): error TS2322: Error {}.\n",
                    f, i, i
                ));
            }
        }
        let result = compress(&stdout, 1).unwrap();
        assert!(
            result.contains("... and") && result.contains("more errors across"),
            "should show total cap; got:\n{}",
            result
        );
    }

    #[test]
    fn compress_mixed_config_and_file() {
        let stdout = concat!(
            "error TS5023: Unknown compiler option 'foo'.\n",
            "/project/src/index.ts(1,1): error TS2322: Type error.\n",
        );
        let result = compress(stdout, 1).unwrap();
        let config_pos = result.find("CONFIG:").unwrap();
        let file_pos = result.find("src/index.ts").unwrap();
        assert!(
            config_pos < file_pos,
            "CONFIG: section should appear before file section"
        );
        assert!(
            result.contains("Unknown compiler option 'foo'"),
            "config message should have period stripped; got:\n{}",
            result
        );
    }

    #[test]
    fn compress_path_relativization() {
        let stdout1 = "/project/src/foo.ts(1,1): error TS2322: Error.\n";
        let r1 = compress(stdout1, 1).unwrap();
        assert!(
            r1.contains("src/foo.ts:1:1"),
            "should relativize path with inline location; got:\n{}",
            r1
        );
        assert!(
            !r1.contains("/project/src/foo.ts"),
            "should not contain absolute path"
        );

        // Path outside cwd stays absolute
        let stdout2 = "/other/src/bar.ts(1,1): error TS2322: Error.\n";
        let r2 = compress(stdout2, 1).unwrap();
        assert!(
            r2.contains("/other/src/bar.ts:1:1"),
            "outside-cwd path should be absolute with inline location; got:\n{}",
            r2
        );
    }

    #[test]
    fn compress_crlf_lines() {
        let stdout = "/project/src/a.ts(1,1): error TS2322: Error.\r\n";
        let result = compress(stdout, 1).unwrap();
        assert!(
            result.contains("src/a.ts:1:1"),
            "should parse CRLF lines correctly; got:\n{}",
            result
        );
        assert!(
            result.contains("TS2322"),
            "should parse CRLF lines correctly"
        );
    }

    #[test]
    fn dedup_same_code_and_message() {
        let stdout = concat!(
            "/project/src/a.ts(1,7): error TS2322: Type 'string' is not assignable to type 'number'.\n",
            "/project/src/a.ts(2,7): error TS2322: Type 'string' is not assignable to type 'number'.\n",
            "/project/src/a.ts(3,7): error TS2322: Type 'string' is not assignable to type 'number'.\n",
        );
        let result = compress(stdout, 1).unwrap();
        // Should have exactly one error line (deduped), not three
        let error_lines: Vec<&str> = result
            .lines()
            .filter(|l| l.starts_with("  ") && !l.starts_with("  ..."))
            .collect();
        assert_eq!(
            error_lines.len(),
            1,
            "should collapse to one line; got:\n{}",
            result
        );
        assert!(
            result.contains("1:7,2:7,3:7"),
            "should join locations; got:\n{}",
            result
        );
        assert!(
            result.contains("Type 'string' is not assignable to type 'number'"),
            "should contain message; got:\n{}",
            result
        );
    }

    #[test]
    fn dedup_blocked_by_differing_message() {
        let stdout = concat!(
            "/project/src/a.ts(1,1): error TS2322: Type 'string' is not assignable to type 'number'.\n",
            "/project/src/a.ts(2,1): error TS2322: Type 'boolean' is not assignable to type 'number'.\n",
        );
        let result = compress(stdout, 1).unwrap();
        let error_lines: Vec<&str> = result
            .lines()
            .filter(|l| l.starts_with("  ") && !l.starts_with("  ..."))
            .collect();
        assert_eq!(
            error_lines.len(),
            2,
            "different messages should not dedup; got:\n{}",
            result
        );
    }

    #[test]
    fn dedup_blocked_by_differing_continuations() {
        let stdout = concat!(
            "/project/src/a.ts(1,1): error TS2322: Type error.\n",
            "  Chain line A.\n",
            "/project/src/a.ts(2,1): error TS2322: Type error.\n",
            "  Chain line B.\n",
        );
        let result = compress(stdout, 1).unwrap();
        let error_lines: Vec<&str> = result
            .lines()
            .filter(|l| l.starts_with("  ") && !l.starts_with("  ..."))
            .collect();
        // 2 primary lines + 2 continuation lines = 4, not 2
        assert!(
            error_lines.len() >= 2,
            "different chains should not dedup primaries; got:\n{}",
            result
        );
        assert!(
            result.contains("Chain line A"),
            "should preserve first chain; got:\n{}",
            result
        );
        assert!(
            result.contains("Chain line B"),
            "should preserve second chain; got:\n{}",
            result
        );
    }

    #[test]
    fn code_header_mode_fires_when_all_same_code() {
        let stdout = concat!(
            "/project/src/a.ts(1,1): error TS2322: Message one.\n",
            "/project/src/a.ts(2,1): error TS2322: Message two.\n",
        );
        let result = compress(stdout, 1).unwrap();
        // File header should contain the code
        let header_line = result.lines().next().unwrap();
        assert!(
            header_line.contains("TS2322"),
            "header should contain code; got:\n{}",
            result
        );
        // Error lines should NOT contain TS2322 (it's on the header)
        let error_lines: Vec<&str> = result
            .lines()
            .filter(|l| l.starts_with("  ") && !l.starts_with("  ..."))
            .collect();
        for line in &error_lines {
            assert!(
                !line.contains("TS2322"),
                "error lines should not repeat code; got:\n{}",
                result
            );
        }
    }

    #[test]
    fn code_header_off_when_codes_differ() {
        let stdout = concat!(
            "/project/src/a.ts(1,1): error TS2322: Message one.\n",
            "/project/src/a.ts(2,1): error TS2304: Message two.\n",
        );
        let result = compress(stdout, 1).unwrap();
        // Both codes should appear in error lines
        let lines_with_ts: Vec<&str> = result
            .lines()
            .filter(|l| l.starts_with("  ") && l.contains("TS"))
            .collect();
        assert_eq!(
            lines_with_ts.len(),
            2,
            "both errors should show their code; got:\n{}",
            result
        );
        assert!(
            result.contains("TS2322"),
            "should contain TS2322; got:\n{}",
            result
        );
        assert!(
            result.contains("TS2304"),
            "should contain TS2304; got:\n{}",
            result
        );
    }

    #[test]
    fn inline_mode_triggers() {
        let stdout = "/project/src/a.ts(5,3): error TS2304: Cannot find name 'foo'.\n";
        let result = compress(stdout, 1).unwrap();
        // Inline: no file header line separate from the error; path, loc, code all on one line
        let first_non_summary = result.lines().next().unwrap();
        assert!(
            first_non_summary.contains("src/a.ts:5:3"),
            "inline should use colon separator; got:\n{}",
            result
        );
        assert!(
            first_non_summary.contains("TS2304"),
            "inline should contain code; got:\n{}",
            result
        );
    }

    #[test]
    fn inline_blocked_by_continuations() {
        let stdout = concat!(
            "/project/src/a.ts(5,3): error TS2322: Type error.\n",
            "  Chain line.\n",
        );
        let result = compress(stdout, 1).unwrap();
        // Should NOT be inline — should have a file header line + error line
        assert!(
            !result.lines().next().unwrap().contains(":5:3"),
            "should not be inline when continuations present; got:\n{}",
            result
        );
        assert!(
            result.contains("Chain line"),
            "should preserve chain; got:\n{}",
            result
        );
    }

    #[test]
    fn inline_blocked_by_multi_location_grouped_entry() {
        let stdout = concat!(
            "/project/src/a.ts(1,1): error TS2322: Type error.\n",
            "/project/src/a.ts(2,1): error TS2322: Type error.\n",
        );
        let result = compress(stdout, 1).unwrap();
        // Grouped to 1 entry with 2 locations → not inline (locations.len() > 1)
        // Should be block mode (code-header since single code)
        assert!(
            !result.lines().next().unwrap().contains(":1:1"),
            "multi-location grouped entry should not be inline; got:\n{}",
            result
        );
        assert!(
            result.contains("1:1,2:1"),
            "should show joined locations; got:\n{}",
            result
        );
    }

    #[test]
    fn period_strip_primary() {
        let stdout = "/project/src/a.ts(1,1): error TS2322: Type 'string' is not assignable to type 'number'.\n";
        let result = compress(stdout, 1).unwrap();
        assert!(
            !result.contains("'number'."),
            "trailing period should be stripped; got:\n{}",
            result
        );
        assert!(
            result.contains("'number'"),
            "message body should be present; got:\n{}",
            result
        );
    }

    #[test]
    fn period_strip_continuations() {
        let stdout = concat!(
            "/project/src/a.ts(1,1): error TS2322: Type error.\n",
            "/project/src/a.ts(2,1): error TS2322: Another error.\n",
            "  Chain continuation.\n",
        );
        let result = compress(stdout, 1).unwrap();
        // The continuation "Chain continuation." should have period stripped
        assert!(
            !result.contains("continuation."),
            "continuation trailing period should be stripped; got:\n{}",
            result
        );
        assert!(
            result.contains("Chain continuation"),
            "continuation body should be present; got:\n{}",
            result
        );
    }

    #[test]
    fn period_strip_preserves_inner_periods() {
        let stdout =
            "/project/src/a.ts(1,1): error TS2322: Property 'a.b' is missing in type 'Foo'.\n";
        let result = compress(stdout, 1).unwrap();
        // Only the final period is stripped; inner periods in 'a.b' are kept
        assert!(
            result.contains("'a.b'"),
            "inner period in property name should be preserved; got:\n{}",
            result
        );
        assert!(
            !result.contains("'Foo'."),
            "trailing period should be stripped; got:\n{}",
            result
        );
    }

    #[test]
    fn per_group_location_cap() {
        let mut stdout = String::new();
        for i in 1u32..=25 {
            stdout.push_str(&format!(
                "/project/src/a.ts({},1): error TS2322: Same message.\n",
                i
            ));
        }
        let result = compress(&stdout, 1).unwrap();
        assert!(
            result.contains("... and 5 more locations"),
            "should cap at 20 locations with overflow suffix; got:\n{}",
            result
        );
    }

    #[test]
    fn single_newline_separator() {
        let stdout = concat!(
            "/project/src/a.ts(1,1): error TS2322: Error in a.\n",
            "/project/src/b.ts(1,1): error TS2304: Error in b.\n",
        );
        let result = compress(stdout, 1).unwrap();
        // Both should be inline. Check that there's no double newline between them
        assert!(
            !result.contains("\n\n"),
            "should use single newline separator; got:\n{}",
            result
        );
    }
}
