# minco-aws-adapters

Production adapters for Minco's provider-neutral object-storage, event,
notification, identity-administration, and static-site ports.

Adapters accept normal AWS SDK clients, so `AWS_ENDPOINT_URL` can route supported
S3 and SQS seams through Rustack without emulator-specific application code.
Runtime construction is explicit and makes no network call during plugin
composition.

Enable only the required service features:

```toml
minco-aws-adapters = { version = "1.1.0", features = ["s3", "sqs"] }
```

The `s3` feature includes `S3ObjectStorage`, which composes the object store,
private download signer, exact checksummed POST signer, and `HeadObject`
metadata reader from one bucket/prefix/client configuration. Managed uploads
bind the S3 policy to one generated key, exact byte count, exact media type,
SHA-256 checksum, short expiry, encryption field, and signed Minco attributes.
Single POST uploads are rejected above S3's 5 GiB boundary rather than emitting
a capability that the provider cannot honor.

Prefer `S3ObjectStorage::from_sdk_builder` so the SDK client and manual POST
signer share the generated S3 endpoint resolver, region, credentials, endpoint
override, and path-style decision. Custom endpoints default to path style and
dotted AWS bucket names avoid virtual-host TLS mismatch. Temporary credentials
shorten the bearer capability with a safety skew; invalid or insufficient
credential lifetime fails closed. The compatible `new` constructor is retained
for intentionally preconfigured clients, where configuration drift must be
reviewed explicitly.

Rustack exercises SDK transport, managed issuance/POST, and fail-closed
verification, but does not currently reproduce S3's checksum metadata contract.
The ignored S3-only real-provider test uses a pre-existing bucket, journals
every operation, performs no `GetObject`, and cleans its run-owned keys. It is
never a substitute for explicit authorization to use a target AWS account.

The `full` feature enables S3, SQS, SES v2, Cognito user-pool administration,
signed webhooks, S3/CloudFront static-site publication, and AppSync Events
publication. Select only `appsync-events` for the minimal realtime adapter. It
accepts the exact regional AppSync `/event` endpoint, obtains credentials from
the normal AWS credentials provider, signs for the `appsync` service, publishes
one bounded envelope per call, and never includes provider response bodies or
credential material in its public errors.

The `static-site` adapter consumes an exact `StaticSiteReleaseManifest`. It
uses a conditional `.minco/deployment-lock`, uploads each object with SHA-256,
rechecks S3 checksum/size/media/cache metadata before deleting stale keys, and
waits for the deterministic CloudFront invalidation. A missing or ambiguous
lock cleanup fails closed; the adapter never steals or silently expires a lock.

Register the crate's explicit `AwsAdaptersPlugin` marker with the selected
provider flags so Plan resource/cost intent and least-privilege IAM are derived
from AWS selections, not from generic capabilities that may be backed by
memory. Production migrations and network calls remain explicit operations;
plugin composition performs neither.

See `docs/deployment/aws-plugin-adapters.md` in the Minco repository for local
Rustack, bounded real-AWS, IAM, migration, and cleanup procedures. See
`docs/how-to/object-uploads.md` for the private upload/download golden path.
