---
id: M14-T48
title: Add requester-safe public projections and the requester API surface
milestone: M14
status: active
priority: high
area: plugins/ticketing/requester
depends_on: [M14-T45]
operations:
  - listTicketingRequesterTickets
  - getTicketingRequesterTicket
  - replyToTicketingRequesterTicket
owned_paths:
  - docs/DECISIONS.md
  - docs/adrs/0050-requester-safe-public-projections.md
  - plugins/minco-plugin-ticketing/**
  - tasks/M14/M14-T48-requester-safe-projections.md
  - verification/1.9-performance-baseline.json
  - verification/source-manifest.json
  - verification/operational-evidence-validation.json
  - verification/static-validation.json
  - verification/publish-validation.json
  - docs/reference/generated/plugins.md
checks:
  - cargo test -p minco-plugin-ticketing --all-features --locked
  - cargo clippy -p minco-plugin-ticketing --all-targets --all-features --locked -- -D warnings
  - cargo minco plugin validate
  - cargo package -p minco-plugin-ticketing --locked
---

# M14-T48 - Add requester-safe public projections and the requester API surface

Stage B1 of the Ticketing sequence (ADR-0050): close the requester-side
correctness gaps that Stage A's review confirmed — public message authorship
leakage, internal system-event leakage, no public status labels, no
requester-scoped list/detail operations, and no own-ticket-enforced
requester list.

## Goal

- Requester projections serialize a public shape only: message author is an
  enum (`requester | support | system`) with no internal actor subject,
  internal notes stay invisible, and internal system events are not exposed
  as conversation entries.
- Public status labels (`open | in_progress | waiting_for_you | on_hold |
  resolved | closed`) map deterministically from internal statuses without
  exposing internal workflow vocabulary.
- New requester operations over the same ticket model:
  `GET /_minco/ticketing/requester/tickets` (own tickets only, newest-first
  compact summaries, cursor pagination),
  `GET /_minco/ticketing/requester/tickets/{ticketId}` (public projection,
  own-ticket enforced), and
  `POST /_minco/ticketing/requester/tickets/{ticketId}/replies`
  (requester-scoped alias of the existing reply operation).

## Acceptance

- No requester-facing serialization contains `author_subject`,
  `assignee_subject`, object keys, digests, internal notes, AI context,
  audit or provider data (asserted by tests, including a serialization
  scan).
- Requester list forcibly isolates by the authenticated subject regardless
  of client-supplied filters; another requester's ticket is 404.
- Memory and SQLite produce identical requester results.
- OpenAPI, descriptor and `minco-plugin.json` parity holds (24 → 27
  operations).

## Non-goals (deferred to later Stage B tranches, by design)

- Durable portal sessions, cookies, logout and CSRF (B2; needs a
  sessions-store port and an ADR-level decision on the handoff → session
  exchange).
- Append-only message persistence replacing whole-aggregate saves (B3).
- Shared HTTP idempotency middleware on requester mutations (B2).
- Activity-intent dispatch lifecycle (B4 / Stage C boundary).

## Evidence

Run 2026-08-23 in the `minco-task-m14-t48` workspace:

- `cargo test -p minco-plugin-ticketing --all-features --locked` — ok,
  **44 passed** (was 38; +6 requester-safety tests), including real
  temporary SQLite.
- Feature matrix — `--no-default-features` (19), sqlite-only (26),
  `--features full` (44) — all ok.
- `cargo clippy -p minco-plugin-ticketing --all-targets --all-features
  --locked -- -D warnings` — clean.
- `cargo minco plugin validate` — `[]`.
- `cargo package -p minco-plugin-ticketing --locked` — verified.
- `cargo minco contract sync --check` — passes.
- `rustfmt --check` over changed files — clean.
- `scripts/docs/generate-reference.sh` + `--check` — current.

Leakage proofs: requester projection serialization contains no
`author_subject`, no internal agent subject, no internal note bodies, no
internal status vocabulary; requester list contains no
`assignee_subject`/`requester_subject`; foreign ticket detail is 404; the
caller-injected requester filter is forcibly overridden; public status
filter accepts public labels only (`pending_internal` is rejected with
422).
