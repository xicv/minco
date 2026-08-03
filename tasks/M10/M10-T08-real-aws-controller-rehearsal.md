---
id: M10-T08
title: Run a bounded real-AWS controller promotion and rollback rehearsal
milestone: M10
status: in_progress
priority: critical
area: deployment/aws/recovery
depends_on: [M10-T04, M10-T05, M10-T06, M10-T07]
operations: [getLive, getReady, placeOrder, getOrder]
owned_paths:
  - crates/minco-deploy-aws/**
  - crates/minco-release/**
  - crates/minco-cli/**
  - crates/minco-plan/**
  - docs/deployment/**
  - docs/reference/generated/cli.md
  - docs/reference/generated/diagnostics.md
  - infra/aws/**
  - scripts/aws/**
  - tasks/M10/M10-T08-real-aws-controller-rehearsal.md
  - verification/aws-rehearsal/**
  - verification/adoption-measurements.json
  - verification/deep-review.json
  - verification/source-manifest.json
  - verification/static-validation.json
checks:
  - ./scripts/quality.sh
  - scripts/aws/validate.sh
  - scripts/dev/rustack-smoke.sh
  - scripts/aws/run-bounded-root-bootstrap.sh
---

## Goal

Prove the M10 controller path in one disposable, approved non-production AWS
boundary: exact release and change-set review, apply, hosted verification,
promotion, compatibility-checked rollback to an exact prior artifact, fresh
verification, traffic restoration and complete teardown.

## Authority gate

`ready` records only that source dependencies are complete. Before the first
AWS API call, the operator must explicitly approve the exact non-production
account, Region, role/profile, environment, database boundary, resource
allowlist, maximum duration/spend and whole-run cleanup blast radius. An old
login or approval for another task is not authority for this rehearsal.

## Acceptance

- the exact task head passes complete local quality and hosted qualification
  before provider mutation;
- account, Region, role, environment, source, release, migration, change-set,
  destructive-action and operator-approval guards fail closed;
- one exact candidate is applied, passes all required hosted checks and is
  promoted without rebuild or replan;
- rollback assessment binds the exact current and prior releases and never
  promises SQL reversal or automatic data repair;
- the exact prior artifact is redeployed as candidate, receives a new hosted
  verification report and is promoted through the same guarded boundary;
- runtime identity, request IDs, artifact digests, receipts and provider touch
  classes are retained in redacted form without account IDs, ARNs, endpoints,
  parameter names, credentials, tokens or customer data;
- cleanup proves every run-owned compute, API, identity, storage, database,
  network, log and local credential boundary absent before M10 can close;
- any source defect found by the rehearsal is fixed with a red regression and
  the exact candidate is requalified before another provider run.

## Non-goals

- production or persistent-staging deployment;
- automatic promotion, rollback, SQL reversal or data repair;
- changing an application-owned DNS name, certificate or shared database;
- publishing crates, tags, releases or the documentation site;
- claiming canary or static-site provider proof unless separately included in
  the approved resource and cost boundary.

## Evidence

Provider execution has not run and remains blocked on the authority gate above.

Local preflight on 2026-08-03 found that the bounded runners previously relied
on an out-of-band review statement and could reach STS without a digest-bound
account, role/profile, source, database, resource, duration/spend and cleanup
approval. A red-first shell regression now proves that the direct runner, root
bootstrap and account inspection fail before build or AWS contact when that
authority is absent. The exact local document is schema-closed, expires within
24 hours, accepts only three fixed resource/cleanup profiles, limits new work to
60 minutes, preserves cleanup authority after expiry and writes only a redacted
receipt. Caller account and role are rechecked after STS without retaining them
in the authority receipt.

Local non-provider evidence currently passes:

- `scripts/aws/validate.sh`, including the authority regression, static
  validation and real SAM lint;
- `scripts/dev/rustack-smoke.sh` for S3, SQS, SSM, STS and the Minco adapters;
- `cargo minco deploy plan --environment dev --json --stdout`, retaining the
  no-NAT, no-fixed-compute, no-provisioned-concurrency and no-schedule plan;
- `cargo minco rollback --dry-run --json`, which made no AWS contact and failed
  closed on the intentionally absent current and target promotion receipts;
- Bash syntax and ShellCheck for every AWS script.

The remaining source-design gate is the multi-release rehearsal boundary. The
current bounded runner creates, verifies and promotes one release, then cleans
the stack immediately. It cannot yet establish a prior live release, promote
the current release, assess their exact evidence chains, redeploy the prior
artifact from its clean source checkout, reverify it and restore traffic in the
same stack before teardown. Do not weaken source provenance or reuse a
historical hosted report to bypass that gate. Complete local and hosted quality,
the closed multi-release design, exact provider authority and the live evidence
remain required before this task can complete.

The first post-merge multi-release slice now makes rollback assessment
explicitly multi-root. Current and prior promotion chains stay in separate
absolute, existing, non-symlink clean checkouts that resolve to canonical paths;
a complete assessment verifies each checkout is at the exact source revision
sealed by its release. Dry-run is still local-only and names both roots while
explicitly prohibiting historical hosted-report reuse. Red-green CLI coverage
proved the new arguments, canonical root reporting and rejection of relative
roots. The remaining controller work must parent the shared provider resources,
phase-specific immutable evidence and one cleanup trap before the single-release
runner's immediate cleanup can be relaxed.
