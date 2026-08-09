# minco-plugin-object-storage

Provider-neutral object storage for Minco plugins and applications. The crate
ships an in-memory conformance implementation for tests and local development;
production applications inject S3, filesystem, or another adapter through the
same `ObjectStore` port.

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
