# Serve browser and native clients from one API

Minco applications should expose one reviewed OpenAPI contract and one set of
application use cases to browser, iOS, Android, desktop, automation, and
server-to-server clients. A "mobile API" is not a second business API. Native
clients mainly change authentication, retry, compatibility, payload, and
device-trust constraints.

## Browser and native client differences

| Boundary | Browser client | Native client | Minco application policy |
|---|---|---|---|
| Authentication | Bearer tokens or secure cookies; cookie sessions require CSRF protection | OAuth public client; use the authorization-code flow with PKCE in an external user agent and do not embed a client secret | Validate identity at the provider or gateway, inject a provider-neutral `Principal`, and authorize inside the application use case |
| Cross-origin policy | The browser enforces CORS and restricts script access to response fields | Native networking stacks are not protected by browser CORS | Keep exact browser origins and headers; never treat CORS as authentication |
| Connectivity | Often foreground and quickly recoverable | Radio changes, suspension, duplicated delivery, and offline queues are normal | Make writes replay-safe, return explicit retry timing, and keep synchronization domain-specific |
| Release cadence | A deployed frontend can usually move with the API | Installed versions can remain in use through app-store review and delayed user upgrades | Prefer additive contracts, compatibility checks, and explicit deprecation and sunset metadata |
| Device signal | Browser risk controls are application-specific | App Attest or Play Integrity can add device/app-instance evidence | Treat attestation as optional defence in depth for selected high-risk actions, never as user authentication or authorization |

## Authentication profile

Native apps are public OAuth clients. Use a system browser or platform
authentication session for the authorization-code flow with PKCE. Do not put a
client secret in an iOS or Android package: an installed application cannot keep
one confidential.

On the default AWS path, Amazon Cognito or another OIDC provider can issue the
tokens and API Gateway's JWT authorizer can reject invalid issuer, audience,
expiry, and scope claims before invoking Lambda. `minco-aws-lambda` maps the
verified claims into Minco's provider-neutral `Principal`. Application use cases
still own permission checks and business authorization.

A protected operation should return an RFC 6750 bearer challenge as well as a
stable Problem Details body:

```rust
use axum::response::IntoResponse;
use http::StatusCode;
use minco_http::{ApiFailure, ApiResponseMetadata, BearerChallenge};

let response = ApiResponseMetadata::new()
    .bearer_challenge(BearerChallenge::InvalidToken)
    .wrap(ApiFailure::new(
        StatusCode::UNAUTHORIZED,
        "invalid_token",
        "Invalid access token",
        "The access token is missing, expired, or invalid.",
        "request-1",
    ))
    .into_response();
```

Browser applications that deliberately use cookies should enable the exact
cookie/CSRF boundary with `HttpHeaderPolicy::enable_cookie_csrf` and exact
credentialed origins. Native apps should store refresh credentials in the
platform-protected credential store and must not use development identity
headers.

## Retry and offline safety

Clients should not retry every failure uniformly.

- Safe reads can use bounded retries for transport failures.
- A create or command can be retried only with the same `Idempotency-Key` and
  the same logical payload. A new intent needs a new key.
- Updates and deletes should use the last strong `ETag` as `If-Match`. A `412`
  response means the client must fetch the current state and resolve the
  conflict instead of overwriting it.
- `429` and temporary `503` responses should include `Retry-After`. Clients
  should respect it and use bounded exponential backoff with jitter.
- Validation, authorization, and business-rule failures are not transient.
  Token refresh should be bounded to one coordinated attempt rather than
  creating a retry loop.
- Offline queues, merge rules, tombstones, and conflict UI remain explicit
  application behavior. Minco does not invent generic synchronization policy.

```rust
use http::StatusCode;
use minco_http::{ApiFailure, ApiResponseMetadata};

let response = ApiResponseMetadata::new()
    .retry_after_seconds(30)
    .wrap(ApiFailure::new(
        StatusCode::TOO_MANY_REQUESTS,
        "rate_limited",
        "Too many requests",
        "Retry this operation after the indicated delay.",
        "request-2",
    ));
```

Use `retry_after_seconds` for delay-seconds. An application that already has a
validated HTTP-date can attach it with `retry_after`.

Minco's resource convention already supplies bounded opaque cursors,
idempotency-protected creates, strong entity tags, conditional writes, stable
Problem Details, and request IDs. Keep page sizes bounded and allow compression
rather than creating a separate mobile response shape by default.

On AWS, do not start a new product on Amazon Cognito Sync: AWS stopped accepting
new Cognito Sync customers on 30 July 2026 and recommends AppSync or DynamoDB as
alternatives. Even with a managed service, merge policy and conflict UX remain
application decisions rather than framework defaults.

## Payloads and background transfer

Keep the JSON API as a bounded control plane. Large media, documents, exports,
and background transfers should normally use the object-storage plugin's
`ObjectAccessService` to obtain a short-lived, narrowly scoped presigned request
instead of proxying bytes through Lambda.

- Authorize the object key, content type, maximum size, and expiry before
  signing.
- Prefer a signed multipart POST when the provider must enforce an upload-size
  range; use a signed PUT only when the same policy is enforced elsewhere.
- Let the native platform's background transfer service send the bytes directly
  to object storage, then call a replay-safe finalize operation that verifies
  ownership, object metadata, and any required digest.
- Configure the storage bucket's browser CORS separately from the API's CORS;
  neither policy is authentication.
- Never log presigned URLs, signatures, form values, or temporary credentials.

This preserves the zero-idle architecture, avoids Lambda payload and duration
limits, and lets mobile transfers survive foreground suspension without giving
the application broad storage credentials.

## Browser interoperability

Native clients can read any response field, while browser JavaScript can read
only CORS-safelisted fields or fields named by `Access-Control-Expose-Headers`.
The default `HttpHeaderPolicy` therefore:

- allows `Authorization`, `Content-Type`, `Idempotency-Key`, `X-Request-ID`,
  `If-Match`, and `If-None-Match`; and
- exposes `ETag`, `Location`, `Retry-After`, `WWW-Authenticate`, `Link`,
  `Deprecation`, `Sunset`, and `X-Request-ID`.

The AWS HTTP API ingress must carry the same boundary. When API Gateway CORS is
configured, API Gateway owns the CORS response and ignores CORS headers returned
by Lambda. Minco's SAM renderer therefore adds the conditional request fields
and the standard exposed response fields to the gateway configuration as well
as the Axum runtime. A product or plugin that introduces a custom response
header still needs explicit reviewed ingress support; adding it only to
`HttpHeaderPolicy` is not enough for a hosted browser client.

Those are standard cross-client protocol fields. Plugin- or product-specific
headers must still be contributed explicitly through the owning
`HttpHeaderPolicy`; wildcard origins and wildcard headers remain unsupported.
Every operation that emits or requires one of these fields must also declare it
in the application's canonical OpenAPI document. The transport helper does not
silently mutate the contract.

## Compatibility for installed apps

Treat an old installed app as a current production client until telemetry and a
reviewed support policy prove otherwise.

1. Make additive response fields optional to consumers and preserve the
   meaning of existing fields.
2. Avoid changing an operation from optional to required input without a new
   compatibility boundary.
3. Run Minco's OpenAPI compatibility checks before promotion and keep old
   operations available for the documented support window.
4. Announce lifecycle changes through documentation and standard response
   metadata. Do not rely only on an `X-App-Version` gate.
5. Remove behavior only after the sunset date, observed client adoption, and
   rollback/data compatibility have been reviewed.

`Deprecation` is an RFC 9745 Structured Field Date. `Sunset` is an RFC 8594
HTTP-date and must not precede the deprecation instant. `Link` can point clients
to migration instructions or a successor operation:

```rust
use http::HeaderValue;
use minco_http::ApiResponseMetadata;
use std::time::{Duration, UNIX_EPOCH};

let metadata = ApiResponseMetadata::new()
    .deprecation_at(UNIX_EPOCH + Duration::from_secs(1_861_920_000))?
    .sunset(HeaderValue::from_static(
        "Tue, 01 Jan 2030 00:00:00 GMT",
    ))
    .link(HeaderValue::from_static(
        r#"<https://api.example.invalid/migrations/orders-v2>; rel="deprecation""#,
    ));
# Ok::<(), minco_http::ApiResponseMetadataError>(())
```

The application owns semantic validation, dates, and migration policy. Minco
formats the deprecation instant and attaches the caller-supplied transport
metadata.

## Optional app-integrity signals

Apple App Attest and Google Play Integrity can help a backend assess whether a
sensitive request came from a legitimate app instance. Bind the platform
assertion to a server challenge or stable request digest, verify it on the
server, prevent replay, and degrade safely when the platform signal is
unavailable. Apply it selectively to abuse-sensitive operations because it
adds latency, provider dependency, rollout, privacy, and recovery work.

These signals supplement TLS, OAuth token validation, authorization,
idempotency, audit, and rate limits. They do not identify the user and must not
become the only route to account recovery or ordinary access.

## Readiness checklist

A Minco API is ready for native clients when:

- one canonical OpenAPI 3.1 contract covers every supported frontend;
- native authentication uses authorization code plus PKCE without an embedded
  secret;
- authorization remains in application use cases;
- unsafe retries use idempotency keys and conditional writes;
- failures are stable `application/problem+json` documents with request IDs;
- throttling and temporary unavailability return actionable retry metadata;
- pagination and payload bounds work on slow or metered links;
- large transfers use short-lived direct object capabilities rather than broad
  storage credentials or Lambda byte proxying;
- browser CORS exposes the standard fields its JavaScript client must read at
  both the application runtime and any authoritative ingress layer;
- contract evolution accounts for delayed app upgrades; and
- device attestation, push, deep links, offline sync, and background transfer
  are added only when the product actually needs them.

## Standards and provider references

- [RFC 8252: OAuth 2.0 for Native Apps](https://www.rfc-editor.org/rfc/rfc8252)
- [RFC 9700: OAuth 2.0 Security Best Current Practice](https://www.rfc-editor.org/rfc/rfc9700)
- [RFC 6750: OAuth 2.0 Bearer Token Usage](https://www.rfc-editor.org/rfc/rfc6750)
- [RFC 9457: Problem Details for HTTP APIs](https://www.rfc-editor.org/rfc/rfc9457)
- [RFC 9110: HTTP Semantics](https://www.rfc-editor.org/rfc/rfc9110)
- [RFC 6585: Additional HTTP Status Codes](https://www.rfc-editor.org/rfc/rfc6585)
- [RFC 9745: The Deprecation HTTP Response Header Field](https://www.rfc-editor.org/rfc/rfc9745)
- [RFC 8594: The Sunset HTTP Header Field](https://www.rfc-editor.org/rfc/rfc8594)
- [Amazon Cognito authorization endpoint](https://docs.aws.amazon.com/cognito/latest/developerguide/authorization-endpoint.html)
- [API Gateway HTTP API JWT authorizers](https://docs.aws.amazon.com/apigateway/latest/developerguide/http-api-jwt-authorizer.html)
- [API Gateway HTTP API CORS](https://docs.aws.amazon.com/apigateway/latest/developerguide/http-api-cors.html)
- [Amazon Cognito Sync availability change](https://docs.aws.amazon.com/cognito/latest/developerguide/cognito-sync.html)
- [Apple App Attest](https://developer.apple.com/documentation/devicecheck/establishing-your-app-s-integrity)
- [Google Play Integrity standard requests](https://developer.android.com/google/play/integrity/standard)
