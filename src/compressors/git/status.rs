use crate::compressors::Compressor;

pub struct GitStatusCompressor;

impl Compressor for GitStatusCompressor {
    fn can_compress(&self, args: &[String]) -> bool {
        // Match any `git status` invocation. Safe because only agent/user calls
        // reach token-saver (via shell function). Tools like Oh My Zsh use
        // `command git` which bypasses the function entirely.
        args.first().map(|s| s.as_str()) == Some("status")
    }

    fn normalized_args(&self, _original_args: &[String]) -> Vec<String> {
        vec![
            "status".to_string(),
            "--porcelain=v2".to_string(),
            "--branch".to_string(),
            "-z".to_string(),
        ]
    }

    fn compress(&self, stdout: &str, _stderr: &str, exit_code: i32) -> Option<String> {
        if exit_code != 0 {
            return None;
        }
        parse_porcelain_v2(stdout)
    }
}

struct BranchInfo {
    head: String,
    oid: String,
    upstream: Option<String>,
    ahead: i32,
    behind: i32,
}

struct FileChanges {
    staged: Vec<String>,
    modified: Vec<String>,
    deleted: Vec<String>,
    renamed: Vec<String>,
    conflict: Vec<String>,
    untracked: Vec<String>,
}

fn parse_porcelain_v2(output: &str) -> Option<String> {
    let mut branch = BranchInfo {
        head: String::new(),
        oid: String::new(),
        upstream: None,
        ahead: 0,
        behind: 0,
    };
    let mut files = FileChanges {
        staged: Vec::new(),
        modified: Vec::new(),
        deleted: Vec::new(),
        renamed: Vec::new(),
        conflict: Vec::new(),
        untracked: Vec::new(),
    };

    // Split on NUL bytes (from -z flag). Filter empty entries.
    let entries: Vec<&str> = output.split('\0').filter(|s| !s.is_empty()).collect();

    let mut i = 0;
    while i < entries.len() {
        let entry = entries[i];

        if let Some(oid) = entry.strip_prefix("# branch.oid ") {
            branch.oid = oid.to_string();
        } else if let Some(head) = entry.strip_prefix("# branch.head ") {
            branch.head = head.to_string();
        } else if let Some(upstream) = entry.strip_prefix("# branch.upstream ") {
            branch.upstream = Some(upstream.to_string());
        } else if let Some(ab) = entry.strip_prefix("# branch.ab ") {
            let parts: Vec<&str> = ab.split_whitespace().collect();
            if parts.len() == 2 {
                branch.ahead = parts[0].parse().ok()?;
                branch.behind = parts[1].parse().ok()?;
            }
        } else if entry.starts_with("1 ") {
            parse_ordinary_entry(entry, &mut files);
        } else if entry.starts_with("2 ") {
            // Renamed/copied entry: next NUL-delimited field is the original path
            let orig_path = if i + 1 < entries.len() {
                i += 1;
                entries[i]
            } else {
                ""
            };
            parse_rename_entry(entry, orig_path, &mut files);
        } else if entry.starts_with("u ") {
            parse_unmerged_entry(entry, &mut files);
        } else if let Some(path) = entry.strip_prefix("? ") {
            files.untracked.push(path.to_string());
        }

        i += 1;
    }

    Some(format_output(&branch, &files))
}

fn parse_ordinary_entry(entry: &str, files: &mut FileChanges) {
    // Format: 1 XY sub mH mI mW hH hI path
    let parts: Vec<&str> = entry.splitn(9, ' ').collect();
    if parts.len() < 9 {
        return;
    }

    let xy = parts[1].as_bytes();
    if xy.len() < 2 {
        return;
    }
    let x = xy[0];
    let y = xy[1];
    let path = parts[8].to_string();

    // Staged changes (X position)
    if x != b'.' {
        files.staged.push(path.clone());
    }

    // Unstaged changes (Y position)
    match y {
        b'M' | b'T' => files.modified.push(path),
        b'D' => files.deleted.push(path),
        _ => {}
    }
}

fn parse_rename_entry(entry: &str, orig_path: &str, files: &mut FileChanges) {
    // Format: 2 XY sub mH mI mW hH hI Xscore path
    let parts: Vec<&str> = entry.splitn(10, ' ').collect();
    if parts.len() < 10 {
        return;
    }

    let xy = parts[1].as_bytes();
    if xy.len() < 2 {
        return;
    }
    let y = xy[1];
    let score_field = parts[8]; // e.g., "R100" or "C075"
    let new_path = parts[9];

    let is_copy = score_field.starts_with('C');

    if is_copy {
        files
            .renamed
            .push(format!("{} -> {} (copy)", orig_path, new_path));
    } else {
        files.renamed.push(format!("{} -> {}", orig_path, new_path));
    }

    // Unstaged changes on the renamed/copied file
    match y {
        b'M' | b'T' => files.modified.push(new_path.to_string()),
        b'D' => files.deleted.push(new_path.to_string()),
        _ => {}
    }
}

fn parse_unmerged_entry(entry: &str, files: &mut FileChanges) {
    // Format: u XY sub m1 m2 m3 mW h1 h2 h3 path
    let parts: Vec<&str> = entry.splitn(11, ' ').collect();
    if parts.len() >= 11 {
        files.conflict.push(parts[10].to_string());
    }
}

fn format_branch_line(branch: &BranchInfo) -> String {
    if branch.head == "(detached)" {
        let short_oid = if branch.oid.len() >= 7 {
            &branch.oid[..7]
        } else {
            &branch.oid
        };
        return format!("branch: HEAD (detached at {})", short_oid);
    }

    let tracking = match &branch.upstream {
        None => "(no upstream)".to_string(),
        Some(upstream) => {
            if branch.ahead == 0 && branch.behind == 0 {
                format!("(up to date with {})", upstream)
            } else if branch.behind == 0 {
                format!("(+{} ahead of {})", branch.ahead, upstream)
            } else if branch.ahead == 0 {
                format!("(-{} behind {})", branch.behind.abs(), upstream)
            } else {
                format!("(+{} {} vs {})", branch.ahead, branch.behind, upstream)
            }
        }
    };

    format!("branch: {} {}", branch.head, tracking)
}

fn format_output(branch: &BranchInfo, files: &FileChanges) -> String {
    let mut lines = vec![format_branch_line(branch)];

    let categories: &[(&str, &Vec<String>)] = &[
        ("staged", &files.staged),
        ("modified", &files.modified),
        ("deleted", &files.deleted),
        ("renamed", &files.renamed),
        ("conflict", &files.conflict),
        ("untracked", &files.untracked),
    ];

    let has_any_files = categories.iter().any(|(_, v)| !v.is_empty());

    if !has_any_files {
        lines.push("clean".to_string());
    } else {
        for (label, paths) in categories {
            if !paths.is_empty() {
                lines.push(format!("{}: {}", label, paths.join(", ")));
            }
        }
    }

    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn compress(input: &str) -> Option<String> {
        GitStatusCompressor.compress(input, "", 0)
    }

    #[test]
    fn test_clean_repo() {
        let input = "# branch.oid abc123def456\0# branch.head main\0# branch.upstream origin/main\0# branch.ab +0 -0\0";
        let result = compress(input);
        assert_eq!(
            result,
            Some("branch: main (up to date with origin/main)\nclean".to_string())
        );
    }

    #[test]
    fn test_modified_and_untracked() {
        let input = "\
# branch.oid abc123\0\
# branch.head main\0\
# branch.upstream origin/main\0\
# branch.ab +0 -0\0\
1 .M N... 100644 100644 100644 abc123 def456 src/main.rs\0\
? .claude/\0";
        let result = compress(input);
        assert_eq!(
            result,
            Some("branch: main (up to date with origin/main)\nmodified: src/main.rs\nuntracked: .claude/".to_string())
        );
    }

    #[test]
    fn test_staged_files() {
        let input = "\
# branch.oid abc123\0\
# branch.head feature-x\0\
# branch.ab +0 -0\0\
1 A. N... 000000 100644 100644 000000 abc123 src/new.rs\0\
1 M. N... 100644 100644 100644 abc123 def456 src/lib.rs\0";
        let result = compress(input);
        assert_eq!(
            result,
            Some("branch: feature-x (no upstream)\nstaged: src/new.rs, src/lib.rs".to_string())
        );
    }

    #[test]
    fn test_ahead_behind() {
        let input = "\
# branch.oid abc123\0\
# branch.head main\0\
# branch.upstream origin/main\0\
# branch.ab +3 -1\0";
        let result = compress(input);
        assert_eq!(
            result,
            Some("branch: main (+3 -1 vs origin/main)\nclean".to_string())
        );
    }

    #[test]
    fn test_ahead_only() {
        let input = "\
# branch.oid abc123\0\
# branch.head main\0\
# branch.upstream origin/main\0\
# branch.ab +3 -0\0";
        let result = compress(input);
        assert_eq!(
            result,
            Some("branch: main (+3 ahead of origin/main)\nclean".to_string())
        );
    }

    #[test]
    fn test_behind_only() {
        let input = "\
# branch.oid abc123\0\
# branch.head main\0\
# branch.upstream origin/main\0\
# branch.ab +0 -2\0";
        let result = compress(input);
        assert_eq!(
            result,
            Some("branch: main (-2 behind origin/main)\nclean".to_string())
        );
    }

    #[test]
    fn test_detached_head() {
        let input = "\
# branch.oid abc123def456789\0\
# branch.head (detached)\0\
1 .M N... 100644 100644 100644 abc123 def456 src/main.rs\0";
        let result = compress(input);
        assert_eq!(
            result,
            Some("branch: HEAD (detached at abc123d)\nmodified: src/main.rs".to_string())
        );
    }

    #[test]
    fn test_deleted_file() {
        let input = "\
# branch.oid abc123\0\
# branch.head main\0\
# branch.upstream origin/main\0\
# branch.ab +0 -0\0\
1 .D N... 100644 100644 000000 abc123 000000 old_file.rs\0";
        let result = compress(input);
        assert_eq!(
            result,
            Some("branch: main (up to date with origin/main)\ndeleted: old_file.rs".to_string())
        );
    }

    #[test]
    fn test_renamed_file() {
        let input = "\
# branch.oid abc123\0\
# branch.head main\0\
# branch.upstream origin/main\0\
# branch.ab +0 -0\0\
2 R. N... 100644 100644 100644 abc123 def456 R100 new_name.rs\0old_name.rs\0";
        let result = compress(input);
        assert_eq!(
            result,
            Some(
                "branch: main (up to date with origin/main)\nrenamed: old_name.rs -> new_name.rs"
                    .to_string()
            )
        );
    }

    #[test]
    fn test_conflict_files() {
        let input = "\
# branch.oid abc123\0\
# branch.head main\0\
# branch.upstream origin/main\0\
# branch.ab +0 -0\0\
u UU N... 100644 100644 100644 100644 abc123 def456 789abc src/conflict.rs\0";
        let result = compress(input);
        assert_eq!(
            result,
            Some(
                "branch: main (up to date with origin/main)\nconflict: src/conflict.rs".to_string()
            )
        );
    }

    #[test]
    fn test_staged_and_modified_same_file() {
        let input = "\
# branch.oid abc123\0\
# branch.head main\0\
# branch.upstream origin/main\0\
# branch.ab +0 -0\0\
1 MM N... 100644 100644 100644 abc123 def456 src/main.rs\0";
        let result = compress(input);
        assert_eq!(
            result,
            Some("branch: main (up to date with origin/main)\nstaged: src/main.rs\nmodified: src/main.rs".to_string())
        );
    }

    #[test]
    fn test_nonzero_exit_returns_none() {
        let result = GitStatusCompressor.compress("anything", "fatal: error", 128);
        assert_eq!(result, None);
    }

    #[test]
    fn test_compress_bare_status() {
        assert!(GitStatusCompressor.can_compress(&["status".into()]));
    }

    #[test]
    fn test_compress_status_with_flags() {
        let c = GitStatusCompressor;
        // All git status variants are safe to compress because only agent/user
        // calls reach token-saver (shell function). Tools use `command git`.
        assert!(c.can_compress(&["status".into(), "--porcelain".into()]));
        assert!(c.can_compress(&["status".into(), "-s".into()]));
        assert!(c.can_compress(&["status".into(), "-u".into()]));
        assert!(c.can_compress(&["status".into(), ".".into()]));
        assert!(c.can_compress(&["status".into(), "--porcelain".into(), "-b".into()]));
    }

    #[test]
    fn test_skip_non_status_commands() {
        let c = GitStatusCompressor;
        assert!(!c.can_compress(&["diff".into()]));
        assert!(!c.can_compress(&["log".into()]));
        assert!(!c.can_compress(&[]));
    }
}
