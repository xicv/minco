---
id: M8-T05
title: Publish Minco 0.1.1 with cargo-minco docs.rs support
milestone: M8
status: active
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
