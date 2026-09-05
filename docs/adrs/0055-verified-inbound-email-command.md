# ADR 0055: Verified inbound email job command

## Status

Accepted.

## Context

Stage D needs inbound email as a durable command (ADR-0048/0054 seam). The
continuation contract fixes the shape: raw MIME is authoritative and lives
in object storage; the queue message is only a wake-up; the payload
carries bounded identities and digests, never content; unverified or
malformed content fails closed; ambiguous results are not blindly
retried. The existing `ingest_external_message` use case already provides
idempotent, identity-checked, revision-checked external ingress.

## Decision

1. `ticketing.process-inbound-email` v1 joins the jobs bridge under the
   optional `jobs` feature. Its payload carries identifiers and digests
   only: project, provider, mailbox scope, external message id, content
   SHA-256, raw object key, target ticket id with expected revision, and
   bounded optional threading headers.
2. Envelope policy: profile `ticketing-mail`; dedupe key
   `mail:<sha256(provider|scope|external-id)>` so redelivery returns the
   existing job; overlap key `mailbox:<sha256(scope)>` so one mailbox
   processes serially; partition = project; bounded exponential retry; a
   six-hour deadline so very stale mail never lands on a moved-on
   conversation; causation = correlation id.
3. The handler verifies before it processes, and every failure is
   classified exactly once:
   - raw object missing → permanent `ticketing.inbound_object_missing`;
   - content digest mismatch → permanent
     `ticketing.inbound_digest_mismatch` (unverified content is never
     ingested);
   - unparseable MIME → permanent `ticketing.inbound_mime_invalid` (same
     `mail-parser` crate the notifications plugin uses);
   - no text body → permanent `ticketing.inbound_body_missing`;
   - ingestion then flows through the existing
     `ingest_external_message` use case under an explicitly registered
     worker identity holding only `ticketing.ingest` — the worker cannot
     bypass ticketing authorization; a misconfigured identity is permanent
     `ticketing.ingest_unauthorized`;
   - stale ticket revision → retryable `ticketing.inbound_revision_stale`;
   - external identity conflict (same id, different content) → permanent
     `ticketing.inbound_identity_conflict`; store failures → retryable.
4. Registration is static through the same composition-root function as
   the notification command; a dependencies struct carries the ticketing
   service, the notifications and object-storage services, and the worker
   identity.

## Consequences

- The SES adapter (Stage D2) only has to deposit raw MIME in object
  storage and submit this command with the digest — all verification,
  parsing, classification and idempotency live here.
- Nothing in the base plugin changes: the command exists only under the
  `jobs` feature and adds no topology.

## Alternatives considered

- **Parsing MIME in the future SES adapter** — rejected: the durable
  command is the recoverable unit; classification must live with it.
- **Storing the parsed body in the payload** — rejected: payload bound;
  raw MIME stays authoritative in object storage.
