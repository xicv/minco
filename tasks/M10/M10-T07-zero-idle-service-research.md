---
id: M10-T07
title: Research zero-idle AWS profiles and cost evidence
milestone: M10
status: planned
priority: high
area: deployment/research
depends_on: [M10-T03]
operations: []
owned_paths:
  - docs/deployment/**
  - docs/adrs/**
  - crates/minco-plan/**
  - tasks/M10/M10-T07-zero-idle-service-research.md
checks:
  - cargo test -p minco-plan --all-features --locked
  - cargo minco deploy plan
---

## Goal

Use dated primary-provider evidence and bounded prototypes to decide whether
Plan IR needs a small structured extension for cost class, pricing confidence,
database profile and lifecycle cleanup.

## Acceptance

- Aurora DSQL, DynamoDB on-demand, Aurora Serverless v2, Neon and specialist
  RDS Data API profiles compare correctness, transactions, wake behavior,
  connections, Region/eligibility, quotas, storage and price dimensions;
- a DSQL experiment tests current Rust/SQLx connector behavior and documented
  DDL/DML/transaction limits without presenting it as a production adapter;
- CloudFront request/transfer and flat-rate profiles record eligibility and
  dated allowance behavior;
- one-time EventBridge cleanup records `ActionAfterCompletion=DELETE`, residual
  resources and manual fallback;
- cost-budget enforcement distinguishes structural facts from live,
  Region-specific or eligibility-dependent pricing;
- any schema extension is proven by at least two materially different profiles
  and preserves Plan IR schema compatibility policy.

## Non-goals

- shipping database or CloudFront adapters from research alone;
- a general cloud pricing engine;
- default schedules or unattended deletion;
- claiming free-tier or account eligibility;
- changing the `0.4.0` release boundary.
