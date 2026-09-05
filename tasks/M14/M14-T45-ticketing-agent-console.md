---
id: M14-T45
title: Add the integration-ready ticketing agent console and agent API seam
milestone: M14
status: active
priority: high
area: plugins/ticketing/agent-console
depends_on: [M14-T44]
operations:
  - getTicketingAgentConsole
  - getTicketingAgentConsoleScript
  - getTicketingAgentConsoleStyles
  - getTicketingAgentBootstrap
  - listTicketingAgentTickets
  - getTicketingAgentTicket
  - manageTicketingAgentTicket
owned_paths:
  - docs/DECISIONS.md
  - docs/adrs/0049-integration-ready-ticketing-agent-console.md
  - docs/reference/generated/plugins.md
  - plugins/minco-plugin-ticketing/**
  - tasks/M14/M14-T45-ticketing-agent-console.md
  - verification/static-validation.json
checks:
  - cargo test -p minco-plugin-ticketing --all-features --locked
  - cargo clippy -p minco-plugin-ticketing --all-targets --all-features --locked -- -D warnings
  - cargo minco plugin validate
  - cargo package -p minco-plugin-ticketing --locked
  - cargo minco check --with-cargo
---

# M14-T45 - Add the integration-ready ticketing agent console and agent API seam

Stage A of the Ticketing / Minco Desk sequence (ADR-0049). Based on the exact
remote Ticketing head `bb1e37d29aa06cbe97981591af82a0057a2008bd` plus the
prerequisite deterministic `minco contract sync` header regeneration, because
published main still stamps `examples/orders/api/src/generated.rs` with the
1.11.0 generation version.

## Goal

Give support agents a first-party, dependency-free console surface over the
same Ticketing API: an agent-scoped OpenAPI contract, truthful agent
permissions, a compact newest-first ticket summary list with a stable
URL-safe cursor, full-ticket detail, and one atomic management operation —
without weakening requester isolation and without adding a worker, queue,
schedule or any Jobs dependency.

## Acceptance

- Agent operations exist in the plugin OpenAPI contract, the code descriptor
  and `minco-plugin.json` in exact parity:
  `GET /_minco/ticketing/agent` (console page),
  `GET /_minco/ticketing/agent/console.js`,
  `GET /_minco/ticketing/agent/console.css`,
  `GET /_minco/ticketing/agent/bootstrap`,
  `GET /_minco/ticketing/agent/tickets`,
  `GET /_minco/ticketing/agent/tickets/{ticketId}`,
  `PATCH /_minco/ticketing/agent/tickets/{ticketId}/management`.
- New enforced capabilities `ticketing.agent-console`, `ticketing.agent.read`,
  `ticketing.agent.manage` are provided and checked; no capability is claimed
  that the plugin does not enforce.
- The agent ticket list returns compact summaries (no descriptions, message
  bodies, object keys, digests, audit or provider data), ordered
  `updated_at DESC, id DESC`, with a cursor accepted by `minco_http::Cursor`.
- Cursor pagination is proven for: more than one page, tied timestamps, no
  duplicate IDs, no omitted IDs, invalid cursor rejection, update moving a
  ticket to the first page, and memory/SQLite equivalence.
- The SQLite summary query never reads `ticket_json`.
- One management operation loads once, validates the complete requested
  change set against domain invariants, saves once, records one activity
  intent, and returns one authoritative ETag; a late validation failure
  leaves no partial update.
- The console page and assets are dependency-free, same-origin, served with
  `Content-Security-Policy`, `X-Content-Type-Options: nosniff` and
  `Referrer-Policy: no-referrer`, contain no credentials, and every control
  maps to a real operation.
- ADR-0046's decision-register status is corrected to Accepted.

## Non-goals

- Requester portal sessions, requester-safe projections and append-only
  persistence (Stage B).
- Any Jobs bridge, queue, worker, schedule or provider contact (Stage C/D).
- Changing the semantics of the existing requester-facing `listTickets`
  operation beyond fixing the cursor character-set defect.
- Operator Jobs CLI, email ingress, productivity features, automation.

## Evidence

Recorded below before task finish; commands and exact results only.

### Implementation evidence

- Reproduced Phase 2 findings from source: cursor encoding emits `.` which
  `minco_http::Cursor::parse` rejects (`http.rs` `encode_cursor`,
  `crates/minco-http/src/resource.rs` `Cursor::parse`), agent list returns
  full aggregates oldest-first (`persistence.rs` `list` `ORDER BY updated_at,
  id`), `TicketMessage.author_subject` serializes into requester projections
  (`model.rs` `requester_projection` filters only `InternalNote`), bootstrap
  hard-codes capability claims (`http.rs` `bootstrap`), memory store is the
  plugin default (`plugin.rs` `Default`), and no agent console surface exists.

### Verification evidence

All commands run on 2026-08-23 in the `minco-task-m14-t45` JJ workspace,
based on ticketing head `bb1e37d2` + the 1.12.0 `contract sync` header
regeneration:

- `cargo test -p minco-plugin-ticketing --all-features --locked`
  — ok, 38 passed, 0 failed (memory + real temporary SQLite engines).
- Feature matrix (`--locked`): default (31 passed), `--no-default-features`,
  `--features http`, `--no-default-features --features sqlite` (23 passed,
  real SQLite), `--features full` — all ok, 0 failed.
- `cargo clippy -p minco-plugin-ticketing --all-targets --all-features --locked -- -D warnings`
  — clean.
- `cargo clippy -p minco -p cargo-minco --all-targets --all-features --locked -- -D warnings`
  — clean.
- `cargo test -p minco --all-features --locked` — ok.
- `cargo minco plugin validate` — `[]` (no findings).
- `cargo package -p minco-plugin-ticketing --locked` — packaged and verified.
- `cargo minco contract sync --check` — passes against the regenerated
  `examples/orders/api/src/generated.rs`.
- `rustfmt --check --edition 2024` on every changed `.rs` file — clean after
  formatting only the changed files.
- Stub-marker scan (`TODO|FIXME|todo!\(|unimplemented!\(|placeholder|fake
  success`) over `plugins/minco-plugin-ticketing` — one hit: the HTML
  `placeholder` attribute of the search input (legitimate UI attribute).
- `./scripts/quality.sh` — `OK` (after regenerating the deterministic
  `docs/reference/generated/plugins.md` via `scripts/docs/generate-reference.sh`;
  `--check` now reports "generated reference is current").

### Additional correctness fix found and fixed

- `TicketingService::create_ticket` derived the display reference from the
  first 12 hex characters of a v7 UUID, which encode only the millisecond
  timestamp: two tickets created within the same millisecond collided with
  `DuplicateDisplayReference` (surfaced as HTTP 503). The reference now uses
  the full v7 suffix. Proven by the multi-ticket pagination tests that
  originally failed with 503.

### Known gate failure (recorded, not converted to a pass)

`cargo minco check --with-cargo` (the `task-finish` gate) fails on exactly
one command:

```
Error: quality gate failed: uv run --locked python scripts/validate_operational_evidence.py --check-output verification/operational-evidence-validation.json
```

Cause: `verification/1.9-performance-baseline.json` binds measured hosted
performance to source-tree digest `2ac2b816…`, which equals the published
main manifest digest. Any source change — including this task — makes the
current tree digest differ, so the receipt reports
`PERF-BASELINE-003: performance baseline source does not match the current
verified tree` and exits 1 until the baseline is re-measured and re-bound to
the new exact tree. Re-measuring requires hosted performance runs (provider
contact), and editing the digest without measuring would falsify release
evidence; both are out of scope for this task. The release-bound
`verification/source-manifest.json` and
`verification/operational-evidence-validation.json` were restored to their
committed values; the deterministic `verification/static-validation.json`
refresh (task count 124→125) is included. Every other `check --with-cargo`
gate command passed, and `./scripts/quality.sh` exits `OK`. The baseline
re-binding belongs to the next release-qualification task.

### Known limitations (truthful)

- Browser-level qualification (Playwright journeys, accessibility passes,
  200% zoom, screen-reader verification) was **not** run in this task; the
  console is qualified by HTTP contract tests and asset-header tests only.
  No browser test harness exists for the ticketing plugin yet.
- The legacy `listTickets` operation remains oldest-first full-aggregate by
  contract; only its cursor character-set defect was fixed. Retiring it is
  Stage B work.
- `last_activity_at` is the newest message timestamp; a ticket without
  messages reports `null`.
- Activity intents are still persisted without a dispatch lifecycle (finding
  unchanged; Stage B/C scope).

