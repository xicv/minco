---
id: M10-T02
title: Add CloudFormation change sets and environment guards
milestone: M10
status: planned
priority: critical
area: deployment/aws
depends_on: [M10-T01]
operations: []
owned_paths:
  - crates/minco-deploy-aws/**
  - crates/minco-cli/**
  - infra/aws/**
  - scripts/aws/**
  - docs/adrs/**
  - docs/deployment/**
  - tasks/M10/M10-T02-cloudformation-changesets.md
checks:
  - cargo test -p minco-deploy-aws -p cargo-minco --all-features --locked
  - cargo clippy -p minco-deploy-aws -p cargo-minco --all-targets --all-features --locked -- -D warnings
  - cargo minco deploy changeset --dry-run
  - sam validate --lint --template-file infra/aws/generated/template.yaml
---

## Goal

Create and inspect CloudFormation change sets only after exact account, region,
environment, role, release-manifest, drift, migration-plan, and clean-source
guards pass.

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
