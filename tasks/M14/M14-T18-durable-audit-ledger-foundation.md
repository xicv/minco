---
id: M14-T18
title: Define the durable audit ledger foundation
milestone: M14
status: complete
priority: critical
area: plugins/audit
depends_on: [M14-T17]
operations: []
owned_paths:
  - docs/DECISIONS.md
  - docs/adrs/0040-durable-audit-ledger.md
  - docs/research/audit-ledger-costs-2026-08.md
  - docs/reference/generated/plugins.md
  - plugins/minco-plugin-audit/**
  - roadmap/tasks.mmd
  - tasks/M14/M14-T18-durable-audit-ledger-foundation.md
  - verification/source-manifest.json
  - verification/static-validation.json
checks:
  - cargo test -p minco-plugin-audit --all-features --locked
  - cargo clippy -p minco-plugin-audit --all-targets --all-features --locked -- -D warnings
  - RUSTDOCFLAGS='-D warnings' cargo doc -p minco-plugin-audit --all-features --no-deps --locked
  - rustfmt --edition 2024 --check plugins/minco-plugin-audit/src/lib.rs plugins/minco-plugin-audit/src/v2.rs
  - uv run --locked python scripts/validate_static.py
  - scripts/docs/generate-reference.sh --check
  - uv run --locked python scripts/source_manifest.py --check
---

## Goal

Define and implement the additive provider-neutral contract for durable,
queryable audit history without turning Minco into an ORM, hiding a worker, or
allowing an indefinitely growing audit history to consume an operational SQL
database or SQLite file.

## Acceptance

- legacy `AuditEvent`, `AuditSink` and `AuditService` remain source and behavior
  compatible;
- an additive V2 record distinguishes semantic action, primary resource,
  related resources, actor/effective actor, operation, resource revision,
  correlation/causation, changed fields, origin and data classification;
- records and batches are strictly bounded before provider contact, and field
  changes express redacted or digested values without requiring raw secrets;
- the reference ledger atomically appends idempotent batches, rejects an event
  ID reused for different content, and serves stable cursor pages across size-
  rotated segments without duplicates or gaps;
- resource-history queries can include explicitly related resources without a
  source-database foreign key or join;
- size, rotation, archive, retention and pending-journal thresholds are explicit
  policy rather than one hard-coded global file size;
- storage health reports active/hot bytes, segment state, free-disk reserve,
  journal backlog and archive watermark with deterministic severity;
- the ADR keeps DynamoDB the default AWS ledger but not a global dependency,
  explains direct DynamoDB transactions versus SQL journals, and records the
  current Sydney request/storage price dimensions without freezing them into
  runtime code; and
- no AWS resource, database, release, tag, crate publication or deployment is
  created or mutated.

## Non-goals

- PostgreSQL, SQLite or DynamoDB production adapters;
- an Orders HTTP operation or application adoption;
- database triggers, PostgreSQL logical decoding or generic table observation;
- automatic archive deletion, a hidden schedule, or a hosted Minco control
  plane; or
- changing Plan IR, SAM, IAM, migrations or deployment defaults in this task.

## Evidence

The provider-neutral package has ten passing tests covering V1 compatibility,
V2 capability composition, atomic/idempotent batch append, conflicting ID reuse,
concurrent duplicate delivery, cursor pagination across sealed and archived
segments, explicit related-resource gathering, bounded values and secret-class
redaction, lifecycle validation, and early warning versus hard disk/backlog
limits. Strict Clippy and warning-denying rustdoc pass for the package; the
existing 54-test Feedback package suite and 21-test plugin CLI distribution
suite also pass.

`cargo minco inspect --json` preserves audit plugin contract `1.0.0` and
`audit.append` `1.0.0`, and adds `audit.ledger`/`audit.query` `2.0.0` plus
`audit.health` `1.0.0`. Static validation reports zero errors and warnings over
98 tasks and 199 Rust files. The source manifest verifies after regeneration.
The machine's installed `uv 0.12.3` does not satisfy the repository's exact
`uv ==0.11.32` requirement, so the Python checks used a temporary official
`uv 0.11.32` release whose archive SHA-256 matched its published checksum; the
global toolchain was not modified.

The security review made literal changes scalar and at most 4 KiB, rejects raw
literal changes under `Secret`, validates SHA-256 digests and principal
attribution, rejects control characters in record identity fields, and prevents
public infrastructure errors from embedding provider diagnostics. No AWS API,
database, deployment, release, tag or registry operation was performed.

`cargo minco check --with-cargo` advanced through static validation, all 53
repository-truth tests, deployment assurance and current-product-truth checks,
then failed exactly at
`uv run --locked python scripts/validate_operational_evidence.py --check-output verification/operational-evidence-validation.json`
with `checked operational-evidence receipt is stale`. The release-bound receipt
is owned by active task M14-T10 and was not rewritten or presented as current by
this feature task.

Provider, database and end-to-end evidence belongs to dependent tasks.
