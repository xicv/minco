---
id: M12-T03
title: Close second-application adoption evidence
milestone: M12
status: ready
priority: critical
area: stabilization/adoption
depends_on: [M7-T02, M10-T06, M11-T05]
operations: []
owned_paths:
  - docs/adoption/**
  - docs/reference/supported-matrix.md
  - verification/adoption/**
  - tasks/M12/M12-T03-adoption-completion.md
checks:
  - uv run --locked python scripts/validate_static.py
  - uv run --locked python scripts/source_manifest.py --check
  - cargo test --workspace --all-targets --all-features --locked
---

## Goal

Reconcile bounded real GarmentIQ and CGSP adoption evidence against the
completed golden path, close reusable Minco gaps in separate tasks, and record
the remaining product/provider boundaries before compatibility freeze.

## Acceptance

- two application slices identify exact Minco and product revisions;
- contract, code, capability, resource, evidence, rollback/removal, and cost
  effects are recorded;
- no product-specific type, permission, workflow, or schema enters core;
- external provider/live evidence is labelled accurately;
- temporary bridges have owners and deletion criteria.

## Non-goals

- modifying product repositories in this Minco task;
- declaring whole-product migration;
- converting application policy into a generic framework abstraction from one
  example.
