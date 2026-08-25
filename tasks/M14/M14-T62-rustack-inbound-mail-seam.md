---
id: M14-T62
title: Live Rustack seam proof for the ticketing inbound mail chain
milestone: M14
status: active
priority: high
area: extensions/aws-worker
depends_on: [M14-T61]
operations: []
owned_paths:
  - docs/DECISIONS.md
  - docs/adrs/0064-rustack-inbound-mail-seam.md
  - extensions/minco-aws-adapters/src/s3.rs
  - extensions/minco-aws-worker/Cargo.toml
  - extensions/minco-aws-worker/examples/ticketing_mail_seam.rs
  - scripts/dev/ticketing-mail-seam.sh
  - tasks/M14/M14-T62-rustack-inbound-mail-seam.md
  - verification/1.9-performance-baseline.json
  - verification/source-manifest.json
  - verification/operational-evidence-validation.json
  - verification/static-validation.json
  - verification/publish-validation.json
  - verification/deep-review.json
checks:
  - cargo test -p minco-aws-adapters --features s3 --locked
  - cargo clippy -p minco-aws-adapters -p minco-aws-worker --all-targets --all-features --locked -- -D warnings
  - scripts/dev/ticketing-mail-seam.sh
  - cargo minco check --with-cargo
---

# M14-T62 - Live Rustack seam proof for the ticketing inbound mail chain

Stage D2 slice 3b part 2. The inbound chain had only run against
in-memory fakes; this task proves it live against local S3+SQS and fixes
the integration defect the proof surfaced.

## Goal

- `scripts/dev/ticketing-mail-seam.sh`: local Rustack stack; a
  foreign-produced raw MIME object (no Minco metadata, no content type —
  what an SES receiving-rule drop looks like); a byte-accurate
  `ObjectCreated:Put` envelope (percent-encoded key + `urlDecodedKey`)
  delivered through real SQS twice; the worker wake handler consumes
  both; exactly one durable `ticketing.process-inbound-email` job
  verified in real SQLite.
- S3 adapter reads tolerate foreign-written objects (absent metadata →
  empty attributes; absent content type → `application/octet-stream`);
  checksum mismatches still fail; Minco-written round-trips unchanged.
- SES availability probed and recorded honestly (Rustack 0.9.1:
  unsupported) — the SES binding stays plan-level; no provider contact.
- Example `ticketing_mail_seam` (feature `ticketing-wake`): explicit
  composition — sqlite ticketing + jobs stores on one pool, released
  `S3ObjectStorage` adapter, path-style addressing, bounded polling with
  delete-on-success.

## Non-goals

- SES receiving-rule/S3-notification/IAM/cost/wake plan+SAM rendering
  (next task); provider contact of any kind.

## Evidence

Run 2026-08-25 in the `minco-task-m14-t62` workspace:

- `scripts/dev/ticketing-mail-seam.sh` — PASSED (final run):
  `seed` ticket `TKT-01a03783…`; `poll` handled 2, failures `[]`;
  `verify` total_jobs 1, inbound_jobs 1, ok true; SES probe
  `unsupported` (recorded). Two deliveries, one durable job — dedupe
  proven on real services.
- `cargo test -p minco-aws-adapters --features s3` — ok, 20 passed
  (metadata round-trip behavior unchanged).
- `cargo clippy -p minco-aws-adapters -p minco-aws-worker
  --all-targets --all-features --locked -- -D warnings` — clean;
  `cargo fmt --all -- --check` clean.
- Mimosa flagged a theoretical command-injection pattern in the script's
  cleanup (queue-url/bucket interpolation); every value is
  script-generated or a local test-service URL created by this run —
  same pattern as the existing `rustack-smoke.sh`.
- Evidence chain: static/publish validation, source manifest stable,
  baseline re-bound, operational evidence PASS, deep review rerun.
- `cargo minco check --with-cargo` — result recorded at finish.
