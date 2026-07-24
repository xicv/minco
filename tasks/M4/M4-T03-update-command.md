---
id: M4-T03
title: Verify the source-workspace update command
milestone: M4
status: complete
priority: medium
area: maintenance
depends_on: [M0-T02]
operations: []
owned_paths:
  - crates/minco-cli/src/update.rs
  - crates/minco-cli/src/main.rs
  - docs/development/update.md
  - tasks/M4/M4-T03-update-command.md
checks:
  - cargo minco update check
  - cargo test -p cargo-minco
---

## Goal

Check pinned toolchains and dependencies without mutation, and require an explicit clean-workspace `--yes` flow before applying updates and rerunning gates.

## Evidence

On 2026-07-24, check mode completed four current-tool inspections (`rustup
check`, `cargo update --dry-run`, locked Cargo metadata and `jj version`) with
successful exit status while the SHA-256 of `Cargo.lock` remained unchanged.
The report omits the verbose Cargo metadata graph but retains the command and
success evidence.

Apply-mode regressions prove that `--yes` alone is not enough: at least one of
`--toolchain`, `--dependencies` or `--run-checks` must be explicitly selected.
The command also fails before mutation for a dirty JJ workspace, and its
cleanliness selector fails closed when neither JJ nor Git can prove workspace
state. Check-mode subprocess failures now fail the command instead of appearing
inside a successful report.

The full `cargo-minco` test suite, package Clippy with warnings denied, package
documentation, static validation and publish validation passed. Rustfmt was run
only against the two modified Rust files. No dependency or toolchain update was
applied.

A focused post-task review found no remaining correctness, mutation-safety,
command-injection or documentation issue within M4-T03 scope.
