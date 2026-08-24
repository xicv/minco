# ADR 0060: S3 wake event translation for ticketing inbound email

## Status

Accepted.

## Context

ADR-0059 defined the engine-neutral wake use case; the remaining seam is
the provider side: an S3 `ObjectCreated` notification (SES receiving
drops raw MIME into a private bucket and S3 notifies, typically through
SQS) must become the wake inputs — object key, arrival time, provider
identity. The SQS worker runtime (ADR-0015) already delivers message
bodies to `MessageHandler` implementations with bounded sizes, partial
batch responses and explicit FIFO semantics.

## Decision

1. The translation lives in `minco-aws-worker` behind the `jobs` feature
   as a new `ticketing_wake` module — the worker crate is where provider
   runtimes compose, and the plugin stays free of AWS vocabulary
   (ADR-0059).
2. `TicketingMailWakeEvent` is a bounded, `deny_unknown_fields` parse of
   one notification record: `bucket`, `key`, `eventTime`, `sequencer`,
   and an optional SES receipt identifier. The provider-scoped external
   id is the SES receipt id when present, otherwise a digest of the
   bucket and key — a bounded identity, never message content. Unknown
   shapes, oversized bodies, control characters or missing fields fail
   closed with stable codes before any wake is attempted.
3. `TicketingMailWakeHandler` implements the existing `MessageHandler`
   trait: one valid record produces exactly one
   `TicketingService::wake_inbound_email` call with the record's
   `eventTime` parsed as the arrival anchor — the same anchor that makes
   the durable job's semantic fingerprint stable across queue
   redelivery. The handler holds only the ticketing service and a fixed
   mailbox scope from explicit configuration; it receives no
   credentials and asserts no identity.
4. Service error classification maps one-to-one to stable worker failure
   codes (`ticketing.inbound_object_missing`,
   `ticketing.inbound_thread_unresolved`, …), so SQS redelivery — not
   the handler — decides retry. Unresolved threading is reported and
   redelivered: the queue is delivery, never truth.

## Consequences

- Applications enable the SES receiving → S3 → SQS → worker path by
  composing existing pieces; the worker adds no queue, schedule or
  provider resource of its own.
- The translation is fully unit-testable with the released fakes; the
  later Rustack seam proof exercises the same bytes end-to-end.

## Alternatives considered.

- **Translating inside the ticketing plugin** — rejected: puts AWS
  vocabulary in the plugin, violating ADR-0059's boundary.
- **An EventBridge/Lambda-direct path skipping SQS** — deferred with
  slice 3b; the SQS path is the released, explicitly-selected runtime.
