# ADR 0049: Integration-ready ticketing agent console and agent API seam

## Status

Accepted.

## Context

ADR-0046 made Ticketing portal-first with one privacy-bounded handoff
contract, and deliberately left the handling side open: the same ticket model
serves requesters and agents, but the only agent-facing surface today is the
raw authenticated CRUD operations (`listTickets`, `getTicket`,
`replyToTicketAsAgent`, the four management PATCHes). A review of that surface
(2026-08-23, recorded in task M14-T45) found five properties that block real
agent use:

1. `listTickets` returns full ticket aggregates — every description, message
   body and attachment descriptor — ordered oldest-first, and its cursor
   encoding emits `.` which `minco_http::Cursor::parse` rejects, so any page
   with more results fails with an internal error.
2. The SQLite list reads and decodes `ticket_json` for every row; there is no
   compact projection.
3. Agent handling requires up to four separate PATCH requests (status,
   priority, assignment, queue) to express one management decision, each with
   its own `If-Match`; a late failure leaves earlier partial updates applied.
4. The plugin advertises nine capabilities but no agent-scoped capability
   exists: "agent" access is only `ticketing.read`/`ticketing.manage` on the
   full aggregate, overlapping the requester projection surface.
5. There is no first-party console: `support-entry.js` is requester-facing.

Minco 1.12 adds durable typed work (ADR-0048), but an agent console does not
need it, and pulling Jobs in would add queues, workers and schedules that a
handling surface must not require (ADR-0015's explicit-workers rule and the
zero-hidden-compute rule in AGENTS.md).

## Decision

1. Add an agent API seam as seven new operations in the Ticketing plugin's
   single canonical OpenAPI contract, mirrored exactly in the code descriptor
   and `minco-plugin.json`:
   `GET /_minco/ticketing/agent` (console page), `GET .../agent/console.js`,
   `GET .../agent/console.css`, `GET .../agent/bootstrap`,
   `GET .../agent/tickets`, `GET .../agent/tickets/{ticketId}` and
   `PATCH .../agent/tickets/{ticketId}/management`.
2. Provide and enforce three new capabilities, and no others:
   `ticketing.agent-console` (bootstrap and assets metadata), 
   `ticketing.agent.read` (summary list and detail) and
   `ticketing.agent.manage` (atomic management). We do not add
   `ticketing.agent.create` or `ticketing.agent.reply` in this stage: the
   existing `createTicket`, `replyToTicketAsAgent` and `addTicketInternalNote`
   operations already enforce `ticketing.create`, `ticketing.reply` and
   `ticketing.manage`, and duplicating those gates would recreate the exact
   requester/agent permission overlap this ADR removes. The console reuses
   those operations and its bootstrap reports truthfully which of them the
   authenticated principal may call.
3. `GET /agent/tickets` returns compact summaries — identifiers, subject,
   status/clock, priority, queue, assignee, requester subject, message and
   attachment counts, last-activity timestamp, needs-attention flag and
   revision — and never descriptions, message bodies, object keys, digests,
   audit, AI context or provider data. Ordering is `updated_at DESC, id DESC`.
   The cursor is the existing composite `(updated_at, id)` encoding with the
   character-set defect fixed so `minco_http::Cursor` accepts it; the same fix
   repairs the existing requester-facing list cursor.
4. The store gains a use-case-shaped `list_summaries` port. The SQLite
   implementation selects projection columns and child-table counts only and
   never reads `ticket_json`; a new explicit migration adds the two missing
   projection columns (`subject`, `priority`) with a `json_extract` backfill,
   because altering a shipped migration is forbidden.
5. `PATCH /agent/tickets/{ticketId}/management` is one operation, one
   `If-Match`, one load, one complete validation of the requested change set
   (`status` + `resolution` + `close_reason`, `priority`, `assignee`,
   `queue`) through the existing domain invariants, one save, one activity
   intent and one authoritative ETag. Any validation failure rejects the
   whole request; no partial management update can commit. The four legacy
   single-field PATCHes remain unchanged for compatibility.
6. The console is dependency-free vanilla HTML/CSS/JS served same-origin
   under `/_minco/ticketing/agent`, with `Content-Security-Policy`,
   `X-Content-Type-Options: nosniff` and `Referrer-Policy: no-referrer`, no
   credentials in the browser, no remote fonts/scripts/analytics, native
   controls, keyboard operability, cursor pagination and current-page search.
   Every control calls a real operation; there is no dead UI.
7. This stage adds no Jobs dependency, no worker, no schedule, no queue and
   no provider contact. The base plugin remains usable exactly as before;
   Jobs integration is deferred to the Stage C bridge decision.

## Consequences

- The parity test (`openapi_and_descriptor_operation_inventories_match`)
  keeps the OpenAPI file, descriptor and manifest in lockstep; the plugin
  grows from 17 to 24 operations.
- Requester isolation is unchanged and measurably separated: requester
  operations never accept agent capabilities and the agent summary contains
  no field that requester projections must hide.
- `list_summaries` is a second read port alongside `list`; the legacy list
  stays oldest-first (its contract) until the requester-safe rework (Stage B)
  retires it, and its cursor fix is backward-compatible because the previous
  encoding never produced a valid cursor.
- The SQLite store now maintains `subject`/`priority` projection columns;
  applications upgrading apply migration 0002 as an explicit release
  operation.
- The console is an integration surface, not a customization surface: theming
  stays the application's `support_brand`/`support_label` configuration.

## Alternatives considered

- **Agent views over the existing CRUD operations only** — rejected: keeps
  full-aggregate leakage, the broken cursor and four-round-trip management.
- **A separate agent plugin** — rejected for this stage: one ticket model,
  many entry surfaces (ADR-0046); a second crate adds packaging cost without
  a second implementation to prove the seam.
- **Optional Jobs-backed activity dispatch inside the console task** —
  rejected: violates the one-workspace/one-decision rule and the
  smallest-boundary rule; deferred to Stage C with its own ADR.
