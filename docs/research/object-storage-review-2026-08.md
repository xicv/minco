# Object storage and file-transfer review — 2026-08

Initial research: 2026-08-07. Senior hardening review: 2026-08-09
(Australia/Adelaide).

## Executive assessment

Minco's pre-change object-storage foundation was **sound for bounded internal
objects and private direct browser transfer**, but it was not yet a complete
everyday upload workflow. The provider-neutral port, S3 POST size policy,
short-lived signed GET, exact key-prefix IAM, server-write checksum, SSE-S3
field, key validation, and credential redaction were stronger than a typical
first implementation. The missing layer was the application lifecycle around
those primitives: generated keys, a closed reusable policy, separation of
client bearer data from trusted pending state, exact byte integrity, and cheap
post-upload verification.

The implemented change closes that bounded lifecycle without proxying bytes
through Lambda or adding an always-on service. It does not declare large-file
streaming, content scanning, multipart resumability, or CDN publication
complete.

The 2026-08-09 hardening pass additionally found that hand-built POST URLs,
credential-expiry assumptions, metadata-only checksum trust, collision-prone
task/ADR numbering, and policy-shape-only tests were not sufficient release
evidence. The final design uses the locked `aws-sdk-s3 1.140.0` generated
endpoint resolver, bounds bearer lifetime by temporary credential expiry,
requires a provider checksum, and tests a fixed independent signature vector.

## Current code path before this change

### Server-side objects

`ObjectStore::put` accepts a complete `Vec<u8>`. The S3 adapter computes SHA-256,
sends exact content length/type, requests SSE-S3, and stores bounded Minco
metadata. `get` downloads and buffers the complete body, recomputes SHA-256, and
checks size/metadata. `delete` is private and prefix-scoped through the adapter.
The memory implementation mirrors the same port for deterministic tests.

This is appropriate for small exports, generated documents, feedback
attachments already bounded by the application, and tests. It is not a
streaming large-file API and should not be placed on an unbounded request path.

### Direct browser objects

`ObjectAccessSigner::sign_put` maps to an S3 POST policy, not a presigned PUT.
That is the correct choice for the existing maximum-size contract because S3
POST policy can enforce `content-length-range`; an ordinary presigned PUT URL
does not express the same body-size condition. The policy binds bucket, key,
content type, encryption, credentials, expiry, and Minco attributes. Signed GET
supports a private, time-bounded download and optional attachment filename.
Signed values are redacted from `Debug`.

Before this task, callers still supplied the complete object key, the client
bearer request and trusted pending state had no separate types, direct POST did
not require a checksum, and there was no provider-neutral `head` operation. A
caller could accidentally reuse a key or persist a signature, while verifying a
completed upload required provider-specific code or downloading the object.

## Laravel 13 lessons and Minco adaptation

Laravel's filesystem API is valuable because one developer model covers local,
S3 and S3-compatible providers, streaming writes, metadata, temporary download
URLs, direct temporary upload URLs, scoped/read-only disks, explicit write
failure behavior, custom drivers, and test fakes.

Minco adopts the *developer outcomes*, not Laravel's runtime architecture:

| Laravel outcome | Minco adaptation |
|---|---|
| `putFile` generates a safe unique name and streams the file | Generate an extensionless UUIDv7 key before signing; the browser sends bytes directly to S3 |
| `temporaryUploadUrl` returns URL and required headers | Return a typed bearer `ObjectUploadGrant`, including every required POST form field |
| `size`, `mimeType`, `lastModified` metadata APIs | Add provider-neutral `ObjectMetadataReader` and `ObjectHead` |
| named/scoped/read-only disks | Keep statically constructed adapters and exact S3 bucket/prefix IAM; advertise capabilities only when selected |
| `throw` makes write failure explicit | Continue typed `Result` errors and fail closed on invalid or incomplete provider metadata |
| fake disks for tests | Extend the deterministic memory implementation with metadata behavior |
| global `Storage` facade and runtime driver extension | Reject; typed injection and static plugin descriptors remain authoritative |

Minco adds stronger workflow contracts where the framework boundary needs them:
exact byte count, mandatory SHA-256, a signed upload identity, and distinct
client-grant versus server-pending types. This produces an AI-friendly golden
path whose policy, trust boundary, key namespace, service types, capability
graph, provider bundle, and verification step are explicit in source and
rustdoc instead of inferred from string configuration.

## External guidance reviewed

- Laravel 13 filesystem documentation: disks, streaming `putFile`, temporary
  URLs/uploads, metadata, visibility, scoped/read-only disks, custom drivers,
  and fakes: <https://laravel.com/docs/13.x/filesystem>.
- OWASP File Upload Cheat Sheet: allowlist extensions/types, do not trust
  `Content-Type`, generate filenames, enforce size/authentication, store outside
  the web root or on a separate host, and use defense in depth:
  <https://cheatsheetseries.owasp.org/cheatsheets/File_Upload_Cheat_Sheet.html>.
- AWS S3 presigned URL guidance: URLs are bearer capabilities; expiry,
  credential scope, signature age, checksum, and network policy matter:
  <https://docs.aws.amazon.com/AmazonS3/latest/userguide/using-presigned-url.html>.
- AWS S3 POST policy and `PostObject`: every submitted field needs a policy
  condition except the documented signature/file/policy exclusions; exact form conditions,
  `content-length-range`, checksum algorithm/value fields, and checksum
  rejection on mismatch:
  <https://docs.aws.amazon.com/AmazonS3/latest/developerguide/sigv4-HTTPPOSTConstructPolicy.html>
  and <https://docs.aws.amazon.com/AmazonS3/latest/developerguide/RESTObjectPOST.html>.
- AWS `HeadObject`: checksum mode returns stored checksums without downloading
  bytes, and SSE-KMS checksum retrieval can require additional KMS permissions:
  <https://docs.aws.amazon.com/AmazonS3/latest/API/API_HeadObject.html>.
- AWS bucket and virtual-host rules: reserved prefixes/suffixes must be rejected,
  dotted bucket names do not match the standard wildcard certificate, and
  China-region endpoints use the `.amazonaws.com.cn` partition suffix:
  <https://docs.aws.amazon.com/AmazonS3/latest/userguide/bucketnamingrules.html>
  and <https://docs.aws.amazon.com/AmazonS3/latest/userguide/VirtualHosting.html>.
- AWS conditional writes: `If-None-Match: *` protects create-only PUT and
  multipart-complete operations, but it is not a condition in the current POST
  workflow:
  <https://docs.aws.amazon.com/AmazonS3/latest/userguide/conditional-writes.html>.
- AWS multipart guidance and lifecycle cleanup: multipart enables parallel,
  retryable parts but incomplete uploads retain billable data until completed,
  aborted, or expired:
  <https://docs.aws.amazon.com/AmazonS3/latest/userguide/mpuoverview.html> and
  <https://docs.aws.amazon.com/AmazonS3/latest/userguide/mpu-abort-incomplete-mpu-lifecycle-config.html>.
- AWS Object Ownership, Block Public Access, CORS, checksums, default encryption,
  transfer performance, and CloudFront private-content guidance were also
  reviewed. Bucket-owner-enforced ownership and Block Public Access are the
  private default; exact CORS is required for browser transfer; SSE-S3 is
  automatic and does not replace access policy; CDN or acceleration should be
  justified by measured delivery needs.
- AWS Lambda quotas were reviewed to keep the application API out of the file
  byte path. Synchronous invocation payload limits make it unsuitable as a
  generic upload relay even before memory, duration, and double-transfer costs
  are considered:
  <https://docs.aws.amazon.com/lambda/latest/dg/gettingstarted-limits.html>.

## Findings by severity

### P0 closed in this task

1. **No framework-owned safe-key workflow.** `ObjectUploadService` now generates
   extensionless UUIDv7 keys and never accepts a client filename or suffix as
   storage identity.
2. **No closed reusable upload policy.** `ObjectUploadPolicy` owns the exact
   content-type allowlist, maximum bytes, prefix, and short expiry.
3. **Bearer capability mixed with trusted state.** Issuance now returns a
   serializable client `ObjectUploadGrant` separately from a non-secret
   `PendingObjectUpload`; the combined `IssuedObjectUpload` is intentionally not
   a wire/persistence type.
4. **No content-integrity binding.** The managed path requires hexadecimal
   SHA-256 and exact declared bytes. S3 signs exact `content-length-range`,
   `x-amz-checksum-algorithm`, and `x-amz-checksum-sha256` conditions.
5. **No trusted upload identity.** Each capability carries a signed reserved
   `minco.upload_id`, and application attributes cannot use Minco's namespace.
6. **No cheap completion verification.** `ObjectMetadataReader` and S3
   `HeadObject` with checksum mode verify media type, exact size, checksum, and
   signed attributes without downloading the body.
7. **Configuration drift between S3 store/signers/head.** `S3ObjectStorage`
   constructs server storage, private downloads, managed upload signing, and
   metadata lookup from one bucket/prefix/client configuration.
8. **Provider-invalid large POST capabilities.** S3 signing now rejects a
   single POST above 5 GiB instead of returning a capability S3 cannot honor.
9. **Weak operational guidance.** The new how-to documents authorization,
   browser hashing/request fidelity, secret separation, idempotent completion,
   private downloads, exact CORS, lifecycle, and content-safety boundaries.
10. **Hand-built S3 POST endpoint.** `S3Addressing` now delegates endpoint
    construction to the locked SDK's generated resolver, including China,
    dotted-bucket path style, and path-style custom loopback endpoints.
11. **Capability could outlive temporary credentials.** Issuance now uses the
    earlier of requested expiry and credential expiry minus a safety skew, and
    exposes typed failures for invalid or insufficient lifetime.
12. **Metadata could masquerade as provider checksum evidence.** S3 metadata is
    now corroborating only; missing or conflicting provider checksums fail the
    managed verification contract closed.
13. **Policy tests did not prove SigV4 output.** A fixed clock, credentials,
    request, exact decoded policy, field coverage, and independently reproduced
    HMAC signature vector make policy/signature drift visible.
14. **Provider maximum rejected too late.** `S3ObjectStorage::plugin` rejects a
    policy over S3's 5 GiB single-POST limit during composition, before any
    request can be authorized or issued.

### P1 retained explicitly

1. **Content inspection and quarantine.** MIME and filename are assertions, and
   a checksum only binds bytes. Applications need purpose-specific magic-byte
   validation, safe decoding, malware scanning, CDR, quarantine, and promotion
   where risk requires them. No always-on scanner belongs in Minco's default
   profile.
2. **Real-provider conformance.** An ignored S3-only, pre-existing-bucket test
   now covers managed issue/POST/verify, provider checksum/metadata, changed
   bytes, changed size/type/attributes, rejected-object absence, missing
   provider checksum, conflicts, and cleanup without `GetObject`. It is not a
   default mutating CI job and remains unexecuted when the bounded AWS
   environment variables are absent. Rustack covers standard transport and
   fail-closed behavior but not successful provider-checksum verification.
3. **Bucket deployment contract.** Object storage consumes an
   application-supplied bucket/prefix. A future explicit Plan IR profile can
   render Block Public Access, ownership, exact CORS, lifecycle, HTTPS-only
   policy, and least-privilege IAM without silently taking ownership of an
   existing bucket.
4. **Completion/outbox semantics.** The framework verifies an object, but the
   application must make completion idempotent and coordinate its database
   transition, audit record, and any processing event/outbox.
5. **Download filename internationalization.** The current safe ASCII filename
   avoids header injection. A tested RFC 6266 `filename*` representation can be
   additive later.
6. **Multiple product upload profiles.** One managed plugin installs one typed
   service and one exact policy. A broad union would weaken prefix/type/size
   separation, so M14-T08 retains statically composed named profiles for
   avatars, images, documents, and attachments with application-owned
   authorization.

### P2 separate capabilities

- streaming server reads/writes with bounded backpressure;
- multipart issue/part/complete/abort, per-part/full-object checksums, and stale
  upload cleanup;
- resumable mobile/browser SDK guidance and persisted part manifests;
- range downloads and high-throughput parallel transfer;
- CloudFront signed delivery for justified download volume;
- optional S3 Transfer Acceleration only after regional benchmark evidence;
- transforms, thumbnails, archive expansion, and asynchronous scanning workers.

## Cost and performance judgment

Direct transfer remains the default because it avoids API Gateway/Lambda body
limits, Lambda memory copies, execution duration, and double data movement. The
application performs small authorization/issuance and completion calls. A
successful upload adds one S3 `HeadObject` verification request; browser hashing
uses client CPU and, with Web Crypto, a bounded in-memory copy.

S3 retained bytes, versions, requests, logs, and incomplete multipart parts
remain storage-only or request-driven cost even while application compute is
zero. CloudFront, KMS, Transfer Acceleration, scanners, queues, and multipart
are not free defaults. They should appear only through an explicit application
profile with measurable traffic/security need and visible resource/cost intent.

## Readiness after this task

For ordinary bounded private files, with authorization before issuance, trusted
pending-state persistence, exact checksum/byte verification, exact CORS,
idempotent completion, and an application-appropriate content-safety process,
the foundation is ready for everyday web application use.

It is not a generic large-media platform. Applications needing resumability,
streaming multi-gigabyte transfer, byte-range delivery, automatic content
scanning, transformations, or CDN publication should implement the retained
P1/P2 capabilities rather than stretching the buffered `ObjectStore` or
single-POST path.
