# Upload and download private objects

Use direct object transfer for ordinary browser files: the application
**authorizes and signs**, the browser transfers bytes directly to private object
storage, and the application **verifies exact provider metadata before
committing business state**. The application API carries only small metadata,
so Lambda stays out of the byte path and Minco preserves its
zero-provisioned-compute default.

## 1. Configure one closed upload policy

Create the policy in the composition root, never from request data:

```rust
use chrono::TimeDelta;
use minco_aws_adapters::{s3::S3Addressing, s3_storage::S3ObjectStorage};
use minco_plugin_object_storage::{ObjectKey, ObjectUploadPolicy};

let policy = ObjectUploadPolicy::new(
    ObjectKey::parse("tenant-uploads/images")?,
    10 * 1024 * 1024,
    ["image/jpeg", "image/png", "image/webp"],
)?
.with_expiry(TimeDelta::minutes(10))?;

let addressing = S3Addressing::new(region.clone(), endpoint_override)?;
let storage = S3ObjectStorage::from_sdk_builder(
    aws_sdk_s3::config::Builder::from(&shared_aws_config),
    credentials,
    bucket_name,
    "application-owned-prefix",
    addressing,
)?;
plugin_manager.register(storage.plugin(policy)?)?;
```

The wrapper installs the existing `ObjectStoreService` and
`ObjectAccessService`, plus `ObjectMetadataService` and `ObjectUploadService`.
The graph advertises the additional metadata and upload capabilities only for
this managed composition. `from_sdk_builder` is the golden path: the SDK client
and manual POST signer derive region, endpoint override, credentials,
path-versus-virtual-host addressing, bucket, and prefix from one configuration.
Endpoint overrides default to path-style addressing; dotted AWS bucket names
also use path style so HTTPS hostname validation remains sound. The compatible
`S3ObjectStorage::new` constructor remains available when an application
deliberately supplies an already-configured SDK client and signing credentials
separately, but that split configuration must be reviewed for drift.

One managed plugin instance currently installs one `ObjectUploadService` with
one policy. Use a policy narrow enough for one product purpose. Do not combine
avatars, documents, and attachments into a broad prefix/type/size union merely
to share the service; typed multi-profile composition is retained as M14-T08.

## 2. Hash the bounded file

Managed uploads require the complete object's SHA-256 before issuance. For a
small browser file, Web Crypto is sufficient:

```javascript
async function sha256Hex(file) {
  const digest = await crypto.subtle.digest(
    "SHA-256",
    await file.arrayBuffer(),
  );
  return Array.from(new Uint8Array(digest), (byte) =>
    byte.toString(16).padStart(2, "0"),
  ).join("");
}

const issueCommand = {
  content_type: file.type,
  size_bytes: file.size,
  sha256: await sha256Hex(file),
};
```

`file.arrayBuffer()` buffers the whole file. Keep this golden path deliberately
bounded. A large-media client needs a streaming hash and a future multipart
contract rather than increasing the policy until browser memory becomes the
failure mode.

## 3. Authorize, issue, and split the result

The HTTP handler should extract/map, call one application use case, and map the
response. The application use case verifies tenant ownership, purpose, quota,
and business rules before calling the upload service:

```rust
let issued = uploads
    .issue(IssueObjectUpload {
        content_type: command.content_type,
        size_bytes: command.size_bytes,
        sha256: command.sha256,
        attributes: BTreeMap::from([
            ("tenant".into(), command.tenant_id.to_string()),
            ("purpose".into(), "profile-image".into()),
        ]),
    })
    .await?;

upload_repository.insert(
    application_upload_id,
    authorized_owner,
    issued.pending,
    UploadStatus::Pending,
).await?;

return_to_client(issued.grant);
```

`ObjectUploadGrant` contains the provider URL, policy, signature, and possibly a
temporary security token. It is a bearer credential intended for the authorized
client. `PendingObjectUpload` contains only the expected key, media type, exact
byte count, SHA-256, signed attributes, and bearer-capability expiry. That
timestamp does not expire the trusted pending record: an upload accepted before
capability expiry may be verified later. Application-owned retention and
cleanup decide how long pending state survives. Persist only the pending record;
do not store or log the grant.

The generated object key is an extensionless UUIDv7 under the configured
prefix. A client filename never becomes storage identity. Store a separately
validated display filename in application data only when the product needs one.

## 4. Send the exact provider request

For the S3 adapter, `request.method` is `POST`. Add every returned form field,
then the file, without changing names or values:

```javascript
const grant = await issueUpload(issueCommand);
const form = new FormData();
for (const [name, value] of Object.entries(grant.request.form_fields)) {
  form.append(name, value);
}
form.append("file", file);

const response = await fetch(grant.request.url, {
  method: grant.request.method,
  headers: grant.request.headers,
  body: form,
});
if (!response.ok) throw new Error(`upload failed: ${response.status}`);
```

The S3 policy binds the request to the generated key, canonical media type,
exact byte count, SHA-256, short expiry, SSE-S3 field, and signed Minco
attributes. S3 rejects a changed byte count or checksum before accepting the
object.

Do not log the URL, policy, signature, temporary security token, or form values.
Do not add an application API `Content-Type` header to the S3 request; the
browser must produce the multipart boundary. Append the file after all signed
fields.

## 5. Verify before completing application state

After the browser reports success, call a completion use case with the stable
application upload ID. Do not accept an object key or serialized pending record
from the client as authorization. Load the trusted record and verify it:

```rust
let trusted_pending = upload_repository
    .get_pending_for_owner(application_upload_id, authenticated_owner)
    .await?;
let verified = uploads.verify(&trusted_pending).await?;
upload_repository.complete_once(application_upload_id, verified).await?;
```

Verification uses S3 `HeadObject`, not `GetObject`, and checks the issued key,
canonical media type, exact byte count, SHA-256, and signed attributes. Make the
completion transition idempotent. A client can retry the POST or completion
request; the signed checksum means a replay can only write the same declared
bytes to its unique key.

Minco treats user metadata as corroborating evidence only. Managed completion
requires the checksum returned by S3 with checksum mode enabled and fails closed
when it is absent or conflicts with `minco-sha256` metadata. This is why an
S3-like emulator accepting a POST is not, by itself, managed-verification
evidence.

This establishes transfer integrity, not content safety. For hostile input, keep
the object in an application-owned quarantine state and run the required magic
signature checks, safe decoder, malware scanner, or content-disarm process
before promotion. Never serve uploaded HTML/SVG inline, extract an archive, or
execute a file merely because its extension, `Content-Type`, or checksum looks
valid.

## 6. Issue private downloads

Use the existing `ObjectAccessService::sign_get` after authorizing the object:

```rust
let download = access
    .sign_get(PresignGetObject {
        key,
        expires_in: TimeDelta::minutes(5),
        download_file_name: Some("report.pdf".into()),
    })
    .await?;
```

Clients must send the returned method and headers exactly. Keep the bucket
private; do not turn an application object into a public S3 URL merely to make
downloads easier. For justified high-volume delivery, put CloudFront in front
of a separate publication profile rather than changing the private upload
bucket.

The compatibility download filename is deliberately limited to visible ASCII
and excludes quotes, backslashes, slashes, and semicolons before it is placed in
`Content-Disposition`. Store international display names in application data;
an RFC 6266 `filename*` representation remains a separate additive capability.

## S3 deployment checklist

The application-owned bucket should have all S3 Block Public Access controls
enabled, Object Ownership set to bucket-owner enforced, private IAM scoped to
the exact bucket/prefix, and HTTPS-only access. SSE-S3 is the low-cost default;
select SSE-KMS only when its key policy, request cost, quota, and the additional
KMS permissions needed to retrieve checksum metadata through `HeadObject` are
explicitly qualified.

Configure browser CORS for the exact application origins and only the methods
the UI uses (`POST`, and optionally `GET`/`HEAD`). Allow only headers observed in
the browser preflight and expose only response headers the client consumes.
Do not use `*` origins for an authenticated application.

A bucket policy can further restrict stale signatures with `s3:signatureAge`,
require TLS/SigV4, or constrain expected network paths. Test those controls in
the target account because presigned requests execute with the signing
principal's permissions.

Deliberately choose retention, versioning, noncurrent-version cleanup,
quarantine expiry, access logging, and deletion behavior. Before enabling
multipart, add a lifecycle rule that aborts incomplete multipart uploads. S3
bytes, retained versions, requests, logs, and abandoned parts can cost money
while application compute is at zero.

## Boundaries

- `ObjectStore::put/get` buffers complete objects and is for bounded internal
  payloads. Direct transfer is the normal user-file path.
- Managed S3 POST requires an exact SHA-256. `ObjectHead.sha256` remains optional
  because legacy objects and the older compatibility `sign_put` path may not
  have a provider checksum. Metadata alone is never promoted to a verified
  provider checksum.
- One S3 POST is capped at 5 GiB and Minco rejects a larger capability. In
  practice, browser memory and user experience usually require a much lower
  application policy.
- Multipart upload, streaming/range downloads, resumability, transforms,
  malware scanning, and CDN publication are separate application or future
  framework capabilities, not hidden defaults.
