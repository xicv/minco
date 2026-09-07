# ADR 0052: Append-oriented ticket conversation persistence

## Status

Accepted.

## Context

Every ticket save today rewrites the entire aggregate: `update_ticket`
overwrites `ticket_json` and `replace_children` deletes and re-inserts
every message and attachment row. A long conversation is therefore
rewritten in full on every reply — write amplification that grows with
conversation length — and message history has no independent read path:
the only way to read a conversation is to deserialize the whole ticket.

## Decision

1. Ticket rows become the authoritative relational record. Migration
   `0004` adds the remaining ticket columns (description, channel,
   requester display name/email, first-response, waiting, resolved,
   closed timestamps, resolution, close reason) with a `json_extract`
   backfill; followers, tags, source references and resource references
   already live in child tables. Reads (`get`, `list`, transactional
   loads) reconstruct the aggregate from columns plus child tables and
   never parse `ticket_json`. The `ticket_json` column remains as a
   create-time diagnostic snapshot that full saves refresh; it is not
   authoritative.
2. A new use-case-shaped store port, `append_ticket_message`, commits one
   message append atomically: an optimistic-revision-checked projection
   update (status, clock, waiting/resolved timestamps where the domain
   changed them, `updated_at`, `revision`), one message-row insert, and
   the activity intent. Requester replies, agent replies, internal notes
   and external-message ingestion use it; no aggregate rewrite occurs.
3. Management and status saves remain full saves (they legitimately
   change many projection fields at once) and continue to refresh the
   diagnostic snapshot.
4. `GET /_minco/ticketing/requester/tickets/{ticketId}/messages`
   paginates the public conversation independently: newest-first,
   `updated DESC, id DESC`-style cursor over `(created_at, id)`, bounded
   `page[limit]`, internal notes never visible. The agent detail view
   continues to receive the full aggregate through its own operation.

## Consequences

- Write cost per reply is constant instead of proportional to
  conversation length.
- `Ticket` remains the domain aggregate; only the storage strategy
  changes. The memory store keeps whole aggregates (it is the
  deterministic test profile) and implements the same port semantics.
- `ticket_json` may be stale after an append; nothing reads it, and the
  migration records that truth.

## Alternatives considered

- **Event-sourced store** — rejected for this stage: larger than the
  correctness ask; the aggregate stays authoritative.
- **Keeping whole-aggregate saves and only adding pagination** — rejected:
  leaves the write amplification, which is the defect.
