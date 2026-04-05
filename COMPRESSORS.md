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

## Planned

### General (cross-ecosystem)

- [ ] `git blame` — group consecutive lines by commit, deduplicate metadata
- [ ] `head` / `tail` — pass through (hook point for future structure-aware truncation)
- [ ] `curl` — strip response headers, summarize status code + content-type, pass body (mac/linux only)

### JavaScript / TypeScript

- [ ] `tsc --noEmit` / `npx tsc` — group errors by file, dedupe paths, strip redundant location info
- [ ] `eslint` — group by file, count warnings vs errors, collapse fixable violations
- [ ] `jest` / `vitest` — summary only (X passed, Y failed), list only failures with context
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
