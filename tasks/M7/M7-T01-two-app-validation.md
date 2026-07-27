---
id: M7-T01
title: Validate Minco against bounded GarmentIQ and CGSP slices
milestone: M7
status: planned
priority: critical
area: stabilization
depends_on: [M6-T10, M9-T07, M10-T05]
operations: []
owned_paths:
  - docs/adoption/**
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
