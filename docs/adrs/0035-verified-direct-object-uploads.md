# ADR 0035: Verify direct object uploads through typed application policy

## Status

Accepted

## Context

Minco already exposes a provider-neutral `ObjectStore`, optional direct-access
signing, a memory adapter, and an S3 adapter. The S3 upload path correctly uses
a signed POST policy so the provider can enforce key, media type, byte range,
expiry, encryption fields, and metadata. Applications still had to invent the
surrounding workflow: safe key generation, media allowlists, trusted pending
state, content integrity, and post-upload verification.

Routing browser file bytes through Lambda would centralize those decisions but
would also add payload limits, memory pressure, execution duration, and double
transfer to the default path. Copying Laravel's global `Storage` facade or
runtime disk discovery would conflict with Minco's static typed composition and
inspectable capability graph.

## Decision

Minco retains direct provider transfer and adds a typed, opt-in upload lifecycle
inside the object-storage plugin:

1. the application authorizes an upload before invoking the service;
2. `ObjectUploadPolicy` closes the key prefix, exact media-type allowlist,
   maximum byte count, and short expiry at composition time;
3. the client declares the exact byte count and hexadecimal SHA-256 of the
   complete object before issuance;
4. `ObjectUploadService::issue` generates an extensionless UUIDv7 key, never
   uses the client filename as a key, and adds a signed upload-identity
   attribute;
5. issuance returns an `ObjectUploadGrant`, containing the client bearer
   request, separately from the non-secret `PendingObjectUpload` that the
   application persists as trusted state;
6. the S3 implementation binds its POST policy to the one key, exact media type,
   exact `content-length-range`, SHA-256 checksum, expiry, encryption field, and
   signed attributes;
7. `ObjectUploadService::verify` first revalidates trusted pending state against
   the configured key prefix, canonical media type, byte limit, checksum,
   upload identity and attributes, then performs one metadata lookup and
   accepts only the issued key, media type, exact byte count, SHA-256, and
   attributes;
8. `ObjectMetadataReader` is a provider-neutral port implemented by the memory
   adapter and by S3 `HeadObject` with checksum mode enabled;
9. user metadata can corroborate but never replace the provider checksum; and
10. the S3 SDK's generated endpoint resolver is authoritative for commercial,
    China, dotted-bucket, and custom path-style addressing.

The existing `ObjectStoragePlugin`, `ObjectStore`, `ObjectAccessSigner`, signing
requests, and serialized presigned-request types remain source compatible. The
stronger managed path uses the additive `ObjectUploadSigner` contract.
Applications opt into `ManagedObjectStoragePlugin`; S3 applications use the
`S3ObjectStorage` bundle so storage, private downloads, upload signing, and
metadata lookup share one exact bucket and prefix configuration. The golden
`from_sdk_builder` constructor creates the SDK client and signer from the same
region, endpoint override, credentials, and addressing mode. An upload
capability is shortened to the signing credentials' remaining lifetime with a
safety skew; credentials that are invalid or expire too soon fail closed.

Checksum and metadata verification establish that the stored bytes match the
bytes for which the capability was issued. They are not content inspection.
Applications that accept untrusted documents, images, archives, or executable
formats must add magic-byte validation, safe decoding, malware scanning,
content disarm/reconstruction, or a quarantine/promotion workflow appropriate
to their threat model.

## Consequences

- Ordinary browser uploads avoid Lambda body limits and require no fixed
  compute, NAT Gateway, scheduler, or provisioned concurrency.
- Every managed upload has a non-user-controlled key, exact byte/checksum
  contract, and short-lived bearer capability.
- The bearer grant is not part of trusted persistence by construction. The
  pending record contains no URL, signature, policy, or temporary security
  token.
- Completion verification costs one provider metadata request instead of a
  complete object download.
- Replaying the capability before expiry can only write the same declared bytes
  to its unique key. Application completion endpoints must still be idempotent
  because a client can repeat both upload and completion requests.
- `ObjectHead.sha256` remains optional because legacy objects and the older
  compatibility-preserving `sign_put` path may not have a provider checksum.
  Managed uploads require one and fail closed if S3 does not return it.
- `PendingObjectUpload.capability_expires_at` describes the bearer capability,
  not trusted-state retention. An accepted object may be verified later;
  applications own pending-record expiry and cleanup.
- One managed plugin installs one exact upload policy. Multiple named product
  profiles remain M14-T09 so this change does not introduce a runtime service
  locator or a dangerously broad union policy.
- S3 single POST uploads are rejected above 5 GiB. Multipart upload needs its
  own upload-ID, part-manifest, retry, complete/abort, checksum, and lifecycle
  contracts.
- The byte-buffering `ObjectStore::put/get` remains suitable only for bounded
  application-owned objects. Streaming writes, range downloads, and multipart
  transfer need separate ports rather than widening this compatibility change.
- Browser hashing can require a complete in-memory buffer with Web Crypto. The
  documented direct path is therefore for deliberately bounded files; large
  media should use a future streaming or multipart client.
- Bucket ownership, Block Public Access, exact browser CORS, lifecycle cleanup,
  retention, versioning, and scanning remain explicit application deployment
  policy. Minco documents the required profile but does not silently mutate an
  externally supplied bucket.
- Rustack accepts the signed multipart transport but does not currently
  reproduce S3's provider-checksum metadata. Emulator verification therefore
  proves fail-closed behavior; deterministic policy/signature tests and a
  bounded ignored pre-existing-bucket real-S3 conformance suite cover the exact
  checksum contract.

## Alternatives rejected

### Proxy every upload through the web API

This would make the application runtime a byte relay and weaken Minco's default
cost and performance profile. It remains valid for very small trusted payloads
or transformations that must execute synchronously.

### Add a Laravel-compatible global filesystem facade

A string-selected runtime disk hides dependencies and resource implications.
Typed plugin services and explicit adapters are easier for the compiler, the
application graph, and AI development tools to inspect.

### Persist the complete issued value

The provider request is a bearer credential. Combining it with the trusted
pending record invites accidental database persistence and logging of the URL,
policy, signature, or temporary token. Distinct grant and pending types make the
trust boundary visible to humans, generated code, and serializers.

### Use client filenames or sanitized extensions in object keys

Sanitization does not make a user-controlled filename a stable identity or a
content-type allowlist. Generated extensionless keys remove collision,
traversal, spoofed-extension, normalization, and disclosure concerns. A safe
name belongs in authorized download presentation, not storage identity.

### Treat MIME or extension checks as content validation

The browser-provided content type is an assertion. Even an exact checksum only
binds the issued capability to bytes; it does not establish that those bytes are
safe. The API and documentation keep provider integrity separate from
application content safety.

### Add multipart upload to the same change

Multipart introduces upload IDs, part manifests, retries, completion/abort
semantics, lifecycle cleanup, and a materially larger public API. It should be a
separate task with real-provider conformance and explicit cost evidence.

## Compatibility

This is an additive post-1.0 public API change. Existing object storage, direct
access signing, and S3 construction continue to compile. The old plugin
capability set is unchanged; the managed wrapper adds
`storage.object.metadata` and `storage.object.upload` only when selected.

## Safety

Presigned URLs and form signatures are bearer capabilities and remain redacted
from `Debug`. The service generates keys, rejects reserved metadata names,
requires an exact media allowlist, byte count, and SHA-256, and verifies provider
metadata before application state marks the upload complete. Authorization,
trusted pending-state storage, idempotent completion, content safety, retention,
and deletion remain application-owned.
