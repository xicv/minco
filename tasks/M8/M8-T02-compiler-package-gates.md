---
id: M8-T02
title: Run compiler and crates.io package gates
milestone: M8
status: complete
priority: critical
area: release/crates-io
depends_on: [M8-T01]
operations: []
owned_paths:
  - Cargo.lock
  - verification/**
checks:
  - cargo generate-lockfile
  - cargo fmt --all -- --check
  - cargo check -p minco --no-default-features --locked
  - cargo check -p minco --locked
  - cargo check -p minco --all-features --locked
  - cargo check -p cargo-minco --locked
  - cargo test -p minco --no-default-features --locked
  - cargo test -p minco --locked
  - cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
  - cargo test --workspace --all-targets --all-features --locked
  - scripts/test/generated_apps.sh
  - cargo doc --workspace --all-features --no-deps --locked
  - scripts/release/publish.sh
  - scripts/test/generated_apps.sh
---

## Goal

Generate and review the dependency lockfile, compile every supported facade
feature shape, run the full Rust quality suite, and execute Cargo's complete
multi-package publication dry run on the pinned Rust 1.97.1 toolchain.

## Non-goals

This task does not upload any crate and does not create a crates.io token.

## Acceptance

Every command succeeds without `--allow-dirty`, `--no-verify`, ignored tests,
or suppressed meaningful lints. Packaged `.crate` contents and sizes are
reviewed before this task is marked complete.
