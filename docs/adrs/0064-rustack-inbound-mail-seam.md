# ADR 0064: The inbound mail chain is proven live against local S3/SQS

## Status

Accepted.

## Context

Stage D2 requires a local seam proof with Rustack before any provider
wiring. The inbound chain (real S3 `Records` envelope → wake handler →
raw MIME fetch → threading → durable `ticketing.process-inbound-email`
job) had only ever run against in-memory fakes.

## Decision

1. `scripts/dev/ticketing-mail-seam.sh` proves the chain live against a
   local Rustack stack (s3, sqs, ssm, sts): a foreign-produced raw MIME
   object (no Minco metadata, no content type — exactly what an SES
   receiving-rule drop looks like), a byte-accurate `ObjectCreated:Put`
   envelope delivered through real SQS twice, the worker wake handler
   consuming both deliveries, and exactly one durable job verified in
   real SQLite. The envelope's key is percent-encoded with
   `urlDecodedKey` present, proving the decode path live.
2. The seam exposed a real defect: the S3 object adapter's reads required
   Minco's own object metadata (`minco-attributes`) and a content type.
   Foreign-written objects are now readable — absent metadata decodes to
   empty attributes and absent content type defaults to
   `application/octet-stream`; a present checksum mismatch still fails.
   Integrity of inbound mail is verified from the body digest by the
   ingest use case, never trusted from metadata.
3. SES availability is probed and recorded, never assumed: Rustack 0.9.1
   does not implement SES (`list-identities` unsupported), so the SES
   receiving-rule binding stays plan-level rendering; no provider contact
   happens in this repository.
4. The seam example (`ticketing_mail_seam`) is an explicit composition:
   path-style addressing against the local endpoint, sqlite ticketing +
   jobs stores on one pool, the released `S3ObjectStorage` adapter, and
   bounded SQS polling with delete-on-success.

## Consequences

- At-least-once delivery and dedupe are proven on real services, not
  asserted from fakes; the seam is rerunnable as a regression gate.
- Reading foreign objects through the S3 adapter no longer fails
  closed; Minco-written objects keep their full metadata round-trip.
- The remaining slice-3b work is plan/SAM-level: SES receiving rule,
  bucket notification and IAM/cost/wake-source rendering.
