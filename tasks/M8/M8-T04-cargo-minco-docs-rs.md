---
id: M8-T04
title: Make cargo-minco documentation build on docs.rs
milestone: M8
status: complete
priority: high
area: release/crates-io
depends_on: [M8-T02]
operations: []
owned_paths:
  - .github/workflows/minco-manual.yml
  - crates/minco-cli/src/lib.rs
  - crates/minco-cli/src/main.rs
  - crates/minco-cli/README.md
  - scripts/validate_publish.py
  - scripts/quality.sh
  - scripts/release/publish.py
  - docs/development/publishing.md
  - tasks/M8/M8-T04-cargo-minco-docs-rs.md
checks:
  - python3 scripts/validate_publish.py
  - cargo rustdoc -p cargo-minco --lib --all-features --locked
  - cargo clippy -p cargo-minco --all-targets --locked -- -D warnings
  - cargo test -p cargo-minco --locked
---

## Goal

Give the `cargo-minco` package a useful library documentation target so the
docs.rs `cargo rustdoc --lib` build succeeds, while keeping the executable
installed as `cargo-minco`.

## Regression

The immutable `cargo-minco 0.1.0` archive contains only a binary target. Its
docs.rs build therefore fails with `no library targets found in package
\`cargo-minco\``. The corrected documentation target will first be available in
the next lock-step Minco release.

## Non-goals

This task does not change the CLI's command behavior, bump the workspace
version, publish a crate, or attempt to replace the immutable `0.1.0` release.

## Evidence

- `python3 scripts/validate_publish.py` reports zero errors and warnings.
- `cargo rustdoc -p cargo-minco --lib --all-features --locked` generates
  `target/doc/cargo_minco/index.html`.
- `cargo test -p cargo-minco --locked` passes the library, binary, and doc-test
  targets.
- `cargo clippy -p cargo-minco --all-targets --all-features --locked -- -D warnings`
  passes.
- `uv run --with pyyaml -- scripts/quality.sh` passes the complete workspace
  static, compiler, test, lint, and documentation suite.
