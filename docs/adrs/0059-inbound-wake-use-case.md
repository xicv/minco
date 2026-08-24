# ADR 0059: Engine-neutral inbound email wake use case

## Status

Accepted.

## Context

M14-T56 routes a verified inbound reference to a ticket and submits the
durable ingest job, but something must turn "an object landed in storage"
into that reference: read the raw MIME, compute its digest, and extract
the threading headers. The continuation contract keeps raw MIME
authoritative in object storage and the queue message a wake-up only.

## Decision

1. The wake is an ordinary ticketing use case over the object-storage
   port — engine-neutral, no AWS, Lambda or S3 vocabulary inside the
   plugin. `wake_inbound_email(object_key, arrived_at, provider,
   mailbox_scope, external_id)` reads the object through the registered
   `ObjectStoreService` (a required install dependency that now becomes
   a used one), computes the content digest, parses the MIME headers with
   the same `mail-parser` the D1 handler uses, and extracts `Message-ID`,
   `In-Reply-To` and a bounded, whitespace-split `References` list.
2. The wake submits through the M14-T56 routing use case with the
   extracted threading facts and the wake's arrival timestamp as the
   fingerprint anchor; it passes the extracted `Message-ID` so the
   ingested external identity records the anchor future replies thread
   against. The wake never verifies-and-ingests itself: the durable job
   re-reads, re-verifies the digest and owns ingestion — the wake only
   extracts routing facts from the same authoritative bytes.
3. Missing objects and unparseable MIME fail closed with classified
   service errors; no ticket is guessed and nothing is submitted.

## Consequences

- The future S3-event/Lambda adapter (slice 3) is a thin translation:
  event → (bucket-scoped key, event time, provider identity) → this use
  case; Rustack proves that seam locally without any plugin change.
- The object is read twice per email (wake + job); both reads are from
   the authoritative store and the job re-verifies the digest, so a
   mutated object between the two fails closed in the job.

## Alternatives considered

- **Doing verification and ingestion in the wake** — rejected: the
  durable job exists precisely to own classification and recovery; a
  wake that ingests makes the queue message authoritative.
- **Parsing threading in the AWS adapter** — rejected: untestable seam,
  duplicated semantics.
