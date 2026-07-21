# Compressors

> Source of truth is `src/compressors/`. This doc is for planning and tracking.

Tracking which command compressors are implemented and which are planned.

## Implemented

- [x] `git status` — porcelain v2 branch/modified/untracked summary
- [x] `git diff` — reduced context, collapsed whitespace hunks, stat summary
- [x] `git log` — one-line-per-commit, capped at 20, compressed stat bars
- [x] `git show` — compact commit + diff, tag header support
- [x] `ls -l` — type indicators + human-readable sizes, no metadata noise
- [x] `find` — noise filtering (.git, __pycache__, .DS_Store, *.pyc), tree-style output, 500-entry cap
- [x] `grep` / `rg` — group matches by file, deduplicate path prefixes, right-align line numbers, 200-match cap
- [x] `git branch` — compact branch list with tracking info, 50-branch cap
- [x] `cat` — binary detection, minified line collapsing (>2000 chars), 1000-line cap with truncation footer
- [x] `eslint` / `npx eslint` — JSON-based parsing, group by file, errors before warnings, per-file + total caps (50/200), fatal error separation, fixable count summary
- [x] `prettier` / `npx prettier` — `--check` file list + count, `--write` summary, bare stdout passthrough
- [x] `jest` / `npx jest` — JSON-based parsing, failures grouped by suite with error truncation (15-line cap), per-suite (10) + total (20) failure caps, directory-grouped suite list, optional coverage table, summary with skipped counts
- [x] `tsc` / `npx tsc` — text parsing (no JSON mode), group by file, CONFIG: section for global errors, per-file + total caps (30/100), chain continuations preserved; deduplicates same-code+message errors (comma-joined locations), hoists uniform error code to file header, inlines single-error files, strips trailing periods

## Planned

### General (cross-ecosystem)

- [ ] `git blame` — group consecutive lines by commit, deduplicate metadata
- [ ] `head` / `tail` — pass through (hook point for future structure-aware truncation)
- [ ] `curl` — strip response headers, summarize status code + content-type, pass body (mac/linux only)

### JavaScript / TypeScript

- [ ] `vitest` — summary only (X passed, Y failed), list only failures with context
- [ ] `npm install` / `yarn` / `pnpm install` — strip progress/fetch noise, show: added N, removed N, warnings
- [ ] `npm ls` — flatten dependency tree, show only top-level + flagged duplicates
- [ ] `webpack` / `vite` build — summary: success/fail, total bundle size, warnings only

### Python

- [ ] `pip install` — strip download progress, show: added N, already-satisfied N, errors
- [ ] `ruff` — group violations by file, count by rule

### Rust

- [ ] `cargo build` / `cargo check` — strip "Compiling" lines, show only errors + warnings
- [ ] `cargo test` — summary + only failed test details
- [ ] `cargo clippy` — group by lint, strip redundant code spans

### Docker

- [ ] `docker build` — strip layer-by-layer output, show: success/fail, image ID, warnings
- [ ] `docker ps` — compact format: name, status, ports only

## Agent integrations (research)

Agents the install wizard cannot auto-configure yet — they fall back to printed
manual instructions. Research deeper before they become scriptable targets:

- [ ] `opencode` — no documented config mechanism to inject env (`TOKEN_SAVER=1`) into tool subprocesses; only `{env:...}` substitution exists. Research: plugin/hook API or upstream feature request
- [ ] `cursor cli` — shell execution + env injection mechanism unknown (config docs are JS-rendered). Research: how cursor-agent spawns commands and where env can be set, global + project
