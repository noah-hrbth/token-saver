use crate::compressors::Compressor;

pub struct LsCompressor;

impl Compressor for LsCompressor {
    fn can_compress(&self, args: &[String]) -> bool {
        let mut has_l = false;

        for arg in args {
            if !arg.starts_with('-') {
                continue; // path argument, skip
            }
            let flags = arg.trim_start_matches('-');
            if flags.contains('R') {
                return false; // recursive — skip
            }
            if flags.contains('l') {
                has_l = true;
            }
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

struct LsEntry {
    entry_type: EntryType,
    size: u64,
    name: String,
}

fn parse_ls_line(line: &str) -> Option<LsEntry> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() < 9 {
        return None;
    }

    let perms = parts[0];
    let type_char = perms.as_bytes().first()?;
    let size: u64 = parts[4].parse().ok()?;

    // Everything from field 8 onward is the name (may contain spaces or ` -> target`)
    let name = parts[8..].join(" ");

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

fn format_entry(entry: &LsEntry) -> String {
    match entry.entry_type {
        EntryType::Directory => format!("{}/", entry.name),
        EntryType::Symlink => entry.name.clone(), // already contains ` -> target`
        EntryType::Executable => format!("{}* ({})", entry.name, format_size(entry.size)),
        EntryType::Regular => format!("{} ({})", entry.name, format_size(entry.size)),
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
}
