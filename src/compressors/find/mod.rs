use std::collections::BTreeMap;

use crate::compressors::Compressor;
use crate::compressors::filters::should_filter;

/// Skip flags: if any of these appear in args, we don't compress.
const SKIP_FLAGS: &[&str] = &[
    "-exec", "-execdir", "-delete", "-ok", "-okdir", "-print0", "-printf", "-fprintf", "-ls",
];

/// Compressor for `find` output. `dirs_only` is set when the caller used `-type d`, so every
/// entry in stdout is known to be a directory even without a trailing slash.
pub struct FindCompressor {
    dirs_only: bool,
}

impl Compressor for FindCompressor {
    fn can_compress(&self, args: &[String]) -> bool {
        for arg in args {
            if SKIP_FLAGS.contains(&arg.as_str()) {
                return false;
            }
        }
        true
    }

    fn normalized_args(&self, original_args: &[String]) -> Vec<String> {
        original_args.to_vec()
    }

    fn compress(&self, stdout: &str, stderr: &str, exit_code: i32) -> Option<String> {
        // Step 1: early exit -- non-zero exit with no output means real failure
        if exit_code != 0 && stdout.trim().is_empty() {
            return None;
        }

        // Step 2: parse paths, strip leading "./" and bare "." entries
        let raw_paths: Vec<String> = stdout
            .lines()
            .filter(|l| !l.is_empty())
            .map(|l| l.strip_prefix("./").unwrap_or(l).to_string())
            .filter(|p| p != ".")
            .collect();

        // Step 3: filter noise
        let mut filtered_count: usize = 0;
        let mut paths: Vec<String> = Vec::with_capacity(raw_paths.len());

        for path in raw_paths {
            if should_filter(&path) {
                filtered_count += 1;
            } else {
                paths.push(path);
            }
        }

        // Step 4: cap at 500
        let total = paths.len();
        let remainder_count = total.saturating_sub(500);
        paths.truncate(500);

        // Step 5: pre-pass to tag implicit directories.
        // Two sources of directory knowledge:
        // (a) A path that is a strict prefix of another listed path is implicitly a directory even
        //     without a trailing slash. Use a sorted binary-search probe for the "prefix/" pattern.
        // (b) When dirs_only is set (caller used `-type d`), every listed path is a directory.
        let mut sorted = paths.clone();
        sorted.sort_unstable();

        let paths: Vec<String> = paths
            .into_iter()
            .map(|p| {
                if !p.ends_with('/') && (self.dirs_only || has_child_in_sorted(&p, &sorted)) {
                    format!("{}/", p)
                } else {
                    p
                }
            })
            .collect();

        // Step 6: build tree and render
        let mut root = TreeNode::default();
        for path in &paths {
            insert_path(&mut root, path);
        }

        let rendered = render_tree(&root, 0);

        // Step 7: footer
        let mut output = rendered;

        if remainder_count > 0 {
            output.push_str(&format!("\n... and {} more entries", remainder_count));
        }
        if filtered_count > 0 {
            output.push_str(&format!("\n{} entries filtered", filtered_count));
        }
        if !stderr.is_empty() {
            output.push_str("\nerrors:");
            for line in stderr.lines() {
                output.push_str(&format!("\n  {}", line));
            }
        }

        Some(output)
    }
}

/// Returns true if `sorted` (a sorted slice of paths) contains any entry that starts with
/// `prefix + "/"`, meaning `prefix` is a directory ancestor of at least one other listed path.
fn has_child_in_sorted(prefix: &str, sorted: &[String]) -> bool {
    let probe = format!("{}/", prefix);
    let idx = sorted.partition_point(|p| p.as_str() < probe.as_str());
    sorted.get(idx).is_some_and(|p| p.starts_with(&probe))
}

/// A node in the path tree.
#[derive(Default)]
struct TreeNode {
    /// Child nodes, keyed by name component. BTreeMap gives sorted iteration for free.
    children: BTreeMap<String, TreeNode>,
    /// True if this node was explicitly listed by find (not just an intermediate component).
    is_leaf: bool,
    /// True if the original path had a trailing slash (explicit directory entry).
    is_dir_entry: bool,
}

/// Insert a path string into the tree rooted at `root`.
fn insert_path(root: &mut TreeNode, path: &str) {
    let (path_clean, trailing_slash) = if let Some(stripped) = path.strip_suffix('/') {
        (stripped, true)
    } else {
        (path, false)
    };

    let components: Vec<&str> = path_clean.split('/').filter(|s| !s.is_empty()).collect();

    if components.is_empty() {
        return;
    }

    let mut current = root;
    let last_idx = components.len() - 1;

    for (i, component) in components.iter().enumerate() {
        let node = current.children.entry(component.to_string()).or_default();
        if i == last_idx {
            node.is_leaf = true;
            node.is_dir_entry = trailing_slash;
        } else {
            // Intermediate segment: definitely a directory.
            node.is_dir_entry = true;
        }
        current = node;
    }
}

/// Render the tree at `node` with the given indent depth.
/// At each level: directories first (alphabetical), then files (alphabetical).
/// BTreeMap already provides alphabetical order.
fn render_tree(node: &TreeNode, depth: usize) -> String {
    let indent = "  ".repeat(depth);
    let mut dirs: Vec<(&String, &TreeNode)> = Vec::new();
    let mut files: Vec<(&String, &TreeNode)> = Vec::new();

    for (name, child) in &node.children {
        if !child.children.is_empty() || child.is_dir_entry {
            dirs.push((name, child));
        } else {
            files.push((name, child));
        }
    }

    let mut lines: Vec<String> = Vec::new();

    for (name, child) in &dirs {
        lines.push(format!("{}{}/", indent, name));
        let subtree = render_tree(child, depth + 1);
        if !subtree.is_empty() {
            lines.push(subtree);
        }
    }

    for (name, _child) in &files {
        lines.push(format!("{}{}", indent, name));
    }

    lines.join("\n")
}

/// Returns true if the args contain `-type d` (directories only).
fn is_dirs_only(args: &[String]) -> bool {
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if arg == "-type" && iter.next().is_some_and(|v| v == "d") {
            return true;
        }
    }
    false
}

/// Find a compressor for the given find args.
pub fn find_compressor(args: &[String]) -> Option<Box<dyn Compressor>> {
    let compressor = FindCompressor {
        dirs_only: is_dirs_only(args),
    };
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
        FindCompressor { dirs_only: false }.compress(input, "", 0)
    }

    fn compress_dirs_only(input: &str) -> Option<String> {
        FindCompressor { dirs_only: true }.compress(input, "", 0)
    }

    // --- can_compress ---

    #[test]
    fn test_can_compress_bare() {
        assert!(FindCompressor { dirs_only: false }.can_compress(&[]));
    }

    #[test]
    fn test_can_compress_dot() {
        assert!(FindCompressor { dirs_only: false }.can_compress(&[".".into()]));
    }

    #[test]
    fn test_can_compress_with_name() {
        assert!(FindCompressor { dirs_only: false }.can_compress(&[
            ".".into(),
            "-name".into(),
            "*.rs".into()
        ]));
    }

    #[test]
    fn test_can_compress_with_type() {
        assert!(FindCompressor { dirs_only: false }.can_compress(&[
            ".".into(),
            "-type".into(),
            "f".into()
        ]));
    }

    #[test]
    fn test_can_compress_skips_exec() {
        assert!(!FindCompressor { dirs_only: false }.can_compress(&[
            ".".into(),
            "-exec".into(),
            "echo".into(),
            "{}".into(),
            ";".into()
        ]));
    }

    #[test]
    fn test_can_compress_skips_execdir() {
        assert!(!FindCompressor { dirs_only: false }.can_compress(&[
            ".".into(),
            "-execdir".into(),
            "rm".into()
        ]));
    }

    #[test]
    fn test_can_compress_skips_delete() {
        assert!(!FindCompressor { dirs_only: false }.can_compress(&[".".into(), "-delete".into()]));
    }

    #[test]
    fn test_can_compress_skips_ok() {
        assert!(!FindCompressor { dirs_only: false }.can_compress(&[
            ".".into(),
            "-ok".into(),
            "rm".into()
        ]));
    }

    #[test]
    fn test_can_compress_skips_okdir() {
        assert!(!FindCompressor { dirs_only: false }.can_compress(&[
            ".".into(),
            "-okdir".into(),
            "rm".into()
        ]));
    }

    #[test]
    fn test_can_compress_skips_print0() {
        assert!(!FindCompressor { dirs_only: false }.can_compress(&[".".into(), "-print0".into()]));
    }

    #[test]
    fn test_can_compress_skips_printf() {
        assert!(!FindCompressor { dirs_only: false }.can_compress(&[
            ".".into(),
            "-printf".into(),
            "%p\n".into()
        ]));
    }

    #[test]
    fn test_can_compress_skips_fprintf() {
        assert!(!FindCompressor { dirs_only: false }.can_compress(&[
            ".".into(),
            "-fprintf".into(),
            "out.txt".into(),
            "%p\n".into()
        ]));
    }

    #[test]
    fn test_can_compress_skips_ls_flag() {
        assert!(!FindCompressor { dirs_only: false }.can_compress(&[".".into(), "-ls".into()]));
    }

    // --- normalized_args ---

    #[test]
    fn test_normalized_args_passthrough() {
        let args: Vec<String> = vec![".".into(), "-name".into(), "*.rs".into()];
        assert_eq!(
            FindCompressor { dirs_only: false }.normalized_args(&args),
            args
        );
    }

    // --- compress ---

    #[test]
    fn test_compress_basic_tree() {
        let input = "src/main.rs\nsrc/lib.rs\ntests/foo.rs\n";
        let result = compress(input);
        assert_eq!(
            result,
            Some("src/\n  lib.rs\n  main.rs\ntests/\n  foo.rs".to_string())
        );
    }

    #[test]
    fn test_compress_strips_dot_slash() {
        let input = "./src/main.rs\n./Cargo.toml\n";
        let result = compress(input);
        assert_eq!(result, Some("src/\n  main.rs\nCargo.toml".to_string()));
    }

    #[test]
    fn test_compress_strips_bare_dot() {
        let input = ".\n./src/main.rs\n";
        let result = compress(input);
        assert_eq!(result, Some("src/\n  main.rs".to_string()));
    }

    #[test]
    fn test_compress_filters_git() {
        let input = ".git\n.git/config\n.git/objects/abc123\nsrc/main.rs\n";
        let result = compress(input);
        assert_eq!(
            result,
            Some("src/\n  main.rs\n3 entries filtered".to_string())
        );
    }

    #[test]
    fn test_compress_filters_nested_git() {
        let input = "src/main.rs\nvendor/repo/.git\nvendor/repo/.git/config\n";
        let result = compress(input);
        assert_eq!(
            result,
            Some("src/\n  main.rs\n2 entries filtered".to_string())
        );
    }

    #[test]
    fn test_compress_filters_pycache() {
        let input = "__pycache__\nsrc/__pycache__/foo.pyc\napp/__pycache__\nsrc/main.py\n";
        let result = compress(input);
        let s = result.unwrap();
        assert!(s.contains("src/"));
        assert!(s.contains("entries filtered"));
    }

    #[test]
    fn test_compress_filters_ds_store() {
        let input = ".DS_Store\nsubdir/.DS_Store\nsrc/main.rs\n";
        let result = compress(input);
        assert_eq!(
            result,
            Some("src/\n  main.rs\n2 entries filtered".to_string())
        );
    }

    #[test]
    fn test_compress_filters_pyc() {
        let input = "src/foo.pyc\nsrc/bar.py\n";
        let result = compress(input);
        assert_eq!(
            result,
            Some("src/\n  bar.py\n1 entries filtered".to_string())
        );
    }

    #[test]
    fn test_compress_sorts_dirs_first() {
        let input = "z_file.txt\nalpha/readme.txt\n";
        let result = compress(input);
        let s = result.unwrap();
        let dir_pos = s.find("alpha/").unwrap();
        let file_pos = s.find("z_file.txt").unwrap();
        assert!(dir_pos < file_pos, "directory should come before file");
    }

    #[test]
    fn test_compress_alphabetical() {
        let input = "src/z.rs\nsrc/a.rs\nsrc/m.rs\n";
        let result = compress(input);
        assert_eq!(result, Some("src/\n  a.rs\n  m.rs\n  z.rs".to_string()));
    }

    #[test]
    fn test_compress_empty_dir_shown() {
        let input = "empty_dir/\nsrc/main.rs\n";
        let result = compress(input);
        let s = result.unwrap();
        assert!(
            s.contains("empty_dir/"),
            "empty dir should appear with trailing slash"
        );
        assert!(s.contains("src/"));
    }

    #[test]
    fn test_compress_cap_at_500() {
        let input: String = (0..600)
            .map(|i| format!("dir/file_{:03}.txt\n", i))
            .collect();
        let result = compress(&input);
        let s = result.unwrap();
        assert!(s.contains("... and 100 more entries"));
    }

    #[test]
    fn test_compress_summary_only_when_filtered() {
        let input = "src/main.rs\n";
        let result = compress(input);
        let s = result.unwrap();
        assert!(!s.contains("entries filtered"));
    }

    #[test]
    fn test_compress_stderr_appended() {
        let result = FindCompressor { dirs_only: false }.compress(
            "src/main.rs\n",
            "find: permission denied\n",
            0,
        );
        let s = result.unwrap();
        assert!(s.contains("errors:"));
        assert!(s.contains("  find: permission denied"));
    }

    #[test]
    fn test_compress_nonzero_exit_with_output() {
        let result = FindCompressor { dirs_only: false }.compress(
            "src/main.rs\n",
            "find: permission denied",
            1,
        );
        assert!(result.is_some());
        let s = result.unwrap();
        assert!(s.contains("src/"));
        assert!(s.contains("errors:"));
    }

    #[test]
    fn test_compress_nonzero_exit_no_output() {
        let result =
            FindCompressor { dirs_only: false }.compress("", "find: no such file or directory", 1);
        assert_eq!(result, None);
    }

    #[test]
    fn test_compress_deep_nesting() {
        let input = "a/b/c/d/e.txt\n";
        let result = compress(input);
        assert_eq!(
            result,
            Some("a/\n  b/\n    c/\n      d/\n        e.txt".to_string())
        );
    }

    #[test]
    fn test_compress_single_file() {
        let input = "foo.txt\n";
        let result = compress(input);
        assert_eq!(result, Some("foo.txt".to_string()));
    }

    // Verify that -type d output renders all entries as directories.
    #[test]
    fn test_compress_type_d_output() {
        let input = "src\nsrc/compressors\ntests\n";
        let result = compress_dirs_only(input);
        let s = result.unwrap();
        assert!(s.contains("src/"), "src should be a directory");
        assert!(
            s.contains("compressors/"),
            "compressors should be a directory"
        );
        assert!(s.contains("tests/"), "tests should be a directory");
    }
}
