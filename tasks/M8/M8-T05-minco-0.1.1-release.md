---
id: M8-T05
title: Publish Minco 0.1.1 with cargo-minco docs.rs support
milestone: M8
status: complete
priority: critical
area: release/crates-io
depends_on: [M8-T04]
operations: []
owned_paths:
  - Cargo.toml
  - Cargo.lock
  - CHANGELOG.md
  - VERIFICATION.md
  - docs/development/publishing.md
  - tasks/M8/M8-T03-first-crates-io-release.md
  - tasks/M8/M8-T05-minco-0.1.1-release.md
checks:
  - python3 scripts/validate_publish.py --check-registry --require-registry
  - cargo rustdoc -p cargo-minco --lib --all-features --locked
  - scripts/quality.sh
  - scripts/release/publish.sh
  - scripts/release/publish.sh --execute
---

## Goal

Publish the lock-step `0.1.1` crate family from an exact reviewed tag so the
`cargo-minco` docs.rs target and its regression gates are available publicly.

## Release boundary

This patch release contains the `M8-T04` documentation-target fix already
merged to `main`. It does not add or change CLI behavior or broaden the public
API beyond the README-backed library documentation target.

## Safety

Run the full local and hosted gates against the exact release commit before
tagging. Cargo multi-package publication is non-atomic: after any interrupted
upload, query crates.io and publish only the missing `0.1.1` packages without
attempting to replace accepted versions.

## Acceptance

- All 14 manifests resolve to lock-step version `0.1.1`.
- The complete quality suite and multi-package publish dry run pass.
- Exact-head hosted CI passes before merge and exact merged-main CI passes
  before tagging.
- All 14 versions are accepted by crates.io, not yanked, and owned by `xicv`.
- The `cargo-minco` docs.rs library route succeeds and a public locked install
  reports version `0.1.1`.

## Evidence

- PR `#5` head `23afb15d8b2ec71baa5da203467fca9d7969be01`
  passed hosted run `30069887615`.
- Merge commit `3da298c094ef515a68dcc18ee6a2b867dcd4889e`
  passed merged-main hosted run `30070145165`.
- Remote tag `v0.1.1` resolves exactly to that merge commit.
- Cargo accepted all 14 `0.1.1` packages without a partial failure.
- Every exact registry record is not yanked, its downloadable archive matches
  the registry SHA-256 checksum, and `cargo owner --list` reports owner `xicv`.
- `cargo install cargo-minco --version 0.1.1 --locked` succeeds from crates.io;
  both direct and Cargo-subcommand argument shapes report `minco 0.1.1`.
- Every `0.1.1` library documentation route returns directly with HTTP 200.
  In particular, docs.rs renders `cargo_minco 0.1.1` from the new library
  target with the README-backed usage documentation.
