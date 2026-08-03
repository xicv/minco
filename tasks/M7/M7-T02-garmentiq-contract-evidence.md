---
id: M7-T02
title: Record a bounded GarmentIQ contract-only Minco slice
milestone: M7
status: planned
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
