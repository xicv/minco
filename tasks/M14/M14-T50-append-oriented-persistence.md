---
id: M14-T50
title: Make ticket conversation persistence append-oriented with paginated messages
milestone: M14
status: active
priority: high
area: plugins/ticketing/persistence
depends_on: [M14-T49]
operations:
  - listTicketingRequesterMessages
owned_paths:
  - docs/DECISIONS.md
  - docs/adrs/0052-append-oriented-ticket-persistence.md
  - plugins/minco-plugin-ticketing/**
  - tasks/M14/M14-T50-append-oriented-persistence.md
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

# M14-T50 - Make ticket conversation persistence append-oriented with paginated messages

Stage B3 of the Ticketing sequence (ADR-0052).

## Goal

- Ticket reads reconstruct the aggregate from projection columns and child
  tables; `ticket_json` is no longer authoritative.
- Replying (requester, agent) and internal notes commit through a new
  `append_ticket_message` store port: one message-row insert plus a
  projection-column update plus the activity intent in one transaction —
  no whole-aggregate rewrite of the conversation.
- Management/status saves remain full saves (they change many fields).
- `GET /_minco/ticketing/requester/tickets/{ticketId}/messages` paginates
  the public conversation independently, newest-first with the standard
  cursor.

## Acceptance

- Migration `0004` adds the remaining ticket columns with a `json_extract`
  backfill; reads never parse `ticket_json`.
- Append is atomic and optimistic-revision-checked: stale revision →
  `StaleRevision`, nothing inserted.
- Memory and SQLite produce identical results for append + pagination.
- Message pagination proves: >1 page, tied timestamps, no duplicate or
  omitted IDs, invalid cursor rejection, internal notes invisible.
- OpenAPI/descriptor/manifest parity holds (29 → 30 operations).

## Non-goals

- Activity-intent dispatch lifecycle (Stage B4 / C boundary).
- Any Jobs dependency; schema changes beyond the projection columns.

## Evidence

Run 2026-08-24 in the `minco-task-m14-t50` workspace:

- `cargo test -p minco-plugin-ticketing --all-features --locked` — ok,
  **52 passed** (was 49; +3 append/columnar/pagination proofs including
  real temporary SQLite).
- Feature matrix — `--no-default-features` (20), sqlite-only (30),
  `--features full` (52) — all ok.
- `cargo clippy -p minco-plugin-ticketing --all-targets --all-features
  --locked -- -D warnings` — clean; `rustfmt --check` over changed files —
  clean.
- `cargo minco plugin validate` — `[]`; `cargo package` verified;
  `cargo minco contract sync --check` passes; docs reference current.
- `cargo minco check --with-cargo` — result recorded at finish.

Behavioral proofs: the append path leaves the `ticket_json` diagnostic
snapshot byte-identical while the columnar read reflects the appended
message and every reconstructed field (subject, description, requester
display name/email, channel, queue, tags, followers, source references,
first-response timestamp); stale-revision appends are rejected on both
engines with nothing inserted; message pagination is newest-first, gap- and
duplicate-free across pages, hides internal notes from the requester
surface, rejects half-cursors, and memory/SQLite produce identical
results; the requester messages endpoint enforces own-ticket access
(foreign requester 404) and never serializes `author_subject`.
