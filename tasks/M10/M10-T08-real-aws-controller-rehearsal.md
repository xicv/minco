---
id: M10-T08
title: Run a bounded real-AWS controller promotion and rollback rehearsal
milestone: M10
status: ready
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

Not run. Provider execution remains blocked on the authority gate above.
