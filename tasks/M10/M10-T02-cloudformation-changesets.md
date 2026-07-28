---
id: M10-T02
title: Add CloudFormation change sets and environment guards
milestone: M10
status: complete
priority: critical
area: deployment/aws
depends_on: [M10-T01]
operations: []
owned_paths:
  - Cargo.toml
  - Cargo.lock
  - crates/minco-deploy-aws/**
  - crates/minco-cli/**
  - infra/aws/**
  - scripts/aws/**
  - docs/DECISIONS.md
  - docs/adrs/**
  - docs/deployment/**
  - docs/development/publishing.md
  - docs/reference/cli.md
  - verification/repository-truth.toml
  - verification/static-validation.json
  - verification/publish-validation.json
  - verification/deep-review.json
  - verification/rust-dependency-hygiene.json
  - verification/source-manifest.json
  - verification/adoption-measurements.json
  - tasks/M10/M10-T02-cloudformation-changesets.md
checks:
  - cargo test -p minco-deploy-aws -p cargo-minco --all-features --locked
  - cargo clippy -p minco-deploy-aws -p cargo-minco --all-targets --all-features --locked -- -D warnings
  - cargo minco deploy changeset --dry-run
  - sam validate --lint --template-file infra/aws/generated/template.yaml
---

## Goal

Create and inspect CloudFormation change sets only after exact account, Region,
environment, role, release-manifest, drift and clean-source guards pass.
Execute the separately approved change set only after an exact successful
migration plan/receipt guard also passes.

## Acceptance

- plan and change-set creation are distinct from apply;
- additions, modifications, replacements, and deletions are classified;
- unexpected account/region/environment or missing operator approval fails
  closed;
- secret values never enter command output, template, or receipt;
- read-only and bounded live-AWS tests are separated from mutating rehearsal.

## Non-goals

- claiming a change set guarantees runtime success;
- bypass flags for dirty or unreviewed source;
- replacing SAM/CloudFormation as the default renderer.

## Review corrections

- The original ownership omitted the root workspace manifests required to add
  the task's explicitly named `minco-deploy-aws` crate. Those bounded paths are
  now explicit.
- The original goal compressed preview and apply guards even though the
  framework golden path creates the change set before database plan/migrate.
  The task now states that migration evidence gates apply, not the earlier
  non-executing provider preview.

## Completion evidence

Completed on 2026-07-28 in the isolated `minco-task-m10-t02` JJ workspace
against merged-main parent `c0053479`.

- Red-green-refactor coverage proves strict deployment-target parsing; exact
  account, Region, environment, role, release, configuration, source, drift,
  migration and approval guards; complete CloudFormation action and
  replacement classification; digest-sealed immutable change receipts; and
  refusal of stale, tampered, schema-extended, mismatched or indeterminate
  apply evidence.
- `cargo minco deploy changeset --dry-run --json` and `cargo minco deploy apply
  --dry-run --json` exercised the local, non-contacting review paths. The
  provider-backed paths independently recheck identity, target state, release,
  source, drift, migration and the exact described change set immediately
  before mutation; change-set creation and execution remain separate commands.
- `cargo test -p minco-deploy-aws -p cargo-minco --all-features --locked`
  passed, including CLI integration tests and the CloudFormation CREATE
  `REVIEW_IN_PROGRESS` lifecycle regression.
- `cargo clippy -p minco-deploy-aws -p cargo-minco --all-targets
  --all-features --locked -- -D warnings`, `shellcheck -x
  scripts/aws/deploy.sh scripts/aws/run-bounded-smoke.sh`, and `sam validate
  --lint --template-file infra/aws/generated/template.yaml` passed.
- `./scripts/quality.sh` passed compilation, formatting, workspace tests,
  generated PostgreSQL and SQLite application checks, Rustdoc, dependency and
  license policy, `cargo audit`, package-lock audit, and secret scanning against
  the final deterministic source/adoption evidence chain.
- `python3 scripts/validate_publish.py` passed the repository's static
  publication contract. `cargo package -p minco-deploy-aws --locked
  --allow-dirty --no-verify` created the crate archive. Isolated package
  compilation is intentionally deferred to the coordinated next-version
  release: Cargo resolves the already-published `minco-release 0.3.1`, which
  predates M10-T01's `ReleaseEnvironment` and extended manifest fields. The
  unhidden verification attempt failed with `error[E0432]: unresolved import
  minco_release::ReleaseEnvironment`; this task does not widen into a
  workspace-wide version release.
- No AWS API was contacted and no change set or cloud resource was created.
