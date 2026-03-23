# Token Saver — Design Spec

## Problem

LLM agents (e.g., Claude Code) waste tokens on verbose CLI output. `git status` returns ~8 lines of human-friendly text when 2 lines of structured data would suffice. Existing solutions (e.g., rtk) use explicit wrapper commands (`ts git status`), but agents detect the indirection, distrust the compressed output, and retry with raw commands — consuming *more* tokens than no optimization at all.

## Core Insight

The agent must not know compression is happening. If it calls `git status` and gets output, it trusts it. If it calls `ts git status` and gets unfamiliar output, it retries with `git status` to verify.

## Solution

A single Rust binary (`token-saver`) that acts as a transparent proxy for CLI commands using the busybox symlink pattern. When running inside an agent environment (detected via `TOKEN_SAVER=1` env var), it intercepts command output and returns a compressed, LLM-friendly version. Outside that environment, it passes through to the real command unchanged.

## Architecture

### Dispatch model: single binary + symlinks

One compiled binary: `token-saver`. Symlinks named after each proxied command (`git`, `ls`, `grep`, etc.) point to it. The binary reads `argv[0]` to determine which command it's proxying.

```
~/.token-saver/bin/token-saver    (the binary)
~/.token-saver/bin/git            → token-saver (symlink)
~/.token-saver/bin/ls             → token-saver (symlink, future)
```

### Execution flow

```
Agent calls "git status"
    → OS resolves ~/.token-saver/bin/git via PATH priority
    → token-saver binary starts, argv[0] = "git"
    → checks TOKEN_SAVER env var
        → not set → exec real git with original args (full passthrough, token-saver exits)
        → set →
            → finds real git binary (skipping self in PATH)
            → looks up compressor for "git status"
                → no compressor found → exec real git (passthrough)
                → compressor found →
                    → normalizes args (e.g., "git status -sb" → "git status --porcelain=v2 --branch")
                    → runs real git with normalized args, captures stdout/stderr
                    → parse succeeds → prints compressed output, exits with same exit code
                    → parse fails → runs real git with ORIGINAL args, prints raw output
```

### Finding the real binary

The shim must avoid calling itself recursively. Strategy:
1. Walk `$PATH` entries in order
2. For each entry, check if the target command exists there
3. Skip any entry that is the token-saver bin directory (by comparing canonical paths)
4. Return the first remaining match

### Graceful fallback

If compression fails at any point (no compressor, parse error, unexpected output format), token-saver falls back to running the real command with the original arguments and returning raw output. The agent always gets valid, usable output.

## Compressor System

### Trait

```rust
pub trait Compressor {
    /// Can this compressor handle the given subcommand/args?
    fn can_compress(&self, args: &[String]) -> bool;

    /// What args to pass to the real binary for machine-parseable output.
    fn normalized_args(&self, original_args: &[String]) -> Vec<String>;

    /// Parse raw output and return compressed version.
    /// Returns None on parse failure (triggers raw fallback).
    /// The caller preserves the original exit code separately — exit_code here
    /// lets the compressor decide whether to compress (e.g., skip on error).
    fn compress(&self, stdout: &str, stderr: &str, exit_code: i32) -> Option<String>;
}
```

### Adding a new compressor

1. Create a new file in `src/compressors/<command>/` implementing `Compressor`
2. Register it in the parent module's dispatcher
3. Add tests

No binary changes, no config changes, no new symlinks needed for subcommands. New top-level commands (e.g., adding `ls`) need one new symlink.

## Project Layout

```
token-saver/
  Cargo.toml                         # Project config, dependencies
  src/
    main.rs                          # Entry: argv[0] dispatch, env check, fallback
    runner.rs                        # Find real binary, execute, capture output
    compressors/
      mod.rs                         # Compressor trait, registry
      git/
        mod.rs                       # Git subcommand dispatcher
        status.rs                    # git status compressor
  tests/
    git_status_test.rs               # Integration tests for git status
  scripts/
    install.sh                       # Build, install binary, create symlinks
```

## git status Compressor (First Implementation)

### Normalization

Regardless of input flags, always run:
```
git status --porcelain=v2 --branch -z
```

The `-z` flag produces NUL-delimited output, which correctly handles filenames with spaces, newlines, or other special characters.

This produces stable, machine-parseable output across git versions.

### Porcelain v2 format (input)

```
# branch.oid abc123def456
# branch.head main
# branch.upstream origin/main
# branch.ab +0 -0
1 .M N... 100644 100644 100644 abc123 def456 src/main.rs
? .claude/
? untracked-file.txt
```

Key lines:
- `# branch.oid <sha>` — commit SHA, or `(initial)` for repos with no commits
- `# branch.head <name>` — current branch (or `(detached)`)
- `# branch.upstream <name>` — tracking branch (absent if none)
- `# branch.ab +N -M` — ahead/behind counts
- `1 <XY> ...` — tracked file with changes (X = staged, Y = unstaged)
- `2 <XY> ... <TAB> <path> <TAB> <origPath>` — renamed/copied file (fields separated by TAB, not space)
- `? <path>` — untracked file
- `u <XY> ... ` — unmerged (conflict) file

Note: Use `-z` flag for NUL-delimited output to handle filenames with spaces/special characters robustly. The normalized args will be `git status --porcelain=v2 --branch -z`.

### Compressed output format

Key-value pairs, one category per line. Omit empty categories.

```
branch: <name> (<tracking info>)
[staged: <files>]
[modified: <files>]
[deleted: <files>]
[renamed: <old> -> <new>, ...]
[conflict: <files>]
[untracked: <files>]
```

If working tree is clean: `clean` on its own line.

### Tracking info format

- Up to date: `(up to date with origin/main)`
- Ahead only: `(+3 ahead of origin/main)`
- Behind only: `(-2 behind origin/main)`
- Diverged: `(+3 -2 vs origin/main)`
- No upstream: `(no upstream)`
- Detached: `HEAD (detached at <short-oid>)` (first 7 characters of `branch.oid`)

### File categorization from porcelain v2

For ordinary changed entries (`1 XY ...`):
- X is the staged status, Y is the unstaged status
- `.` means no change in that position
- Status codes: `M` = modified, `A` = added, `D` = deleted, `T` = type changed (e.g., file → symlink)

Mapping:
- X != `.` → file goes in `staged:` category
- Y = `M` or Y = `T` → file goes in `modified:` category (type changes treated as modifications)
- Y = `D` → file goes in `deleted:` category
- A file can appear in both staged and modified if it has staged changes AND unstaged changes

For renamed/copied entries (`2 XY ...`):
- Renames (`R` score) go in `renamed:` as `<origPath> -> <newPath>`
- Copies (`C` score) go in `renamed:` as `<origPath> -> <newPath> (copy)`
- If the entry also has unstaged changes (Y != `.`), the file additionally appears in the appropriate unstaged category (`modified:`, `deleted:`)

For unmerged entries (`u ...`):
- All unmerged entries go in `conflict:` regardless of their specific sub-status (`UU`, `AA`, `DD`, etc.). Differentiating conflict types is out of scope for v1.

For untracked (`? <path>`):
- Goes in `untracked:`

### Known limitations (v1)

- Filenames containing `, ` (comma-space) will be ambiguous in comma-separated output. This is acceptable for v1 — repos with such filenames are rare. Future versions may switch to newline-per-file if needed.

### Output examples

```
# Diverged branch with mixed changes
branch: main (+3 -1 vs origin/main)
staged: src/new.rs, src/lib.rs
modified: src/main.rs, README.md
deleted: old_file.rs
untracked: notes.txt

# Clean repo
branch: main (up to date with origin/main)
clean

# Detached HEAD
branch: HEAD (detached at abc123f)
modified: src/main.rs

# No upstream configured
branch: feature-x (no upstream)
staged: src/feature.rs
untracked: .env.example

# New repo, initial commit pending
branch: main (no upstream)
staged: src/main.rs, Cargo.toml
untracked: .gitignore
```

## Setup & Integration

### Installation

`scripts/install.sh` (idempotent — safe to re-run):
1. Creates `~/.token-saver/bin/` if it doesn't exist
2. Runs `cargo build --release`
3. Copies binary to `~/.token-saver/bin/token-saver`
4. Removes old symlinks if they exist, then creates fresh ones (e.g., `git → token-saver`)
5. Prints Claude Code configuration snippet with the correct absolute path

### Claude Code configuration

In the project's `.claude/settings.json` or user's `~/.claude/settings.json`:

```json
{
  "env": {
    "TOKEN_SAVER": "1",
    "PATH": "/Users/<username>/.token-saver/bin:$PATH"
  }
}
```

Note: Use the absolute path — `~` may not expand in all contexts. The install script will generate this snippet with the correct absolute path for the current user.

This scopes token-saver exclusively to Claude Code. The user's normal terminal is unaffected.

### Direct invocation

When `argv[0]` is `token-saver` (not a symlink), the binary supports direct invocation: `token-saver <command> [args...]`. This is useful for development and testing without setting up symlinks.

Example: `token-saver git status` — treats `git` as the command and `status` as its argument.

### Manual testing

```bash
# Build
cargo build

# Passthrough mode (no env var, normal git output)
./target/debug/token-saver git status

# Compressed mode
TOKEN_SAVER=1 ./target/debug/token-saver git status

# Via symlink (after install)
TOKEN_SAVER=1 ~/.token-saver/bin/git status
```

## Testing Strategy

### Unit tests

Per-compressor tests that feed raw porcelain strings into the parser and assert compressed output. Located in the same file as the compressor code (`#[cfg(test)] mod tests {}`).

Cover: clean repo, dirty repo, staged + unstaged, renamed files, deleted files, merge conflicts, detached HEAD, no upstream, ahead/behind, diverged, empty output, non-zero exit codes.

### Integration tests

Located in `tests/`. Create a real temporary git repo, run the token-saver binary, verify output matches expectations. Tests both compressed and passthrough modes.

### Manual smoke test

Configure Claude Code with token-saver, observe agent behavior. Confirm:
- Agent calls `git status` and gets compressed output
- Agent does NOT retry or call git status multiple times
- Agent correctly interprets the compressed output

## Non-Goals (Explicitly Out of Scope)

- Compressing commands beyond git status in this first implementation
- Custom configuration files or per-command config
- Caching or memoization of command output
- Support for non-Claude-Code agents (future consideration)
- Windows support (macOS/Linux only for now)

## Future Commands (Planned)

After git status is stable:
- `git diff` — strip context, show only changed lines with minimal context
- `git log` — compact format, relevant fields only
- `ls` — structured output, no permission/owner noise
- `grep` / `rg` — already compact, may not need compression

Each follows the same pattern: implement `Compressor`, register, add tests.
