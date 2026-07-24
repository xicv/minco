---
id: M8-T03
title: Publish Minco 0.1.0 and configure trusted publishing
milestone: M8
status: active
priority: critical
area: release/crates-io
depends_on: [M8-T02]
operations: []
owned_paths:
  - CHANGELOG.md
  - VERIFICATION.md
  - docs/development/publishing.md
  - .github/workflows/publish-crates.yml
checks:
  - python3 scripts/validate_publish.py --expect-unpublished --require-registry
  - scripts/release/publish.sh --execute
---

## Goal

Publish the first immutable crate-family release from the reviewed `v0.1.0`
tag, verify every package on crates.io and docs.rs, add co-maintainer ownership,
and then configure the protected GitHub OIDC trusted publisher.

## Safety

Crates.io uploads are permanent. Recheck name availability immediately before
the first upload, publish only from the tagged release, and never attempt to
replace an accepted version after a partial multi-package failure.

## Progress

All 14 Minco packages were published at `0.1.0` on 2026-07-24 under owner
`xicv`. Public installation succeeds. The `cargo-minco` archive is usable but
its docs.rs build exposed the missing library documentation target tracked by
`M8-T04`. Co-maintainer ownership and protected OIDC trusted publishing remain
open, so this task stays active.
