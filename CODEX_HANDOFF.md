# Minco adoption-readiness handoff

Date: 2026-07-26
Task: `M6-T06`
Published baseline: `0.1.1`
Workspace candidate: `0.2.0`

## Objective

Harden Minco before any GarmentIQ or CGSP migration by making duplicated
repository truth executable, moving HTTP header ownership to applications and
installed plugins, adding an opt-in SQS Lambda worker, strengthening OpenAPI
policy, measuring facade/artifact cost, and documenting incremental adoption.

## Working boundary

- repository only; no GarmentIQ or CGSP edits;
- JJ workspace `minco-adoption-readiness`;
- no AWS deployment or mutation;
- no crates.io upload, release tag, or `publish.sh --execute`;
- dry-run publication only;
- no runtime plugin scanning, dynamic loading, global locator, ORM, or hidden
  worker schedule.

## Canonical reading order

1. `AGENTS.md`
2. `tasks/M6/M6-T06-adoption-readiness.md`
3. `verification/repository-truth.toml`
4. `VERIFICATION.md`
5. `docs/architecture/adoption-readiness-review.md`
6. `docs/adoption/incremental-adoption.md`
7. `docs/development/adopting-existing-application.md`

## Completion rule

Do not call the candidate ready or open a non-draft pull request unless the
exact final source passes static truth/contract checks, format, all-feature
check/Clippy/tests/docs, generated consumer checks, security/dependency gates,
native artifact review, package inventory, and the complete multi-package Cargo
publish dry run. Preserve unavailable live/provider evidence as explicit gaps.

## Current evidence

- Base Git SHA: `6fe9121ea9284e2fa4e2dbfd76f21bd8a13e263a`.
- Candidate identity: immutable `source-tree-sha256` in
  `verification/source-manifest.json`, cross-checked against
  `verification/adoption-measurements.json`; record the final pushed Git SHA
  separately after transport.
- Repository truth: 29 workspace packages, 24 publishable packages, 16 catalog
  entries, 4 reference operations, 10 schemas, zero static errors/warnings.
- `./scripts/quality.sh`: passed after an earlier storage-exhaustion failure was
  corrected by sharing the Cargo target cache; that failed attempt is not
  evidence.
- Generated PostgreSQL and SQLite consumers: both compiled and tested.
- Browser: 38 Chromium/Firefox tests passed.
- Orders HTTP E2E, Plan generation, SAM rendering/lint, native Orders/worker
  ARM64 packaging, cargo-deny, cargo-audit, npm audit and Gitleaks: passed.
- Docker-backed PostgreSQL/Rustack refresh: environment-blocked because the
  local shared Docker daemon did not answer read-only status calls.
- Context7 current-doc lookup: quota-blocked; local resolved source/CLI help was
  used instead.
- Package publication remains dry-run only; no AWS, registry or tag mutation is
  authorized.

The draft PR and hosted exact-head run are recorded on the PR after the
immutable source is pushed; they are not embedded here because doing so would
change the head they qualify.
