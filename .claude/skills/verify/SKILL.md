---
name: verify
description: Run all checks — formatting, linting, and tests — to verify the codebase is clean before marking work done
---

Run the following commands in sequence, stopping on the first failure:

1. `cargo fmt --check` — verify formatting
2. `cargo clippy -- -D warnings` — lint with warnings as errors
3. `cargo test` — run all unit and integration tests

If any step fails, fix the issue and re-run from that step. Only report success when all three pass.
