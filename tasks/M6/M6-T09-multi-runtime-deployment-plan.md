---
id: M6-T09
title: Model multi-function and trigger-aware serverless deployments
milestone: M6
status: planned
priority: high
area: deployment/plan
depends_on: [M6-T08]
operations: []
owned_paths:
  - crates/minco-plan/**
  - crates/minco-cli/**
  - extensions/minco-aws-worker/**
  - infra/aws/**
  - docs/adrs/**
  - docs/deployment/**
  - tasks/M6/M6-T09-multi-runtime-deployment-plan.md
checks:
  - cargo test -p minco-plan -p cargo-minco --all-features --locked
  - cargo clippy -p minco-plan -p cargo-minco --all-targets --all-features --locked -- -D warnings
  - cargo minco deploy plan
  - cargo minco deploy render-sam
  - sam validate --lint --template-file infra/aws/generated/template.yaml
---

## Goal

Extend Minco's deployment Plan IR beyond the initial single HTTP API Lambda so
it can faithfully describe a modular application's API function, SQS worker,
event-source mapping, dead-letter policy, and an explicitly selected recovery
trigger without making scheduled work a framework default.

## Evidence for the gap

CGSP's first Minco contract adoption proved that Minco 0.3.0 validates the API
and provides an independent SQS worker runtime, but the current deployment-plan
validator still requires exactly one API function. CGSP therefore cannot compare
its existing API Lambda, worker Lambda, SQS mapping, and costed outbox-recovery
schedule against one complete Minco plan; Pulumi correctly remains authoritative.

## Design boundary

- introduce a versioned Plan IR change rather than overloading the current
  function record with ambiguous fields;
- distinguish HTTP API functions from worker functions and other future roles;
- model SQS event-source mappings, partial-batch responses, batch size,
  concurrency, visibility-timeout assumptions, and DLQ/redrive intent;
- represent schedules as explicit trigger resources with cost/wake metadata;
- preserve the minimal profile: no schedule, queue, worker, or fixed-capacity
  resource is created unless selected by the application;
- derive local Rustack requirements and exact IAM intents from selected
  resources;
- keep product event schemas, retry business policy, and message processors in
  the application;
- provide backward-compatible reading or a deterministic migration diagnostic
  for schema-v1 plans;
- do not replace an application's existing Pulumi/Terraform/SAM deployment
  until advisory-plan parity and rollback evidence pass.

## Acceptance

- one plan can describe an HTTP API Lambda and an SQS worker independently;
- HTTP routes bind only to the selected API function;
- SQS mappings require `ReportBatchItemFailures` when using the official worker;
- FIFO and standard-queue constraints are validated explicitly;
- database connection budgets sum only functions that actually connect to the
  database;
- scheduled wakeups are rejected by the minimal-idle policy unless a reviewed
  exception/profile enables them;
- SAM rendering and Plan JSON are deterministic and validate through SAM CLI;
- the original single-function Orders profile remains supported;
- a CGSP-shaped fixture proves API + worker + SQS + optional recovery schedule
  without embedding CGSP business concepts;
- no AWS mutation is required for task completion.

## Non-goals

- importing or managing an existing CGSP Pulumi stack;
- defining order, routing, garment, fulfilment, or notification business logic;
- automatically introducing EventBridge polling;
- claiming live-cloud parity from renderer tests alone.
