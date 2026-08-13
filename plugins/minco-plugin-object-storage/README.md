# minco-plugin-object-storage

Provider-neutral object storage for Minco plugins and applications. The crate
ships an in-memory conformance implementation for tests and local development;
production applications inject S3, filesystem, or another adapter through the
same `ObjectStore` port.

For large browser and mobile files, use the additive transfer contracts instead
of the byte-buffering compatibility store. `MultipartObjectService` issues an
immutable generated key and exact 5 MiB-or-larger part plan, signs one
checksummed part at a time, validates provider receipts, requires a consecutive
ordered manifest, and exposes explicit completion and abort. Applications must
persist `PendingMultipartUpload` and the latest trusted receipt for each part;
the provider upload ID is redacted from `Debug` and no presigned request belongs
in trusted persistence.

S3 is the production-targeted transfer adapter; its real-provider conformance
run remains explicit and opt-in. Other providers retain the same validation and
quarantine contracts, but must supply their own streaming, download-signing and
multipart implementations; the compatibility `ObjectStore` alone does not
imply resumable HTTP support.

`ObjectDownloadService` issues a short-lived private full or single-range grant
bound to the current strong entity tag and optional provider version. A stopped
mobile download resumes by requesting a new range grant with the observed
validator. `ObjectReadService` is the provider-neutral streaming seam for native
or deliberately bounded proxy use; dropping the stream is its cancellation
boundary. The minimal AWS path does not relay large bytes through Lambda or API
Gateway.

The optional HTTP module is a control plane only. Its six authenticated JSON
operations initiate, issue parts, complete, abort, issue downloads, and
conditionally read cache metadata through one application-owned
`ObjectTransferHttpUseCases` method per handler. That port owns principal
authorization, quotas, durable session state, immutable revision replacement,
and mapping stable application object IDs to provider keys.

Completed untrusted uploads begin as `ObjectValidationState::Quarantined`.
Checksum, byte count, MIME and provider metadata prove integrity, not safety;
only an application-selected `ObjectContentInspector` can record accepted or
rejected content. Do not serve a quarantined object or treat this API as an
antivirus implementation.

`estimate_object_transfer_cost` exposes storage, incomplete multipart, request,
egress, optional acceleration and optional edge dimensions. It deliberately has
no changing AWS rates and returns an incomplete projection when priced provider
dimensions are present. Direct S3 retains storage-only idle cost; CloudFront and
Transfer Acceleration are optional measured profiles, not defaults.

For ordinary browser uploads, use `ManagedObjectStoragePlugin` and its injected
`ObjectUploadService`. It generates an extensionless UUIDv7 key, applies an
exact content-type/byte-count/expiry policy, requires the complete object's
SHA-256, and verifies provider metadata without downloading the object body.
Issuance deliberately returns two values: send the bearer `ObjectUploadGrant`
to the authorized client and persist only the non-secret `PendingObjectUpload`
in trusted application state.

`PendingObjectUpload.capability_expires_at` is the bearer request's expiry, not
the trusted record's retention deadline. An upload accepted before expiry can
be verified afterward; the application owns pending-record cleanup. Managed
verification also checks that the provider reports the same logical key and
requires a provider checksum rather than trusting user metadata alone.

One managed plugin instance intentionally installs one exact upload policy.
Keep that policy purpose-specific instead of combining unrelated product
limits. Statically composed named profiles are tracked separately in M14-T09.
Applications that need separate principals may use
`ManagedObjectStoragePlugin::new_with_signers` to supply distinct private
download and upload signers without a runtime locator.

The application remains responsible for authorization, ownership/quota rules,
and content inspection or malware scanning required by its risk model. MIME,
filename, and checksum metadata do not establish that an untrusted file is safe
to decode, serve inline, or execute.

See `docs/how-to/object-uploads.md` in the Minco repository for the complete S3
composition, browser hashing/request, verification, CORS, lifecycle, and
security checklist.

For failure-policy tests, `FakeObjectStore` records typed put/get/delete
attempts and consumes operation-scoped failures once. Successful behavior uses
the same `MemoryObjectStore` semantics; a failed put never mutates retained
state. `Debug` reports structure without object bytes or attribute values.
