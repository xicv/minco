---
id: M14-T58
title: Translate S3 ObjectCreated notifications into ticketing inbound wakes
milestone: M14
status: active
priority: high
area: extensions/aws-worker/ticketing
depends_on: [M14-T57]
operations: []
owned_paths:
  - docs/DECISIONS.md
  - docs/adrs/0060-s3-wake-event-translation.md
  - extensions/minco-aws-worker/Cargo.toml
  - extensions/minco-aws-worker/src/lib.rs
  - extensions/minco-aws-worker/src/ticketing_wake.rs
  - tasks/M14/M14-T58-s3-wake-event-translation.md
  - verification/1.9-performance-baseline.json
  - verification/source-manifest.json
  - verification/operational-evidence-validation.json
  - verification/static-validation.json
  - verification/publish-validation.json
  - verification/deep-review.json
  - docs/reference/generated/plugins.md
checks:
  - cargo test -p minco-aws-worker --all-features --locked
  - cargo clippy -p minco-aws-worker --all-targets --all-features --locked -- -D warnings
  - cargo test -p minco-plugin-ticketing --all-features --locked
  - cargo minco check --with-cargo
---

# M14-T58 - Translate S3 ObjectCreated notifications into ticketing inbound wakes

Stage D2 slice 3a (ADR-0060): the bounded, engine-testable translation
from an S3 `ObjectCreated` notification record (as delivered through the
existing SQS worker message body) into the ADR-0059 wake inputs. SES
receiving-rule wiring, Rustack live seams and outbound email remain
slice 3b.

## Goal

- New `ticketing_wake` module in `minco-aws-worker` (feature `jobs`):
  `TicketingMailWakeEvent` — a bounded, deny-unknown parse of one S3
  notification record carrying `bucket`, `key`, `eventTime`, `sequencer`
  and the provider identity (`ses` receipt id when present, else the
  object key digest). Oversized or malformed bodies fail closed with
  stable codes; nothing is guessed.
- A `TicketingMailWakeHandler` implementing the existing `MessageHandler`
  trait: each valid record maps to exactly one
  `TicketingService::wake_inbound_email` call with the record's
  `eventTime` as the arrival anchor; classified service errors map to
  stable worker failure codes (queue redelivery decides retry; ambiguous
  outcomes stay visible, never silently dropped).
- No new topology, no provider contact: the handler composes the
  already-released SQS worker with the already-shipped ticketing use
  case.

## Non-goals

- SES receiving-rule/S3 notification-filter configuration, Lambda
  bindings, Rustack E2E, outbound mail, new-ticket creation for
  unmatched mail (slice 3b).

## Evidence

Run 2026-08-25 in the `minco-task-m14-t58` workspace:

- `cargo test -p minco-aws-worker --all-features` — ok, **19 + 2 + 0
  passed** (existing worker suite unchanged; +4 wake-translation proofs
  in the new module).
- `cargo clippy -p minco-aws-worker --all-targets --all-features
  --locked -- -D warnings` — clean; `rustfmt --check` clean; ticketing
  plugin 74/74 re-verified after the workspace dependency addition;
  docs reference regenerated.
- `cargo minco check --with-cargo` — result recorded at finish.

Behavioral proofs: a valid threaded notification (bucket, key,
`eventTime`, sequencer) with a real MIME object in storage produces
exactly one durable `ticketing.process-inbound-email` job, and queue
redelivery of the same record dedupes to the same job (the `eventTime`
anchor makes the semantic fingerprint stable); malformed JSON, unknown
injected fields, empty or multi-record bodies, invalid timestamps and
unbounded fields all fail closed with stable codes before any wake;
missing object preserves the classified
`ticketing.inbound_object_missing` code through the translation; the
external id prefers the SES receipt id and otherwise derives a bounded
`s3-<sha256>` digest of bucket and key — never message content.
