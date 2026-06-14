use crate::compressors::Compressor;

pub struct LsCompressor;

impl Compressor for LsCompressor {
    fn can_compress(&self, args: &[String]) -> bool {
        let mut has_l = false;
        let mut path_operands: usize = 0;
        let mut after_double_dash = false;

        for arg in args {
            if after_double_dash {
                path_operands += 1;
                continue;
            }
            if arg == "--" {
                after_double_dash = true; // rest are paths
                continue;
            }
            // long option (--foo): not a short-flag bundle, but GNU long forms of
            // the lossy short flags must still decline (their behavior is dropped)
            if arg.starts_with("--") {
                // R recursive, r reverse, any --sort (covers t/S/time/size), d directory-entry
                if arg == "--recursive"
                    || arg == "--reverse"
                    || arg == "--sort"
                    || arg.starts_with("--sort=")
                    || arg == "--directory"
                {
                    return false;
                }
                continue;
            }
            if !arg.starts_with('-') {
                path_operands += 1;
                continue;
            }
            // single-dash bundle: inspect its letters
            let flags = &arg[1..];
            // lossy modes we cannot faithfully compress -> decline (passthrough):
            // R recursive, t/S/r sort, d directory-entry
            if flags.contains('R')
                || flags.contains('t')
                || flags.contains('S')
                || flags.contains('r')
                || flags.contains('d')
            {
                return false;
            }
            if flags.contains('l') {
                has_l = true;
            }
        }

        // multiple operands -> ls emits "dir:" section headers we can't faithfully
        // group; decline so the real, grouped output survives
        if path_operands > 1 {
            return false;
        }

        has_l
    }

    fn normalized_args(&self, original_args: &[String]) -> Vec<String> {
        let mut paths: Vec<String> = Vec::new();
        let mut after_double_dash = false;

        for arg in original_args {
            if after_double_dash {
                paths.push(arg.clone());
            } else if arg == "--" {
                after_double_dash = true;
            } else if !arg.starts_with('-') {
                paths.push(arg.clone());
            }
        }

        let mut result = vec!["-la".to_string(), "--".to_string()];
        result.extend(paths);
        result
    }

    fn compress(&self, stdout: &str, _stderr: &str, exit_code: i32) -> Option<String> {
        if exit_code != 0 {
            return None;
        }

        let mut output_lines: Vec<String> = Vec::new();

        for line in stdout.lines() {
            // Skip the "total N" line
            if line.starts_with("total ") {
                continue;
            }

            if let Some(entry) = parse_ls_line(line) {
                output_lines.push(format_entry(&entry));
            }
        }

        Some(output_lines.join("\n"))
    }
}

enum EntryType {
    Directory,
    Symlink,
    Executable,
    Regular,
}

enum Size {
    Bytes(u64),
    // device number kept verbatim: "8, 0" (Linux) or "0x3000002" (macOS)
    Device(String),
}

/// True if `s` is a BSD/macOS hex device number, e.g. "0x3000002".
fn is_hex_device_number(s: &str) -> bool {
    s.strip_prefix("0x")
        .is_some_and(|hex| !hex.is_empty() && hex.bytes().all(|b| b.is_ascii_hexdigit()))
}

struct LsEntry {
    entry_type: EntryType,
    size: Size,
    name: String,
}

/// Parse one line of `ls -la` output into an `LsEntry`.
/// Assumes the standard 9-field layout: perms links owner group size month day time name.
/// This matches both BSD (macOS) and GNU (Linux) `ls -la` output.
fn parse_ls_line(line: &str) -> Option<LsEntry> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() < 9 {
        return None;
    }

    let perms = parts[0];
    let type_char = perms.as_bytes().first()?;

    // device files show a device number instead of a byte size: GNU/Linux uses a
    // "major, minor" pair (e.g. "8, 0") that spans two fields and shifts the name
    // one field right; BSD/macOS uses a single hex token (e.g. "0x3000002").
    let (size, name_field) = if parts[4].ends_with(',') {
        let minor = parts.get(5)?;
        (Size::Device(format!("{} {}", parts[4], minor)), 9)
    } else if let Ok(bytes) = parts[4].parse::<u64>() {
        (Size::Bytes(bytes), 8)
    } else if is_hex_device_number(parts[4]) {
        (Size::Device(parts[4].to_string()), 8)
    } else {
        return None;
    };

    // Everything from the name field onward is the name (may contain consecutive
    // spaces or ` -> target`). Slice from the byte offset of that field in the
    // original line so internal spacing survives, rather than collapsing via join.
    let name = line[nth_field_offset(line, name_field)?..].to_string();

    // Skip . and ..
    if name == "." || name == ".." {
        return None;
    }

    let entry_type = match type_char {
        b'd' => EntryType::Directory,
        b'l' => EntryType::Symlink,
        _ => {
            // Check user execute bit (position 3 in permissions string)
            if perms.len() >= 4 && (perms.as_bytes()[3] == b'x' || perms.as_bytes()[3] == b's') {
                EntryType::Executable
            } else {
                EntryType::Regular
            }
        }
    };

    Some(LsEntry {
        entry_type,
        size,
        name,
    })
}

/// Byte offset where the `n`th whitespace-delimited field (0-indexed) begins.
/// Used to recover the filename field verbatim, preserving internal spaces.
fn nth_field_offset(line: &str, n: usize) -> Option<usize> {
    let mut field_index = 0;
    let mut in_field = false;
    for (i, c) in line.char_indices() {
        if c.is_whitespace() {
            in_field = false;
        } else if !in_field {
            if field_index == n {
                return Some(i);
            }
            field_index += 1;
            in_field = true;
        }
    }
    None
}

fn format_entry(entry: &LsEntry) -> String {
    match entry.entry_type {
        EntryType::Directory => format!("{}/", entry.name),
        EntryType::Symlink => entry.name.clone(), // already contains ` -> target`
        EntryType::Executable => format!("{}* ({})", entry.name, format_size_field(&entry.size)),
        EntryType::Regular => format!("{} ({})", entry.name, format_size_field(&entry.size)),
    }
}

/// Render a size field: byte sizes get human-readable units; device files keep
/// their verbatim "major, minor" text.
fn format_size_field(size: &Size) -> String {
    match size {
        Size::Bytes(bytes) => format_size(*bytes),
        Size::Device(text) => text.clone(),
    }
}

fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * 1024;
    const GB: u64 = 1024 * 1024 * 1024;

    if bytes >= GB {
        let value = bytes as f64 / GB as f64;
        if value.fract() < 0.05 {
            format!("{}G", value as u64)
        } else {
            format!("{:.1}G", value)
        }
    } else if bytes >= MB {
        let value = bytes as f64 / MB as f64;
        if value.fract() < 0.05 {
            format!("{}M", value as u64)
        } else {
            format!("{:.1}M", value)
        }
    } else if bytes >= KB {
        let value = bytes as f64 / KB as f64;
        if value.fract() < 0.05 {
            format!("{}K", value as u64)
        } else {
            format!("{:.1}K", value)
        }
    } else {
        format!("{}B", bytes)
    }
}

/// Find a compressor for the given ls args.
/// Returns None if args don't contain `-l` or contain skip flags.
pub fn find_compressor(args: &[String]) -> Option<Box<dyn Compressor>> {
    let compressor = LsCompressor;
    if compressor.can_compress(args) {
        Some(Box::new(compressor))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn compress(input: &str) -> Option<String> {
        LsCompressor.compress(input, "", 0)
    }

    #[test]
    fn compress_regular_files() {
        let input = "\
total 16
drwxr-xr-x  4 noah  staff   128 Mar 30 10:00 .
drwxr-xr-x 10 noah  staff   320 Mar 30 09:00 ..
-rw-r--r--  1 noah  staff  1234 Mar 30 09:50 Cargo.toml
-rw-r--r--  1 noah  staff    52 Mar 30 09:50 README.md\n";
        let result = compress(input);
        assert_eq!(
            result,
            Some("Cargo.toml (1.2K)\nREADME.md (52B)".to_string())
        );
    }

    #[test]
    fn compress_directory() {
        let input = "\
total 0
drwxr-xr-x  4 noah  staff  128 Mar 30 10:00 .
drwxr-xr-x 10 noah  staff  320 Mar 30 09:00 ..
drwxr-xr-x  3 noah  staff   96 Mar 30 09:50 src\n";
        let result = compress(input);
        assert_eq!(result, Some("src/".to_string()));
    }

    #[test]
    fn compress_executable() {
        let input = "\
total 8
drwxr-xr-x  3 noah  staff   96 Mar 30 10:00 .
drwxr-xr-x 10 noah  staff  320 Mar 30 09:00 ..
-rwxr-xr-x  1 noah  staff  8192 Mar 30 10:00 run.sh\n";
        let result = compress(input);
        assert_eq!(result, Some("run.sh* (8K)".to_string()));
    }

    #[test]
    fn compress_symlink() {
        let input = "\
total 0
drwxr-xr-x  3 noah  staff  96 Mar 30 10:00 .
drwxr-xr-x 10 noah  staff 320 Mar 30 09:00 ..
lrwxr-xr-x  1 noah  staff  11 Mar 28 09:00 link -> target\n";
        let result = compress(input);
        assert_eq!(result, Some("link -> target".to_string()));
    }

    #[test]
    fn compress_hidden_files() {
        let input = "\
total 8
drwxr-xr-x  4 noah  staff  128 Mar 30 10:00 .
drwxr-xr-x 10 noah  staff  320 Mar 30 09:00 ..
-rw-r--r--  1 noah  staff   52 Mar 28 09:00 .env
drwxr-xr-x  8 noah  staff  256 Mar 30 10:00 .git\n";
        let result = compress(input);
        assert_eq!(result, Some(".env (52B)\n.git/".to_string()));
    }

    #[test]
    fn compress_empty_dir() {
        let input = "\
total 0
drwxr-xr-x  2 noah  staff  64 Mar 30 10:00 .
drwxr-xr-x 10 noah  staff 320 Mar 30 09:00 ..\n";
        let result = compress(input);
        assert_eq!(result, Some(String::new()));
    }

    #[test]
    fn compress_nonzero_exit_returns_none() {
        let result = LsCompressor.compress("anything", "ls: error", 2);
        assert_eq!(result, None);
    }

    #[test]
    fn compress_size_bytes() {
        assert_eq!(format_size(0), "0B");
        assert_eq!(format_size(52), "52B");
        assert_eq!(format_size(1023), "1023B");
    }

    #[test]
    fn compress_size_kilobytes() {
        assert_eq!(format_size(1024), "1K");
        assert_eq!(format_size(1234), "1.2K");
        assert_eq!(format_size(15360), "15K");
    }

    #[test]
    fn compress_size_megabytes() {
        assert_eq!(format_size(1_048_576), "1M");
        assert_eq!(format_size(1_572_864), "1.5M");
    }

    #[test]
    fn compress_size_gigabytes() {
        assert_eq!(format_size(1_073_741_824), "1G");
        assert_eq!(format_size(2_254_857_830), "2.1G");
    }

    #[test]
    fn compress_mixed_entry_types() {
        let input = "\
total 24
drwxr-xr-x  6 noah  staff   192 Mar 30 10:00 .
drwxr-xr-x 10 noah  staff   320 Mar 30 09:00 ..
-rw-r--r--  1 noah  staff  1234 Mar 30 09:50 Cargo.toml
drwxr-xr-x  3 noah  staff    96 Mar 29 14:00 src
-rwxr-xr-x  1 noah  staff  8192 Mar 30 10:00 run.sh
lrwxr-xr-x  1 noah  staff    11 Mar 28 09:00 link -> target
-rw-r--r--  1 noah  staff    52 Mar 28 09:00 .env\n";
        let result = compress(input);
        assert_eq!(
            result,
            Some("Cargo.toml (1.2K)\nsrc/\nrun.sh* (8K)\nlink -> target\n.env (52B)".to_string())
        );
    }

    #[test]
    fn compress_unparseable_line_is_skipped() {
        let input = "\
total 8
drwxr-xr-x  4 noah  staff  128 Mar 30 10:00 .
drwxr-xr-x 10 noah  staff  320 Mar 30 09:00 ..
this is not a valid ls line
-rw-r--r--  1 noah  staff  52 Mar 30 09:50 file.txt\n";
        let result = compress(input);
        assert_eq!(result, Some("file.txt (52B)".to_string()));
    }

    #[test]
    fn can_compress_l_flag() {
        let c = LsCompressor;
        assert!(c.can_compress(&["-l".into()]));
    }

    #[test]
    fn can_compress_la_flag() {
        let c = LsCompressor;
        assert!(c.can_compress(&["-la".into()]));
    }

    #[test]
    fn can_compress_al_flag() {
        let c = LsCompressor;
        assert!(c.can_compress(&["-al".into()]));
    }

    #[test]
    fn can_compress_lah_flag() {
        let c = LsCompressor;
        assert!(c.can_compress(&["-lah".into()]));
    }

    #[test]
    fn can_compress_l_with_path() {
        let c = LsCompressor;
        assert!(c.can_compress(&["-l".into(), "/tmp".into()]));
    }

    #[test]
    fn skip_bare_ls() {
        let c = LsCompressor;
        assert!(!c.can_compress(&[]));
    }

    #[test]
    fn skip_no_l_flag() {
        let c = LsCompressor;
        assert!(!c.can_compress(&["-a".into()]));
        assert!(!c.can_compress(&["src".into()]));
    }

    #[test]
    fn skip_recursive() {
        let c = LsCompressor;
        assert!(!c.can_compress(&["-lR".into()]));
        assert!(!c.can_compress(&["-l".into(), "-R".into()]));
    }

    #[test]
    fn normalized_args_bare_l() {
        let c = LsCompressor;
        assert_eq!(c.normalized_args(&["-l".into()]), vec!["-la", "--"]);
    }

    #[test]
    fn normalized_args_strips_extra_flags() {
        let c = LsCompressor;
        assert_eq!(c.normalized_args(&["-lh".into()]), vec!["-la", "--"]);
    }

    #[test]
    fn normalized_args_preserves_paths() {
        let c = LsCompressor;
        assert_eq!(
            c.normalized_args(&["-la".into(), "src".into(), "tests".into()]),
            vec!["-la", "--", "src", "tests"]
        );
    }

    #[test]
    fn normalized_args_path_after_double_dash() {
        let c = LsCompressor;
        assert_eq!(
            c.normalized_args(&["-l".into(), "--".into(), "-weird-name".into()]),
            vec!["-la", "--", "-weird-name"]
        );
    }

    // U15-1: long options that merely contain 'l'/'R' must not be read as short flags.
    #[test]
    fn ls_color_long_option_does_not_trigger() {
        let c = LsCompressor;
        assert!(!c.can_compress(&["--color".into()]));
        assert!(!c.can_compress(&["--classify".into()]));
        assert!(!c.can_compress(&["--all".into()]));
        assert!(!c.can_compress(&["--full-time".into()]));
        // long opt alongside a real -l still compresses
        assert!(c.can_compress(&["--color".into(), "-l".into()]));
    }

    // U15-2 / U15-3: sort flags and -d are lossy -> decline (passthrough).
    #[test]
    fn sort_and_d_flags_decline() {
        let c = LsCompressor;
        assert!(!c.can_compress(&["-lt".into()]));
        assert!(!c.can_compress(&["-lS".into()]));
        assert!(!c.can_compress(&["-lr".into()]));
        assert!(!c.can_compress(&["-ld".into()]));
        assert!(!c.can_compress(&["-l".into(), "-t".into()]));
        // plain -l / -la must still compress
        assert!(c.can_compress(&["-l".into()]));
        assert!(c.can_compress(&["-la".into()]));
    }

    // U15-4: filenames with consecutive spaces must survive verbatim.
    #[test]
    fn multi_space_filename_preserved() {
        let input = "\
total 8
drwxr-xr-x  4 noah  staff  128 Mar 30 10:00 .
-rw-r--r--  1 noah  staff   52 Mar 28 09:00 a  b.txt\n";
        let result = compress(input).unwrap();
        assert!(
            result.contains("a  b.txt ("),
            "two-space filename must be preserved; got: {}",
            result
        );
    }

    // ls:14 — GNU long forms of lossy short flags must decline (their behavior is dropped otherwise).
    #[test]
    fn long_form_lossy_flags_decline() {
        let c = LsCompressor;
        assert!(!c.can_compress(&["-l".into(), "--recursive".into()]));
        assert!(!c.can_compress(&["-l".into(), "--reverse".into()]));
        assert!(!c.can_compress(&["-l".into(), "--sort=time".into()]));
        assert!(!c.can_compress(&["-l".into(), "--sort=size".into()]));
        assert!(!c.can_compress(&["-l".into(), "--sort".into()]));
        assert!(!c.can_compress(&["-l".into(), "--directory".into()]));
        // benign long options still compress
        assert!(c.can_compress(&["-l".into(), "--color".into()]));
    }

    // ls:69 — multiple directory operands produce "dir:" section headers we can't group -> decline.
    #[test]
    fn multiple_path_operands_decline() {
        let c = LsCompressor;
        assert!(!c.can_compress(&["-la".into(), "src".into(), "tests".into()]));
        assert!(!c.can_compress(&["-l".into(), "--".into(), "src".into(), "tests".into()]));
        // single operand still compresses
        assert!(c.can_compress(&["-la".into(), "src".into()]));
    }

    // ls:102 — device files show "major, minor" in the size column and must not be dropped.
    #[test]
    fn device_file_size_preserved() {
        let input = "\
total 0
drwxr-xr-x  4 noah  staff  128 Mar 30 10:00 .
crw-rw-rw-  1 root  wheel  3,   2 Jun 12 10:00 null\n";
        let result = compress(input).unwrap();
        assert_eq!(result, "null (3, 2)".to_string());
    }

    // ls:102 (macOS) — BSD device files show a single hex device number, not "major, minor".
    #[test]
    fn device_file_hex_size_preserved() {
        let input = "\
total 0
crw-rw-rw-  1 root  wheel  0x3000002 Jun 14 18:32 null
brw-r-----  1 root  operator  0x1000000 Jun  7 12:12 disk0\n";
        let result = compress(input).unwrap();
        assert_eq!(result, "null (0x3000002)\ndisk0 (0x1000000)".to_string());
    }

    #[test]
    fn is_hex_device_number_detects_form() {
        assert!(is_hex_device_number("0x3000002"));
        assert!(!is_hex_device_number("0x")); // empty body
        assert!(!is_hex_device_number("128")); // decimal size
        assert!(!is_hex_device_number("0xZZ")); // non-hex
    }
}
