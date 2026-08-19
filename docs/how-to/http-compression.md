# Compress HTTP delivery without adding a proxy

Minco does not need an always-running Nginx tier to obtain the useful part of
`gzip on`. The AWS-native topology already has two separate compression
boundaries, and each boundary should use the cheapest component that already
owns the bytes.

## Current delivery paths

| Traffic | Minco path | Compression owner |
|---|---|---|
| Dynamic JSON, text, HTML and other API responses | client -> API Gateway HTTP API -> Lambda -> Axum/Tower | `minco-http` negotiated gzip |
| Static JavaScript, CSS, HTML, SVG and similar site assets | client -> CloudFront -> private S3 | CloudFront automatic Brotli/gzip |
| Large uploads and downloads | client -> short-lived private object-storage capability | object format or application-owned transfer; never relay large bytes through Lambda merely to compress them |

The static-site SAM already emits `Compress: true`, enables both
`EnableAcceptEncodingBrotli` and `EnableAcceptEncodingGzip`, and uses HTTP/2 and
HTTP/3 at CloudFront. When a viewer advertises both `br` and `gzip`, CloudFront
can prefer Brotli and cache the encoded variant at the edge.

The dynamic API is intentionally not placed behind CloudFront by default. API
Gateway HTTP API remains Minco's direct managed ingress, and the existing Axum
middleware performs response compression inside Lambda.

## Dynamic response policy

`HttpRuntimeConfig::default()` enables response compression. The standard stack:

- negotiates through the request's `Accept-Encoding` header;
- offers gzip, the one compression codec included in Minco's current Tower HTTP
  feature set;
- uses Tower HTTP's fastest compression level to bound Lambda CPU time;
- compresses known-size responses only when they are at least 1 KiB;
- retains Tower HTTP's exclusions for gRPC, images other than SVG, and
  Server-Sent Events;
- does not recompress a response that already has `Content-Encoding`;
- adds `Vary: Accept-Encoding`; and
- leaves clients that do not advertise gzip on the identity representation.

The 1 KiB threshold is deliberately more conservative than Tower HTTP's generic
32-byte default. Tiny response bodies often save little or become larger after
gzip framing, while Lambda must still spend CPU and transport compressed bytes
through its proxy envelope. Nginx's own documentation uses a 1000-byte minimum
in its example configuration and defaults to gzip level 1; Minco applies the
same low-CPU idea rather than pursuing the smallest possible byte count.

Browsers and normal native HTTP clients handle `Content-Encoding` negotiation
and decompression in their networking stack. Application JavaScript should keep
reading the response as JSON or text; it should not implement a gzip decoder.

## Lambda and API Gateway transport

Tower produces binary gzip bytes and sets `Content-Encoding: gzip`. The official
Rust `lambda_http` adapter treats any response with `Content-Encoding` as a
binary body. Its API Gateway v2 response mapping sets the Lambda proxy binary
flag, allowing API Gateway to carry the encoded bytes to the client rather than
attempting to interpret them as UTF-8 text.

This differs from API Gateway REST API's provider-side compression feature. AWS
documents `minimumCompressionSize` for REST APIs, but the
`AWS::Serverless::HttpApi` / API Gateway v2 resource used by Minco exposes no
equivalent response-compression property. Reusing the existing Tower layer is
therefore the smallest real HTTP API implementation; it adds no proxy, cache,
worker, database, schedule or fixed idle compute.

## Disable compression safely

Compression can expose length differences when a response combines a secret
with attacker-controlled reflected text. This is the BREACH class of risk
called out by Nginx's gzip documentation. Do not reflect credentials, CSRF
secrets, session values or comparable secret material into response bodies.

Disable all dynamic compression for an application when its response model
cannot satisfy that rule:

```rust
use minco_http::HttpRuntimeConfig;

let config = HttpRuntimeConfig {
    compression: false,
    ..HttpRuntimeConfig::default()
};
```

For one response, insert the explicit marker before returning it:

```rust
use axum::response::{IntoResponse, Response};
use minco_http::DisableResponseCompression;

fn response_with_reflected_secret() -> Response {
    let mut response = "application-owned sensitive representation".into_response();
    response.extensions_mut().insert(DisableResponseCompression);
    response
}
```

The marker is local response metadata. It is not serialized, exposed through
CORS or sent to the client.

## Why request-body decompression is not a default

Nginx's ordinary gzip module compresses responses. Compressed request bodies are
a different contract: the client sends `Content-Encoding`, the server must
reject unsupported encodings, and limits must apply to the decompressed size to
avoid compression bombs.

Minco does not enable generic request decompression because:

- browser `fetch` does not transparently gzip ordinary JSON request bodies in
  the same way it negotiates response compression;
- API Gateway's documented native request decompression belongs to the REST API
  compression feature, not the HTTP API resource Minco deploys;
- decompression would require a second, explicit post-decompression size limit
  and carefully ordered authentication/observability behavior; and
- large bytes already use Minco's direct object-transfer path, avoiding API
  Gateway and Lambda payload overhead entirely.

An application with a controlled native client and a measured large-JSON use
case can add request decompression as an explicit protocol feature later. It
must declare supported content codings, return `415 Unsupported Media Type` for
unsupported encodings, cap the expanded body, and test malformed and highly
compressible input. It should not be hidden inside the default web stack.

## Why not add CloudFront in front of every API

CloudFront can automatically compress eligible custom-origin responses with
Brotli or gzip, but putting it in front of the dynamic API is not a free toggle.
It introduces another request hop and resource, requires exact forwarding of
authorization headers, cookies and query strings, and creates a cache-policy
boundary where private or stale API data must never be cached accidentally.

That topology can be valuable for an application that already needs a global
custom-domain edge, WAF integration or carefully reviewed public GET caching.
It should be an opt-in deployment profile with explicit cost, origin-request,
cache, authentication and invalidation evidence. It is not necessary merely to
obtain gzip.

## Why dynamic Brotli and zstd remain opt-in research

CloudFront already supplies Brotli for static assets, where the same compressed
object can be reused many times. Enabling Brotli or zstd inside Lambda would add
codec code and compression CPU to every selected dynamic response. Zstandard is
a registered HTTP content coding and Tower HTTP supports it behind a feature,
but CloudFront automatic compression currently documents gzip and Brotli, not
zstd.

Minco therefore keeps fastest gzip as the universal dynamic baseline. A future
codec expansion must provide exact ARM64 Lambda artifact-size, cold-start, CPU,
latency and transfer-byte comparisons on realistic JSON payloads before it can
become a default.

## Local verification

The focused boundary is covered by:

```bash
cargo test -p minco-http -p minco-aws-lambda --locked
cargo clippy -p minco-http -p minco-aws-lambda \
  --all-targets --all-features --locked -- -D warnings
rustfmt --check --edition 2024 \
  crates/minco-http/src/lib.rs \
  crates/minco-http/src/middleware.rs \
  extensions/minco-aws-lambda/src/lib.rs
```

The tests prove negotiated gzip, the 1 KiB threshold, global and per-response
opt-out behavior, gzip framing, `Vary: Accept-Encoding`, and the
`lambda_http::Body::Binary` transport boundary. They do not claim live AWS
provider behavior, production compression ratios or a frontend performance SLO.

## Primary references

- [Nginx gzip module](https://nginx.org/en/docs/http/ngx_http_gzip_module.html)
- [Amazon CloudFront compressed-file delivery](https://docs.aws.amazon.com/AmazonCloudFront/latest/DeveloperGuide/ServingCompressedFiles.html)
- [API Gateway REST API payload compression](https://docs.aws.amazon.com/apigateway/latest/developerguide/api-gateway-gzip-compression-decompression.html)
- [AWS SAM HTTP API resource](https://docs.aws.amazon.com/serverless-application-model/latest/developerguide/sam-resource-httpapi.html)
- [Tower HTTP compression predicate](https://docs.rs/tower-http/latest/tower_http/compression/struct.DefaultPredicate.html)
- [HTTP Semantics: content codings](https://www.rfc-editor.org/rfc/rfc9110.html#name-content-codings)
