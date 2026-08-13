# Object-transfer review, 2026-08

This is design input, not provider proof. Sources were reviewed on 2026-08-14;
the repository's pinned Rust dependencies and local tests remain authoritative
for compilation. No AWS account or application was contacted.

## AWS S3

S3 multipart upload separates initiate, independently retriable part uploads and
ordered completion. Part numbers are 1 through 10,000; a repeated number replaces
the previous part, each ordinary part is 5 MiB through 5 GiB, and checksummed
completion uses consecutive part numbers. The resulting current maximum is
50,000 GiB (about 48.8 TiB), not the older 5 TiB limit.
Applications must retain their own accepted part manifest. Incomplete parts are
billed until completion or abort, so explicit abort plus an incomplete-upload
lifecycle rule is required.

- <https://docs.aws.amazon.com/AmazonS3/latest/userguide/mpuoverview.html>
- <https://docs.aws.amazon.com/AmazonS3/latest/userguide/qfacts.html>
- <https://docs.aws.amazon.com/AmazonS3/latest/userguide/abort-mpu.html>
- <https://docs.aws.amazon.com/AmazonS3/latest/userguide/checking-object-integrity-upload.html>

`GetObject` supports one byte range and conditional validators, not a multi-range
request. Parallel aligned ranges can increase throughput for large objects.
Minco therefore models one exact range per grant and does not promise arbitrary
multi-range proxy behavior.

- <https://docs.aws.amazon.com/AmazonS3/latest/API/API_GetObject.html>
- <https://docs.aws.amazon.com/AmazonS3/latest/userguide/optimizing-performance-guidelines.html>

Presigned URLs are bearer capabilities. A request already transferring may
finish after expiry, while a later reconnect must obtain a new capability.
Transfer Acceleration uses edge locations and multipart can improve distant
clients, but it is a priced, bucket-level opt-in that should be measured with the
AWS comparison tool rather than enabled universally.

- <https://docs.aws.amazon.com/AmazonS3/latest/userguide/using-presigned-url.html>
- <https://docs.aws.amazon.com/AmazonS3/latest/userguide/transfer-acceleration-getting-started.html>
- <https://docs.aws.amazon.com/AmazonS3/latest/userguide/transfer-acceleration-speed-comparison.html>

S3 pricing has independent storage, request/retrieval, transfer, acceleration,
management, replication and transformation dimensions. Account, Region, storage
class, reuse and egress destination are needed for an actual bill.

- <https://aws.amazon.com/s3/pricing/>

## Edge delivery and AWS HTTP runtimes

Private CloudFront delivery requires viewer authorization and restricted origin
access. Cache keys vary with selected headers, cookies and query parameters, so
an edge profile must keep identity-bearing values out of the cache key wherever
the authorization design permits. It is an optional high-reuse profile, not the
storage-only-idle default.

- <https://docs.aws.amazon.com/AmazonCloudFront/latest/DeveloperGuide/private-content-overview.html>
- <https://docs.aws.amazon.com/AmazonCloudFront/latest/DeveloperGuide/private-content-restricting-access-to-s3.html>
- <https://docs.aws.amazon.com/AmazonCloudFront/latest/DeveloperGuide/cache-key-understand-cache-policy.html>

Lambda response streaming is bounded and bills execution duration. API Gateway
response streaming applies to REST APIs, while Minco's golden runtime uses HTTP
API; API Gateway does not stream request bodies. Those constraints support a
direct S3 byte plane for large upload/download rather than a default relay.

- <https://docs.aws.amazon.com/lambda/latest/dg/configuration-response-streaming.html>
- <https://docs.aws.amazon.com/apigateway/latest/developerguide/response-transfer-mode.html>

## Rust and open protocols

Apache `object_store` models stateless atomic writes, conditional update/create,
versioned/ranged reads, multipart abort and a provider conformance suite. Minco
uses the same useful separation but keeps application-shaped ports and static
plugin composition.

- <https://docs.rs/object_store/0.14.1/object_store/>

The tus protocol demonstrates durable upload URLs, acknowledged offsets and
idempotent resume semantics. Relaying tus `PATCH` bodies through Lambda would
conflict with Minco's AWS byte-plane rule, so Minco borrows durable session and
retry semantics while signing direct S3 multipart parts.

- <https://github.com/tus/tus-resumable-upload-protocol>

Laravel's `UploadedFile::isValid` checks that the HTTP upload itself succeeded;
it does not make the content safe. Its storage API can target a local disk or
S3 and generates a unique filename by default. Minco preserves those useful
boundaries—transport validation is separate from content inspection and keys
are generated—but does not relay large default-AWS bodies through application
temporary files.

- <https://laravel.com/docs/13.x/requests#files>

The pinned implementation target is `aws-sdk-s3 1.141.0`, `axum 0.8.9`,
`http-body 1.1.0` and `http-body-util 0.1.5` from `Cargo.lock`. SDK-generated
builders were inspected locally before use; newer online examples are not
assumed to compile against that exact graph.

## Mobile clients

Apple background `URLSession` transfers use file-backed uploads when work must
survive app suspension or termination. Resumable downloads require a range-aware
server and a validator such as `ETag` or `Last-Modified`. Minco therefore returns
stable validators and makes a fresh range grant the portable resume contract;
platform-specific opaque resume data is an optimization, not trusted server
state.

- <https://developer.apple.com/documentation/foundation/downloading-files-in-the-background>
- <https://developer.apple.com/documentation/foundation/urlsession/downloadtask%28withresumedata%3Acompletionhandler%3A%29>

Android background-work quotas and foreground-service policy change over time.
The server contract consequently does not depend on one Android scheduler: it
uses idempotent session operations, exact part receipts, short capabilities and
fresh range grants that WorkManager or an application-selected foreground
transfer service can drive.

- <https://developer.android.com/develop/background-work/services/fgs/changes>
