# AWS plugin adapter research — 2026-07

Research date: 2026-07-25 (Australia/Adelaide).

Context7 was attempted first as required by the repository contract, but its
workspace quota was exhausted. The implementation therefore uses current
official AWS documentation plus the exact generated Rust SDK 1.x sources
resolved by Cargo. No third-party blog is an authority for the contracts below.

## Current Rust SDK baseline

The workspace pins the compatible versions reviewed and compiled here:

- `aws-sdk-s3` 1.140.0;
- `aws-sdk-sqs` 1.105.0;
- `aws-sdk-sesv2` 1.128.0;
- `aws-sdk-cognitoidentityprovider` 1.127.0;
- `aws-sdk-cloudfront` 1.126.0;
- `aws-credential-types` 1.3.0.

Every client is constructed with
[`BehaviorVersion::latest`](https://docs.aws.amazon.com/sdk-for-rust/latest/dg/behavior-versions.html).
Normal SDK endpoint overrides remain the local-emulator seam described by
[AWS SDK for Rust endpoint configuration](https://docs.aws.amazon.com/sdk-for-rust/latest/dg/endpoints.html).

## Decisions

### S3 object access

[AWS Rust presigning](https://docs.aws.amazon.com/sdk-for-rust/latest/dg/presigned-urls.html)
supports time-bounded requests, but a presigned PUT does not enforce Minco's
maximum-upload-size contract. S3 enforces that boundary through a signed POST
policy with
[`content-length-range`](https://docs.aws.amazon.com/AmazonS3/latest/developerguide/sigv4-HTTPPOSTConstructPolicy.html).
The object port therefore models POST form fields explicitly. The policy binds
bucket, key, content type, AES256 encryption, Minco metadata, credential scope,
session token when present, expiry, and size range. Real AWS remains the
authority for policy enforcement; Rustack proves SDK and request shape locally.

Server-side writes include SHA-256 checksum, exact content length, content type,
AES256 encryption, and bounded metadata. Reads recompute SHA-256 and reject
mismatching stored checksum metadata. Bucket names and key prefixes fail closed
against traversal, reserved bucket forms, and IP-address lookalikes.

### SQS publication and outbox

Current
[`SendMessage`](https://docs.aws.amazon.com/AWSSimpleQueueService/latest/APIReference/API_SendMessage.html)
permits messages up to 1 MiB and only the documented Unicode ranges. The
adapter validates serialized bytes before the provider call. FIFO publication
uses the event ID for deduplication and aggregate identity for message grouping;
standard fair-queue grouping is opt-in.

SQS is only the publication transport. PostgreSQL owns transactional outbox
storage. `enqueue_in` joins an application adapter's existing SQL transaction;
claiming uses `FOR UPDATE SKIP LOCKED`, a bounded limit, exact worker ownership,
and explicit expired-lease recovery. Minco creates no schedule.

### SES and webhooks

The SES v2 adapter follows the
[AWS SDK for Rust SES examples](https://docs.aws.amazon.com/sdk-for-rust/latest/dg/rust_sesv2_code_examples.html)
and permits only a configured sender identity and email channel. Header
controls, malformed addresses, oversized rendered bodies, and control-bearing
or oversized links are rejected before `SendEmail`. A bounded real test may use the
[SES mailbox simulator](https://docs.aws.amazon.com/ses/latest/dg/send-an-email-from-console.html)
only when the account already has a verified sender; the test does not create
or verify customer-facing email identities.

Signed webhooks use HMAC-SHA256 over `timestamp + "." + exact JSON bytes`.
Construction resolves and pins only public DNS addresses, disables redirects,
and applies connect/overall timeouts. HTTPS and a DNS host are mandatory in
production. Loopback HTTP exists only inside the test boundary.

### Cognito administration

The provider-neutral administration port maps invite, get, disable, and delete
to Cognito's administrative APIs. Bounded tests set
[`MessageAction=SUPPRESS`](https://docs.aws.amazon.com/cognito-user-identity-pools/latest/APIReference/API_AdminCreateUser.html)
so they never send an invitation. Reserved provider attributes are rejected by
both the application-facing service and direct adapter boundary.

### Private static sites

CloudFront uses
[Origin Access Control](https://docs.aws.amazon.com/AmazonCloudFront/latest/DeveloperGuide/private-content-restricting-access-to-s3.html)
with `SigningBehavior=always` and SigV4. The S3 bucket is encrypted, uses bucket
owner enforcement, and blocks every public ACL/policy path. Its bucket policy
permits only the CloudFront service principal and the exact distribution ARN.
SPA fallback maps 403/404 to the configured index using
[custom error responses](https://docs.aws.amazon.com/AmazonCloudFront/latest/DeveloperGuide/custom-error-pages-procedure.html).

CloudFront custom domains require an existing ACM certificate in `us-east-1`;
Minco does not silently create regional certificates. Route 53 aliases require
an explicit hosted-zone ID and mirror the distribution's IPv6 setting with a
conditional `AAAA` alias. Publication rejects symlinks and repository escapes,
owns only an explicit S3 prefix unless bucket-root ownership is deliberately
enabled, streams files instead of buffering a complete asset in memory, deletes
stale objects only inside that boundary, and invalidates once after successful
synchronization. Releases must serialize publication per bucket/prefix because
S3 synchronization is not a distributed lock.

## Fidelity and cost boundary

| Boundary | Pure/local proof | Rustack proof | Real AWS authority |
|---|---|---|---|
| S3 store and signed requests | policy/signature and metadata tests | put/get/delete, signed POST/GET | POST size enforcement, IAM, regional S3 behavior |
| SQS event publication | size/character/FIFO tests | SDK send/receive | IAM and managed queue behavior |
| PostgreSQL outbox | compiler tests | local PostgreSQL 18 | same PostgreSQL semantics; no AWS service required |
| SQLite stores | in-memory/file behavioral tests | not applicable | not applicable |
| SES | request validation | unsupported | verified identity, sandbox/simulator, IAM |
| Cognito admin | mapping/validation | unsupported | user-pool administrative APIs and IAM |
| Static site | CloudFormation structure/path tests | S3 publication subset | CloudFront OAC, invalidation, template validation |
| Webhook | signed loopback server | not applicable | product-owned HTTPS endpoint |

The selected AWS marker plugin declares provider-managed/storage-only resources
and no fixed compute, NAT Gateway, provisioned concurrency, or hidden wake
schedule. IAM generation keys off explicit AWS provider markers and exact ARNs;
missing resources fail closed rather than widening to `*`.
