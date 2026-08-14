# ADR 0045: Keep resumable object transfers direct, immutable and inspectable

## Status

Accepted

## Context

ADR-0035 established verified direct uploads for deliberately bounded files.
It intentionally retained a byte-buffering compatibility store and deferred
multipart upload, range streaming, cache behavior and content inspection. Large
browser and mobile transfers now need pause/resume, background execution and
stable validators without turning the Lambda/API runtime into a byte relay.

The provider mechanisms are easy to expose unsafely. A presigned URL is a bearer
capability. Multipart upload leaks storage cost until completed or aborted.
Overwriting a cache-visible key makes old and new bytes ambiguous. MIME,
extension and checksum validation bind metadata and integrity but do not prove
that untrusted content is safe. CloudFront and S3 Transfer Acceleration can
improve specific workloads while increasing topology and price dimensions.

## Decision

Minco separates the application control plane from the object byte plane.
Authenticated HTTP lifecycle operations call one injected application use case
to authorize and retain trusted state. The resulting short-lived capability is
used directly by the browser, iOS or Android client against private storage.
Large bodies do not traverse the minimal Lambda/API Gateway HTTP API.

The object-storage plugin adds four additive contracts:

1. a provider-neutral streaming read port with HEAD metadata, one exact byte
   range, `ETag`/version validators and drop-based in-process cancellation;
2. a checksummed multipart port with generated immutable keys, opaque provider
   upload IDs, exact expected part sizes, replace-by-part-number retry,
   consecutive ordered completion and explicit abort;
3. an application-facing HTTP module whose handlers require a principal,
   preserve request identity and delegate authorization/session transitions to
   one injected use case; and
4. explicit integrity, inspection and cost models that never promote an
   uninspected object or an incomplete price projection into a safety/billing
   claim.

The HTTP completion endpoint accepts at most 3 MiB of JSON, each provider part
`ETag` is bounded to 64 bytes, and a manifest contains at most 10,000 parts.
This admits the complete provider limit while remaining below the golden AWS
API Gateway and synchronous Lambda request limits. Conditional metadata reads
use GET weak comparison for strong tags, weak tags, lists and `*`, but authorize
before deciding whether to return `304 Not Modified`.

S3 is the AWS byte-plane implementation. It uses multipart upload for large
objects, provider-validated per-part SHA-256, server-side encryption, signed
headers, `HeadObject`, one-range `GetObject`, conditional validators and
`AbortMultipartUpload`. The application persists the provider upload ID and
accepted part receipts but never the presigned request. A lifecycle rule that
aborts incomplete multipart uploads is a required deployment fallback because
client cleanup cannot be guaranteed.

Download grants are private and short-lived. A client may stop at any byte and
resume by asking the application for a new range grant bound to the same strong
validator. A transfer already in progress can outlive a URL's wall-clock expiry,
but reconnecting after expiry needs a new capability. Direct responses use
`private` cache semantics by default. Shared edge caching is a separate explicit
profile with origin access control and signed viewer authorization.

Object updates never overwrite the previous cache-visible key. They upload and
validate a new immutable revision, then the owning application conditionally
changes its logical reference with its normal `If-Match`/version rule. Retention
or deletion of superseded revisions remains explicit application policy.

## Cost and performance

The minimal profile has storage-only idle cost and request/transfer usage cost.
Multipart adds initiation, one request per attempted part, completion or abort,
metadata verification and retained incomplete-part bytes. Resume adds range GET
requests but avoids retransmitting accepted bytes. Direct transfer avoids Lambda
duration, memory, API payload and double-transfer costs.

Minco records structural dimensions and missing provider rates; it does not
embed a changing AWS price table or promise an account bill. Transfer
Acceleration and CloudFront remain optional and must be selected only after
measuring client geography, object reuse and cache hit behavior.

## Security and validation

- provider keys are generated and are not accepted from the HTTP client as an
  ownership decision;
- presigned URLs, policies, upload IDs and temporary credentials are redacted
  from `Debug` and excluded from trusted client-visible persistence records;
- exact content type, byte count, checksum, metadata and part manifests fail
  closed before provider completion;
- completed bytes enter quarantine unless the application risk policy explicitly
  accepts an inspection verdict;
- inline display is separate from download authorization and requires a
  purpose-specific safe-media policy; and
- CORS exposes only the exact range/validator/checksum headers needed by the
  selected client flow.

## Consequences

- browsers and mobile background services can retry only failed parts and resume
  downloads from a known validator;
- local/custom applications may stream through the provider-neutral port, while
  the AWS golden path remains a direct capability;
- a stopped direct transfer has no server process to cancel; aborting upload
  state and dropping in-process read streams are explicit, separate actions;
- multipart SHA-256 is a composite of verified part checksums, not automatically
  the hexadecimal SHA-256 of the complete object used by ADR-0035's single POST;
- optional scanning may need a bounded worker and object read cost, but no worker
  or schedule is silently added; and
- real provider behavior remains a separate conformance claim from local unit,
  Rustack, contract and compile evidence.

## Alternatives rejected

### Relay all bytes through Axum on Lambda

This centralizes cancellation and validation but adds runtime limits, execution
duration, memory pressure and double transfer. It is available to a custom
bounded application through the stream port, not the minimal AWS profile.

### Overwrite stable object keys and invalidate caches

Invalidation is an extra request/cost and cannot make every client cache
coherent. Immutable revisions plus a conditional logical pointer make old and
new bytes unambiguous and enable long-lived private/edge caching where selected.

### Mark checksum-verified objects safe

Checksums prove integrity, not benign content. Inspection is an explicit
application verdict so malware scanning, magic-byte validation and safe decoding
can match the actual threat model.

### Enable acceleration or an edge distribution by default

Both can improve particular global or high-reuse workloads, but both add cost
and configuration. The storage-only-idle direct path remains the default.

## Compatibility

The existing `ObjectStore`, `ObjectAccessSigner`, single-request upload service
and serialized grants remain available. Streaming, multipart, HTTP lifecycle,
inspection and cost contracts are additive capabilities. M14-T09 still owns
static named upload profiles and is not replaced by this decision.
