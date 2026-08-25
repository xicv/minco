---
id: M14-T61
title: Outbound ticketing email delivery evidence with ambiguity recovery
milestone: M14
status: active
priority: high
area: plugins/ticketing
depends_on: [M14-T60]
operations: []
owned_paths:
  - docs/DECISIONS.md
  - docs/adrs/0063-outbound-delivery-evidence.md
  - plugins/minco-plugin-notifications/src/lib.rs
  - plugins/minco-plugin-ticketing/migrations/sqlite/0005_ticketing_delivery_evidence.sql
  - plugins/minco-plugin-ticketing/src/http.rs
  - plugins/minco-plugin-ticketing/src/jobs.rs
  - plugins/minco-plugin-ticketing/src/persistence.rs
  - plugins/minco-plugin-ticketing/src/service.rs
  - plugins/minco-plugin-ticketing/src/store.rs
  - tasks/M14/M14-T61-outbound-delivery-evidence.md
  - verification/1.9-performance-baseline.json
  - verification/source-manifest.json
  - verification/operational-evidence-validation.json
  - verification/static-validation.json
  - verification/publish-validation.json
  - verification/deep-review.json
checks:
  - cargo test -p minco-plugin-ticketing --all-features --locked
  - cargo clippy -p minco-plugin-ticketing -p minco-plugin-notifications --all-targets --all-features --locked -- -D warnings
  - cargo minco check --with-cargo
---

# M14-T61 - Outbound ticketing email delivery evidence with ambiguity recovery

Stage D2 slice 3b, part 1. The deliver job previously fire-and-forgot
email through the notifications port: acceptance was indistinguishable
from delivery, ambiguous results were retried blindly, and bounce or
complaint feedback had nowhere to land.

## Goal

- Email-channel public replies submit through the observable
  `MailService`; provider acceptance (`MailReceipt`) is recorded as
  append-only evidence — never claimed as delivery.
- Reconcile before send: an existing acceptance row for the exact
  (project, ticket, message) suppresses the resend; redelivery and
  ambiguous retries cannot duplicate outbound mail.
- `MailRetryAdvice` maps one-to-one to job outcomes with stable codes:
  `ticketing.notification_transport_retryable` (no evidence row),
  `ticketing.notification_ambiguous` (ambiguous evidence row),
  `ticketing.notification_permanent` (permanent_failure evidence row).
- Bounce/complaint/delay feedback enters through the authorized
  `record_delivery_feedback` use case (`ticketing.ingest`), bounded
  identifiers, fail-closed on unknown targets before any persistence.
- Email channel without a configured mail service fails closed
  permanently; in-app notifications unchanged.
- SQLite migration 0005 with a natural-key idempotent append; memory and
  sqlite behavioral parity.

## Non-goals

- SES receiving-rule / S3 / SQS plan binding, IAM/cost/wake rendering and
  the Rustack live seam proof (slice 3b part 2, next task).
- Provider-side delivery-status reconciliation queries; the evidence
  store is the reconciliation authority until a real provider exists.

## Evidence

Run 2026-08-25 in the `minco-task-m14-t61` workspace:

- `cargo test -p minco-plugin-ticketing --all-features` — ok,
  **84 passed** (10 new: 7 jobs tests with a scriptable mail transport —
  acceptance recorded, redelivery suppressed, ambiguous result records
  evidence then resends only after reconciliation, permanent failure
  recorded, transient failure records nothing, unconfigured mail fails
  closed, in-app unchanged; 3 service tests — ingest permission required,
  invalid/unknown targets fail before persistence, bounce+complaint
  recorded; plus the sqlite round-trip/parity test).
- `cargo clippy -p minco-plugin-ticketing -p minco-plugin-notifications
  --all-targets --all-features --locked -- -D warnings` — clean;
  `cargo fmt --all -- --check` clean.
- No OpenAPI change (internal job and use case only); the generated
  request boundary and ref-integrity checks stay green.
- Evidence chain: static/publish validation, source manifest (stable
  across re-runs after the content freeze), baseline re-bound,
  operational evidence PASS, deep review rerun.
- `cargo minco check --with-cargo` — result recorded at finish.
