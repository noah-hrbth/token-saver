use crate::compressors::Compressor;
use crate::compressors::filters::should_filter;

const SKIP_FLAGS: &[&str] = &[
    "-l",
    "--files-with-matches",
    "-c",
    "--count",
    "--json",
    "-Z",
    "--null",
    "-q",
    "--quiet",
];

const MAX_MATCHES: usize = 200;

/// Compressor for `grep` output. Groups matches by file with indented, aligned line numbers.
pub struct GrepCompressor;

/// Compressor for `rg` (ripgrep) output. Same grouping logic, different normalized args.
pub struct RgCompressor;

impl Compressor for GrepCompressor {
    fn can_compress(&self, args: &[String]) -> bool {
        !args.iter().any(|a| SKIP_FLAGS.contains(&a.as_str()))
    }

    fn normalized_args(&self, original_args: &[String]) -> Vec<String> {
        let mut result = vec!["--color=never".to_string()];
        result.extend_from_slice(original_args);
        result
    }

    fn compress(&self, stdout: &str, stderr: &str, exit_code: i32) -> Option<String> {
        compress_grep_output(stdout, stderr, exit_code)
    }
}

impl Compressor for RgCompressor {
    fn can_compress(&self, args: &[String]) -> bool {
        !args.iter().any(|a| SKIP_FLAGS.contains(&a.as_str()))
    }

    fn normalized_args(&self, original_args: &[String]) -> Vec<String> {
        let mut result = vec!["--no-heading".to_string(), "--color=never".to_string()];
        result.extend_from_slice(original_args);
        result
    }

    fn compress(&self, stdout: &str, stderr: &str, exit_code: i32) -> Option<String> {
        compress_grep_output(stdout, stderr, exit_code)
    }
}

/// Find a compressor for the given grep args.
pub fn find_grep_compressor(args: &[String]) -> Option<Box<dyn Compressor>> {
    let compressor = GrepCompressor;
    if compressor.can_compress(args) {
        Some(Box::new(compressor))
    } else {
        None
    }
}

/// Find a compressor for the given rg args.
pub fn find_rg_compressor(args: &[String]) -> Option<Box<dyn Compressor>> {
    let compressor = RgCompressor;
    if compressor.can_compress(args) {
        Some(Box::new(compressor))
    } else {
        None
    }
}

/// Detected output format, used to select the correct parse strategy.
#[allow(clippy::enum_variant_names)]
enum OutputFormat {
    MultiFileWithNums,
    MultiFileNoNums,
    SingleFileWithNums,
    SingleFileNoNums,
}

/// A parsed line from grep output.
enum ParsedLine {
    Match {
        file: String,
        line_num: Option<u64>,
        content: String,
    },
    Context {
        file: String,
        line_num: Option<u64>,
        content: String,
    },
    Separator,
    Binary {
        raw: String,
    },
}

/// One file's group of output lines.
struct FileGroup {
    filename: String,
    lines: Vec<GroupLine>,
}

enum GroupLine {
    Match {
        line_num: Option<u64>,
        content: String,
    },
    Context {
        line_num: Option<u64>,
        content: String,
    },
    Separator,
}

/// Shared compression logic for both grep and rg output.
fn compress_grep_output(stdout: &str, stderr: &str, exit_code: i32) -> Option<String> {
    // Step 1: exit code handling
    match exit_code {
        0 | 1 => {}
        _ => return None,
    }

    // Step 2: empty output
    if stdout.trim().is_empty() {
        return Some(String::new());
    }

    // Step 3: detect format
    let format = detect_format(stdout);

    // Step 4: parse into file groups
    let groups = match format {
        OutputFormat::SingleFileWithNums | OutputFormat::SingleFileNoNums => {
            // Single-file: no grouping, no filtering — just cap and return
            let (body, remaining) = compress_single_file(stdout);
            let mut output = body;
            if remaining > 0 {
                output.push_str(&format!("\n... and {} more matches", remaining));
            }
            if !stderr.is_empty() {
                output.push_str("\nerrors:");
                for line in stderr.lines() {
                    output.push_str(&format!("\n  {}", line));
                }
            }
            return Some(output);
        }
        OutputFormat::MultiFileWithNums => parse_multi_file_with_line_nums(stdout),
        OutputFormat::MultiFileNoNums => parse_multi_file_no_line_nums(stdout),
    };

    // Step 5: normalize paths (strip ./) and filter noise
    let (clean_groups, filtered_count) = filter_and_normalize_groups(groups);

    // Step 6: render with match cap, build footer
    let (body, cap_remaining) = render_file_groups(&clean_groups);
    let mut output = body;

    if cap_remaining > 0 {
        output.push_str(&format!("\n... and {} more matches", cap_remaining));
    }
    if filtered_count > 0 {
        output.push_str(&format!("\n{} matches filtered", filtered_count));
    }
    if !stderr.is_empty() {
        output.push_str("\nerrors:");
        for line in stderr.lines() {
            output.push_str(&format!("\n  {}", line));
        }
    }

    Some(output)
}

/// Detect the output format by finding the first match line (with `:` separator).
/// Skips context lines (which use `-` separator) so that `-B`/`-C` flags don't
/// confuse detection.
fn detect_format(stdout: &str) -> OutputFormat {
    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed == "--" || trimmed.starts_with("Binary file ") {
            continue;
        }

        // Skip lines without ':' — these are context lines or plain content.
        // We need a match line (which always uses ':') to detect the format.
        let Some(colon_pos) = line.find(':') else {
            continue;
        };

        let first_field = &line[..colon_pos];

        if first_field.chars().all(|c| c.is_ascii_digit()) && !first_field.is_empty() {
            return OutputFormat::SingleFileWithNums;
        }

        // First field is a filename. Check second field.
        let after_first = &line[colon_pos + 1..];
        if let Some(second_colon) = after_first.find(':') {
            let second_field = &after_first[..second_colon];
            if second_field.chars().all(|c| c.is_ascii_digit()) && !second_field.is_empty() {
                return OutputFormat::MultiFileWithNums;
            }
        }

        return OutputFormat::MultiFileNoNums;
    }

    // No lines with ':' found — either plain content or empty
    OutputFormat::SingleFileNoNums
}

/// Compress single-file output (no grouping, just apply cap).
fn compress_single_file(stdout: &str) -> (String, usize) {
    let mut match_count = 0usize;
    let mut capped = false;
    let mut cut_byte_offset = 0usize;

    for line in stdout.lines() {
        let is_match = is_single_file_match(line);

        if is_match {
            if match_count >= MAX_MATCHES {
                capped = true;
                break;
            }
            match_count += 1;
        }

        // Track byte offset past this line (including the newline separator)
        cut_byte_offset += line.len() + 1;
    }

    if !capped {
        // No cap hit — return stdout exactly as-is (preserving trailing newline)
        return (stdout.trim_end().to_string(), 0);
    }

    // Count remaining match lines after the cut point
    let remaining_count = stdout[cut_byte_offset..]
        .lines()
        .filter(|line| is_single_file_match(line))
        .count();

    // Return everything up to the cut point
    let body = stdout[..cut_byte_offset].trim_end();
    (body.to_string(), remaining_count)
}

fn is_single_file_match(line: &str) -> bool {
    if line == "--" {
        return false;
    }
    if let Some(colon) = line.find(':') {
        let prefix = &line[..colon];
        prefix.chars().all(|c| c.is_ascii_digit())
    } else if let Some(dash) = line.find('-') {
        let prefix = &line[..dash];
        !prefix.chars().all(|c| c.is_ascii_digit())
    } else {
        true
    }
}

/// Parse a multi-file line with line numbers: `filename:linenum:content` or `filename-linenum-content`.
/// `current_file` is the most recently seen match file (for context line disambiguation).
fn parse_multi_file_line_with_nums(
    line: &str,
    current_file: &Option<String>,
) -> Option<ParsedLine> {
    if line == "--" {
        return Some(ParsedLine::Separator);
    }

    if line.starts_with("Binary file ") && line.ends_with(" matches") {
        return Some(ParsedLine::Binary {
            raw: line.to_string(),
        });
    }

    // Try to identify as a context line using the known current_file prefix.
    // Context line format: `filename-linenum-content`
    // We use the known filename as an anchor to avoid ambiguity with hyphens in filenames.
    if let Some(file) = current_file {
        let prefix = format!("{}-", file);
        if line.starts_with(&prefix) {
            let after = &line[prefix.len()..];
            // Parse line number: digits up to next '-'
            if let Some(dash_pos) = after.find('-') {
                let num_str = &after[..dash_pos];
                if !num_str.is_empty() && num_str.chars().all(|c| c.is_ascii_digit()) {
                    let line_num: u64 = num_str.parse().ok()?;
                    let content = after[dash_pos + 1..].to_string();
                    return Some(ParsedLine::Context {
                        file: file.clone(),
                        line_num: Some(line_num),
                        content,
                    });
                }
            }
        }
    }

    // Match line: find first ':', everything before is filename.
    let colon1 = line.find(':')?;
    let filename = &line[..colon1];
    let after1 = &line[colon1 + 1..];

    // Try to parse line number (second field)
    if let Some(colon2) = after1.find(':') {
        let num_str = &after1[..colon2];
        if !num_str.is_empty() && num_str.chars().all(|c| c.is_ascii_digit()) {
            let line_num: u64 = num_str.parse().ok()?;
            let content = after1[colon2 + 1..].to_string();
            return Some(ParsedLine::Match {
                file: filename.to_string(),
                line_num: Some(line_num),
                content,
            });
        }
    }

    // Fallback: treat as match without line number
    Some(ParsedLine::Match {
        file: filename.to_string(),
        line_num: None,
        content: after1.to_string(),
    })
}

/// Parse a multi-file line without line numbers: `filename:content` or `filename-content`.
fn parse_multi_file_line_no_nums(line: &str, current_file: &Option<String>) -> Option<ParsedLine> {
    if line == "--" {
        return Some(ParsedLine::Separator);
    }

    if line.starts_with("Binary file ") && line.ends_with(" matches") {
        return Some(ParsedLine::Binary {
            raw: line.to_string(),
        });
    }

    // Context line: try matching `filename-content` with known filename
    if let Some(file) = current_file {
        let prefix = format!("{}-", file);
        if line.starts_with(&prefix) {
            let content = line[prefix.len()..].to_string();
            return Some(ParsedLine::Context {
                file: file.clone(),
                line_num: None,
                content,
            });
        }
    }

    // Match line: split on first ':'
    let colon_pos = line.find(':')?;
    Some(ParsedLine::Match {
        file: line[..colon_pos].to_string(),
        line_num: None,
        content: line[colon_pos + 1..].to_string(),
    })
}

/// Group parsed lines into `FileGroup` entries.
fn build_file_groups(parsed_lines: Vec<ParsedLine>) -> Vec<FileGroup> {
    let mut groups: Vec<FileGroup> = Vec::new();

    for parsed in parsed_lines {
        match parsed {
            ParsedLine::Binary { raw } => {
                // Binary matches get their own single-line group
                groups.push(FileGroup {
                    filename: raw,
                    lines: Vec::new(),
                });
            }
            ParsedLine::Separator => {
                // Append separator to the current group if there is one
                if let Some(group) = groups.last_mut() {
                    group.lines.push(GroupLine::Separator);
                }
            }
            ParsedLine::Match {
                file,
                line_num,
                content,
            } => {
                // Start new group if file changes
                if groups.last().map(|g| g.filename.as_str()) != Some(file.as_str()) {
                    groups.push(FileGroup {
                        filename: file.clone(),
                        lines: Vec::new(),
                    });
                }
                if let Some(group) = groups.last_mut() {
                    group.lines.push(GroupLine::Match { line_num, content });
                }
            }
            ParsedLine::Context {
                file,
                line_num,
                content,
            } => {
                if groups.last().map(|g| g.filename.as_str()) != Some(file.as_str()) {
                    groups.push(FileGroup {
                        filename: file.clone(),
                        lines: Vec::new(),
                    });
                }
                if let Some(group) = groups.last_mut() {
                    group.lines.push(GroupLine::Context { line_num, content });
                }
            }
        }
    }

    // Remove trailing separators from each group
    for group in &mut groups {
        while matches!(group.lines.last(), Some(GroupLine::Separator)) {
            group.lines.pop();
        }
    }

    groups
}

/// Strip `./` prefix from filenames and filter out noise files (.git, __pycache__, etc.).
/// Returns (cleaned groups, count of filtered match lines).
fn filter_and_normalize_groups(groups: Vec<FileGroup>) -> (Vec<FileGroup>, usize) {
    let mut clean = Vec::new();
    let mut filtered_count = 0usize;

    for mut group in groups {
        // Strip ./ prefix
        if let Some(stripped) = group.filename.strip_prefix("./") {
            group.filename = stripped.to_string();
        }

        // Filter noise files
        if should_filter(&group.filename) {
            filtered_count += group
                .lines
                .iter()
                .filter(|l| matches!(l, GroupLine::Match { .. }))
                .count();
            // Binary match groups have no sub-lines — count as 1
            if group.lines.is_empty() {
                filtered_count += 1;
            }
            continue;
        }

        clean.push(group);
    }

    (clean, filtered_count)
}

/// Render a list of `FileGroup`s to a string, enforcing the match cap.
/// Returns (rendered_string, remaining_match_count).
fn render_file_groups(groups: &[FileGroup]) -> (String, usize) {
    let mut parts: Vec<String> = Vec::new();
    let mut match_count = 0usize;
    let mut capped = false;
    let mut remaining = 0usize;

    'outer: for (group_idx, group) in groups.iter().enumerate() {
        // Binary file lines have no sub-lines; emit as-is.
        if group.lines.is_empty() {
            if capped {
                // Still count as 1 match for binary?
                // Binary lines are rare; treat them as a single match.
                remaining += 1;
            } else {
                if match_count >= MAX_MATCHES {
                    capped = true;
                    remaining += 1;
                    continue;
                }
                parts.push(group.filename.clone());
                match_count += 1;
            }
            continue;
        }

        // Determine padding width from line numbers in this group.
        let max_digits = group
            .lines
            .iter()
            .filter_map(|l| match l {
                GroupLine::Match {
                    line_num: Some(n), ..
                }
                | GroupLine::Context {
                    line_num: Some(n), ..
                } => Some(count_digits(*n)),
                _ => None,
            })
            .max()
            .unwrap_or(0);

        let mut group_lines: Vec<String> = Vec::new();
        group_lines.push(group.filename.clone());

        for gl in &group.lines {
            match gl {
                GroupLine::Match { line_num, content } => {
                    if capped {
                        remaining += 1;
                        continue;
                    }
                    if match_count >= MAX_MATCHES {
                        capped = true;
                        remaining += 1;
                        continue;
                    }
                    match_count += 1;
                    let formatted = format_match_line(*line_num, content, max_digits);
                    group_lines.push(formatted);
                }
                GroupLine::Context { line_num, content } => {
                    if !capped {
                        let formatted = format_context_line(*line_num, content, max_digits);
                        group_lines.push(formatted);
                    }
                }
                GroupLine::Separator => {
                    if !capped {
                        group_lines.push("  --".to_string());
                    }
                }
            }
        }

        // Only emit the group if it has more than just the filename header.
        if group_lines.len() > 1 || capped {
            // If we hit cap mid-group, still push what we have
            if group_lines.len() > 1 {
                parts.push(group_lines.join("\n"));
            }
        }

        if capped {
            // Count remaining matches in all subsequent groups
            for remaining_group in groups.iter().skip(group_idx + 1) {
                for gl in &remaining_group.lines {
                    if matches!(gl, GroupLine::Match { .. }) {
                        remaining += 1;
                    }
                }
            }
            break 'outer;
        }
    }

    (parts.join("\n\n"), remaining)
}

fn format_match_line(line_num: Option<u64>, content: &str, max_digits: usize) -> String {
    match line_num {
        Some(n) => {
            let padded = format!("{:>width$}", n, width = max_digits);
            format!("  {}: {}", padded, content)
        }
        None => format!("  {}", content),
    }
}

fn format_context_line(line_num: Option<u64>, content: &str, max_digits: usize) -> String {
    match line_num {
        Some(n) => {
            let padded = format!("{:>width$}", n, width = max_digits);
            format!("  {}  {}", padded, content)
        }
        None => format!("  {}", content),
    }
}

fn count_digits(n: u64) -> usize {
    if n == 0 {
        return 1;
    }
    let mut count = 0;
    let mut v = n;
    while v > 0 {
        count += 1;
        v /= 10;
    }
    count
}

/// Parse multi-file output with line numbers into file groups.
fn parse_multi_file_with_line_nums(stdout: &str) -> Vec<FileGroup> {
    let mut parsed_lines: Vec<ParsedLine> = Vec::new();
    let mut current_file: Option<String> = None;
    // Buffer for context lines that appear before the first match in a group (from -B flag)
    let mut pending_context: Vec<(String, Option<u64>)> = Vec::new();

    for line in stdout.lines() {
        if line.trim().is_empty() {
            continue;
        }

        match parse_multi_file_line_with_nums(line, &current_file) {
            None => continue,
            Some(ParsedLine::Separator) => {
                // Flush pending context with unknown file
                for (content, ln) in pending_context.drain(..) {
                    parsed_lines.push(ParsedLine::Context {
                        file: current_file.clone().unwrap_or_default(),
                        line_num: ln,
                        content,
                    });
                }
                parsed_lines.push(ParsedLine::Separator);
            }
            Some(ParsedLine::Binary { raw }) => {
                pending_context.clear();
                parsed_lines.push(ParsedLine::Binary { raw });
            }
            Some(ParsedLine::Match {
                file,
                line_num,
                content,
            }) => {
                // Flush pending context now that we know the file
                for (ctx_content, ctx_ln) in pending_context.drain(..) {
                    parsed_lines.push(ParsedLine::Context {
                        file: file.clone(),
                        line_num: ctx_ln,
                        content: ctx_content,
                    });
                }
                current_file = Some(file.clone());
                parsed_lines.push(ParsedLine::Match {
                    file,
                    line_num,
                    content,
                });
            }
            Some(ParsedLine::Context {
                file,
                line_num,
                content,
            }) => {
                if current_file.is_none() {
                    // Before first match — buffer it, we'll assign file retroactively
                    pending_context.push((content, line_num));
                } else {
                    // Flush any remaining pending context first
                    for (ctx_content, ctx_ln) in pending_context.drain(..) {
                        parsed_lines.push(ParsedLine::Context {
                            file: file.clone(),
                            line_num: ctx_ln,
                            content: ctx_content,
                        });
                    }
                    parsed_lines.push(ParsedLine::Context {
                        file,
                        line_num,
                        content,
                    });
                }
            }
        }
    }

    // Flush any remaining pending context
    for (content, ln) in pending_context.drain(..) {
        parsed_lines.push(ParsedLine::Context {
            file: current_file.clone().unwrap_or_default(),
            line_num: ln,
            content,
        });
    }

    build_file_groups(parsed_lines)
}

/// Parse multi-file output without line numbers into file groups.
fn parse_multi_file_no_line_nums(stdout: &str) -> Vec<FileGroup> {
    let mut parsed_lines: Vec<ParsedLine> = Vec::new();
    let mut current_file: Option<String> = None;

    for line in stdout.lines() {
        if line.trim().is_empty() {
            continue;
        }

        match parse_multi_file_line_no_nums(line, &current_file) {
            None => continue,
            Some(ParsedLine::Separator) => {
                parsed_lines.push(ParsedLine::Separator);
            }
            Some(ParsedLine::Binary { raw }) => {
                parsed_lines.push(ParsedLine::Binary { raw });
            }
            Some(ParsedLine::Match {
                file,
                line_num,
                content,
            }) => {
                current_file = Some(file.clone());
                parsed_lines.push(ParsedLine::Match {
                    file,
                    line_num,
                    content,
                });
            }
            Some(ParsedLine::Context {
                file,
                line_num,
                content,
            }) => {
                parsed_lines.push(ParsedLine::Context {
                    file,
                    line_num,
                    content,
                });
            }
        }
    }

    build_file_groups(parsed_lines)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(strs: &[&str]) -> Vec<String> {
        strs.iter().map(|s| s.to_string()).collect()
    }

    fn compress(stdout: &str) -> Option<String> {
        compress_grep_output(stdout, "", 0)
    }

    // --- can_compress ---

    #[test]
    fn can_compress_bare_args() {
        assert!(GrepCompressor.can_compress(&args(&["-rn", "pattern", "."])));
    }

    #[test]
    fn can_compress_skip_files_with_matches() {
        assert!(!GrepCompressor.can_compress(&args(&["-l", "pattern", "."])));
    }

    #[test]
    fn can_compress_skip_long_files_with_matches() {
        assert!(!GrepCompressor.can_compress(&args(&["--files-with-matches", "pattern"])));
    }

    #[test]
    fn can_compress_skip_count() {
        assert!(!GrepCompressor.can_compress(&args(&["-c", "pattern", "."])));
    }

    #[test]
    fn can_compress_skip_long_count() {
        assert!(!GrepCompressor.can_compress(&args(&["--count", "pattern"])));
    }

    #[test]
    fn can_compress_skip_json() {
        assert!(!GrepCompressor.can_compress(&args(&["--json", "pattern"])));
    }

    #[test]
    fn can_compress_skip_null() {
        assert!(!GrepCompressor.can_compress(&args(&["-Z", "pattern", "."])));
    }

    #[test]
    fn can_compress_skip_long_null() {
        assert!(!GrepCompressor.can_compress(&args(&["--null", "pattern"])));
    }

    #[test]
    fn can_compress_skip_quiet() {
        assert!(!GrepCompressor.can_compress(&args(&["-q", "pattern", "."])));
    }

    #[test]
    fn can_compress_skip_long_quiet() {
        assert!(!GrepCompressor.can_compress(&args(&["--quiet", "pattern"])));
    }

    // --- normalized_args ---

    #[test]
    fn normalized_args_grep_adds_color_never() {
        let input = args(&["-rn", "pattern", "."]);
        let result = GrepCompressor.normalized_args(&input);
        assert_eq!(result[0], "--color=never");
        assert_eq!(&result[1..], &input[..]);
    }

    #[test]
    fn normalized_args_rg_adds_no_heading_and_color_never() {
        let input = args(&["-n", "pattern", "src/"]);
        let result = RgCompressor.normalized_args(&input);
        assert_eq!(result[0], "--no-heading");
        assert_eq!(result[1], "--color=never");
        assert_eq!(&result[2..], &input[..]);
    }

    // --- compress ---

    #[test]
    fn compress_multifile_with_line_nums() {
        let stdout = "src/main.rs:5:fn main() {\nsrc/main.rs:10:    println!(\"hello\");\nsrc/lib.rs:3:fn helper() {\n";
        let result = compress(stdout).unwrap();
        // File headers on own lines
        assert!(result.contains("src/main.rs\n"));
        assert!(result.contains("src/lib.rs\n") || result.contains("src/lib.rs"));
        // Matches indented
        assert!(result.contains("  "));
        // Line numbers present
        assert!(result.contains("5"));
        assert!(result.contains("10"));
        assert!(result.contains("3"));
    }

    #[test]
    fn compress_multifile_without_line_nums() {
        let stdout = "src/main.rs:fn main() {\nsrc/lib.rs:fn helper() {\n";
        let result = compress(stdout).unwrap();
        assert!(result.contains("src/main.rs"));
        assert!(result.contains("src/lib.rs"));
        // Content indented
        assert!(result.contains("  fn main()"));
        assert!(result.contains("  fn helper()"));
        // Files on separate groups
        let main_pos = result.find("src/main.rs").unwrap();
        let lib_pos = result.find("src/lib.rs").unwrap();
        assert!(lib_pos > main_pos);
    }

    #[test]
    fn compress_single_file_with_line_nums() {
        let stdout = "5:fn main() {\n10:    let x = 1;\n";
        let result = compress(stdout).unwrap();
        // No grouping — output preserved as-is
        assert!(result.contains("5:fn main()"));
        assert!(result.contains("10:    let x = 1;"));
    }

    #[test]
    fn compress_single_file_without_line_nums() {
        let stdout = "fn main() {\nlet x = 1;\n";
        let result = compress(stdout).unwrap();
        assert!(result.contains("fn main()"));
        assert!(result.contains("let x = 1;"));
    }

    #[test]
    fn compress_context_lines_preserved() {
        // grep -n -C1 output: match lines use ':', context lines use '-', groups separated by '--'
        let stdout = concat!(
            "src/a.rs:5:fn foo() {\n",
            "src/a.rs-6-    let x = 1;\n",
            "--\n",
            "src/a.rs:10:fn bar() {\n",
            "src/a.rs-11-    let y = 2;\n",
        );
        let result = compress(stdout).unwrap();
        // Context line uses spaces, not colon
        assert!(result.contains("  6  ") || result.contains("6  "));
        // Match line uses colon
        assert!(result.contains("5:") || result.contains(": "));
        // Separator indented
        assert!(result.contains("  --"));
    }

    #[test]
    fn compress_line_nums_right_aligned() {
        let stdout = "src/main.rs:5:line five\nsrc/main.rs:100:line hundred\nsrc/main.rs:1000:line thousand\n";
        let result = compress(stdout).unwrap();
        // All numbers should be padded to 4 digits (1000 has 4 digits)
        assert!(result.contains("    5:") || result.contains("   5:"));
        assert!(result.contains(" 100:") || result.contains("  100:"));
        assert!(result.contains("1000:"));
    }

    #[test]
    fn compress_200_match_cap() {
        // Generate 250 match lines in one file
        let stdout: String = (1..=250)
            .map(|i| format!("src/main.rs:{}:match line {}\n", i, i))
            .collect();
        let result = compress(&stdout).unwrap();
        // Count match lines in output (lines containing ": match line")
        let match_line_count = result
            .lines()
            .filter(|l| l.contains(": match line"))
            .count();
        assert_eq!(match_line_count, MAX_MATCHES);
    }

    #[test]
    fn compress_cap_footer_message() {
        let stdout: String = (1..=250)
            .map(|i| format!("src/main.rs:{}:match line {}\n", i, i))
            .collect();
        let result = compress(&stdout).unwrap();
        assert!(result.contains("... and 50 more matches"));
    }

    #[test]
    fn compress_binary_file_matches() {
        let stdout = "Binary file image.png matches\n";
        let result = compress(stdout).unwrap();
        assert!(result.contains("Binary file image.png matches"));
    }

    #[test]
    fn compress_stderr_appended() {
        let result =
            compress_grep_output("src/a.rs:1:hello\n", "grep: error reading file\n", 0).unwrap();
        assert!(result.contains("errors:"));
        assert!(result.contains("  grep: error reading file"));
    }

    #[test]
    fn compress_exit_0() {
        let result = compress_grep_output("src/a.rs:1:foo\n", "", 0);
        assert!(result.is_some());
    }

    #[test]
    fn compress_exit_1_no_matches() {
        let result = compress_grep_output("", "", 1);
        assert_eq!(result, Some(String::new()));
    }

    #[test]
    fn compress_exit_2_returns_none() {
        let result = compress_grep_output("", "grep: invalid option", 2);
        assert_eq!(result, None);
    }

    #[test]
    fn compress_empty_stdout() {
        let result = compress_grep_output("", "", 0);
        assert_eq!(result, Some(String::new()));
    }

    #[test]
    fn compress_file_groups_separated_by_blank_line() {
        let stdout = "src/main.rs:1:foo\nsrc/lib.rs:1:bar\n";
        let result = compress(stdout).unwrap();
        // Two groups should be separated by a blank line
        assert!(result.contains("\n\n"));
    }

    #[test]
    fn compress_preserves_original_whitespace() {
        let stdout = "src/main.rs:5:    indented content\n";
        let result = compress(stdout).unwrap();
        assert!(result.contains("    indented content"));
    }

    #[test]
    fn compress_before_context_detects_format_correctly() {
        // -B/C context: first line is a context line (using '-' separator), not a match line.
        // detect_format must skip context lines and find the match line to detect format.
        let stdout = concat!(
            "src/main.rs-4-    let x = 1;\n",
            "src/main.rs:5:    println!(\"hello\");\n",
            "src/main.rs-6-    let y = 2;\n",
        );
        let result = compress(stdout).unwrap();
        // Should detect as multi-file with line nums, not single-file-no-nums
        assert!(result.contains("src/main.rs\n"));
        assert!(result.contains("5:"));
    }
}
