# Context

Glossary of token-saver's domain terms. No implementation details.

## Terms

- **Compressor** — per-command output reducer; the product's core value.
- **Wrapper** — shell function shadowing a real command, active only when `TOKEN_SAVER=1`.
- **Shell hook** — the `eval "$(token-saver install <shell>)"` line in the user's shell profile that defines the wrappers. Always global — shell profiles are per-user.
- **Install scope** — *global* writes agent config under `$HOME`; *project* writes agent config into the repo (`.claude/`, `.pi/`, `.codex/`). The shell hook is global in both scopes.
- **Agent target** — an AI coding CLI that token-saver auto-configures: *scriptable* (claude, pi, codex) vs *manual* (opencode, cursor — printed instructions only, pending research).
- **Detection** — locating agent targets via their config dirs (`.claude`, `.codex`, `.pi`, `.cursor`, `.opencode`). Global detection assumes default `$HOME` locations.
