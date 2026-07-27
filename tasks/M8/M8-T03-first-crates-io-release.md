---
id: M8-T03
title: Complete crates.io ownership and trusted publishing
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
  - uv run --locked python scripts/validate_publish.py --check-registry --require-registry
  - scripts/release/publish.sh --skip-quality
  - gh workflow view publish-crates.yml
---

## Goal

Complete the remaining ecosystem-resilience work after the successful 0.1.0,
0.1.1, 0.2.0, 0.3.0, and 0.3.1 releases: add trusted co-maintainer or restricted
team ownership to every current package and configure the protected GitHub OIDC
trusted publisher.

## Safety

Crates.io ownership and publisher changes affect every future release. Resolve
the exact 24-package inventory from workspace metadata, verify the requested
owner/team and protected GitHub environment, and keep upload testing dry-run
unless a separate release task explicitly authorizes publication.

## Progress

The 0.3.1 lock-step family contains 24 published packages owned by `xicv`.
Checksums, non-yanked state, external consumer installation, CLI installation,
and docs.rs routes are recorded in `VERIFICATION.md`. Co-maintainer/team
ownership and protected OIDC trusted publishing remain open, so this task stays
active.

No crate upload is required to close the ownership/configuration work. A later
release must independently qualify the exact tag and may use the trusted
publisher only after its configuration is verified.
