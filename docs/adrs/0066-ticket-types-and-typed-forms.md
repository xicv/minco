# ADR 0066: Ticket types and typed form answers

## Status

Accepted.

## Context

Stage E (small-helpdesk productivity) begins with the data model: tickets
need a bounded taxonomy and typed, structured intake instead of a single
free-text description. The continuation prompt forbids a generic
automation DSL — forms must stay small, explicit and reviewable.

## Decision

1. `TicketType` is a bounded closed enum — `question` (default so every
   existing requester keeps a valid home), `incident`, `problem`,
   `task`. It is first-class plan-neutral domain data, carried on the
   ticket, the agent summary, and every requester projection.
2. Typed form answers ride `TicketFormAnswer`: a bounded `field_id`
   (`[a-z0-9_-]`, unique, at most 16 answers), a `kind`
   (`text | number | boolean | date_time`), and exactly one value slot.
   `date_time` answers carry an RFC 3339 string; numbers are bounded
   integers within the f64-safe range — floating point is deliberately
   out of contract. Two value slots is a fail-closed validation error;
   the generated request boundary rejects it at extraction (422) before
   the domain check (400 semantics preserved for non-HTTP creators).
3. There is no per-type field registry or form-definition DSL in this
   slice: the answers are typed and bounded, the field vocabulary is the
   application's own. A registry, if ever needed, must arrive with its
   own decision.
4. SQLite migration 0006 stores `ticket_type` and `form_answers_json`
   as columnar authority (ADR-0052): reads reconstruct them without
   touching `ticket_json`; the agent summary list reads `ticket_type`
   from the projection query.
5. Contract: `CreateTicket` and `ExchangeHandoff` accept both fields
   (optional, defaulting); `Ticket`, `TicketSummary`,
   `RequesterTicket`, `RequesterTicketSummary` expose them; the
   generated request boundary is re-synced deterministically.

## Consequences

- Curated views and deadline snapshots (later Stage E slices) can filter
  on the taxonomy through the existing summary machinery.
- Existing clients are unaffected: omitted type defaults to `question`,
  omitted answers yield an empty list.
- Handoff-created (widget) tickets carry the same typed intake as API
  creates.
