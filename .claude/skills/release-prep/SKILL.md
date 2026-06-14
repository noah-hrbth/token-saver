---
name: release-prep
description: Prepare a new token-saver release — validates a clean main, summarizes commits since the last tag, proposes the next semantic version, runs the CI gates (fmt/clippy/test) plus the release and cross-target builds, then pre-bumps Cargo.toml + Cargo.lock and stages them. Does NOT commit, tag, or push — those stay a manual step requiring user confirmation.
allowed-tools: [Read, Edit, Bash, Grep]
---

# Release Prep

Goal: validate release readiness, propose a version bump, and stage the version change so the user only has to commit, tag, and push. **Never `git commit`, `git tag`, or `git push` yourself, and never edit the Homebrew tap** — those actions affect history and remotes and are out of scope. token-saver releases are tag-driven: pushing a `v*` tag triggers `.github/workflows/release.yml`, which builds the binaries and bumps the tap. This skill prepares the commit that the tag will point at; it does not cut it.

## Procedure

### 1. Confirm clean state

```bash
git status --porcelain
git rev-parse --abbrev-ref HEAD
```

Abort with a clear message if there are uncommitted changes or the branch is not `main`. Releases cut from a clean `main`. (Step 6 will intentionally dirty the tree at the very end — that is the only change this skill should introduce.)

### 2. Identify the current and previous tag

```bash
git describe --tags --abbrev=0     # latest tag, e.g. v0.4.0
git tag --sort=-v:refname | head -5
```

Read the current crate version too — it should match the latest tag if the last release was prepped cleanly:

```bash
grep '^version' Cargo.toml
```

If `Cargo.toml` already shows a version *ahead* of the latest tag, a release commit may have been prepped but not tagged. Report this and stop so the user can decide, rather than double-bumping.

### 3. Summarize commits since the last tag

```bash
git log <last-tag>..HEAD --no-merges --pretty=format:'%h %s'
```

Group the commits into a changelog. token-saver commit subjects are only loosely conventional, so classify each one:

- If the subject has a conventional prefix `type(scope)?!?:`, use the type:
  - `feat` → **Features**
  - `fix` → **Bug fixes**
  - `refactor` → **Refactors**
  - `perf` → **Performance**
  - `chore(deps)` / `build(deps)` → **Dependencies**
  - `docs` / `test` / `ci` / `style` / bare `chore` → **Chores**
- If the subject is free-form (most commits here are), classify by its leading verb:
  - `add` / `implement` / `introduce` / `support` → **Features**
  - `fix` / `correct` / `harden` / `prevent` / `guard` → **Bug fixes**
  - `rename` / `extract` / `refactor` / `restructure` / `simplify` → **Refactors**
  - anything else → **Other**

Exclude merge commits (already dropped by `--no-merges`) and any `chore(release):` bump commits from the changelog. Note any subject containing `!:` or a `BREAKING CHANGE:` body — those drive the version decision in step 4.

### 4. Propose the next version

token-saver is pre-1.0 (currently `0.y.z`), so the minor digit absorbs both features and breaking changes:

- Any breaking/incompatible change — a changed or removed CLI flag/subcommand, altered default behavior, `feat!:`, or `BREAKING CHANGE:` — **or** any new user-facing capability (a new compressor, a new subcommand) → bump **minor** (`0.4.0` → `0.5.0`). This matches the project's history: each feature batch shipped as a minor.
- Only bug fixes, internal hardening, refactors, or dependency bumps with no new capability and no breaking change → bump **patch** (`0.4.0` → `0.4.1`).
- No releasable commits since the last tag (only merges and `chore(release):`) → no release; report and stop.

State the proposed version and the one-line rule that produced it.

> Once the project reaches `1.0.0`, switch to standard semver: breaking → major, feat → minor, fix → patch. Update this step then.

### 5. Validate (run the CI gates locally, before bumping)

Run the same gates CI enforces on the current tree. Surface any failure verbatim and **stop without bumping** if any fail — never prep a release on a red tree.

```bash
cargo fmt --check
cargo clippy --locked -- -D warnings
cargo test --locked
```

These mirror `.github/workflows/ci.yml`'s `check` job. The `--locked` test run is also what `release.yml` gates the binary upload on.

Then validate the additional release targets that CI builds. The Apple Intel cross-build mirrors `ci.yml`'s `build-darwin-x86` job and only runs if the target is installed:

```bash
if rustup target list --installed | grep -q x86_64-apple-darwin; then
  cargo build --locked --target x86_64-apple-darwin   # a failure here is a real failure — surface it
else
  echo "x86_64-apple-darwin target not installed — skipping (add with: rustup target add x86_64-apple-darwin)"
fi
```

> Coverage note: the `x86_64-unknown-linux-gnu` release target generally can't be built on macOS without cross-tooling, so its compile is left to CI. State this in the report so the gap is explicit.

### 6. Pre-bump the version and stage it

Only reach this step if every gate in step 5 passed. Apply the bump to `Cargo.toml` (use the Edit tool — change the `version = "..."` line to the proposed version), then refresh the lockfile and confirm the release profile builds:

```bash
cargo build --release          # refreshes Cargo.lock's token-saver entry + validates the lto/strip release profile
git diff --stat                # expect ONLY Cargo.toml and Cargo.lock changed, one line each
git add Cargo.toml Cargo.lock
cargo build --release --locked # final proof the locked release build (what CI runs) passes with the staged lockfile
```

If `git diff --stat` shows anything other than the one-line `Cargo.toml` + `Cargo.lock` change, stop and report — `cargo build` should not have touched dependency versions. Leave the change **staged but uncommitted**. Do not commit.

### 7. Report

```
# Release prep: <proposed-version>

## Branch: main (clean)
## Last tag: <vX.Y.Z>   Crate version: <was X.Y.Z → now staged at X.Y.Z+bump>
## Commits since: N

### Features
- <hash> <subject>

### Bug fixes
- <hash> <subject>

### Refactors
- <hash> <subject>

### Dependencies / Chores / Other
- <hash> <subject>

## Proposed version: <vX.Y.Z'>
## Bump rule: <minor | patch> — <one-line justification>

## CI gates: fmt <pass/fail> · clippy <pass/fail> · test --locked <pass/fail>
## Release build: <pass/fail>  ·  x86_64-apple-darwin cross-build: <pass/skip/fail>
## Linux target (x86_64-unknown-linux-gnu): deferred to CI
## Version bump: Cargo.toml + Cargo.lock staged (uncommitted)
```

End with the exact commands the user runs to cut the release — **do not run them**. The bump is already staged, so the commit just needs to be made:

```bash
git commit -m "chore(release): <vX.Y.Z'>"
git tag <vX.Y.Z'>
git push origin main
git push origin <vX.Y.Z'>      # pushing the tag triggers .github/workflows/release.yml
```

Flag the Homebrew tap dependency: the `bump-tap` job in `release.yml` runs `scripts/update-tap.sh` using the repo secret `HOMEBREWTOKENSAVER` (a PAT with `contents:write` + `pull-requests:write` on `noah-hrbth/homebrew-token-saver`). It is a **repository secret, not a local env var** — there is nothing to set locally. But if it is missing or expired, the tag will still produce the GitHub release and uploaded binaries while the tap bump job fails. Suggest the user confirm the secret exists (`gh secret list` if available) before pushing the tag.
