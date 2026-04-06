# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build & Test Commands

```bash
cargo build                          # Debug build
cargo build --release                # Release build
cargo test                           # Run all tests (unit + integration)
cargo test <test_name>               # Run a single test by name
cargo test --test git_status         # Run one integration test file
cargo test --test git_diff           # Run git diff integration tests
cargo test --test compare -- --ignored --nocapture  # Token comparison benchmarks (visual output)
TOKEN_SAVER=1 cargo run -- git status               # Manual end-to-end test
```

Run `cargo fmt` and `cargo clippy` after changes. Use `/verify` to run all checks at once.

## Architecture

token-saver is a transparent CLI proxy that compresses verbose command output for LLM agents. It intercepts commands via shell functions (installed by `scripts/install.sh`), runs the real binary with machine-parseable flags, then compresses the output. When `TOKEN_SAVER=1` is not set, it passes through to the real command unchanged.

### Execution flow (`src/main.rs`)

1. Determine command name from argv (supports both direct `token-saver git status` and symlink invocation)
2. Find the real binary on PATH, skipping token-saver's own directory (`src/runner.rs`)
3. If `TOKEN_SAVER != "1"`, exec passthrough (replaces process)
4. Look up a `Compressor` for the command+args (`src/compressors/mod.rs`)
5. If found: run with `normalized_args`, compress stdout, print result
6. If not found or compression fails: fall back to passthrough with original args

### Compressor trait (`src/compressors/mod.rs`)

All compressors implement three methods:
- `can_compress(args)` — decides if this compressor handles the given args
- `normalized_args(args)` — rewrites args for machine-parseable output (e.g., `--porcelain=v2`)
- `compress(stdout, stderr, exit_code)` — returns compressed string or `None` to fall back

Dispatch chain: `find_compressor(command, args)` → `git::find_compressor(args)` → iterates registered compressors.

### Current compressors

- **`git status`** (`src/compressors/git/status.rs`) — parses `--porcelain=v2 --branch -z` output into `branch: ...\nmodified: ...\nuntracked: ...` format
- **`git diff`** (`src/compressors/git/diff.rs`) — parses unified diff, reduces context to 1 line (`--unified=1`), collapses whitespace-only hunks, adds stat summary for multi-file diffs. Has skip flags (e.g., `--stat`, `--name-only`) that trigger passthrough.

### Test structure

- **Unit tests**: inline `#[cfg(test)]` modules in each source file
- **Integration tests** (`tests/`): use `tests/common/` harness that creates temp git repos, runs the binary with `TOKEN_SAVER=1`, and asserts on compressed output via `Scenario` structs
- **Token comparison** (`tests/compare.rs`): `#[ignore]`-tagged tests that show raw vs compressed output with token counts (uses `tiktoken-rs` in dev-dependencies)
- **Passthrough tests** (`tests/passthrough.rs`): verify correct behavior when TOKEN_SAVER is unset or no compressor matches

### Adding a new compressor

1. Create `src/compressors/<command>/<subcommand>.rs` implementing `Compressor`
2. Register in `src/compressors/<command>/mod.rs` dispatcher
3. Add unit tests in the module + integration test scenarios in `tests/common/`
4. Add the command to the shell function block in `scripts/install.sh`

## Key Design Decisions

- **Shell functions, not PATH manipulation**: `scripts/install.sh` installs guarded shell functions (only active when `TOKEN_SAVER=1`). Tools using `command git` bypass the function and hit real git — this is intentional and critical for compatibility with Oh My Zsh, IDE integrations, etc.
- **Graceful fallback**: if compression fails or returns `None`, token-saver re-execs with the original args. The agent never sees an error from token-saver itself.
- **Minimal dependencies at runtime**: `serde` and `serde_json` for JSON-based compressors (eslint). Dev-dependencies (`tempfile`, `tiktoken-rs`) are used for testing.
- **Rust edition 2024**.
