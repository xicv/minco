---
id: M6-T10
title: Add trigger-aware multi-runtime deployment planning
milestone: M6
status: planned
priority: high
area: deployment/plan
depends_on: [M6-T09]
operations: []
owned_paths:
  - crates/minco-plan/**
  - crates/minco-cli/**
  - extensions/minco-aws-worker/**
  - infra/aws/**
  - docs/adrs/**
  - docs/deployment/**
  - tasks/M6/M6-T10-multi-runtime-deployment-plan.md
checks:
  - cargo test -p minco-plan -p cargo-minco --all-features --locked
  - cargo clippy -p minco-plan -p cargo-minco --all-targets --all-features --locked -- -D warnings
  - cargo minco deploy plan
  - cargo minco deploy render-sam
  - sam validate --lint --template-file infra/aws/generated/template.yaml
---

## Goal

Extend Minco's schema-versioned deployment Plan IR beyond the initial single
HTTP API Lambda so it can explicitly describe one API function, worker
functions, queues, dead-letter policy, SQS mappings, partial-batch behavior,
and reviewed recovery schedules without making scheduled work a default.

## Design boundary

- keep exactly one HTTP API function in the first multi-runtime schema;
- model worker functions, SQS queues, DLQs, mappings, and schedules explicitly;
- validate FIFO, visibility timeout, redrive, concurrency, aggregate database
  connection, wake-source, cost, and stable-reference invariants;
- derive local services, exact IAM intent, and deterministic SAM from selected
  resources only;
- reject enabled schedules under the default minimal-idle policy;
- keep product event schemas, retry business policy, and processors in the
  application;
- provide deterministic schema migration or stable rejection diagnostics;
- treat the public serialized redesign as a likely `0.4.0` boundary.

## Acceptance

- the original single-API plan remains supported;
- generic fixtures cover API-only, standard/FIFO workers, DLQs, invalid
  references, partial-batch behavior, schedules, and connection budgets;
- local topology and SAM output remain deterministic;
- no queue, worker, poller, schedule, or fixed capacity appears implicitly;
- no AWS mutation is required for task completion.

## Non-goals

- Step Functions, Kinesis, Kafka, ECS/Fargate, arbitrary workflow graphs,
  multi-cloud abstractions, or multi-region deployment;
- product-specific routing, settlement, garment, scan, fulfilment, invitation,
  permission, billing, or rollback policy;
- replacing an application's live IaC before advisory parity and rollback
  evidence pass.
