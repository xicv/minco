# ADR 0050: Requester-safe public projections and requester API surface

## Status

Accepted.

## Context

The Stage A review (task M14-T45) confirmed the requester-side correctness
gaps of the 1.12 Ticketing surface:

1. `TicketMessage.author_subject` serializes into every requester
   projection. `Ticket::requester_projection` filters only `InternalNote`,
   so the internal agent subject, and internal system events such as
   `status changed to pending_internal`, leak into public responses.
2. Internal workflow vocabulary (`pending_internal`, `pending_requester`)
   is exposed verbatim to requesters, although the handling model
   (ADR-0046/0049) requires the requester to see one coherent public
   service regardless of who or what is handling.
3. There is no requester-scoped list or detail operation: a requester with
   `ticketing.read` calls the agent-shaped `listTickets`/`getTicket`, which
   returns full aggregates and relies on a projection that the review
   showed to leak.

## Decision

1. Requester projections serialize a closed public shape:
   `PublicTicketMessage { id, author: requester|support|system, kind:
   reply|status, body, created_at }` — the author is derived by comparing
   the message's internal subject with the ticket requester's subject, and
   **no internal subject is ever serialized**. Internal notes remain
   invisible, and internal system events are exposed only as neutral public
   `status` entries derived from the public status timeline, not from
   internal transition vocabulary.
2. `RequesterTicket.status` becomes a public label
   (`open | in_progress | waiting_for_you | on_hold | resolved | closed`)
   mapped deterministically from internal statuses:
   `new|open → open`, `pending_internal → in_progress`,
   `pending_requester → waiting_for_you`, `on_hold → on_hold`,
   `resolved → resolved`, `closed → closed`. Internal vocabulary never
   crosses the requester boundary.
3. Three requester operations join the single canonical contract:
   `GET /requester/tickets` (compact newest-first summaries of the
   authenticated requester's own tickets, cursor pagination, filters
   forcibly overridden to the authenticated subject),
   `GET /requester/tickets/{ticketId}` (public projection, own-ticket
   enforced, foreign tickets are 404), and
   `POST /requester/tickets/{ticketId}/replies` (requester-scoped alias of
   the existing enforced reply use case). Authorization reuses the existing
   enforced `ticketing.read`/`ticketing.reply` capabilities plus
   subject-match isolation; no new capability is claimed.
4. This is a breaking change to the requester-facing serialized shape of a
   0.1.0 Beta plugin; the alternative — additive fields with the leaking
   ones deprecated — would keep the leak authoritative, which is the defect
   being fixed.

## Consequences

- `RequesterTicket` consumers (the handoff exchange responses and requester
  reply responses) now serialize public fields only; the plugin OpenAPI
  schema is updated in the same change and parity-tested.
- The agent surface is unaffected: agents keep the full aggregate via
  `ticketing.agent.read`.
- Later Stage B tranches (durable portal sessions, append-only persistence,
  shared idempotency) build on this public shape and are deliberately out
  of scope here.

## Alternatives considered

- **Filtering `author_subject` at serialization time per field** — rejected:
  allowlists of fields to hide are fragile; a closed public type cannot
  leak by omission.
- **Keeping internal statuses and documenting them** — rejected: the
  internal handling model must remain private per the ADR-0046 boundary.
