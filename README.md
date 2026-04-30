# token-saver

token-saver is a transparent CLI proxy that compresses verbose command output for LLM agents. It intercepts common shell commands (`git`, `ls`, `grep`, etc.) and produces concise, structured output — saving tokens and reducing noise in AI coding sessions.

## Install

### Homebrew (recommended)

```sh
brew tap noah-hrbth/token-saver
brew install token-saver
token-saver init
```

`token-saver init` auto-detects your shell (zsh or bash), appends the eval line to your shell profile, and adds `"TOKEN_SAVER": "1"` to `~/.claude/settings.json` (creating the file if needed). It is idempotent — re-running is safe.

After `init`, reload your shell:

```sh
source ~/.zshenv   # or ~/.bashrc for bash
```

The shell wrappers are guarded by `TOKEN_SAVER=1` — they are a no-op in normal interactive shells.

#### Manual setup (if you prefer)

If you'd rather wire things up yourself, `token-saver init zsh` (or `init bash`) prints just the shell-function block — pipe it through `eval` from your profile, and add `TOKEN_SAVER=1` to your AI tool's environment.

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
./scripts/install.sh
```

Or via cargo (installs to `~/.cargo/bin`):

```sh
cargo install --path .
token-saver init
```

## License

MIT — see [LICENSE](LICENSE).
