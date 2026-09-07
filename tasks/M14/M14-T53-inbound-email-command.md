---
id: M14-T53
title: Add the ticketing.process-inbound-email job command with verified raw-object ingress
milestone: M14
status: active
priority: high
area: plugins/ticketing/email-ingress
depends_on: [M14-T52]
operations: []
owned_paths:
  - docs/DECISIONS.md
  - docs/adrs/0055-verified-inbound-email-command.md
  - plugins/minco-plugin-ticketing/**
  - tasks/M14/M14-T53-inbound-email-command.md
  - verification/1.9-performance-baseline.json
  - verification/source-manifest.json
  - verification/operational-evidence-validation.json
  - verification/static-validation.json
  - verification/publish-validation.json
  - verification/deep-review.json
  - docs/reference/generated/plugins.md
checks:
  - cargo test -p minco-plugin-ticketing --all-features --locked
  - cargo test -p minco-plugin-ticketing --no-default-features --locked
  - cargo clippy -p minco-plugin-ticketing --all-targets --all-features --locked -- -D warnings
  - cargo minco plugin validate
  - cargo package -p minco-plugin-ticketing --locked
  - cargo minco check --with-cargo
---

# M14-T53 - Add the ticketing.process-inbound-email job command with verified raw-object ingress

Stage D1 of the Ticketing sequence (ADR-0055): the durable command half of
inbound email. The SES/Rustack adapter wiring is Stage D2; no provider is
contacted in this task.

## Goal

- New typed command `ticketing.process-inbound-email` v1 under the optional
  `jobs` feature, payload carrying bounded identities and digests only:
  project, provider, mailbox scope, external message id, content SHA-256,
  raw object key, target ticket and expected revision, optional threading
  headers.
- Envelope policy: worker profile `ticketing-mail`, dedupe
  `mail:<sha256(provider|scope|external-id)>`, overlap
  `mailbox:<sha256(scope)>`, partition = project, bounded exponential
  retry, six-hour deadline, causation = correlation.
- The handler is real and fail-closed at each step: load the raw object
  through the object-storage port (missing → permanent), verify the
  content digest (mismatch → permanent, unverified content is never
  processed), parse MIME with the same `mail-parser` the notifications
  plugin uses (invalid → permanent), extract the first text body (missing
  → permanent), and ingest through the existing idempotent
  `ingest_external_message` use case under an explicitly registered
  worker identity — the worker holds only `ticketing.ingest` and cannot
  bypass authorization. Stale ticket revisions are retryable; identity
  conflicts are permanent; store failures are retryable.

## Non-goals

- SES receiving, S3 wake wiring, Rustack seams, mailbox/threading
  resolution to ticket ids (Stage D2), outbound email.
- Any provider contact.

## Evidence

Run 2026-08-24 in the `minco-task-m14-t53` workspace:

- `cargo test -p minco-plugin-ticketing --all-features --locked` — ok,
  **66 passed** (real temporary SQLite; +6 inbound-command proofs).
- Feature matrix — default (43, unchanged), `--no-default-features` (20),
  sqlite-only (30), `full` (66) — all ok; the default build gains nothing.
- `cargo clippy … -- -D warnings` — clean; `rustfmt --check` clean;
  `plugin validate` `[]`; `cargo package` verified; `contract sync --check`
  passes; docs reference current.
- `cargo minco check --with-cargo` — result recorded at finish.

Behavioral proofs: envelope policy (dedupe `mail:<digest>`, overlap
`mailbox:<digest>`, partition project, deadline, profile `ticketing-mail`;
payload carries no body content); the happy path verifies the digest,
parses real MIME with the notifications plugin's `mail-parser`, flattens
line breaks (v1 normalization — raw MIME stays authoritative), and ingests
idempotently through the authorized use case (replay adds no second
message); digest mismatch → permanent with nothing ingested; missing
object → permanent; unparseable content → permanent; stale ticket
revision → retryable. The worker identity holds only `ticketing.ingest`
and cannot bypass authorization. Discovered and fixed during testing: the
ingest error `Store(Validation(_))` was initially unclassified (fell to
retryable) — now permanent `ticketing.inbound_invalid`.
