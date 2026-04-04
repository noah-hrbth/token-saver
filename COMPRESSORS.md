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

## Planned

- [ ] `git blame` — group consecutive lines by commit, deduplicate metadata
- [ ] `git branch` — compact branch list with tracking info
