---
id: M7-T01
title: Validate Minco against bounded GarmentIQ and CGSP slices
milestone: M7
status: complete
priority: critical
area: stabilization
depends_on: [M6-T10, M9-T07, M10-T05]
operations: []
owned_paths:
  - docs/adoption/**
  - roadmap/roadmap.yaml
  - tasks/M7/M7-T01-two-app-validation.md
  - tasks/M7/M7-T02-garmentiq-contract-evidence.md
  - verification/adoption-measurements.json
  - verification/deep-review.json
  - verification/source-manifest.json
  - verification/static-validation.json
checks:
  - cargo test --workspace --all-targets --all-features --locked
  - uv run --locked python scripts/validate_static.py
  - uv run --locked python scripts/source_manifest.py --check
---

## Goal

Assess separately authorized, bounded real Minco slices from GarmentIQ and CGSP
before stabilizing public framework APIs. Product repositories remain read-only
to this Minco task; product adoption work is owned and reviewed separately.

## Acceptance

- exact Minco and product revisions are recorded;
- application evidence covers contract, runtime, persistence, deployment,
  rollback/removal, and operational boundaries;
- reusable gaps become one bounded Minco task each;
- product-specific behavior remains application policy;
- missing live/provider proof remains explicit.

## Non-goals

- modifying GarmentIQ, CGSP, or another product repository;
- declaring application migration complete from Minco-only fixtures;
- weakening framework boundaries to reproduce product architecture.

## Completion evidence

Completed on 2026-08-03 in the isolated `minco-task-m7-t01` JJ workspace
against Minco main `1430d0512fcd94e7fadea76bf2ab4f6d1aa391f1`.

- The read-only review pinned CGSP main
  `020a18a837233ea7cc08d53f1c35fedcf6dfcb41` and GarmentIQ `origin/main`
  `23a347f3ceb437a66f8d07b5db8bb652b8ab68d3`. The dirty, stale CGSP local
  workspace and GarmentIQ's 13 local-only commits were excluded from product
  claims.
- CGSP pins published Minco `=0.6.0`, whose tag resolves to
  `2c4605b7d4abcd865035196ffc0484c4a0e82f1e`. Its exact operation inventory has
  49 operations, including 15 operations across three complete Minco resource
  families. Domain and application crates remain Minco-free.
- CGSP's exact PR #123 head passed its hosted essential and PostgreSQL checks.
  Existing product evidence keeps HTTP on the legacy runtime, Plan/SAM
  advisory, product SQLx/RLS authoritative, and the HTTP/SQS observation,
  rollback rehearsal, production recovery, and bridge removal gates explicit.
- GarmentIQ's exact tree has 25 OpenAPI operations and zero Minco references.
  Its PR #50 and merge-SHA foundation/database workflows were green, but those
  are native product-quality and rollback controls rather than Minco adoption
  evidence.
- The comparison is recorded in
  `docs/adoption/two-application-validation-2026-08-03.md`. It separates
  compiler, hosted, historical live-provider, deployment and rollback evidence,
  retains product policy outside Minco, and identifies no justified new core
  abstraction.
- `M7-T02` records the remaining externally gated contract-only GarmentIQ
  evidence. M7 therefore remains active and M12 remains correctly blocked; this
  task does not claim two-application adoption or a compatibility freeze.

No GarmentIQ or CGSP file, AWS resource, database, deployment, release, tag,
registry entry or documentation site was changed by this task.
