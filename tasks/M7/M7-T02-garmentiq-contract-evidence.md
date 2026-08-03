---
id: M7-T02
title: Record a bounded GarmentIQ contract-only Minco slice
milestone: M7
status: complete
priority: critical
area: stabilization/adoption
depends_on: [M7-T01]
operations: []
owned_paths:
  - docs/adoption/**
  - tasks/M7/M7-T02-garmentiq-contract-evidence.md
  - verification/adoption-measurements.json
  - verification/deep-review.json
  - verification/source-manifest.json
  - verification/static-validation.json
checks:
  - uv run --locked python scripts/validate_static.py
  - uv run --locked python scripts/source_manifest.py --check
  - cargo test --workspace --all-targets --all-features --locked
---

## Goal

Record separately authorised, product-owned evidence for one real GarmentIQ
slice using Minco's published contract-only surface. The product change is
implemented and reviewed in GarmentIQ; this Minco task only evaluates the exact
result.

## Acceptance

- exact Minco, GarmentIQ, pull-request and hosted-check revisions are recorded;
- the product pins the published lock-step Minco family with only the smallest
  required contract feature surface;
- the existing canonical OpenAPI contract passes Minco policy without changing
  product behavior merely to satisfy a validator;
- operation ownership and existing TDD evidence are inspectable and bounded;
- domain code remains free of Minco, Axum, SQLx, Lambda and AWS dependencies;
- dependency inspection proves that contract-only adoption selects no Minco
  HTTP runtime, database adapter, AWS SDK or deployment provider;
- removal is a source-only dependency/tooling rollback with no data or provider
  mutation;
- product-local, hosted, live-provider and Minco evidence are reported as
  separate states.

## Non-goals

- modifying GarmentIQ from this Minco repository task;
- selecting Minco HTTP, persistence, Lambda, worker or deployment authority;
- changing GarmentIQ's product schema, permissions, workflow or AWS topology;
- treating native GarmentIQ CI or live evidence as proof of Minco adoption.

## External prerequisite

A separately authorised GarmentIQ task must implement and merge the bounded
contract-only slice first. Until that exact product evidence exists, this task
must remain `planned` and M7 must remain `active`.

## Activation

Activated on 2026-08-03 after separately authorised GarmentIQ PR
[`xicv/garmentiq#55`](https://github.com/xicv/garmentiq/pull/55) merged exact
qualified head `2262349989b0df8ad2d202092666e8aaed012b10` as
`8d6e8146a3954db11c64b264f1990f5b853c3192`. The product task, its local and
hosted qualification, and its merge were completed outside this Minco
workspace before this evidence-only task started.

## Qualification evidence

Completed from the isolated `minco-task-m7-t02` JJ workspace based on Minco main
`5ea607e7eebb`. Exact pre-closure head
`d7bb1802e043d410efddc5999f1a30ccea020df3` passed Minco hosted qualification
run [`30791657933`](https://github.com/xicv/minco/actions/runs/30791657933) in PR
[`#83`](https://github.com/xicv/minco/pull/83). The detailed human-readable
record is `docs/adoption/garmentiq-contract-only-2026-08-03.md`; the
corresponding machine-readable record is under
`downstream_application_adoption.garmentiq` in
`verification/adoption-measurements.json`.

- The published Minco tag, release, registry, GarmentIQ base, implementation,
  exact PR head, merge SHA and hosted run identities are recorded separately.
- Source inspection proves the exact `=0.6.0`, no-default, contract-only API
  dev dependency; the three-package Minco closure; the 25-operation inventory;
  the six existing idempotent operations; the Minco-free product domain; and
  source-only removal.
- GarmentIQ's product-local gate passed at implementation commit
  `697b98f32dba555d17f6f23f8df156f8b3650e44`. Exact qualified-head and
  merge-SHA hosted checks passed, including disposable PostgreSQL, browser
  session and real Axum/PostgreSQL HTTP coverage.
- `./scripts/quality.sh` passed the complete Minco local gate: repository,
  static, publish, deep-review, docs/link/browser, facade and feature-isolation,
  workspace test/Clippy/rustdoc, generated PostgreSQL/SQLite application,
  dependency-policy, Cargo deny/audit, npm audit, Gitleaks and exact
  source-manifest checks. Cargo audit found no vulnerability and retained the
  repository's explicit allowed warning for upstream `event-listener 5.4.1`.
- Deep review reported no error. Its existing heuristic findings remain two
  Rust unwrap/expect warnings, one SQLite migration `DROP TABLE` warning and one
  informational example error-boundary item; this evidence-only task does not
  change those source paths.

No GarmentIQ or CGSP file, database, AWS resource, deployment, release, tag,
registry entry or documentation site was changed by this Minco task. The
closure revision changes only this task record and deterministic evidence; a
second exact-head hosted qualification and final review remain required before
merge.
