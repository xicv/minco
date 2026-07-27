# ADR 0019: Trigger-aware multi-runtime deployment planning

## Status

Accepted.

## Context

Plan IR schema 1 represents one HTTP API Lambda. It cannot describe the
already-published SQS worker runtime, its queue and dead-letter policy, an
event-source mapping, or a reviewed recovery schedule. The legacy
`scheduled_wakeups` strings also lack a target function, enablement state and
purpose, so they cannot safely drive infrastructure.

Those omissions break the resource and evidence planes: local services, IAM,
cost, performance and SAM cannot be derived from one inspectable topology.
Adding a worker or schedule implicitly would conflict with Minco's static,
minimal-idle-cost architecture.

## Decision

Plan IR schema 2 is an explicit trigger graph:

- exactly one function has the `http_api` role and owns every OpenAPI route;
- zero or more `worker` functions have independent artifact, memory, timeout,
  concurrency and database-connection budgets;
- queues declare standard/FIFO behavior, visibility, retention and an optional
  paired dead-letter queue/redrive count;
- typed `http_api`, `sqs` and `schedule` triggers reference functions and
  queues by stable IDs;
- the Minco SQS runtime requires `ReportBatchItemFailures`;
- individual and aggregate mapping concurrency cannot exceed the worker's
  reservation;
- queue visibility covers six function timeouts plus the batching window;
- FIFO compatibility, batch limits, references, redrive cycles and SAM logical
  identifier collisions fail with stable diagnostics;
- enabled schedules remain rejected by the default minimal-idle policy.

The plan derives selected local AWS services and application-specific IAM
intent. Local-native plans do not acquire AWS database-parameter permissions.
Schedules are never started by local topology generation.

Cost output exposes each mapping, worker connection pressure, schedule target
and estimated invocations where derivable, wake effects, fixed resources,
request-based resources and the regional rates still required. Performance
output reports every function artifact and its SHA-256 when present.

The SAM renderer emits only declared workers, queues, redrive policies,
mappings and schedules. It keeps the externally provisioned PostgreSQL
boundary. A DynamoDB plan is inspectable and locally projectable, but generic
DynamoDB SAM fails closed until an access-pattern-specific adapter and renderer
declare tables and IAM.

## Consequences

- API-only schema 1 inputs remain accepted and retain their generated
  plan/template compatibility.
- A schema 1 plan can migrate deterministically only when it contains exactly
  one API function and no queues, triggers or legacy schedule strings.
- Every schema 2 function needs its own build artifact.
- Multiple mappings may consume one queue, but cost output preserves every
  mapping and their aggregate concurrency must fit each worker.
- A permitted schedule is a visible wake source and request-cost dimension;
  permission does not make it a default.
- SAM still uses AWS-required platform execution permissions. Queue, database
  parameter, KMS and trigger target resources remain exact.

## Compatibility

The new public Rust types and serialized schema 2 are a likely Minco `0.4.0`
boundary. This implementation does not change Cargo package versions, publish
crates or retire schema 1. The migration procedure and stable rejection codes
are documented in
[`../deployment/plan-schema-v2-migration.md`](../deployment/plan-schema-v2-migration.md).

## Safety

Planning, rendering, linting and artifact builds are local operations. This
decision authorizes no AWS mutation, database migration, crate publication,
tag or release.

## Alternatives rejected

### Infer workers from installed plugins

Plugin presence does not prove an application wants a queue, mapping, retry
policy or running worker. It would hide resources and cost.

### Retain unstructured schedule strings

A string cannot identify ownership, target, enablement or purpose and therefore
cannot support safe migration or review.

### Model arbitrary workflow graphs

Step Functions, streams, Kafka, containers, multi-region and multi-cloud
orchestration require different invariants. They remain separate future
decisions.
