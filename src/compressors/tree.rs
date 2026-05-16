//! Shared radix-tree directory grouping for file-headed compressor output.
//!
//! `tsc`, `eslint` (via `report.rs`) and `grep` all emit one block per file
//! whose first line begins with the file path. When many files share a
//! directory prefix, repeating that prefix on every block wastes tokens. This
//! module factors shared prefixes into nested `dir/` headers.
//!
//! It builds a path-component trie, compresses single-child chains (so a file
//! alone in a deep path keeps its full path inline and never gains a useless
//! header), and renders the result with two spaces of indentation per level.
//!
//! The split between path and the rest of a block's first line is *exact*:
//! callers pass `path` separately and every caller builds line 1 as `path`
//! followed by a constant suffix, so `first_line.strip_prefix(path)` is
//! lossless — never heuristic parsing.

use std::collections::BTreeMap;

/// One pre-rendered file block plus the path it belongs to.
pub struct PathBlock {
    /// `None` => not directory-groupable; the whole batch is emitted
    /// unchanged (used by jest's FAIL blocks as a safety net).
    pub path: Option<String>,
    /// Full pre-rendered block. When `path` is `Some`, line 1 starts with it.
    pub block: String,
}

#[derive(Default)]
struct Node {
    children: BTreeMap<String, Node>,
    /// Present when a file path ends exactly at this node.
    leaf: Option<PathBlock>,
}

/// Group `blocks` into a nested directory tree.
///
/// If every block has a `path`, returns the rendered tree as a list of lines
/// (callers `join("\n")`). If *any* `path` is `None`, returns each block
/// unchanged and in order (no grouping).
pub fn group_by_directory(blocks: Vec<PathBlock>) -> Vec<String> {
    if blocks.iter().any(|b| b.path.is_none()) {
        return blocks.into_iter().map(|b| b.block).collect();
    }

    let mut root = Node::default();
    for pb in blocks {
        let path = pb.path.clone().unwrap_or_default();
        let mut components: Vec<String> = path
            .split('/')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect();
        // Preserve absolute-path semantics: keep the leading `/` on the first
        // segment so an outside-cwd path is never rewritten to look
        // cwd-relative (e.g. `/other/src/a.ts` must not become `other/src/a.ts`).
        if path.starts_with('/')
            && let Some(first) = components.first_mut()
        {
            *first = format!("/{}", first);
        }
        if components.is_empty() {
            // defensive: no usable path — emit as a root-level leaf keyed by block
            let key = pb.block.clone();
            root.children.entry(key).or_default().leaf = Some(pb);
            continue;
        }
        let mut cur = &mut root;
        for comp in &components {
            cur = cur.children.entry(comp.clone()).or_default();
        }
        cur.leaf = Some(pb);
    }

    let mut out: Vec<String> = Vec::new();
    render_children(&root, 0, &mut out);
    out
}

/// True if `node`, after single-child compression, is a directory (has
/// children and no file ends at the compressed node).
fn resolves_to_dir(node: &Node) -> bool {
    let mut n = node;
    loop {
        if n.leaf.is_some() {
            return false;
        }
        if n.children.len() == 1 {
            n = n.children.values().next().unwrap();
            continue;
        }
        return !n.children.is_empty();
    }
}

/// Render `parent`'s children: directories first (alphabetical), then files
/// (alphabetical) — matches the `find` compressor's ordering.
fn render_children(parent: &Node, depth: usize, out: &mut Vec<String>) {
    let mut dirs: Vec<(&String, &Node)> = Vec::new();
    let mut files: Vec<(&String, &Node)> = Vec::new();
    for (name, child) in &parent.children {
        if resolves_to_dir(child) {
            dirs.push((name, child));
        } else {
            files.push((name, child));
        }
    }
    for (name, child) in dirs.into_iter().chain(files) {
        render_node(name.clone(), child, depth, out);
    }
}

/// Render one node, compressing single-child directory chains into `name`.
fn render_node(mut name: String, mut node: &Node, depth: usize, out: &mut Vec<String>) {
    // Radix compression: fold pass-through directories into the path segment.
    loop {
        if node.leaf.is_some() || node.children.len() != 1 {
            break;
        }
        let (cname, cnode) = node.children.iter().next().unwrap();
        name = format!("{}/{}", name, cname);
        node = cnode;
    }

    let indent = "  ".repeat(depth);

    if node.children.is_empty() {
        // File leaf.
        if let Some(pb) = &node.leaf {
            emit_file(&name, pb, &indent, out);
        }
        return;
    }

    // Directory header.
    out.push(format!("{}{}/", indent, name));
    render_children(node, depth + 1, out);
    // A path that is both a file and a directory prefix cannot occur for real
    // file lists; if it ever did, surface the file under the header.
    if let Some(pb) = &node.leaf {
        emit_file(&name, pb, &"  ".repeat(depth + 1), out);
    }
}

/// Emit a file block: swap the leading full path on line 1 for `display`,
/// then shift every line right by `indent`.
fn emit_file(display: &str, pb: &PathBlock, indent: &str, out: &mut Vec<String>) {
    let path = pb.path.as_deref().unwrap_or("");
    let mut lines = pb.block.lines();
    let first = lines.next().unwrap_or("");
    match first.strip_prefix(path) {
        Some(suffix) => out.push(format!("{}{}{}", indent, display, suffix)),
        // contract violation: keep the original line rather than corrupt it
        None => out.push(format!("{}{}", indent, first)),
    }
    for line in lines {
        if line.is_empty() {
            out.push(String::new());
        } else {
            out.push(format!("{}{}", indent, line));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pb(path: &str, block: &str) -> PathBlock {
        PathBlock {
            path: Some(path.to_string()),
            block: block.to_string(),
        }
    }

    fn run(blocks: Vec<PathBlock>) -> String {
        group_by_directory(blocks).join("\n")
    }

    #[test]
    fn nested_multi_dir_with_blocks() {
        // Arrange: 3 files in src/components/, 2 in src/utils/, tsc-style blocks.
        let blocks = vec![
            pb(
                "src/components/button.ts",
                "src/components/button.ts  TS2322\n  1:7  msg",
            ),
            pb(
                "src/components/input.ts",
                "src/components/input.ts  TS2322\n  2:3  msg",
            ),
            pb(
                "src/utils/format.ts",
                "src/utils/format.ts  TS2322\n  4:1  msg",
            ),
            pb(
                "src/utils/parse.ts",
                "src/utils/parse.ts  TS2322\n  5:9  msg",
            ),
        ];
        // Act
        let out = run(blocks);
        // Assert: single src/ header, nested component/utils dirs, indented blocks.
        assert_eq!(
            out,
            "src/\n  \
components/\n    button.ts  TS2322\n      1:7  msg\n    \
input.ts  TS2322\n      2:3  msg\n  \
utils/\n    format.ts  TS2322\n      4:1  msg\n    \
parse.ts  TS2322\n      5:9  msg"
        );
    }

    #[test]
    fn lone_deep_file_stays_inline() {
        // A single file in a deep path: chain fully compresses, no headers.
        let out = run(vec![pb(
            "a/b/c/d.ts",
            "a/b/c/d.ts:5:3  TS2304  Cannot find name 'x'",
        )]);
        assert_eq!(out, "a/b/c/d.ts:5:3  TS2304  Cannot find name 'x'");
    }

    #[test]
    fn root_level_files_no_header() {
        let out = run(vec![
            pb("b.ts", "b.ts  TS1\n  1:1  m"),
            pb("a.ts", "a.ts  TS2\n  2:2  m"),
        ]);
        // Alphabetical, no directory headers, no blank line between.
        assert_eq!(out, "a.ts  TS2\n  2:2  m\nb.ts  TS1\n  1:1  m");
    }

    #[test]
    fn mixed_root_and_dirs_dirs_first() {
        let out = run(vec![
            pb("root.ts", "root.ts  TSx\n  1:1  m"),
            pb("src/a.ts", "src/a.ts  TSy\n  2:2  m"),
            pb("src/b.ts", "src/b.ts  TSz\n  3:3  m"),
        ]);
        // src/ (a dir) is rendered before the bare root.ts file.
        assert_eq!(
            out,
            "src/\n  a.ts  TSy\n    2:2  m\n  b.ts  TSz\n    3:3  m\nroot.ts  TSx\n  1:1  m"
        );
    }

    #[test]
    fn single_child_chain_then_branch() {
        // src/ has only feature/, which branches into two files: src/feature/
        // collapses into one header.
        let out = run(vec![
            pb("src/feature/x.ts", "src/feature/x.ts\n  1:1  a"),
            pb("src/feature/y.ts", "src/feature/y.ts\n  2:2  b"),
        ]);
        assert_eq!(out, "src/feature/\n  x.ts\n    1:1  a\n  y.ts\n    2:2  b");
    }

    #[test]
    fn eslint_style_empty_suffix() {
        // eslint header is the bare path: suffix is "".
        let out = run(vec![
            pb("src/a.ts", "src/a.ts\n  1:1  error  msg  rule"),
            pb("src/b.ts", "src/b.ts\n  2:2  warn  msg  rule"),
        ]);
        assert_eq!(
            out,
            "src/\n  a.ts\n    1:1  error  msg  rule\n  b.ts\n    2:2  warn  msg  rule"
        );
    }

    #[test]
    fn inner_alignment_preserved_on_reindent() {
        // Padded location column must stay aligned after the uniform shift.
        let out = run(vec![
            pb("d/a.ts", "d/a.ts  TS1\n   1:1  short\n  10:1  longer"),
            pb("d/b.ts", "d/b.ts  TS2\n  3:3  x"),
        ]);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines[0], "d/");
        assert_eq!(lines[1], "  a.ts  TS1");
        assert_eq!(lines[2], "     1:1  short");
        assert_eq!(lines[3], "    10:1  longer");
    }

    #[test]
    fn none_path_passthrough_unchanged() {
        let blocks = vec![
            PathBlock {
                path: None,
                block: "FAIL src/x.test.ts\n  ✗ a".to_string(),
            },
            pb("src/y.ts", "src/y.ts  TS1\n  1:1  m"),
        ];
        let out = group_by_directory(blocks);
        assert_eq!(
            out,
            vec![
                "FAIL src/x.test.ts\n  ✗ a".to_string(),
                "src/y.ts  TS1\n  1:1  m".to_string()
            ]
        );
    }

    #[test]
    fn empty_input() {
        assert_eq!(group_by_directory(vec![]), Vec::<String>::new());
    }

    #[test]
    fn absolute_lone_path_keeps_leading_slash() {
        // Outside-cwd file: must stay byte-identical, leading '/' preserved.
        let out = run(vec![pb(
            "/other/src/bar.ts",
            "/other/src/bar.ts:1:1  TS2322  Error",
        )]);
        assert_eq!(out, "/other/src/bar.ts:1:1  TS2322  Error");
    }

    #[test]
    fn absolute_shared_prefix_groups_with_slash_header() {
        let out = run(vec![
            pb("/other/src/a.ts", "/other/src/a.ts  TS1\n  1:1  m"),
            pb("/other/src/b.ts", "/other/src/b.ts  TS2\n  2:2  m"),
        ]);
        assert_eq!(
            out,
            "/other/src/\n  a.ts  TS1\n    1:1  m\n  b.ts  TS2\n    2:2  m"
        );
    }

    #[test]
    fn single_file_in_dir_is_byte_identical() {
        // Depth-2 lone file: chain compresses fully, output unchanged.
        let out = run(vec![pb(
            "src/a.ts",
            "src/a.ts:1:1  TS2304  Cannot find name 'foo'",
        )]);
        assert_eq!(out, "src/a.ts:1:1  TS2304  Cannot find name 'foo'");
    }

    #[test]
    fn per_file_overflow_footer_reindented_under_dir() {
        // report.rs emits the per-file cap footer as the block's last line;
        // it must shift to the file-line column under a dir header.
        let out = run(vec![
            pb(
                "src/x/a.ts",
                "src/x/a.ts  TS1\n  1:1  m\n  ... and 5 more errors in this file",
            ),
            pb("src/x/b.ts", "src/x/b.ts  TS2\n  2:2  m"),
        ]);
        assert_eq!(
            out,
            "src/x/\n  \
a.ts  TS1\n    1:1  m\n    ... and 5 more errors in this file\n  \
b.ts  TS2\n    2:2  m"
        );
    }

    #[test]
    fn eslint_header_plus_footer_only_block_reindented() {
        // eslint enter_first_overflow_group: a group whose block is just the
        // path header + overflow footer (no item lines). Both lines must sit
        // under the dir header at the right indent.
        let out = run(vec![
            pb("src/a.ts", "src/a.ts\n  1:1  error  m  rule"),
            pb(
                "src/b.ts",
                "src/b.ts\n  ... and 3 more problems in this file",
            ),
        ]);
        assert_eq!(
            out,
            "src/\n  a.ts\n    1:1  error  m  rule\n  b.ts\n    ... and 3 more problems in this file"
        );
    }
}
