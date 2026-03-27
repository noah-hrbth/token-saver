# Git Log Compressor — Design Spec

## Problem

`git log` is one of the most frequently used git commands by LLM agents investigating repository history. Default output is extremely verbose — full 40-char hashes, multi-line commit headers with author emails, date in long format, generous whitespace padding. When combined with `-p` (patches) or `--stat`, output can easily reach thousands of tokens for just a handful of commits. An unbounded `git log` returns the entire history.

## Solution

A `GitLogCompressor` that uses a NUL-delimited custom format string (`--format=`) for reliable parsing, compresses commit metadata to single lines, reuses the existing diff compression logic for embedded patches, and injects a default commit cap (`-n 20`) to prevent unbounded output.

## `can_compress`

Returns `true` if first arg is `"log"`, unless any skip flag is present.

### Skip flags (trigger passthrough)

| Flag | Reason |
|------|--------|
| `--oneline` | Already compact, agent chose this deliberately |
| `--format=<custom>` | Agent chose a specific format |
| `--pretty=<custom>` | Same as above (see preset handling below) |
| `--graph` | ASCII topology interleaved with commits; complex to parse, already fairly compact |
| `--color` / `--color=always` | ANSI escape codes break parsing |

### `--pretty` preset handling

Git has built-in named presets. These are NOT custom formats:

| Preset | Action |
|--------|--------|
| `oneline` | Skip — already compact |
| `short`, `medium`, `full`, `fuller` | Compress — these are verbose, we can do better |
| `reference`, `email`, `raw`, `mboxrd` | Skip — specialized formats the agent chose deliberately |

Detection: if the value after `--pretty=` or `--format=` matches a known preset name, treat it as a preset. Otherwise treat it as a custom format string.

## `normalized_args`

Builds the argument list:

1. Start with `["log"]`
2. Add format string: `--format=%x01%h%x00%D%x00%aI%x00%an%x00%s%x00%b`
3. Add `--no-color`
4. If `-p` / `--patch` / `-u` present in original args:
   - Add `--unified=1`, `--no-ext-diff`, `--diff-algorithm=histogram`
   - Re-add `-p` after the format string (since format and patch interact)
5. If `--stat` / `--shortstat` present: preserve the flag
6. If no `-n` / `--max-count` / `-<digit>` present: inject `-n 20`
7. Strip from original args: `log`, `-p`/`--patch`/`-u`, any `--format=*`/`--pretty=*`, `--color*` (already handled or skip-flagged)
8. Append remaining original args: filters (`--author`, `--since`, `--until`, `--grep`), ranges (`<commit>..<commit>`), paths (`-- <path>`), scope flags (`--all`, `--branches`, `--merges`, `--no-merges`), `-n`/`--max-count`, `--stat`/`--shortstat`, etc.

### Format string fields

```
%x01  — record separator (between commits)
%h    — short hash
%D    — decorations (refs, tags) without wrapping parens
%aI   — author date, ISO 8601 format
%an   — author name
%s    — subject (first line of commit message)
%b    — body (remaining lines of commit message)
```

Fields within a commit are separated by `%x00` (NUL byte). Commits are separated by `%x01`.

## Parsing

### Input structure

The format string produces:

```
\x01<hash>\x00<decorations>\x00<date>\x00<author>\x00<subject>\x00<body>
[optional stat output for --stat]
[optional diff output for -p]
\x01<next_commit>...
```

### Data model

```rust
struct LogEntry {
    hash: String,                // short hash (7-12 chars)
    decorations: Vec<String>,    // e.g., ["HEAD -> main", "origin/main", "tag: v1.0"]
    date: String,                // YYYY-MM-DD (extracted from ISO 8601)
    author: String,              // name only, no email
    subject: String,             // first line of commit message
    body: Option<String>,        // remaining lines, None if empty/whitespace
    diff: Option<Vec<DiffFile>>, // parsed with shared diff logic when -p
    stat: Option<String>,        // compressed stat output when --stat
}
```

### Parsing pipeline

1. Split raw stdout on `\x01`, discard first empty element
2. For each chunk:
   a. If `-p` is active: split at first `\ndiff --git ` boundary — format fields before it, diff after
   b. If `--stat` is active: split at stat output (detected by ` | ` pattern with `+`/`-` after the format fields). Stat appears before diff if both present.
   c. Split the format portion on `\x00` → 6 fields: hash, decorations, date, author, subject, body
   d. Parse date: extract `YYYY-MM-DD` from ISO 8601 string
   e. Parse decorations: split on `, ` to get individual refs
   f. Trim body, set to `None` if empty/whitespace-only
   g. Parse diff portion using shared `diff_parser::parse_diff()`
   h. Parse and compress stat lines

### Stat parsing and compression

Input:
```
 src/auth.rs   | 15 +++++++++------
 src/main.rs   |  3 +++
 2 files changed, 12 insertions(+), 6 deletions(-)
```

Compression rules:
- Strip leading whitespace and padding between filename and `|`
- Count `+` and `-` characters in the bar to get insertion/deletion counts
- Format as `N+` and/or `N-` (omit zero side: `3+` not `3+ 0-`)
- Summary line (`N files changed, ...`) passed through as-is
- `--shortstat` only produces the summary line — pass through unchanged

Output:
```
src/auth.rs | 9+ 6-
src/main.rs | 3+
2 files changed, 12 insertions(+), 6 deletions(-)
```

## Compressed output format

### Standard commit (no `-p`, no `--stat`)

```
* a1b2c3f (HEAD -> main, tag: v1.0) 2024-01-15 [John Smith] Add user authentication
  Optional commit body here if non-empty.
  Can be multiple lines, preserved as-is.
```

- One line per commit when no body
- Author name always shown in `[brackets]` between date and subject
- Decorations in parentheses after hash, omitted if none
- Body indented with 2 spaces, included only if non-empty

### With `--stat`

```
* a1b2c3f (main) 2024-01-15 [John Smith] Add user authentication
  src/auth.rs | 9+ 6-
  src/main.rs | 3+
  2 files changed, 12 insertions(+), 6 deletions(-)
```

Stat lines indented with 2 spaces.

### With `-p` (patches)

```
* a1b2c3f (main) 2024-01-15 [John Smith] Add user authentication

--- src/auth.rs (new)
@@ +1 @@
+pub fn authenticate(user: &str) -> bool {
+    true
+}

--- src/main.rs
@@ -5 +5 @@ fn main
-    old_call();
+    authenticate("admin");
```

Blank line after commit header, then compressed diff output using the same shared formatting as `GitDiffCompressor` (1 line of context, whitespace-only hunks collapsed, file status annotations).

### With `--stat` and `-p` combined

```
* a1b2c3f (main) 2024-01-15 [John Smith] Add user authentication
  src/auth.rs | 9+ 6-
  src/main.rs | 3+
  2 files changed, 12 insertions(+), 6 deletions(-)

--- src/auth.rs (new)
@@ +1 @@
+pub fn authenticate() -> bool { true }
```

Stat first (indented), blank line, then diff.

### Empty log

```
(empty)
```

### Truncation notice

When exactly 20 commits are returned (the default cap), append a notice:

```
* ... (20 commits shown above)
(showing 20 commits, use -n to see more)
```

The notice appears at the end so the agent knows the history may have been capped. Note: `compress()` doesn't receive original args, so it can't distinguish "we injected `-n 20`" from "user passed `-n 20`". Showing the notice in both cases is acceptable — it's still useful information either way.

## Shared code extraction

### New file: `src/compressors/git/diff_parser.rs`

Extracted from `diff.rs`, contains all reusable diff parsing and formatting:

**Types moved:**
- `DiffFile` — parsed file with path, status, hunks
- `Hunk` — parsed hunk with line numbers, function context, lines
- `DiffLine` — `Context`, `Added`, `Removed` variants
- `FileStatus` — `New`, `Deleted`, `Renamed`, `ModeChanged`, `Binary`, `Normal`

**Functions moved:**
- `parse_diff(raw: &str) -> Option<Vec<DiffFile>>` — top-level parser
- `parse_file_chunk()` — single file chunk parser
- `parse_hunks()` — hunk extraction
- `parse_hunk_header()` — `@@` line parser
- `format_file(file: &DiffFile) -> String` — single file formatter
- `format_hunk()` — hunk formatter
- `is_whitespace_only_change()` — whitespace collapse detection
- `compute_stats(files: &[DiffFile]) -> String` — stat summary line

### Changes to `diff.rs`

- Remove moved code, import from `diff_parser` instead
- `GitDiffCompressor` struct, `can_compress()`, `normalized_args()`, `compress()` stay
- Diff-specific unit tests stay; parsing/formatting unit tests move with the code

### `log.rs` usage

- Imports `diff_parser::{parse_diff, format_file, DiffFile}` for `-p` handling
- Has its own stat parsing/compression (not shared — diff compressor doesn't handle stat)

This is a pure refactor of `diff.rs` — its external behavior does not change.

## Error handling

| Scenario | Behavior |
|----------|----------|
| Non-zero exit code | Return `None` → passthrough |
| Empty output | Return `Some("(empty)")` |
| Parse failure (malformed format fields) | Return `None` → passthrough |
| Diff parse failure within `-p` | Show commit metadata, skip the diff for that commit |
| Stat parse failure | Pass stat lines through uncompressed |
| No commits match filters | Return `Some("(empty)")` |
| Merge commits (multiple parents) | Handled normally, no special treatment |
| Truncated output | Return `None` → passthrough |

Principle: compress what we can, fall back on what we can't. Token-saver never surfaces its own errors to the agent.

## Testing strategy

### Unit tests (in `log.rs`)

- **`can_compress`**: positive cases (bare `log`, with `-n`, `--author`, `-p`, `--stat`) and negative cases (all skip flags: `--oneline`, `--format=custom`, `--pretty=custom`, `--graph`, `--color`, `--pretty=reference`)
- **`normalized_args`**: format string injection, `-n 20` default cap, `-p` adds `--unified=1`, cap skipped when `-n` present, cap skipped when range `..` present, `--pretty=medium` stripped
- **Parsing**: single commit, multiple commits, with body, without body, with decorations, with diffs, with stat, combined stat+diff, empty decorations
- **Formatting**: standard one-liner with author, with body, with compressed diff, with compressed stat, combined stat+diff, empty log, truncation notice
- **Stat compression**: `+++---` bars → `N+ N-`, insertions only, deletions only, summary line passthrough
- **Edge cases**: merge commits, empty body trimming, decorations parsing

### Integration tests (`tests/git_log.rs`)

Scenarios using existing `Scenario` struct + temp repo harness:

| Scenario | Setup | Key assertions |
|----------|-------|----------------|
| Basic log | Several commits | Contains short hashes, dates, subjects, author names |
| With body | Commit with multi-line message | Contains body text indented |
| With `-p` | File changes across commits | Contains compressed diff output |
| With `--stat` | Multiple file changes | Contains `N+ N-` format |
| With `-n 5` | Many commits | Only 5 commits shown |
| Default cap | 25+ commits | Shows 20, truncation notice |
| Filtered `--since` | Commits across dates | Only matching commits |
| Empty result | Filter matches nothing | Contains `(empty)` |

### Token comparison (`tests/compare.rs`)

Add git log scenarios to existing compare runner — raw vs compressed output with token counts.

### Passthrough tests (`tests/passthrough.rs`)

Verify unmodified output for `--oneline`, `--graph`, `--format=custom`, `--pretty=reference`.

### Shared diff extraction tests

Verify that existing diff compressor behavior is unchanged after the `diff_parser.rs` extraction. All existing `cargo test` must pass with no modifications to assertions.
