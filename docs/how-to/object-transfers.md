# Serve large browser and mobile files

Use the object-transfer HTTP API as a control plane and private S3 as the byte
plane. The API authorizes, validates declarations and changes durable state; the
client sends or receives bytes through the returned short-lived provider
request.

## Compose the services

Create one `S3ObjectStorage` from the exact SDK client, credentials, bucket,
prefix and addressing mode. Keep the existing `ObjectUploadPolicy` for small
single-request files. Add a `MultipartUploadPolicy` with a purpose-specific
prefix, exact media allowlist, maximum size and part size. Sixteen MiB is a
useful starting point: it is comfortably above S3's 5 MiB minimum and limits
per-part request overhead without forcing a mobile client to repeat a very large
part after a connection change.

Construct an `ObjectDownloadPolicy` with a short capability lifetime. Use
`DownloadCachePolicy::Private` only for immutable revision keys; otherwise use
`NoStore`. Call `S3ObjectStorage::transfer_plugin` to install stream, download
and multipart services from the same provider configuration.

The transfer ports and validation states are provider-neutral, but S3 is the
production-targeted byte-plane implementation in this slice. Local tests do not
replace the separate opt-in real-S3 conformance run. A filesystem or other
object provider is not made production-ready merely by implementing the
older buffering `ObjectStore`: it must implement bounded streaming, download
signing and multipart/abort behavior (or expose an application-owned protected
byte endpoint) and pass the same contract tests. Quarantine and inspection
decisions remain the same regardless of provider.

If the application exposes Minco's HTTP lifecycle, inject an implementation of
`ObjectTransferHttpUseCases` with `with_http_api`. That implementation must:

1. authorize the `Principal` for the logical application object and purpose;
2. select a closed static policy, never a bucket/provider supplied by the client;
3. persist pending sessions and latest part receipts without bearer requests;
4. make initiate/complete/abort idempotent under the application's idempotency
   contract;
5. keep completed untrusted bytes quarantined until the required inspector
   accepts them; and
6. map an application object ID to an immutable provider key only after
   conditional update succeeds.

Restore pending state through the typed serde contract rather than rebuilding
fields from a client payload. Opaque provider IDs and object keys are validated
on deserialization, and every part/complete call rechecks the persisted session
against its configured prefix, upload UUID, content policy and part plan before
contacting the provider. Abort retains the narrower identity check so an older
session can still be cleaned up after a policy change.

The same rule applies to a single-request upload: pass only the trusted
`PendingObjectUpload` record to verification. The service rechecks its generated
key, canonical content type and checksum, byte limit, upload identity and
attributes before the provider metadata request. This catches corrupt or
misconfigured retained state without spending a storage request.

## Upload

Call `POST /_minco/objects/uploads`. A small file can receive one exact signed
request. A large file receives `part_size_bytes` and `part_count`.

For every part:

1. read the exact expected bytes from a file-backed source;
2. calculate that part's SHA-256;
3. request its capability from
   `POST /uploads/{uploadId}/parts/{partNumber}`;
4. send the exact content length and all returned signed headers to S3; and
5. send the provider `ETag` and checksum in the completion manifest.

The HTTP completion body is capped at 3 MiB and each provider `ETag` at 64
bytes. That admits S3's maximum 10,000 parts with the required SHA-256 receipts
while keeping the JSON control plane below the synchronous Lambda and API
Gateway request ceilings. File bytes never count toward this body because they
go directly to the provider.

Retry only failed parts. Reissuing the same part number replaces that part; keep
only the latest accepted receipt. Do not use provider `ListParts` output as the
trusted completion manifest.

On cancellation call `DELETE /uploads/{uploadId}`. Configure an S3 lifecycle
rule to abort incomplete multipart uploads after the product's bounded pending
window because mobile clients cannot guarantee cleanup after termination.

For iOS background uploads, provide `URLSession` a file-backed body. Android may
use WorkManager or a policy-compliant foreground transfer service; the server
contract is scheduler-independent.

## Validate and update

Successful provider completion establishes the exact declared parts and
metadata. It does not establish safe content. Keep the application record in
`quarantined` state until the selected risk policy completes magic-byte checks,
safe decoding, malware inspection or another required control.

To replace a file, send the logical object ID and its current `If-Match`. Upload
to a new generated revision key. After validation, conditionally update the
application reference. Never overwrite a cache-visible S3 key. Retain or delete
the superseded revision under an explicit application retention rule.
If the conditional pointer update loses a race, do not publish the new key;
delete it immediately or record it for bounded orphan cleanup so concurrent
mobile updates do not create untracked storage cost.

## Download, stop and resume

Call `POST /_minco/objects/downloads` with the stable application object ID. The
application authorizes it, rejects quarantined/rejected content, resolves the
immutable provider key, and returns the strong entity tag, size, modification
time, cache policy and direct request.

To stop, cancel the client request. There is no Lambda process to terminate. To
resume, retain the acknowledged byte offset and entity tag, then request a new
`from` range capability with `expected_entity_tag`. If the revision changed, the
API fails the precondition so the client does not concatenate different files.
A request already transferring may finish after its signed URL expires; a new
or resumed request after expiry needs a fresh grant.

Use the provider-neutral `ObjectReadService` only when a native/custom runtime
really must inspect or proxy bytes. Consume chunks incrementally and drop the
stream on client disconnect. The minimal AWS profile does not use this as a
large-file HTTP relay.

## CORS and secrets

Allow only exact application origins. For S3 multipart, allow `PUT`, the exact
checksum/content headers returned by the capability, and expose `ETag`. For
direct range downloads, allow `GET` and the signed range/conditional headers as
required by the selected request, and expose `ETag`, `Last-Modified`,
`Content-Range`, `Content-Length` and `Accept-Ranges`.

Keep the completed file in the platform's private file cache under its stable
application object ID plus revision. Before downloading again, call
`GET /_minco/objects/{objectId}` with the cached `ETag` in `If-None-Match`. A
`304 Not Modified` means the local bytes remain current and no new signed URL or
object download is needed. Do not put sensitive files in a shared/public cache.
Weak validators, comma-separated validator lists and `If-None-Match: *` follow
GET weak-comparison semantics; malformed candidates do not match. The
application's authorization still runs before any `304` response.

Keep the two validators distinct. The metadata response `ETag` is an
application representation tag and must change when the immutable object
pointer, validation state or download eligibility changes. The download grant's
entity tag is the provider byte validator used for range resume. Authorization
still runs before every metadata `304`. A private client cache cannot revoke
bytes already downloaded, so use `NoStore` when that product risk outweighs the
repeat-transfer saving.

Presigned URLs, POST fields, provider upload IDs and temporary credentials are
bearer secrets. Redact them from access logs, telemetry, crash reports and
durable application records.

## Cost review

Before deployment, record expected retained bytes, incomplete bytes, upload and
part attempts, metadata requests, download requests and egress in
`ObjectTransferCostUsage`. The projection is structural and purposely marks
current regional prices as missing. Confirm actual S3 storage class, request,
data-transfer and lifecycle rates for the account and Region separately.

Enable Transfer Acceleration only after its speed comparison improves the real
client geography enough to justify its transfer rate. Add private CloudFront
only for measured repeated downloads; use origin access control and signed
viewer authorization, and ensure the cache key does not fragment on irrelevant
identity values.
