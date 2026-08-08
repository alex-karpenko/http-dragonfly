---
name: verify
description: Run the full local CI-equivalent check for http-dragonfly (build, test, fmt, clippy, pre-commit) before pushing or considering a change done. Use when the user asks to verify, check everything passes, or confirm CI will pass.
---

Run these in order and report pass/fail for each. Stop and report immediately if one fails — don't run the rest until it's fixed, except where noted.

1. `cargo clean`
2. `cargo build`
3. `cargo test` — if it fails on insta snapshot mismatches, show the diff and ask before running `cargo insta accept`.
4. `cargo fmt --all -- --check`
5. `cargo clippy --all-targets -- -D warnings`
6. `pre-commit run --all-files`

Steps 1-4 mirror exactly what `.github/workflows/ci.yaml` runs on every PR. Step 5 runs the local dev-workflow gate from `.pre-commit-config.yaml` (trailing whitespace, check-yaml/toml/json, large-file/secret detection, plus `cargo fmt --` and `cargo check` again).

Summarize results at the end: what passed, what failed, and what was fixed.
