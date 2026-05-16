//! Shared "capped grouped report" rendering.
//!
//! Several compressors (eslint, tsc, jest) emit the same shape of output: a
//! list of groups, each with a header line and a list of pre-formatted item
//! blocks, subject to a per-group cap and a global cap, with "... and N more"
//! footers. This module owns ONLY that cap-carry + footer-pluralization
//! algorithm, plus the shared `relativize_path` util.
//!
//! It is deliberately ignorant of severity, location padding, TS codes,
//! deduplication, and any prefix/suffix sections — callers build every header
//! and item string themselves and keep their own surrounding sections.

/// A singular/plural noun pair used in footer text. Use `Noun::new("file",
/// "files")` or `Noun::new("suite", "suites")` — handles both full-word and
/// suffix-style pluralization without any `s`-flag logic.
#[derive(Clone, Copy)]
pub struct Noun {
    pub one: &'static str,
    pub many: &'static str,
}

impl Noun {
    pub fn new(one: &'static str, many: &'static str) -> Self {
        Self { one, many }
    }

    /// Pick the singular form when `n == 1`, plural otherwise.
    pub fn pick(&self, n: usize) -> &'static str {
        if n == 1 { self.one } else { self.many }
    }
}

/// One item inside a group. `block` is fully pre-formatted by the caller and
/// may contain newlines (one logical item = possibly several lines, e.g.
/// jest's "✗ name" plus its indented error). An empty `block` emits no line
/// but still counts toward the caps (jest's suite-level failure with no
/// message). `weight` is how much this item contributes to the *displayed*
/// overflow counts; the cap arithmetic always counts items as 1 each.
pub struct Item {
    pub block: String,
    pub weight: usize,
}

impl Item {
    /// Item with display weight 1 (eslint, jest).
    pub fn new(block: String) -> Self {
        Self { block, weight: 1 }
    }

    /// Item with an explicit display weight (tsc: weight = location count).
    pub fn weighted(block: String, weight: usize) -> Self {
        Self { block, weight }
    }
}

/// One group: a pre-formatted header line plus its items, in display order.
pub struct Group {
    pub header: String,
    pub items: Vec<Item>,
    pub path: Option<String>,
}

/// Where the remainder goes when the *total* cap trips partway through a
/// group's items.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum MidTotalOverflow {
    /// Remainder is reported in the current group's footer ("... and N more
    /// <item> in this <group>"). Used by eslint, tsc.
    AttributeToGroup,
    /// Remainder rolls into the global footer (the group also counts as one
    /// skipped group). Used by jest.
    AttributeToTotal,
}

/// Configuration for one report render.
pub struct ReportConfig {
    /// Per-group cap. `None` = unlimited per group (only the total cap
    /// applies). eslint=Some(50), tsc=Some(30), jest=Some(10).
    pub max_items_per_group: Option<usize>,
    /// Global cap across all groups. eslint=200, tsc=100, jest=20.
    pub max_items_total: usize,
    /// Items already emitted before this render (seed for the running total).
    /// tsc seeds this with its inline-fast-path count so the shared-counter
    /// semantics are preserved; eslint/jest pass 0.
    pub items_already_emitted: usize,
    /// Item noun for footers, e.g. `("problem","problems")`.
    pub item_noun: Noun,
    /// Group noun for the total footer, e.g. `("file","files")`.
    pub group_noun: Noun,
    /// Mid-group total-overflow attribution (see `MidTotalOverflow`).
    pub mid_total: MidTotalOverflow,
    /// Behaviour when the total cap is reached exactly at a group boundary.
    /// `true` (eslint): the next group is still entered — its header prints
    /// and its full weight becomes that group's footer. `false` (tsc, jest):
    /// the next group is wholly skipped (no header) and rolls into the total
    /// footer. This faithfully preserves the differing original behaviour
    /// (eslint's group loop only re-checks a `capped` flag set on a mid-group
    /// trip; tsc/jest re-check the running total at each group top).
    pub enter_first_overflow_group: bool,
}

/// Result the caller interleaves with its own prefix/suffix sections.
pub struct Report {
    /// One PathBlock per emitted group: header + emitted item blocks +
    /// (optionally) the per-group "... and N more ... in this <group>"
    /// footer, joined by '\n', with the group's path carried through.
    pub groups: Vec<crate::compressors::tree::PathBlock>,
    /// The single global "... and N more <item> across M <group>" line, or
    /// `None` if nothing overflowed at the total level.
    pub total_overflow: Option<String>,
}

/// Render groups under the dual cap. See module docs and `ReportConfig` for
/// the exact semantics; the algorithm is a single pass that reproduces the
/// original eslint/tsc/jest behaviour.
pub fn render_groups(groups: Vec<Group>, cfg: &ReportConfig) -> Report {
    let mut rendered_groups: Vec<crate::compressors::tree::PathBlock> =
        Vec::with_capacity(groups.len());
    let mut total = cfg.items_already_emitted;
    // Set only on a mid-group total trip; drives eslint's group-top skip.
    let mut capped = false;
    let mut total_skipped_weight = 0usize;
    let mut total_skipped_groups = 0usize;

    for group in groups {
        let group_weight: usize = group.items.iter().map(|i| i.weight).sum();

        let skip_whole = if cfg.enter_first_overflow_group {
            capped
        } else {
            total >= cfg.max_items_total
        };
        if skip_whole {
            total_skipped_weight += group_weight;
            total_skipped_groups += 1;
            continue;
        }

        let path = group.path;
        let n = group.items.len();
        let mut lines: Vec<String> = Vec::with_capacity(n + 2);
        lines.push(group.header);

        let mut i = 0usize;
        let mut total_tripped = false;
        while i < n {
            if total >= cfg.max_items_total {
                capped = true;
                total_tripped = true;
                break;
            }
            if let Some(per_group) = cfg.max_items_per_group
                && i >= per_group
            {
                break;
            }
            let item = &group.items[i];
            if !item.block.is_empty() {
                lines.push(item.block.clone());
            }
            total += 1;
            i += 1;
        }

        if i < n {
            let remaining_weight: usize = group.items[i..].iter().map(|x| x.weight).sum();
            if total_tripped && cfg.mid_total == MidTotalOverflow::AttributeToTotal {
                total_skipped_weight += remaining_weight;
                total_skipped_groups += 1;
            } else if remaining_weight > 0 {
                lines.push(format!(
                    "  ... and {} more {} in this {}",
                    remaining_weight,
                    cfg.item_noun.pick(remaining_weight),
                    cfg.group_noun.one
                ));
            }
        }

        rendered_groups.push(crate::compressors::tree::PathBlock {
            path,
            block: lines.join("\n"),
        });
    }

    let total_overflow = if total_skipped_groups > 0 {
        Some(format!(
            "... and {} more {} across {} {}",
            total_skipped_weight,
            cfg.item_noun.pick(total_skipped_weight),
            total_skipped_groups,
            cfg.group_noun.pick(total_skipped_groups)
        ))
    } else {
        None
    };

    Report {
        groups: rendered_groups,
        total_overflow,
    }
}

/// Strip `cwd` prefix from an absolute path; otherwise return it unchanged.
/// Consolidated from the byte-identical copies previously in the eslint, tsc,
/// and jest modules.
pub fn relativize_path(path: &str, cwd: &Option<String>) -> String {
    if let Some(prefix) = cwd
        && let Some(stripped) = path.strip_prefix(prefix.as_str())
    {
        return stripped.to_string();
    }
    path.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn group(header: &str, items: &[&str]) -> Group {
        Group {
            header: header.to_string(),
            items: items.iter().map(|s| Item::new(s.to_string())).collect(),
            path: None,
        }
    }

    fn blocks(r: &Report) -> Vec<String> {
        r.groups.iter().map(|g| g.block.clone()).collect()
    }

    fn cfg(
        per_group: Option<usize>,
        total: usize,
        mid: MidTotalOverflow,
        enter_first: bool,
    ) -> ReportConfig {
        ReportConfig {
            max_items_per_group: per_group,
            max_items_total: total,
            items_already_emitted: 0,
            item_noun: Noun::new("problem", "problems"),
            group_noun: Noun::new("file", "files"),
            mid_total: mid,
            enter_first_overflow_group: enter_first,
        }
    }

    #[test]
    fn case1_no_overflow() {
        let groups = vec![group("a", &["x", "y"]), group("b", &["z"])];
        let r = render_groups(
            groups,
            &cfg(Some(50), 200, MidTotalOverflow::AttributeToGroup, true),
        );
        assert_eq!(blocks(&r), vec!["a\nx\ny", "b\nz"]);
        assert_eq!(r.total_overflow, None);
    }

    #[test]
    fn case2_per_group_overflow_independent_of_mid_total() {
        for mid in [
            MidTotalOverflow::AttributeToGroup,
            MidTotalOverflow::AttributeToTotal,
        ] {
            let groups = vec![group("a", &["1", "2", "3", "4", "5"])];
            let r = render_groups(groups, &cfg(Some(2), 999, mid, true));
            assert_eq!(
                blocks(&r),
                vec!["a\n1\n2\n  ... and 3 more problems in this file"]
            );
            assert_eq!(r.total_overflow, None);
        }
    }

    #[test]
    fn case3_total_overflow_mid_group_attribute_to_group() {
        // total=3, groups [2,2,2]. g1 full; g2 emits 1 then total trips.
        let groups = vec![
            group("a", &["a1", "a2"]),
            group("b", &["b1", "b2"]),
            group("c", &["c1", "c2"]),
        ];
        let r = render_groups(
            groups,
            &cfg(Some(99), 3, MidTotalOverflow::AttributeToGroup, true),
        );
        assert_eq!(
            blocks(&r),
            vec!["a\na1\na2", "b\nb1\n  ... and 1 more problem in this file"]
        );
        assert_eq!(
            r.total_overflow,
            Some("... and 2 more problems across 1 file".to_string())
        );
    }

    #[test]
    fn case3_total_overflow_mid_group_attribute_to_total() {
        let groups = vec![
            group("a", &["a1", "a2"]),
            group("b", &["b1", "b2"]),
            group("c", &["c1", "c2"]),
        ];
        let r = render_groups(
            groups,
            &cfg(Some(99), 3, MidTotalOverflow::AttributeToTotal, false),
        );
        // g2 emits b1 (header + 1 item, no per-group footer); remainder of g2
        // (1) + g3 (2) roll to total across 2 groups.
        assert_eq!(blocks(&r), vec!["a\na1\na2", "b\nb1"]);
        assert_eq!(
            r.total_overflow,
            Some("... and 3 more problems across 2 files".to_string())
        );
    }

    #[test]
    fn case4_boundary_enter_first_true_shows_header_and_footer() {
        // total=2; g1 consumes exactly 2 (boundary). enter_first=true (eslint):
        // g2 entered, header + full-weight footer; g3 wholly skipped.
        let groups = vec![
            group("a", &["a1", "a2"]),
            group("b", &["b1", "b2"]),
            group("c", &["c1"]),
        ];
        let r = render_groups(
            groups,
            &cfg(Some(99), 2, MidTotalOverflow::AttributeToGroup, true),
        );
        assert_eq!(
            blocks(&r),
            vec!["a\na1\na2", "b\n  ... and 2 more problems in this file"]
        );
        assert_eq!(
            r.total_overflow,
            Some("... and 1 more problem across 1 file".to_string())
        );
    }

    #[test]
    fn case4_boundary_enter_first_false_skips_header() {
        // Same shape, enter_first=false (tsc/jest): g2 and g3 wholly skipped.
        let groups = vec![
            group("a", &["a1", "a2"]),
            group("b", &["b1", "b2"]),
            group("c", &["c1"]),
        ];
        let r = render_groups(
            groups,
            &cfg(Some(99), 2, MidTotalOverflow::AttributeToGroup, false),
        );
        assert_eq!(blocks(&r), vec!["a\na1\na2"]);
        assert_eq!(
            r.total_overflow,
            Some("... and 3 more problems across 2 files".to_string())
        );
    }

    #[test]
    fn case5_no_per_group_cap() {
        let groups = vec![group("a", &["1", "2", "3", "4", "5"])];
        let r = render_groups(
            groups,
            &cfg(None, 3, MidTotalOverflow::AttributeToGroup, true),
        );
        assert_eq!(
            blocks(&r),
            vec!["a\n1\n2\n3\n  ... and 2 more problems in this file"]
        );
        assert_eq!(r.total_overflow, None);
    }

    #[test]
    fn case6_weighted_items() {
        // 3 items weight 3 each, total cap 2 items. 2 emitted, 1 remains
        // (weight 3) -> footer says 3, not 1.
        let g = Group {
            header: "a".to_string(),
            items: vec![
                Item::weighted("i1".to_string(), 3),
                Item::weighted("i2".to_string(), 3),
                Item::weighted("i3".to_string(), 3),
            ],
            path: None,
        };
        let r = render_groups(
            vec![g],
            &cfg(Some(99), 2, MidTotalOverflow::AttributeToGroup, true),
        );
        assert_eq!(
            blocks(&r),
            vec!["a\ni1\ni2\n  ... and 3 more problems in this file"]
        );
        assert_eq!(r.total_overflow, None);
    }

    #[test]
    fn case7_items_already_emitted_seed() {
        let mut c = cfg(Some(99), 3, MidTotalOverflow::AttributeToGroup, true);
        c.items_already_emitted = 2; // only 1 slot left
        let r = render_groups(vec![group("a", &["1", "2", "3"])], &c);
        assert_eq!(
            blocks(&r),
            vec!["a\n1\n  ... and 2 more problems in this file"]
        );
        assert_eq!(r.total_overflow, None);
    }

    #[test]
    fn case8_pluralization_both_noun_styles() {
        let suite_cfg = ReportConfig {
            max_items_per_group: Some(1),
            max_items_total: 999,
            items_already_emitted: 0,
            item_noun: Noun::new("failure", "failures"),
            group_noun: Noun::new("suite", "suites"),
            mid_total: MidTotalOverflow::AttributeToTotal,
            enter_first_overflow_group: false,
        };
        // 2 items, per-group cap 1 -> "1 more failure in this suite" (singular).
        let r = render_groups(vec![group("s", &["a", "b"])], &suite_cfg);
        assert_eq!(
            blocks(&r),
            vec!["s\na\n  ... and 1 more failure in this suite"]
        );

        // Total footer: 1 group / many -> "1 suite" vs "2 suites".
        let groups = vec![group("g1", &["a", "b"]), group("g2", &["c", "d"])];
        let r = render_groups(
            groups,
            &cfg(Some(99), 2, MidTotalOverflow::AttributeToGroup, false),
        );
        assert_eq!(
            r.total_overflow,
            Some("... and 2 more problems across 1 file".to_string())
        );
    }

    #[test]
    fn case9_empty_and_header_only_group() {
        // Empty-block item: counts toward cap but emits no line.
        let g = Group {
            header: "FAIL x".to_string(),
            items: vec![Item::new(String::new())],
            path: None,
        };
        let r = render_groups(
            vec![g],
            &cfg(Some(10), 20, MidTotalOverflow::AttributeToTotal, false),
        );
        assert_eq!(blocks(&r), vec!["FAIL x"]);
        assert_eq!(r.total_overflow, None);

        // Zero-item group: header only, no footer.
        let g2 = Group {
            header: "FAIL y".to_string(),
            items: vec![],
            path: None,
        };
        let r2 = render_groups(
            vec![g2],
            &cfg(Some(10), 20, MidTotalOverflow::AttributeToTotal, false),
        );
        assert_eq!(blocks(&r2), vec!["FAIL y"]);
        assert_eq!(r2.total_overflow, None);
    }

    #[test]
    fn relativize_strips_cwd_prefix() {
        let cwd = Some("/home/u/proj/".to_string());
        assert_eq!(relativize_path("/home/u/proj/src/a.rs", &cwd), "src/a.rs");
    }

    #[test]
    fn relativize_no_prefix_match_returns_input() {
        let cwd = Some("/other/".to_string());
        assert_eq!(relativize_path("/home/u/a.rs", &cwd), "/home/u/a.rs");
    }

    #[test]
    fn relativize_none_cwd_returns_input() {
        assert_eq!(relativize_path("/home/u/a.rs", &None), "/home/u/a.rs");
    }
}
