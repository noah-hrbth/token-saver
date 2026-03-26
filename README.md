# token-saver

A transparent CLI proxy that compresses verbose command output for LLM agents. Saves tokens without the agent knowing.

## How it works

token-saver sits between the agent and real CLI commands using shell functions. When `TOKEN_SAVER=1` is set, the shell functions route commands through token-saver, which compresses their output. When unset, the functions aren't defined and commands run normally.

Tools like Oh My Zsh, IDE integrations, and scripts use `command git` internally, which bypasses shell functions entirely — so they always talk to real git, unmodified.

```
Agent calls "git status"
  -> shell function intercepts (only when TOKEN_SAVER=1)
  -> token-saver runs real git with machine-parseable flags
  -> returns compressed output

# Before (raw git status): ~350 tokens
On branch main
Your branch is up to date with 'origin/main'.

Changes not staged for commit:
  (use "git add <file>..." to update what will be committed)
  (use "git restore <file>..." to discard changes in working directory)
        modified:   src/main.rs

Untracked files:
  (use "git add <file>..." to include in what will be committed)
        .env.example

no changes added to commit (use "git add" and/or "git commit -a")

# After (token-saver): ~30 tokens
branch: main (up to date with origin/main)
modified: src/main.rs
untracked: .env.example
```

## Supported commands

| Command | Status |
|---------|--------|
| `git status` | Supported |
| `git diff` | Planned |
| `git log` | Planned |

## Install

```bash
# Requires Rust toolchain (https://rustup.rs)
git clone <this-repo>
cd token-saver
bash scripts/install.sh
```

## Configure Claude Code

The install script adds guarded shell functions to your profile (`~/.zshrc` or `~/.bashrc`).
You only need to enable the `TOKEN_SAVER` env var in Claude Code's settings.

Add to `~/.claude/settings.json` (inside the top-level object):

```json
"env": {
  "TOKEN_SAVER": "1"
}
```

## Development

```bash
# Build
cargo build

# Run tests
cargo test

# Compare raw vs compressed output with token counts
bash scripts/compare.sh

# Test manually
TOKEN_SAVER=1 cargo run -- git status

# Install locally
bash scripts/install.sh
```

## Adding a new compressor

1. Create `src/compressors/<command>/<subcommand>.rs` implementing the `Compressor` trait
2. Register it in the parent module's dispatcher
3. Add unit tests + integration tests
4. Add the command to the function block in `scripts/install.sh`
