---
id: M14-T59
title: Parse the real S3 notification envelope for ticketing wakes
milestone: M14
status: active
priority: high
area: extensions/aws-worker/ticketing
depends_on: [M14-T58]
operations: []
owned_paths:
  - docs/DECISIONS.md
  - docs/adrs/0061-real-s3-wake-envelope.md
  - extensions/minco-aws-worker/src/ticketing_wake.rs
  - tasks/M14/M14-T59-real-s3-envelope.md
  - verification/1.9-performance-baseline.json
  - verification/source-manifest.json
  - verification/operational-evidence-validation.json
  - verification/static-validation.json
  - verification/publish-validation.json
  - verification/deep-review.json
checks:
  - cargo test -p minco-aws-worker --all-features --locked
  - cargo clippy -p minco-aws-worker --all-targets --all-features --locked -- -D warnings
  - cargo minco check --with-cargo
---

# M14-T59 - Parse the real S3 notification envelope for ticketing wakes

Blocker fix for M14-T58: the translation shipped with a flat, invented
record shape, but real S3→SQS notifications use the nested `Records`
envelope (`eventSource: aws:s3`, `eventName: ObjectCreated:*`,
`s3.bucket.name`, `s3.object.key`, `eventTime`, `sequencer`). Against
actual AWS the wake would reject every legitimate notification.

## Goal

- Parse the real envelope via the already-pinned `aws_lambda_events`
  S3 types: exactly one `aws:s3` `ObjectCreated:*` record per message
  (the SES receiving drop is one object per notification in the
  configured filter); non-S3 sources, non-ObjectCreated events, zero or
  multiple records, and missing bucket/key/sequencer all fail closed
  with the existing stable codes.
- URL-encode awareness: S3 notification keys arrive percent-encoded;
  the wake uses the provided `urlDecodedKey` when present, else the raw
  key, bounded as before.
- The flat M14-T58 shape is removed — it was never a real wire format.
  Tests prove a byte-accurate real envelope end-to-end (durable job
  submitted, redelivery dedupes), plus every rejection path.

## Non-goals

- SES receiving-rule configuration, Lambda bindings, Rustack seam
  (slice 3b proper), outbound email.

## Evidence

Run 2026-08-25 in the `minco-task-m14-t59` workspace:

- `cargo test -p minco-aws-worker --all-features` — ok, **20 + 2 passed**
  (rewritten suite: byte-accurate real envelope end-to-end, every
  rejection path, urlDecodedKey awareness, bounded external id).
- `cargo clippy -p minco-aws-worker --all-targets --all-features
  --locked -- -D warnings` — clean; `rustfmt --check` clean.
- Enabled the crate's `s3` feature on the existing pinned
  `aws_lambda_events` dependency (workspace Cargo.toml) — no new
  dependency.
- `cargo minco check --with-cargo` — result recorded at finish.

Behavioral proofs: a real `Records` envelope (eventVersion/eventSource/
awsRegion/eventTime/eventName/userIdentity/requestParameters/
responseElements/s3) delivers exactly one durable job and redelivery
dedupes; non-S3 sources → `wake_source_invalid`; `ObjectRemoved:*` →
`wake_event_kind_invalid`; zero/two records → `wake_record_count_invalid`;
missing key → `wake_field_invalid`; malformed eventTime → envelope
deserialization fails closed; percent-encoded keys resolve through
`urlDecodedKey` when present. The invented flat shape is gone.
