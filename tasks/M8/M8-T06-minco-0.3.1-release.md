---
id: M8-T06
title: Publish Minco 0.3.1 text-only and SQLx isolation patch
milestone: M8
status: complete
priority: critical
area: release/crates-io
depends_on: [M6-T08, M6-T09]
operations: []
owned_paths:
  - Cargo.toml
  - Cargo.lock
  - CHANGELOG.md
  - README.md
  - VERIFICATION.md
  - docs/adoption/incremental-adoption.md
  - docs/development/publishing.md
  - docs/development/using-minco-crate.md
  - tasks/M8/M8-T06-minco-0.3.1-release.md
  - verification/**
checks:
  - uv run --locked python scripts/validate_publish.py --check-registry --require-registry
  - cargo rustdoc -p cargo-minco --lib --all-features --locked
  - scripts/quality.sh
  - npm run --prefix plugins/minco-plugin-feedback test:browser
  - scripts/test/e2e.sh
  - scripts/dev/rustack-smoke.sh
  - scripts/aws/plan.sh
  - scripts/aws/validate.sh
  - scripts/aws/build-lambda.sh
  - scripts/release/package-list.sh
  - scripts/release/publish.sh --skip-quality
  - scripts/release/publish.sh --execute --skip-quality
---

## Goal

Publish the lock-step `0.3.1` crate family from an exact reviewed tag with the
already-merged text-only Feedback boundary and SQLx backend feature isolation.

## Release boundary

This patch is compatible with the public Rust APIs and serialized contracts of
`0.3.0`. It contains no multi-runtime Plan IR redesign, no new package, and no
product-specific behavior. The release inventory remains 24 packages.

## Safety

Run complete local and hosted gates against the exact release commit before
tagging. Confirm every exact `0.3.1` version is absent immediately before
publication. Cargo multi-package publication is non-atomic: after any
interrupted upload, query crates.io and publish only the missing packages
without attempting to replace accepted versions.

## Acceptance

- All 29 workspace packages and 24 versioned public dependencies resolve to
  lock-step version `0.3.1`.
- Exact-head local and hosted quality, browser, E2E, Rustack, AWS
  plan/validation/Lambda package, docs.rs-shaped Rustdoc, archive inventory,
  and all-package Cargo dry-run gates pass.
- Exact merged-main hosted qualification passes before tag creation.
- Remote tag `v0.3.1` resolves to that qualified merge commit.
- All 24 versions are accepted by crates.io, not yanked, owned by `xicv`, and
  match the registry archive checksums.
- A fresh locked `cargo-minco 0.3.1` install succeeds from crates.io and all
  exact docs.rs routes become available.

## Evidence

- Source-fix PR `#16` head
  `75690770cbd38fc96784add6076d14084d81d176` passed hosted run
  `30247184469`; merge commit
  `cd679c74d44e04abe1655b71c8ca9b9381aa6f6b` passed merged-main run
  `30247725599`.
- Release PR `#17` exact head
  `36b52a18893aded72284601503272fa0b444a403` passed hosted run
  `30249418058`; merge commit
  `33719376b634e995c0bfdbe6c215f1c304cd6b5d` passed merged-main run
  `30249977158`.
- Remote tag `v0.3.1` resolves exactly to the release merge commit. Cargo
  accepted all 24 uploads in dependency order without a partial failure.
- Trusted-publisher run `30250487113` passed through the complete publish dry
  run, then failed before upload because crates.io had no trusted-publisher
  configuration for `xicv/minco`. Publication used the documented authenticated
  fallback from a clean detached worktree at the exact tag.
- Every exact registry record is not yanked, every downloaded archive matches
  its crates.io SHA-256 checksum, and every package includes owner `xicv`.
- A fresh locked `cargo-minco 0.3.1` install reports `minco 0.3.1`. A fresh
  external `minco = "=0.3.1"` consumer checks successfully on the declared
  Rust 1.97.1 toolchain; Rust 1.91.0 is correctly rejected by package metadata.
- All 24 exact docs.rs library routes return HTTP 200 directly. The final
  `minco` facade build reports that all builds succeeded.
