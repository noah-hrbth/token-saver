# token-saver

token-saver is a transparent CLI proxy that compresses verbose command output for LLM agents. It intercepts common shell commands (`git`, `ls`, `grep`, etc.) and produces concise, structured output — saving tokens and reducing noise in AI coding sessions.

## Install

### Homebrew (recommended)

```sh
brew tap noah-hrbth/token-saver
brew install token-saver
token-saver install
```

`token-saver install` runs an interactive wizard with three steps:

1. **Shell hook** — appends the eval line to your shell profile (or prints it for manual setup)
2. **Scope** — global (config under `$HOME`) or project (config committed to the repo)
3. **Agents** — checkbox selection of detected agents. Auto-configures **claude**, **pi**, and **codex**; **opencode** and **cursor** get printed manual instructions (no config-file env mechanism — see the research section in `COMPRESSORS.md`)

When stdin is not a TTY (scripts, CI), `install` falls back to the silent behavior: shell profile + `~/.claude/settings.json`. Everything is idempotent — re-running is safe. Run `token-saver uninstall` to reverse the setup (including project-scoped configs and the legacy `~/.token-saver/bin` binary), or `token-saver version` to print the installed version.

After `install`, reload your shell:

```sh
source ~/.zshenv   # or ~/.bashrc for bash
```

The shell wrappers are guarded by `TOKEN_SAVER=1` — they are a no-op in normal interactive shells.

#### Manual setup (if you prefer)

If you'd rather wire things up yourself, `token-saver install zsh` (or `install bash`) prints just the shell-function block — pipe it through `eval` from your profile, and add `TOKEN_SAVER=1` to your AI tool's environment.

### Uninstall

```sh
token-saver uninstall     # removes shell hook + agent configs (global and current repo)
brew uninstall token-saver
```

### Why `~/.zshenv` and not `~/.zshrc`

Claude Code's Bash tool runs commands in a **non-interactive** zsh subshell. Non-interactive zsh sources `~/.zshenv` but does **not** source `~/.zshrc`, so shell functions defined in `~/.zshrc` are never available to the agent. `~/.zshenv` is sourced for all zsh instances, interactive or not.

## Wrapped commands

token-saver currently compresses output from:
`cat`, `eslint`, `git`, `jest`, `ls`, `find`, `grep`, `npx`, `prettier`, `rg`, `tsc`.

## Build from source

Requires Rust 1.85+.

```sh
git clone https://github.com/noah-hrbth/token-saver.git
cd token-saver
cargo install --path .
token-saver install
```

This installs the binary to `~/.cargo/bin`. Remove it later with `cargo uninstall token-saver`.

## License

MIT — see [LICENSE](LICENSE).
