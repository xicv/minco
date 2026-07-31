---
id: M10-T07
title: Research zero-idle AWS profiles and cost evidence
milestone: M10
status: complete
priority: high
area: deployment/research
depends_on: [M10-T03]
operations: []
owned_paths:
  - docs/deployment/**
  - docs/adrs/**
  - crates/minco-plan/**
  - tasks/M10/M10-T07-zero-idle-service-research.md
  - verification/adoption-measurements.json
  - verification/deep-review.json
  - verification/source-manifest.json
  - verification/static-validation.json
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

## Evidence

Completed on 2026-07-31 in the isolated `minco-task-m10-t07` JJ workspace
against merged-main parent `5269d79134225924f13d77ef38a651bd0be2af2b`.

- Dated primary-provider evidence compares Aurora DSQL, DynamoDB on-demand,
  Aurora Serverless v2, Neon, RDS Data API and CloudFront commercial profiles
  across correctness, transactions, wake behavior, connections, quotas,
  Region/eligibility and residual cost. Australian Aurora DSQL single-Region
  availability is not misrepresented as Australian multi-Region support.
- A bounded SQLx `0.9.0`/Rust `1.97.1` DSQL probe compiled with `VerifyFull`
  TLS, runtime IAM-token-shaped credentials, zero minimum/one maximum
  connection, a 3,300-second maximum lifetime and separate DDL/DML
  transactions. It made no database connection. The probe found that SQLx
  `PgConnectOptions` `Debug` exposes its password; any future connector must
  wrap/redact token-bearing settings and prove the token cannot enter logs,
  plans, diagnostics or receipts.
- Plan cost output now records typed cost class and pricing confidence.
  DynamoDB proves request/storage dimensions, zero-ACU Aurora is distinct from
  fixed RDS, and Neon Free allowance is no longer reported as a complete
  zero-dollar estimate.
- Schema 2 one-time schedules can record completion deletion, residual
  resources and a manual fallback. Recurring cleanup is rejected and older
  schema 2 schedules remain serializable without the optional field.
- Real `sam validate --lint` attempts rejected both a SAM `ScheduleV2`
  property and an `AWS::Scheduler::Schedule` CloudFormation property because
  neither current schema exposes `ActionAfterCompletion`. The renderer now
  fails closed; a future Scheduler API operation needs an exact-plan guard and
  durable receipt.
- `cargo test -p minco-plan --all-features --locked` passes 14 unit and 41
  integration tests. `cargo minco deploy plan` emits no diagnostics.
  `cargo minco inspect --json`, `cargo minco explain getOrder --json`,
  `cargo fmt --all -- --check`, reverse-apply whitespace validation and
  `jj log -r 'conflicts()'` pass.
- `./scripts/quality.sh` passes the complete local repository gate: static,
  truth, publish and deep-review validation; browser tests; facade/workspace
  compiler, Clippy, tests and docs; generated PostgreSQL/SQLite applications;
  dependency/license/RustSec/npm audits; Gitleaks; and exact source identity.
  Native ARM64 Orders and SQS-worker artifacts were rebuilt from this workspace
  before refreshing adoption/source evidence.

A read-only `aws dsql list-clusters --region ap-southeast-2` found no cluster.
No AWS resource, database, deployment, hosted check, promotion, package,
release tag or publication was created or changed.
