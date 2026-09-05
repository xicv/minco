# ADR 0061: The ticketing wake parses the real S3 notification envelope

## Status

Accepted.

## Context

M14-T58 shipped the S3→wake translation with a flat, invented record
shape. Real S3 event notifications — the bytes SES receiving produces
when it drops raw MIME into a bucket and S3 notifies SQS — use the
nested `Records` envelope with `eventSource`, `eventName`,
`s3.bucket.name`, `s3.object.key`, `eventTime` and `sequencer`. The
shipped parser would reject every legitimate notification: a correctness
defect, fixed before any provider wiring is attempted.

## Decision

1. The translation parses the real envelope through the already-pinned
   `aws_lambda_events` S3 types — no hand-rolled approximation of the
   wire format and no new dependency. The S3 event types are
   `#[non_exhaustive]` and tolerate unknown fields by design, so future
   envelope additions cannot break the wake; the fields the wake uses
   are validated explicitly.
2. Exactly one `aws:s3` `ObjectCreated:*` record is accepted per
   message. Non-S3 sources, non-ObjectCreated events, zero or multiple
   records, and missing bucket/key/sequencer fail closed with the
   existing stable worker codes — the queue message is delivery, never
   truth, and nothing is guessed.
3. S3 notification keys arrive percent-encoded; the wake uses
   `urlDecodedKey` when present, otherwise the raw key, bounded exactly
   as before. The external id rule is unchanged: SES receipt id when
   present (message attributes are a slice-3b concern), otherwise the
   bounded `s3-<sha256(bucket|key)>` digest of the raw key.
4. The invented flat shape from M14-T58 is removed outright — it never
   existed on any wire — rather than kept as a compatibility mode.

## Consequences

- The seam now matches the bytes AWS actually delivers; the Rustack
  proof in slice 3b exercises this exact parser rather than a fixture
  dialect.
- Envelope evolution is carried by the pinned upstream crate, reviewed
  on update like any dependency.

## Alternatives considered

- **Keeping the flat shape as an internal dialect** — rejected: a
  dialect nobody sends is untested surface pretending to be a seam.
- **Hand-rolling the envelope structs** — rejected: duplicates a pinned
  parser and its `non_exhaustive` future-proofing.

## Amendment (2026-09-01, M14-T74 stabilization review 5072859042)

Point 2's "exactly one record" rule is superseded by the bounded
multi-record envelope the worker now implements: S3 event notifications
may batch up to ten `aws:s3` `ObjectCreated:*` records in one message,
and the worker processes each record through the same per-record
validation (source, event name, bucket/key/sequencer presence,
`urlDecodedKey`-aware key handling). Zero valid records, any non-S3 or
non-ObjectCreated record, and any record beyond the bounded set still
fail closed with the existing stable worker codes. The single-record
text above is retained for decision history only.

## Note (2026-09-02)

No further change from review 5083559431; the 2026-09-01 amendment
remains authoritative.
