---
id: M10-T04
title: Add rollback compatibility and optional canary aliases
milestone: M10
status: complete
priority: high
area: deployment/recovery
depends_on: [M10-T03]
operations: []
owned_paths:
  - crates/minco-deploy-aws/**
  - crates/minco-release/**
  - crates/minco-cli/**
  - infra/aws/**
  - docs/adrs/**
  - docs/DECISIONS.md
  - docs/deployment/**
  - verification/adoption-measurements.json
  - verification/deep-review.json
  - verification/source-manifest.json
  - verification/static-validation.json
  - tasks/M10/M10-T04-rollback-canary.md
checks:
  - cargo test -p minco-deploy-aws -p minco-release -p cargo-minco --all-features --locked
  - cargo run --locked -p cargo-minco -- rollback --dry-run
  - cargo run --locked -p cargo-minco -- promote --dry-run --canary
---

## Goal

Assess contract, configuration, resource, migration, and data compatibility
before routing traffic to an older artifact, and support optional alarm-guarded
Lambda alias canaries.

## Acceptance

- rollback reports compatible, incompatible, or operator-decision-required
  with exact reasons;
- arbitrary SQL reversal is never promised;
- canary configuration is opt-in and identifies additional cost/resources;
- pre/post-traffic verification and alarms can stop and reverse a shift;
- API and worker alias/event-source behavior is explicit.

## Non-goals

- hidden traffic shifting;
- automatic data repair;
- provisioned concurrency in the default minimal-idle profile.

## Review corrections

- A rollback does not reuse an old hosted-verification report. The exact older
  release artifact must first be redeployed as the current candidate without a
  rebuild or replan, then pass a new hosted verification before promotion.
- Data compatibility is a strict, release-bound operator decision document;
  an arbitrary file digest cannot authorize routing and no path reverses SQL or
  repairs data automatically.
- Historical promotions are reverified through the same exact deployment chain
  as live promotion, including the target-config/change-set receipt, AWS account,
  Region, role and stack; matching environment labels alone are insufficient.
- Canary routing changes only the API Lambda alias. Existing worker event-source
  mappings remain on their current artifacts and therefore require an explicit
  operator decision whenever worker artifacts differ.
- Canary metric alarms must already exist in the exact target account and Region
  and be `OK` before the shift. Composite alarms remain outside v1 because their
  distinct CloudFormation rollback-trigger type is not encoded by the ARN-only
  policy. CloudFormation rollback monitoring and an explicit
  routing-only cleanup change set bind the alarm and reversal evidence.
- The live executor repeats exact metric-alarm observation after the monitoring
  window; missing, `ALARM` or `INSUFFICIENT_DATA` post-traffic evidence forces
  verified cleanup/reversal instead of relying on CloudFormation's ALARM-only
  rollback trigger.
- A provider or network interruption that prevents routing verification leaves
  the immutable canary receipt in `started`, preserving an honest recovery
  boundary instead of claiming success or reversal without provider evidence.

## Completion evidence

Completed on 2026-08-03 in the isolated `minco-task-m10-t04` JJ workspace
against main parent `3ff1db1e9d26519ca8e677a39890e578bf6ee366`.

- Red-green-refactor coverage proves ordered rollback assessment across exact
  environment, contract, configuration, resource plan, migration, seed, data,
  API-routing and worker-artifact boundaries. Incompatibility dominates an
  operator decision, and uncertainty never authorizes routing.
- `cargo minco rollback --dry-run` is local-only, names missing evidence, and
  explicitly reports `reverse_sql: false` and `automatic_data_repair: false`.
  Complete assessment verifies both promotion/deployment receipt chains and
  every exact release/database-plan binding before producing a decision.
- Opt-in target policy seals bounded traffic percentage, monitoring duration,
  exact alarm ARNs, API-only routing, preserved workers and zero provisioned
  concurrency into the deployment evidence. The canary plan adds no resource
  or fixed-compute baseline and keeps external alarm pricing visibly incomplete.
- Live canary execution preflights the old and candidate Lambda configurations,
  exact alarm existence/state, then creates a routing-only CloudFormation change
  set with rollback monitoring. It verifies weighted alias state, observes the
  monitoring window, removes the weight through a second routing-only change
  set, and records terminal success or verified reversal in a concurrency-locked
  receipt before ordinary exact-artifact promotion can continue.
- `cargo test -p minco-deploy-aws -p minco-release -p cargo-minco --all-features
  --locked` passed: 70 CLI unit tests, 12 deploy CLI tests, 5 canary tests, 5
  rollback tests, 7 release tests, and all existing deployment suites. Focused
  Clippy with all targets/features and warnings denied passed, as did
  `cargo fmt --all -- --check`.
- Both required dry runs passed without AWS contact: rollback failed closed on
  missing local evidence, while canary promotion failed closed on missing exact
  release evidence, approval and opt-in target policy.
- Security review found no secret values, shell-expanded provider commands,
  implicit worker rerouting, SQL reversal, automatic data repair, or unverified
  terminal canary state. Provider arguments remain structured and bounded, and
  account/Region/stack/function/alias/version/alarm identities are exact.
- Documentation and ADR 0029 cover the recovery decision table, strict data
  evidence, exact older-artifact redeployment, alarm prerequisites, pre/post
  traffic proof, API-versus-worker behavior, cost limits and manual recovery.

No AWS API, Lambda alias, CloudFormation stack/change set, alarm, worker route,
release, tag, registry, or documentation site was created, changed, or
published. Live rollback, canary and release operations remain separately
authorised.
