# ADR 0015: Exact HTTP policy and explicit queue workers

Status: Accepted

## Context

Global middleware included Feedback and development-only request headers even
when those capabilities were absent. Minco also shipped event/outbox ports and
SQS publication but no small consumer runtime, encouraging each application to
reimplement Lambda partial-batch, FIFO and payload-failure semantics.

## Decision

1. Applications own a typed baseline `HttpHeaderPolicy`.
2. HTTP-capable plugins contribute only their exact allowed request, exposed
   response and sensitive request headers through `HttpModule`.
3. Composition normalizes and de-duplicates names and rejects wildcard headers,
   wildcard origins, empty origins, zero timeouts and zero body limits.
4. Cookie/CSRF policy is application-selected; plugin/product headers are not
   global defaults.
5. Ship `minco-aws-worker` as an opt-in runtime crate with no AWS SDK dependency.
6. Missing/duplicate SQS identifiers fail the invocation; ordinary invalid or
   rejected records use deterministic partial-batch failures.
7. FIFO batches process in order and fail forward after the first failure.
8. Concurrency is bounded, defaults to one, and every in-flight future is
   awaited. Minco does not create event-source mappings, queues, schedules,
   pollers or detached work.
9. Outbox recovery remains one explicitly invoked bounded pass.

## Consequences

- Installing a plugin can expand HTTP policy only by reviewable exact names.
- Removing Feedback removes its browser-token headers.
- `HttpRuntimeConfig` gains a public field and middleware returns the broader
  `HttpConfigurationError`; this is a documented pre-1.0 candidate break.
- Applications must enable `ReportBatchItemFailures` and own queue/DLQ/IAM/cost
  configuration.
- Worker Plan/SAM synthesis remains future work rather than hidden runtime
  infrastructure.

## Alternatives rejected

- A single global allow-list would retain absent-plugin headers and obscure
  ownership.
- Wildcard CORS, environment-derived header names and log-only validation make
  runtime and ingress policy disagree.
- A business-event worker facade would couple Minco to application schemas.
- Background polling or schedule creation would hide wake sources and cost.

## Compatibility impact

This is an additive worker crate and facade feature. The HTTP field and broader
configuration error are intentional pre-1.0 candidate changes documented in
the upgrade guide.

## Security and cost impact

Exact names reduce CORS and log-disclosure scope. Worker payloads and attribute
values are redacted from `Debug`. Concurrency, message size and batch size are
bounded; queue, DLQ, IAM and event-source costs remain explicit application
decisions.

## Rollback and removal

Applications can remove `aws-worker` without changing domain/application
crates and return to their prior Lambda handler. HTTP callers can restore the
previous middleware constructor while remaining on `0.1.1`; no persisted data
or infrastructure migration is performed by this decision.
