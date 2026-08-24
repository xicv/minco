# ADR 0058: Inbound email routing by threading with durable submission

## Status

Accepted.

## Context

Stage D1 (ADR-0055) processes a verified inbound email for a *known*
ticket. The wake path (SES receiving → S3 raw MIME → S3/SQS wake) only
has a message and its threading headers — nothing says which ticket it
belongs to. The continuation contract forbids subject-only threading and
requires `In-Reply-To`/`References` semantics.

## Decision

1. A new store port, `find_ticket_by_message_identity`, resolves a ticket
   id and its current revision from a previously ingested external
   message's `internet_message_id`. The external-message identity table
   (written atomically by the existing idempotent ingress) is the only
   authority; nothing is inferred from subjects or heuristics.
2. The service use case `submit_inbound_email` (jobs feature) takes a
   verified raw-object reference plus bounded threading headers, resolves
   the ticket — `in_reply_to` first, then `references` newest-first, all
   bounded — and submits `ticketing.process-inbound-email` durably with
   the resolved ticket id and the ticket's current revision. Envelope
   policy is exactly the D1 policy (dedupe by provider-scoped external
   identity, overlap per mailbox, partition by project, bounded retry,
   six-hour deadline).
3. Unresolved threading fails closed with
   `ticketing.inbound_thread_unresolved`: no ticket is guessed and no job
   is submitted. A later slice decides new-ticket creation for unmatched
   mail; until then the caller (wake adapter) records and routes it
   explicitly.
4. The ticketing service holds the optional `JobsServices` handle (jobs
   feature) resolved from the service registry at install, reusing the
   composition the jobs plugin already provides — no new topology.

## Consequences

- The future S3/SQS wake adapter only extracts identities and headers and
  calls this use case; all routing semantics live here, engine-neutral
  and testable without AWS.
- Stale revisions at submission time are handled by the job's retryable
  classification, not by re-resolution loops.

## Alternatives considered.

- **Full-text or subject matching** — rejected explicitly by the
  continuation contract.
- **Routing inside the Lambda handler** — rejected: untestable seam,
  duplicated semantics.
