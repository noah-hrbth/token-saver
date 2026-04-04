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

## Planned

- [ ] `git blame` — group consecutive lines by commit, deduplicate metadata
- [ ] `grep` / `rg` — compress repeated path prefixes, reduce context noise
- [ ] `git branch` — compact branch list with tracking info
