---
id: M8-T03
title: Complete crates.io ownership and trusted publishing
milestone: M8
status: complete
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
0.1.1, 0.2.0, 0.3.0, and 0.3.1 releases: record the explicit
single-maintainer ownership policy and configure the GitHub OIDC trusted
publisher for every current published package.

## Safety

Crates.io ownership and publisher changes affect every future release. Resolve
the exact 24-package inventory from workspace metadata, verify the sole-owner
policy and GitHub environment, and keep upload testing dry-run unless a
separate release task explicitly authorizes publication.

## Progress

The 0.3.1 lock-step family contains 24 published packages owned solely by
`xicv` under the explicit single-maintainer policy. Every package now has the
same trusted-publisher configuration for `xicv/minco`,
`publish-crates.yml`, and the `crates-io` environment.

No crate upload is required to close the ownership/configuration work. A later
release must independently qualify the exact tag and may use the trusted
publisher only after its configuration is verified.

## Evidence

- Authenticated crates.io read-back verified exactly one matching configuration
  for each of the 24 published packages and no conflicts.
- Hosted authentication-only run `30313972544` passed the short-lived-token and
  revocation steps while the complete release job was skipped.
- Independent post-run lookup kept every published package at `0.3.1` and
  returned HTTP 404 for the unpublished `minco-config` candidate.
- `./scripts/quality.sh`, `scripts/release/publish.sh --skip-quality`,
  `gh workflow view publish-crates.yml`, and the behavior-level workflow checks
  passed. The required registry validator returned the expected 24
  `PUBLISH-072` immutability errors for versions already published at `0.3.1`.
