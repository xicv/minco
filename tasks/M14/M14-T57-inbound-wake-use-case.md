---
id: M14-T57
title: Add the engine-neutral inbound email wake use case over the object-storage port
milestone: M14
status: active
priority: high
area: plugins/ticketing/email-ingress
depends_on: [M14-T56]
operations: []
owned_paths:
  - docs/DECISIONS.md
  - docs/adrs/0059-inbound-wake-use-case.md
  - plugins/minco-plugin-ticketing/**
  - tasks/M14/M14-T57-inbound-wake-use-case.md
  - verification/1.9-performance-baseline.json
  - verification/source-manifest.json
  - verification/operational-evidence-validation.json
  - verification/static-validation.json
  - verification/publish-validation.json
  - verification/deep-review.json
  - docs/reference/generated/plugins.md
checks:
  - cargo test -p minco-plugin-ticketing --all-features --locked
  - cargo clippy -p minco-plugin-ticketing --all-targets --all-features --locked -- -D warnings
  - cargo minco plugin validate
  - cargo package -p minco-plugin-ticketing --locked
  - cargo minco check --with-cargo
---

# M14-T57 - Add the engine-neutral inbound email wake use case

Stage D2 slice 2 (ADR-0059): the in-plugin wake half of the inbound
bridge. The AWS S3-event/Lambda adapter and Rustack seam are slice 3; no
provider is contacted here.

## Goal

- Portal services gain the object-storage handle (already a required
  install dependency; now actually used).
- New use case `wake_inbound_email` (jobs feature): given an object key,
  an arrival timestamp, and the provider-scoped identity for the wake,
  read the raw object through the object-storage port, compute the
  content digest, parse the MIME headers with the same `mail-parser` the
  handler uses, extract `Message-ID`, `In-Reply-To` and bounded
  `References`, and submit the durable ingest job through the M14-T56
  routing use case. The D1 job remains the verification and ingestion
  authority; the wake only extracts routing facts.
- `submit_inbound_email` gains the optional `internet_message_id` so the
  ingested identity records the threading anchor for future replies.
- Missing object, unparseable MIME and missing text-free structure fail
  closed with classified errors; nothing is guessed.

## Non-goals

- S3 event JSON parsing, Lambda wiring, SQS wake, SES receiving rules,
  Rustack seams, outbound email (slice 3).
- New-ticket creation for unmatched mail.

## Evidence

Run 2026-08-24 in the `minco-task-m14-t57` workspace:

- `cargo test -p minco-plugin-ticketing --all-features --locked` — ok,
  **74 passed** (+2 wake proofs).
- Feature matrix — default (45), `--no-default-features` (21), sqlite
  (33), `full` (74) — all ok.
- `cargo clippy … -- -D warnings` — clean; `rustfmt --check` clean;
  `plugin validate` `[]`; `cargo package` verified; `contract sync
  --check` passes; docs reference current.
- `cargo minco check --with-cargo` — result recorded at finish.

Behavioral proofs: the wake reads the raw object through the
object-storage port, computes the digest, extracts `Message-ID`,
`In-Reply-To` and the whitespace-split bounded `References` from a real
threaded MIME fixture, and submits the durable job whose payload carries
the resolved ticket, the exact digest, and all threading anchors
(recording the message id future replies thread against); the same wake
replayed at the same arrival anchor returns the existing durable job;
missing object fails closed; headerless garbage parses leniently and
fails closed at threading resolution — never at ingestion (documented in
the test); without the object handle the wake fails closed
`ObjectsUnavailable`. `submit_inbound_email` now records the
`internet_message_id` passed through from the wake.

Security triage addendum: the flagged "credentials" in the sealed scan
were verified line-by-line — all are `env:` references or
`$(openssl rand …)` runtime generation; none is a committed secret.
