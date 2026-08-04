---
id: M12-T03
title: Close second-application adoption evidence
milestone: M12
status: complete
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

## Reconciliation result

- Minco candidate input
  `ee85e9a585195a71567b919e6d78a4250b956aa9` and source-tree SHA-256
  `ce27a74ca0ba5e00525d8b4b1398c7c42e35da9e4eab532208031f296ec49cf5`
  were compared with published 0.6.0 and exact downstream revisions.
- Current CGSP remote main
  `cea46f4608a105cb5507ad371340e92a95d41ac3` advances the prior M7 snapshot
  to 64 operations, including 30 operations across six complete resource
  families. Its exact source/hosted evidence and product-recorded staging
  evidence are separated; this task did not contact its provider.
- Current GarmentIQ remote main remains
  `8d6e8146a3954db11c64b264f1990f5b853c3192`, the exact contract-only merge.
  Local-only commits were excluded, and no runtime/provider adoption is
  inferred.
- Contract, code, capability, resource/data, evidence, deployment,
  rollback/removal and cost effects are recorded in
  `docs/adoption/1.0-adoption-reconciliation-2026-08-05.md` and the redacted
  schema-1 record under `verification/adoption/`.
- `docs/reference/supported-matrix.md` turns the evidence into explicit
  candidate, application-evidenced and unsupported boundaries for M12-T04.
- Every temporary downstream bridge has a task/PR owner and deletion criteria.
  None is authorized for deletion by this Minco review.
- No product type, permission, workflow, schema, deployment controller or
  provider identifier was added to Minco core or the public evidence record.

No CGSP or GarmentIQ file, provider, database, deployment, release, tag or
registry entry was changed by this task.

## Qualification evidence

- `uv run --locked python scripts/validate_static.py`: PASS with zero errors
  and zero warnings.
- `cargo test --workspace --all-targets --all-features --locked`: PASS. The
  explicitly configured real-AWS, Rustack and Orders PostgreSQL cases remained
  ignored without their provider/test URLs; no provider claim is inferred.
- `uv run --locked python scripts/test/repository_truth.py`: PASS, 40 tests.
- Exact read-only Git/GitHub checks reproduced the two remote revisions,
  successful hosted run identities, CGSP's 64/34/30 operation counts, and PR
  #136 head/merge tree equality.
- Deep review reported no error and no new finding; its existing Rust/SQLite
  warnings and example-boundary information remain unchanged. Gitleaks found no
  leak.
- The separate repository-truth qualification child regenerated the
  deterministic reports and passed
  `uv run --locked python scripts/source_manifest.py --check` on the combined
  task tree.
