# token-saver

token-saver is a transparent CLI proxy that compresses verbose command output for LLM agents. It intercepts common shell commands (`git`, `ls`, `grep`, etc.) and produces concise, structured output — saving tokens and reducing noise in AI coding sessions.

## Install

### Homebrew (recommended)

```sh
brew tap noah-hrbth/token-saver
brew install token-saver
```

Then add the shell wrappers to your startup file.

**zsh** — add to `~/.zshenv` (not `~/.zshrc`; see note below):

```sh
echo 'eval "$(token-saver init zsh)"' >> ~/.zshenv
```

**bash** — add to `~/.bashrc`:

```sh
echo 'eval "$(token-saver init bash)"' >> ~/.bashrc
```

Then enable compression in your AI tool. For Claude Code, add to `~/.claude/settings.json`:

```json
{
  "env": {
    "TOKEN_SAVER": "1"
  }
}
```

The shell wrappers are guarded by `TOKEN_SAVER=1` — they are a no-op in normal interactive shells.

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
echo 'eval "$(token-saver init zsh)"' >> ~/.zshenv   # or ~/.bashrc for bash
```

## License

MIT — see [LICENSE](LICENSE).
