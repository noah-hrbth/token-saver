use crate::compressors::Compressor;

pub struct GitBranchCompressor;

const MAX_BRANCHES: usize = 50;

/// Flags that indicate branch mutation (not listing).
const MUTATION_FLAGS: &[&str] = &[
    "-d",
    "-D",
    "-m",
    "-M",
    "-c",
    "-C",
    "--edit-description",
    "--set-upstream-to",
    "--unset-upstream",
];

/// Filter flags that may take a value argument.
const FILTER_FLAGS: &[&str] = &[
    "--merged",
    "--no-merged",
    "--contains",
    "--no-contains",
    "--points-at",
];

impl Compressor for GitBranchCompressor {
    fn can_compress(&self, args: &[String]) -> bool {
        if args.first().map(|s| s.as_str()) != Some("branch") {
            return false;
        }
        !args
            .iter()
            .skip(1)
            .any(|a| MUTATION_FLAGS.contains(&a.as_str()) || a.starts_with("--format"))
    }

    fn normalized_args(&self, original_args: &[String]) -> Vec<String> {
        let mut result = vec![
            "branch".to_string(),
            "--format=%(HEAD)\t%(refname:short)\t%(upstream:short)\t%(upstream:track)\t%(symref:short)\t%(refname)".to_string(),
        ];

        let args = &original_args[1..];
        let mut i = 0;
        while i < args.len() {
            let arg = args[i].as_str();
            match arg {
                "-r" | "--remotes" | "-a" | "--all" => {
                    result.push(args[i].clone());
                }
                _ if FILTER_FLAGS.contains(&arg) => {
                    result.push(args[i].clone());
                    // Next arg is the value if it doesn't start with -
                    if i + 1 < args.len() && !args[i + 1].starts_with('-') {
                        result.push(args[i + 1].clone());
                        i += 1;
                    }
                }
                _ if FILTER_FLAGS
                    .iter()
                    .any(|f| arg.starts_with(&format!("{}=", f))) =>
                {
                    result.push(args[i].clone());
                }
                _ => {} // drop -v, -vv, etc. (--format replaces them)
            }
            i += 1;
        }

        result
    }

    fn compress(&self, stdout: &str, _stderr: &str, exit_code: i32) -> Option<String> {
        if exit_code != 0 {
            return None;
        }
        if stdout.trim().is_empty() {
            return None;
        }
        parse_branches(stdout)
    }
}

struct BranchEntry {
    is_current: bool,
    name: String,
    upstream: String,
    track: String,
    symref: String,
    full_ref: String,
}

fn parse_line(line: &str) -> Option<BranchEntry> {
    let fields: Vec<&str> = line.split('\t').collect();
    if fields.len() < 2 {
        return None;
    }

    let head = fields[0];
    let name = fields[1];
    let upstream = if fields.len() > 2 { fields[2] } else { "" };
    let track = if fields.len() > 3 { fields[3] } else { "" };
    let symref = if fields.len() > 4 { fields[4] } else { "" };
    let full_ref = if fields.len() > 5 { fields[5] } else { "" };

    if name.is_empty() {
        return None;
    }

    Some(BranchEntry {
        is_current: head == "*",
        name: name.to_string(),
        upstream: upstream.to_string(),
        track: track.to_string(),
        symref: symref.to_string(),
        full_ref: full_ref.to_string(),
    })
}

fn is_remote(entry: &BranchEntry) -> bool {
    entry.full_ref.starts_with("refs/remotes/") || !entry.symref.is_empty()
}

fn parse_branches(output: &str) -> Option<String> {
    let mut local: Vec<BranchEntry> = Vec::new();
    let mut remote: Vec<BranchEntry> = Vec::new();

    for line in output.lines() {
        if line.is_empty() {
            continue;
        }
        if let Some(entry) = parse_line(line) {
            if is_remote(&entry) {
                remote.push(entry);
            } else {
                local.push(entry);
            }
        }
    }

    // Pin current branch to first position
    if let Some(pos) = local.iter().position(|b| b.is_current)
        && pos > 0
    {
        let current = local.remove(pos);
        local.insert(0, current);
    }

    let total = local.len() + remote.len();
    let truncated = total > MAX_BRANCHES;

    // Apply cap: fill local first, then remote
    let mut kept_local: Vec<&BranchEntry> = Vec::new();
    let mut kept_remote: Vec<&BranchEntry> = Vec::new();
    let mut count = 0;

    for entry in &local {
        if count >= MAX_BRANCHES {
            break;
        }
        kept_local.push(entry);
        count += 1;
    }

    for entry in &remote {
        if count >= MAX_BRANCHES {
            break;
        }
        kept_remote.push(entry);
        count += 1;
    }

    let mut lines = Vec::new();

    // Format local branches
    for entry in &kept_local {
        lines.push(format_local_branch(entry));
    }

    // Format remote branches grouped by remote name
    if !kept_remote.is_empty() {
        if !lines.is_empty() {
            lines.push(String::new());
        }

        let groups = group_by_remote(&kept_remote);
        for (remote_name, entries) in &groups {
            lines.push(format!("remotes/{}:", remote_name));
            for entry in entries {
                if !entry.symref.is_empty() {
                    let target = entry
                        .symref
                        .strip_prefix(&format!("{}/", remote_name))
                        .unwrap_or(&entry.symref);
                    lines.push(format!("  HEAD -> {}", target));
                } else {
                    let short = entry
                        .name
                        .strip_prefix(&format!("{}/", remote_name))
                        .unwrap_or(&entry.name);
                    lines.push(format!("  {}", short));
                }
            }
        }
    }

    if truncated {
        let shown = kept_local.len() + kept_remote.len();
        let remaining = total - shown;
        lines.push(format!(
            "... and {} more branches ({} total)",
            remaining, total
        ));
    }

    Some(lines.join("\n"))
}

fn format_local_branch(entry: &BranchEntry) -> String {
    let marker = if entry.is_current { "*" } else { " " };
    let mut line = format!("{} {}", marker, entry.name);

    if !entry.upstream.is_empty() {
        line.push_str(&format!("  {}", entry.upstream));
    }
    if !entry.track.is_empty() {
        line.push_str(&format!(" {}", entry.track));
    }

    line
}

fn group_by_remote<'a>(entries: &[&'a BranchEntry]) -> Vec<(String, Vec<&'a BranchEntry>)> {
    let mut groups: Vec<(String, Vec<&'a BranchEntry>)> = Vec::new();

    for entry in entries {
        let remote_name = entry
            .name
            .split('/')
            .next()
            .unwrap_or(&entry.name)
            .to_string();

        if let Some(group) = groups.iter_mut().find(|(name, _)| name == &remote_name) {
            group.1.push(entry);
        } else {
            groups.push((remote_name, vec![entry]));
        }
    }

    groups
}

#[cfg(test)]
mod tests {
    use super::*;

    fn compress(input: &str) -> Option<String> {
        GitBranchCompressor.compress(input, "", 0)
    }

    #[test]
    fn test_single_local_branch() {
        let input = "*\tmain\t\t\t\trefs/heads/main\n";
        assert_eq!(compress(input), Some("* main".to_string()));
    }

    #[test]
    fn test_multiple_local_branches() {
        let input = " \tfeature-x\t\t\t\trefs/heads/feature-x\n*\tmain\torigin/main\t\t\trefs/heads/main\n \thotfix\t\t\t\trefs/heads/hotfix\n";
        let result = compress(input).unwrap();
        let lines: Vec<&str> = result.lines().collect();
        assert!(lines[0].starts_with("* main"));
        assert!(result.contains("feature-x"));
        assert!(result.contains("hotfix"));
    }

    #[test]
    fn test_tracking_info() {
        let input = "*\tmain\torigin/main\t[ahead 3]\t\trefs/heads/main\n \tdev\torigin/dev\t[behind 2]\t\trefs/heads/dev\n";
        let result = compress(input).unwrap();
        assert_eq!(
            result,
            "* main  origin/main [ahead 3]\n  dev  origin/dev [behind 2]"
        );
    }

    #[test]
    fn test_ahead_behind() {
        let input = "*\tmain\torigin/main\t[ahead 1, behind 2]\t\trefs/heads/main\n";
        assert_eq!(
            compress(input),
            Some("* main  origin/main [ahead 1, behind 2]".to_string())
        );
    }

    #[test]
    fn test_all_branches() {
        let input = "*\tmain\torigin/main\t\t\trefs/heads/main\n \tfeature\t\t\t\trefs/heads/feature\n \torigin/HEAD\t\t\torigin/main\trefs/remotes/origin/HEAD\n \torigin/main\t\t\t\trefs/remotes/origin/main\n \torigin/feature\t\t\t\trefs/remotes/origin/feature\n";
        let result = compress(input).unwrap();
        assert!(result.starts_with("* main"));
        assert!(result.contains("feature"));
        assert!(result.contains("remotes/origin:"));
        assert!(result.contains("HEAD -> main"));
    }

    #[test]
    fn test_remote_only() {
        let input = " \torigin/HEAD\t\t\torigin/main\trefs/remotes/origin/HEAD\n \torigin/main\t\t\t\trefs/remotes/origin/main\n \torigin/dev\t\t\t\trefs/remotes/origin/dev\n";
        let result = compress(input).unwrap();
        assert_eq!(result, "remotes/origin:\n  HEAD -> main\n  main\n  dev");
    }

    #[test]
    fn test_head_symref_nonstandard() {
        let input = " \torigin/HEAD\t\t\torigin/develop\trefs/remotes/origin/HEAD\n \torigin/develop\t\t\t\trefs/remotes/origin/develop\n";
        let result = compress(input).unwrap();
        assert!(result.contains("HEAD -> develop"));
    }

    #[test]
    fn test_local_branch_with_slash_not_remote() {
        let input =
            " \tfeature/foo\t\t\t\trefs/heads/feature/foo\n*\tmain\t\t\t\trefs/heads/main\n";
        let result = compress(input).unwrap();
        assert!(result.contains("feature/foo"));
        assert!(!result.contains("remotes/"));
    }

    #[test]
    fn test_truncation_at_50() {
        let mut input = String::from("*\tmain\t\t\t\trefs/heads/main\n");
        for i in 1..60 {
            input.push_str(&format!(
                " \tbranch-{:03}\t\t\t\trefs/heads/branch-{:03}\n",
                i, i
            ));
        }
        let result = compress(&input).unwrap();
        let lines: Vec<&str> = result.lines().collect();
        assert_eq!(lines.len(), 51); // 50 branches + truncation footer
        assert!(
            lines
                .last()
                .unwrap()
                .contains("... and 10 more branches (60 total)")
        );
    }

    #[test]
    fn test_nonzero_exit_returns_none() {
        assert_eq!(GitBranchCompressor.compress("anything", "", 128), None);
    }

    #[test]
    fn test_empty_stdout() {
        assert_eq!(compress(""), None);
        assert_eq!(compress("  \n"), None);
    }

    #[test]
    fn test_can_compress_basic() {
        assert!(GitBranchCompressor.can_compress(&["branch".into()]));
    }

    #[test]
    fn test_can_compress_with_flags() {
        let c = GitBranchCompressor;
        assert!(c.can_compress(&["branch".into(), "-v".into()]));
        assert!(c.can_compress(&["branch".into(), "-a".into()]));
        assert!(c.can_compress(&["branch".into(), "-r".into()]));
        assert!(c.can_compress(&["branch".into(), "-vv".into()]));
    }

    #[test]
    fn test_skip_format_flag() {
        assert!(
            !GitBranchCompressor.can_compress(&["branch".into(), "--format=%(refname)".into()])
        );
    }

    #[test]
    fn test_skip_mutation_flags() {
        let c = GitBranchCompressor;
        assert!(!c.can_compress(&["branch".into(), "-d".into(), "feature".into()]));
        assert!(!c.can_compress(&["branch".into(), "-D".into(), "feature".into()]));
        assert!(!c.can_compress(&["branch".into(), "-m".into(), "old".into(), "new".into()]));
        assert!(!c.can_compress(&["branch".into(), "-M".into(), "old".into(), "new".into()]));
        assert!(!c.can_compress(&["branch".into(), "-c".into(), "src".into()]));
        assert!(!c.can_compress(&["branch".into(), "-C".into(), "src".into()]));
    }

    #[test]
    fn test_normalized_args_basic() {
        let result = GitBranchCompressor.normalized_args(&["branch".into()]);
        assert_eq!(result[0], "branch");
        assert!(result[1].starts_with("--format="));
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_normalized_args_preserves_remote() {
        let result = GitBranchCompressor.normalized_args(&["branch".into(), "-r".into()]);
        assert!(result.contains(&"-r".to_string()));
    }

    #[test]
    fn test_normalized_args_preserves_all() {
        let result = GitBranchCompressor.normalized_args(&["branch".into(), "-a".into()]);
        assert!(result.contains(&"-a".to_string()));
    }

    #[test]
    fn test_normalized_args_preserves_filters() {
        let result = GitBranchCompressor.normalized_args(&[
            "branch".into(),
            "--merged".into(),
            "main".into(),
        ]);
        assert!(result.contains(&"--merged".to_string()));
        assert!(result.contains(&"main".to_string()));
    }

    #[test]
    fn test_current_branch_pinned_first() {
        let input = " \taaa\t\t\t\trefs/heads/aaa\n \tbbb\t\t\t\trefs/heads/bbb\n*\tzzz\t\t\t\trefs/heads/zzz\n";
        let result = compress(input).unwrap();
        let first_line = result.lines().next().unwrap();
        assert!(first_line.starts_with("* zzz"));
    }
}
