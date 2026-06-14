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

/// True if `line` is a binary-match notice from GNU grep (`Binary file <p> matches`)
/// or ripgrep (`<p>: binary file matches (found ...)`).
fn is_binary_match_notice(line: &str) -> bool {
    (line.starts_with("Binary file ") && line.ends_with(" matches"))
        || line.contains(": binary file matches")
}

/// Args-derived hints that decide how to parse output. Carried on the compressor
/// because the `compress` trait method has no access to the original args.
#[derive(Default)]
struct FormatHints {
    /// `-n`/`--line-number` requested -> line numbers are present in output
    line_numbers: bool,
    /// filename prefix present (`-H`, `-r`/`-R`/`--recursive`, multiple file operands)
    multi_file: bool,
    /// known single-file invocation (exactly one file operand, no -H/-r) -> no prefix.
    /// disambiguates `multi_file == false`: when true the format is settled (single
    /// file), so match content like `word: rest` is never misread as a filename.
    single_file_known: bool,
}

/// Compressor for `grep` output. Groups matches by file with indented, aligned line numbers.
#[derive(Default)]
pub struct GrepCompressor {
    hints: FormatHints,
}

/// Compressor for `rg` (ripgrep) output. Same grouping logic, different normalized args.
#[derive(Default)]
pub struct RgCompressor {
    hints: FormatHints,
}

impl Compressor for GrepCompressor {
    fn can_compress(&self, args: &[String]) -> bool {
        if args.iter().any(|a| SKIP_FLAGS.contains(&a.as_str())) {
            return false;
        }
        // decline stdin mode: Command::output() gives the child null stdin, so
        // `ps aux | grep node` reads EOF and we'd report "0 matches" — passthrough
        has_file_operands(args)
    }

    fn normalized_args(&self, original_args: &[String]) -> Vec<String> {
        let mut result = vec!["--color=never".to_string()];
        result.extend_from_slice(original_args);
        result
    }

    fn compress(&self, stdout: &str, stderr: &str, exit_code: i32) -> Option<String> {
        compress_grep_output(stdout, stderr, exit_code, &self.hints)
    }
}

impl Compressor for RgCompressor {
    fn can_compress(&self, args: &[String]) -> bool {
        if args.iter().any(|a| SKIP_FLAGS.contains(&a.as_str())) {
            return false;
        }
        has_file_operands(args)
    }

    fn normalized_args(&self, original_args: &[String]) -> Vec<String> {
        let mut result = vec!["--no-heading".to_string(), "--color=never".to_string()];
        result.extend_from_slice(original_args);
        result
    }

    fn compress(&self, stdout: &str, stderr: &str, exit_code: i32) -> Option<String> {
        compress_grep_output(stdout, stderr, exit_code, &self.hints)
    }
}

/// Find a compressor for the given grep args.
pub fn find_grep_compressor(args: &[String]) -> Option<Box<dyn Compressor>> {
    // grep is not recursive unless asked (-r/-R); a single operand means a single file
    let compressor = GrepCompressor {
        hints: format_hints(args, false),
    };
    if compressor.can_compress(args) {
        Some(Box::new(compressor))
    } else {
        None
    }
}

/// Find a compressor for the given rg args.
pub fn find_rg_compressor(args: &[String]) -> Option<Box<dyn Compressor>> {
    // rg recurses into a directory operand by default -> a dir operand is multi-file
    let compressor = RgCompressor {
        hints: format_hints(args, true),
    };
    if compressor.can_compress(args) {
        Some(Box::new(compressor))
    } else {
        None
    }
}

/// Short flags whose value is the next argument (so that value isn't miscounted
/// as a file operand). Covers GNU grep + ripgrep common value-taking options.
const VALUE_FLAGS: &[&str] = &[
    "-e",
    "-f",
    "-m",
    "-A",
    "-B",
    "-C",
    "-d",
    "--regexp",
    "--file",
    "--max-count",
    "--after-context",
    "--before-context",
    "--context",
];

/// Long flags that supply the pattern (so the first positional is a file, not the pattern).
const PATTERN_LONG_FLAGS: &[&str] = &["--regexp", "--file"];

/// Scan of `args` into the operands grep/rg will read. `pattern_from_flag` is true
/// when `-e`/`-f`/`--regexp`/`--file` supplied the pattern (so no positional is
/// consumed as the pattern). `forces_filename` is true when `-H`/`-r`/`-R`/
/// `--recursive`/`--with-filename` was given. `has_line_numbers` tracks `-n`.
struct ArgScan<'a> {
    positionals: Vec<&'a str>,
    pattern_from_flag: bool,
    forces_filename: bool,
    has_line_numbers: bool,
}

/// Walk args once, classifying flags and collecting positionals. Shared by the
/// stdin-mode check and the format-hint derivation so both agree on what counts.
fn scan_args(args: &[String]) -> ArgScan<'_> {
    let mut scan = ArgScan {
        positionals: Vec::new(),
        pattern_from_flag: false,
        forces_filename: false,
        has_line_numbers: false,
    };
    let mut after_double_dash = false;
    let mut skip_next = false;

    for arg in args {
        let arg = arg.as_str();
        if skip_next {
            // value belonging to the previous flag (e.g. -A 1) — not an operand
            skip_next = false;
            continue;
        }
        if after_double_dash {
            scan.positionals.push(arg);
            continue;
        }
        match arg {
            "--" => {
                after_double_dash = true;
                continue;
            }
            "-" => {
                // explicit stdin operand
                scan.positionals.push(arg);
                continue;
            }
            "-n" | "--line-number" => {
                scan.has_line_numbers = true;
                continue;
            }
            "-H" | "--with-filename" | "-r" | "-R" | "--recursive" => {
                scan.forces_filename = true;
                continue;
            }
            _ => {}
        }
        if arg.starts_with("--") {
            if let Some((name, _)) = arg.split_once('=') {
                if PATTERN_LONG_FLAGS.contains(&name) {
                    scan.pattern_from_flag = true;
                }
            } else {
                if PATTERN_LONG_FLAGS.contains(&arg) {
                    scan.pattern_from_flag = true;
                }
                if VALUE_FLAGS.contains(&arg) {
                    skip_next = true;
                }
            }
            continue;
        }
        if let Some(letters) = arg.strip_prefix('-') {
            // bundled short flags
            if letters.contains('n') {
                scan.has_line_numbers = true;
            }
            if letters.contains('H') || letters.contains('r') || letters.contains('R') {
                scan.forces_filename = true;
            }
            if letters.contains('e') || letters.contains('f') {
                scan.pattern_from_flag = true;
            }
            if VALUE_FLAGS.contains(&arg) {
                skip_next = true;
            }
            continue;
        }
        scan.positionals.push(arg);
    }

    scan
}

/// The file/dir operands grep/rg will read (the pattern positional removed unless
/// supplied by flag). A lone `-` stays in the list and is treated as stdin.
fn file_operands<'a>(scan: &ArgScan<'a>) -> Vec<&'a str> {
    if scan.pattern_from_flag {
        scan.positionals.clone()
    } else {
        // first positional is the pattern; the rest are files
        scan.positionals.iter().skip(1).copied().collect()
    }
}

/// Returns true if args name at least one real file/dir operand (not stdin).
/// Declines stdin mode (`ps aux | grep node`), which under captured execution
/// reads EOF and would falsely report zero matches.
fn has_file_operands(args: &[String]) -> bool {
    let scan = scan_args(args);
    file_operands(&scan).iter().any(|p| *p != "-")
}

/// Derive parse hints from the invocation args. `recursive_by_default` is true
/// for rg (a directory operand is searched recursively -> multi-file output).
fn format_hints(args: &[String], recursive_by_default: bool) -> FormatHints {
    let scan = scan_args(args);
    let real_files: Vec<&str> = file_operands(&scan)
        .into_iter()
        .filter(|p| *p != "-")
        .collect();

    let mut hints = FormatHints {
        line_numbers: scan.has_line_numbers,
        multi_file: scan.forces_filename,
        single_file_known: false,
    };

    // multiple operands always yield filename prefixes
    if real_files.len() > 1 {
        hints.multi_file = true;
    }

    // a single operand: filename prefix appears only if it's a directory (recursive)
    if !hints.multi_file && real_files.len() == 1 {
        let is_dir = std::path::Path::new(real_files[0]).is_dir();
        if recursive_by_default && is_dir {
            // rg recurses the dir -> multi-file with prefixes
            hints.multi_file = true;
        } else if !is_dir {
            // a single regular file -> no prefix; format is settled as single-file
            hints.single_file_known = true;
        }
    }

    hints
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
fn compress_grep_output(
    stdout: &str,
    stderr: &str,
    exit_code: i32,
    hints: &FormatHints,
) -> Option<String> {
    // Step 1: exit code handling
    match exit_code {
        0 | 1 => {}
        _ => return None,
    }

    // Step 2: empty output — surface stderr so errors/warnings aren't lost
    if stdout.trim().is_empty() {
        if stderr.is_empty() {
            return Some(String::new());
        }
        let mut output = String::from("errors:");
        for line in stderr.lines() {
            output.push_str(&format!("\n  {}", line));
        }
        return Some(output);
    }

    // Step 3: detect format
    let format = detect_format(stdout, hints);

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

/// Detect the output format. The args-derived `hints` are authoritative for
/// WHETHER filenames / line numbers are present (so match content that merely
/// looks like `digits:` or `word: rest` isn't misread). Content sniffing is only
/// the fallback when no hints were supplied (e.g. a bare invocation).
fn detect_format(stdout: &str, hints: &FormatHints) -> OutputFormat {
    // hints settle the format unambiguously when set
    if hints.multi_file {
        return if hints.line_numbers {
            OutputFormat::MultiFileWithNums
        } else {
            OutputFormat::MultiFileNoNums
        };
    }
    if hints.single_file_known {
        // single file, no prefix: content's colons/digits are not filename/linenum
        return if hints.line_numbers {
            OutputFormat::SingleFileWithNums
        } else {
            OutputFormat::SingleFileNoNums
        };
    }
    if hints.line_numbers {
        // single file (no filename prefix) but -n requested
        return OutputFormat::SingleFileWithNums;
    }

    // no hints — fall back to sniffing the first match line (with `:` separator),
    // skipping context lines (`-` separator) so -B/-C don't confuse detection
    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed == "--" || is_binary_match_notice(trimmed) {
            continue;
        }

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

    // split_inclusive keeps each line's real terminator (`\n` or `\r\n`), so the
    // accumulated offset always lands on a true line boundary — never mid-line or
    // mid multi-byte char (which `line.len() + 1` undercounts on CRLF -> panic)
    for chunk in stdout.split_inclusive('\n') {
        let line = chunk.trim_end_matches(['\r', '\n']);
        if is_single_file_match(line) {
            if match_count >= MAX_MATCHES {
                capped = true;
                break;
            }
            match_count += 1;
        }
        cut_byte_offset += chunk.len();
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

    if is_binary_match_notice(line) {
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
    // A context line for a *different* file (`newfile-num-content`) has no ':' in
    // its `file-num-` prefix; recognize it so we neither drop it nor fabricate a
    // filename out of match content.
    let colon1 = match line.find(':') {
        Some(pos) => pos,
        None => return parse_new_file_context_with_nums(line),
    };
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

    // No second ':' -> not a real `file:num:` match. It may be a new-file context
    // line whose content happens to contain a ':' (e.g. `newfile-12-foo: bar`).
    if let Some(parsed) = parse_new_file_context_with_nums(line) {
        return Some(parsed);
    }

    // Fallback: treat as match without line number
    Some(ParsedLine::Match {
        file: filename.to_string(),
        line_num: None,
        content: after1.to_string(),
    })
}

/// Parse a new-file context line `filename-linenum-content` (different file than
/// `current_file`). Anchors on the first `-<digits>-` whose filename part has no
/// ':' (a real match line would put its ':' before any content). Returns None if
/// the line doesn't fit this shape.
fn parse_new_file_context_with_nums(line: &str) -> Option<ParsedLine> {
    let bytes = line.as_bytes();
    let mut search_from = 0usize;
    while let Some(rel) = line[search_from..].find('-') {
        let dash1 = search_from + rel;
        let filename = &line[..dash1];
        // a real match's ':' precedes content; reject if filename part holds one
        if filename.is_empty() || filename.contains(':') {
            return None;
        }
        let after = &line[dash1 + 1..];
        if let Some(dash2_rel) = after.find('-') {
            let num_str = &after[..dash2_rel];
            if !num_str.is_empty() && num_str.bytes().all(|b| b.is_ascii_digit()) {
                let line_num: u64 = num_str.parse().ok()?;
                let content = after[dash2_rel + 1..].to_string();
                return Some(ParsedLine::Context {
                    file: filename.to_string(),
                    line_num: Some(line_num),
                    content,
                });
            }
        }
        // advance past this dash and retry (filename may contain '-')
        search_from = dash1 + 1;
        if search_from >= bytes.len() {
            break;
        }
    }
    None
}

/// Parse a multi-file line without line numbers: `filename:content` or `filename-content`.
fn parse_multi_file_line_no_nums(line: &str, current_file: &Option<String>) -> Option<ParsedLine> {
    if line == "--" {
        return Some(ParsedLine::Separator);
    }

    if is_binary_match_notice(line) {
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
    let mut blocks: Vec<crate::compressors::tree::PathBlock> = Vec::new();
    let mut match_count = 0usize;
    let mut capped = false;
    let mut remaining = 0usize;

    'outer: for (group_idx, group) in groups.iter().enumerate() {
        // Binary file lines have no sub-lines; emit as-is.
        if group.lines.is_empty() {
            if capped {
                // Binary lines are rare; treat them as a single match
                remaining += 1;
            } else {
                if match_count >= MAX_MATCHES {
                    capped = true;
                    remaining += 1;
                    continue;
                }
                blocks.push(crate::compressors::tree::PathBlock {
                    path: Some(group.filename.clone()),
                    block: group.filename.clone(),
                });
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
                blocks.push(crate::compressors::tree::PathBlock {
                    path: Some(group.filename.clone()),
                    block: group_lines.join("\n"),
                });
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

    (
        crate::compressors::tree::group_by_directory(blocks).join("\n"),
        remaining,
    )
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
        compress_grep_output(stdout, "", 0, &FormatHints::default())
    }

    fn compress_with_hints(stdout: &str, hints: &FormatHints) -> Option<String> {
        compress_grep_output(stdout, "", 0, hints)
    }

    // --- can_compress ---

    #[test]
    fn can_compress_bare_args() {
        assert!(GrepCompressor::default().can_compress(&args(&["-rn", "pattern", "."])));
    }

    #[test]
    fn can_compress_skip_files_with_matches() {
        assert!(!GrepCompressor::default().can_compress(&args(&["-l", "pattern", "."])));
    }

    #[test]
    fn can_compress_skip_long_files_with_matches() {
        assert!(
            !GrepCompressor::default().can_compress(&args(&["--files-with-matches", "pattern"]))
        );
    }

    #[test]
    fn can_compress_skip_count() {
        assert!(!GrepCompressor::default().can_compress(&args(&["-c", "pattern", "."])));
    }

    #[test]
    fn can_compress_skip_long_count() {
        assert!(!GrepCompressor::default().can_compress(&args(&["--count", "pattern"])));
    }

    #[test]
    fn can_compress_skip_json() {
        assert!(!GrepCompressor::default().can_compress(&args(&["--json", "pattern"])));
    }

    #[test]
    fn can_compress_skip_null() {
        assert!(!GrepCompressor::default().can_compress(&args(&["-Z", "pattern", "."])));
    }

    #[test]
    fn can_compress_skip_long_null() {
        assert!(!GrepCompressor::default().can_compress(&args(&["--null", "pattern"])));
    }

    #[test]
    fn can_compress_skip_quiet() {
        assert!(!GrepCompressor::default().can_compress(&args(&["-q", "pattern", "."])));
    }

    #[test]
    fn can_compress_skip_long_quiet() {
        assert!(!GrepCompressor::default().can_compress(&args(&["--quiet", "pattern"])));
    }

    // --- normalized_args ---

    #[test]
    fn normalized_args_grep_adds_color_never() {
        let input = args(&["-rn", "pattern", "."]);
        let result = GrepCompressor::default().normalized_args(&input);
        assert_eq!(result[0], "--color=never");
        assert_eq!(&result[1..], &input[..]);
    }

    #[test]
    fn normalized_args_rg_adds_no_heading_and_color_never() {
        let input = args(&["-n", "pattern", "src/"]);
        let result = RgCompressor::default().normalized_args(&input);
        assert_eq!(result[0], "--no-heading");
        assert_eq!(result[1], "--color=never");
        assert_eq!(&result[2..], &input[..]);
    }

    // --- format_hints (args -> parse hints) ---

    #[test]
    fn format_hints_recursive_no_n_is_multi_file_no_nums() {
        // -r forces filename prefix; absence of -n means no line numbers
        let hints = format_hints(&args(&["-r", "pat", "."]), false);
        assert!(hints.multi_file);
        assert!(!hints.line_numbers);
        assert!(!hints.single_file_known);
    }

    #[test]
    fn format_hints_two_files_is_multi_file() {
        // multiple operands always produce filename prefixes
        let hints = format_hints(&args(&["pat", "a.rs", "b.rs"]), false);
        assert!(hints.multi_file);
    }

    // --- compress ---

    #[test]
    fn compress_multifile_with_line_nums() {
        let stdout = "src/main.rs:5:fn main() {\nsrc/main.rs:10:    println!(\"hello\");\nsrc/lib.rs:3:fn helper() {\n";
        let result = compress(stdout).unwrap();
        // Both files share src/ — directory header collapses them
        assert!(
            result.contains("src/\n"),
            "expected src/ header, got: {result:?}"
        );
        // Short display names (no dir prefix) under the header
        assert!(
            result.contains("lib.rs\n"),
            "expected lib.rs under header, got: {result:?}"
        );
        assert!(
            result.contains("main.rs\n"),
            "expected main.rs under header, got: {result:?}"
        );
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
        // Both files share src/ — directory header collapses them; short names only
        assert!(
            result.contains("src/\n"),
            "expected src/ header, got: {result:?}"
        );
        assert!(
            result.contains("lib.rs\n"),
            "expected lib.rs under header, got: {result:?}"
        );
        assert!(
            result.contains("main.rs\n"),
            "expected main.rs under header, got: {result:?}"
        );
        // Content indented (tree adds another 2-space layer on top of the existing 2-space indent)
        assert!(result.contains("fn main()"));
        assert!(result.contains("fn helper()"));
        // lib.rs appears before main.rs (alphabetical order in tree)
        let lib_pos = result.find("lib.rs").unwrap();
        let main_pos = result.find("main.rs").unwrap();
        assert!(lib_pos < main_pos);
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
        let result = compress_grep_output(
            "src/a.rs:1:hello\n",
            "grep: error reading file\n",
            0,
            &FormatHints::default(),
        )
        .unwrap();
        assert!(result.contains("errors:"));
        assert!(result.contains("  grep: error reading file"));
    }

    #[test]
    fn compress_exit_0() {
        let result = compress_grep_output("src/a.rs:1:foo\n", "", 0, &FormatHints::default());
        assert!(result.is_some());
    }

    #[test]
    fn compress_exit_1_no_matches() {
        let result = compress_grep_output("", "", 1, &FormatHints::default());
        assert_eq!(result, Some(String::new()));
    }

    #[test]
    fn compress_exit_2_returns_none() {
        let result = compress_grep_output("", "grep: invalid option", 2, &FormatHints::default());
        assert_eq!(result, None);
    }

    #[test]
    fn compress_empty_stdout() {
        let result = compress_grep_output("", "", 0, &FormatHints::default());
        assert_eq!(result, Some(String::new()));
    }

    #[test]
    fn compress_same_dir_files_grouped_under_dir_header() {
        // Both files share src/ — tree groups them under one header, no blank line between
        let stdout = "src/main.rs:1:foo\nsrc/lib.rs:1:bar\n";
        let result = compress(stdout).unwrap();
        assert!(
            result.contains("src/\n"),
            "expected src/ header, got: {result:?}"
        );
        // No blank lines — tree output joins with single \n
        assert!(
            !result.contains("\n\n"),
            "unexpected blank line in tree output: {result:?}"
        );
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

    // --- tree grouping ---

    #[test]
    fn tree_shared_dir_produces_dir_header_with_indented_files() {
        // Arrange: two files sharing src/, each with one match
        let stdout = "src/foo.rs:1:hello\nsrc/bar.rs:2:world\n";
        // Act
        let result = compress(stdout).unwrap();
        // Assert: directory header, both short names indented under it, matches further indented
        assert_eq!(
            result,
            "src/\n  bar.rs\n    2: world\n  foo.rs\n    1: hello"
        );
    }

    #[test]
    fn tree_lone_deep_file_stays_inline_no_header() {
        // Arrange: single file buried in a deep path — no siblings, so chain compresses inline
        let stdout = "a/b/c/needle.rs:5:found\n";
        // Act
        let result = compress(stdout).unwrap();
        // Assert: full path kept, no directory header lines
        assert_eq!(result, "a/b/c/needle.rs\n  5: found");
    }

    #[test]
    fn tree_remaining_overflow_correct_across_grouped_files() {
        // Arrange: 150 matches in src/a.rs + 100 in src/b.rs = 250 total; cap is MAX_MATCHES=200
        let mut stdout = String::new();
        for i in 1..=150u64 {
            stdout.push_str(&format!("src/a.rs:{}:line {}\n", i, i));
        }
        for i in 1..=100u64 {
            stdout.push_str(&format!("src/b.rs:{}:line {}\n", i, i));
        }
        // Act
        let result = compress(&stdout).unwrap();
        // Assert: exactly 50 matches overflow
        assert!(
            result.contains("... and 50 more matches"),
            "expected overflow footer, got: {result:?}"
        );
        // Emitted match lines: lines containing ": line " that are indented
        let emitted = result.lines().filter(|l| l.contains(": line ")).count();
        assert_eq!(emitted, MAX_MATCHES);
    }

    // --- fix: decline stdin mode (no file operands) ---

    #[test]
    fn can_compress_declines_stdin_pattern_only() {
        // `ps aux | grep node` — pattern but no file operand -> reads stdin -> decline
        assert!(!GrepCompressor::default().can_compress(&args(&["node"])));
        assert!(!RgCompressor::default().can_compress(&args(&["node"])));
    }

    #[test]
    fn can_compress_declines_explicit_dash_stdin() {
        // lone `-` operand is stdin, not a file
        assert!(!GrepCompressor::default().can_compress(&args(&["pattern", "-"])));
    }

    #[test]
    fn can_compress_accepts_pattern_with_file() {
        assert!(GrepCompressor::default().can_compress(&args(&["pattern", "file.rs"])));
    }

    #[test]
    fn can_compress_accepts_pattern_via_e_flag_with_file() {
        // -e supplies the pattern, so the lone positional is a file operand
        assert!(GrepCompressor::default().can_compress(&args(&["-e", "pat", "file.rs"])));
        // -e with no file -> stdin -> decline
        assert!(!GrepCompressor::default().can_compress(&args(&["-e", "pat"])));
    }

    // --- fix: empty stdout must surface stderr ---

    #[test]
    fn compress_empty_stdout_with_stderr_surfaces_errors() {
        // no matches but a real error must not be discarded as Some("")
        let result = compress_grep_output(
            "",
            "grep: foo: No such file or directory\n",
            1,
            &FormatHints::default(),
        )
        .unwrap();
        assert!(result.contains("errors:"));
        assert!(result.contains("  grep: foo: No such file or directory"));
    }

    // --- fix: detect_format relies on args, not content sniffing ---

    #[test]
    fn detect_format_no_n_flag_keeps_digit_prefixed_content() {
        // recursive grep WITHOUT -n; content begins with `12:` — must NOT become a line number
        let stdout = "src/a.rs:12: o'clock note\n";
        let hints = FormatHints {
            line_numbers: false,
            multi_file: true,
            single_file_known: false,
        };
        let result = compress_with_hints(stdout, &hints).unwrap();
        // content `12: o'clock note` preserved verbatim (no fake line-number column)
        assert!(
            result.contains("12: o'clock note"),
            "expected raw content kept, got: {result:?}"
        );
    }

    #[test]
    fn detect_format_single_file_word_colon_not_treated_as_filename() {
        // single file, no -n, no prefix; content `word: rest` must stay content
        let stdout = "warning: deprecated call\n";
        let hints = FormatHints {
            line_numbers: false,
            multi_file: false,
            single_file_known: true,
        };
        let result = compress_with_hints(stdout, &hints).unwrap();
        // whole line preserved; not split into filename `warning` + match
        assert_eq!(result, "warning: deprecated call");
    }

    // --- fix: ripgrep binary-match notice recognized ---

    #[test]
    fn binary_notice_recognizes_ripgrep_format() {
        assert!(is_binary_match_notice(
            "src/data.bin: binary file matches (found \"\\0\" byte around offset 10)"
        ));
        assert!(is_binary_match_notice("Binary file image.png matches"));
        assert!(!is_binary_match_notice("src/main.rs:5:fn main() {"));
    }

    #[test]
    fn compress_ripgrep_binary_match_preserved() {
        let stdout = "src/data.bin: binary file matches (found \"\\0\" byte around offset 10)\n";
        let hints = FormatHints {
            line_numbers: true,
            multi_file: true,
            single_file_known: false,
        };
        let result = compress_with_hints(stdout, &hints).unwrap();
        assert!(
            result.contains("binary file matches"),
            "binary notice lost, got: {result:?}"
        );
    }

    // --- fix: context line for a NEW file is not dropped/fabricated ---

    #[test]
    fn context_line_for_new_file_assigned_correctly() {
        // -B context for b.rs precedes its match; current_file is still a.rs.
        // The new-file context must group under b.rs, not corrupt a.rs or drop.
        let stdout = concat!(
            "a.rs:5:alpha\n",
            "b.rs-9-before context\n",
            "b.rs:10:beta\n"
        );
        let hints = FormatHints {
            line_numbers: true,
            multi_file: true,
            single_file_known: false,
        };
        let result = compress_with_hints(stdout, &hints).unwrap();
        // both files present, context content kept, no bogus `b.rs-9` filename group
        assert!(result.contains("a.rs"), "missing a.rs: {result:?}");
        assert!(result.contains("b.rs"), "missing b.rs: {result:?}");
        assert!(
            result.contains("before context"),
            "context dropped: {result:?}"
        );
        assert!(
            !result.contains("b.rs-9"),
            "fabricated filename group: {result:?}"
        );
    }

    // --- fix: CRLF / multi-byte offset is panic-proof ---

    #[test]
    fn compress_single_file_crlf_over_cap_no_panic() {
        // CRLF lines + a multi-byte char; over the cap so the byte-offset slice runs.
        // `line.len() + 1` would undercount the '\r' and could slice mid-char -> panic.
        let mut stdout = String::new();
        for i in 1..=(MAX_MATCHES + 5) {
            stdout.push_str(&format!("{}:café ☕ line\r\n", i));
        }
        let hints = FormatHints {
            line_numbers: true,
            multi_file: false,
            single_file_known: true,
        };
        // must not panic and must report the overflow
        let result = compress_with_hints(&stdout, &hints).unwrap();
        assert!(
            result.contains("... and 5 more matches"),
            "expected overflow footer in result"
        );
    }
}
