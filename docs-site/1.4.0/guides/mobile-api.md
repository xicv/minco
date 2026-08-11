---
title: Browser and native clients
description: Serve browser, iOS, Android, desktop, automation, and server clients from one Minco API with explicit PKCE, CORS, retry, compatibility, and device-trust boundaries.
---

# Browser and native clients

Minco uses one reviewed OpenAPI contract and one set of application use cases
for browser, iOS, Android, desktop, automation, and server-to-server clients. A
“mobile API” is not a second business API. Native applications change the
authentication, retry, compatibility, payload, and device-trust constraints at
the edge; they do not require another domain model.

> The base behavior shipped in Minco 1.2.0. This frozen **1.4.0 candidate** copy
> preserves that contract while the release remains unpublished.

## What Minco now supplies

The current HTTP and Plan boundaries expose the cross-client protocol fields
that resource APIs already need:

| Boundary | Current behavior |
|---|---|
| Runtime CORS requests | exact `Authorization`, `Content-Type`, `Idempotency-Key`, `If-Match`, `If-None-Match`, and `X-Request-ID` headers |
| Browser-visible responses | exact `Deprecation`, `ETag`, `Link`, `Location`, `Retry-After`, `Sunset`, `WWW-Authenticate`, and `X-Request-ID` headers |
| Response metadata | typed bearer challenges, retry timing, deprecation, sunset, and repeated migration links without changing Problem Details bodies |
| AWS ingress | Plan IR records the same request/exposure inventory and the SAM renderer applies it to API Gateway HTTP API CORS |

API Gateway owns CORS responses when gateway CORS is configured and ignores
Lambda-provided CORS fields. Keeping the inventory in Plan IR prevents the
local Axum and hosted browser boundaries from drifting silently.

Application-specific headers still need explicit runtime, ingress, contract,
and test coverage. Wildcard origins and wildcard headers remain unsupported.

## Browser and native differences

### Authentication

- **Browser:** use a bearer token or a deliberately secured cookie session.
- **Native:** use an OAuth public client with authorization code plus PKCE in an
  external user agent.
- **Application policy:** validate provider identity at ingress, map verified
  claims, and authorize inside the use case.

### Cross-origin policy

- **Browser:** CORS restricts which origins can call the API and which response
  headers JavaScript can read.
- **Native:** the networking stack is not protected by browser CORS.
- **Application policy:** keep origins and headers exact, and never treat CORS
  as authentication.

### Connectivity and retries

- **Browser:** requests are usually foreground and quickly recoverable.
- **Native:** radio changes, suspension, duplicate delivery, and offline queues
  are normal.
- **Application policy:** make commands replay-safe and keep synchronization
  rules domain-specific.

### Release cadence

- **Browser:** a deployed frontend can usually move with the API.
- **Native:** installed versions can remain active through delayed app-store
  upgrades.
- **Application policy:** prefer additive contracts and explicit deprecation
  and sunset metadata.

### Device signal

- **Browser:** risk controls are product-specific.
- **Native:** App Attest or Play Integrity may add app-instance evidence.
- **Application policy:** treat attestation as optional defence in depth, not
  identity or authorization.

## Authenticate a public client

An installed application cannot keep a client secret confidential. Use an
external browser or platform authentication session with OAuth authorization
code plus PKCE. Do not embed a client secret in an iOS, Android, or desktop
package.

On AWS, Cognito or another OIDC provider can issue tokens and API Gateway can
validate signatures, issuer, audience, time claims, and explicitly configured
route scopes before Lambda wakes. `minco-aws-lambda` maps verified claims into
the provider-neutral `Principal`; application use cases still own permissions,
tenant or resource scope, and business authorization.

Minco does not provide a hosted identity provider, login screen, refresh-token
store, or account-recovery policy. Development identity headers are local test
seams and must stay disabled in production.

## Return actionable authentication and retry metadata

Attach transport metadata without changing the stable Problem Details shape:

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

For throttling or temporary unavailability, attach `Retry-After` and keep the
client policy bounded:

- retry safe reads only for appropriate transport or temporary failures;
- retry creates or commands only with the same `Idempotency-Key` and logical
  payload;
- send the last strong `ETag` as `If-Match` for update and delete;
- treat `412` as a refetch-and-resolve event, not permission to overwrite;
- respect `Retry-After` on `429` or temporary `503`, with bounded exponential
  backoff and jitter;
- coordinate at most one token-refresh attempt rather than creating a loop;
- do not retry validation, authorization, or business-rule failures as though
  they were transient.

Offline queues, merge rules, tombstones, and conflict UI remain application
behavior. Minco does not invent a generic synchronization policy.

## Keep large transfers out of Lambda

Use the JSON API as a bounded control plane. The object-storage service has a
typed provider seam for short-lived upload and download capabilities, allowing
an application-owned use case to authorize an object key, media type, size,
expiry, and finalization policy before a native background-transfer service
sends bytes directly to storage.

Minco does **not** currently ship a generic signed-upload HTTP route or finalize
operation. The application owns that contract and policy. Never expose broad
storage credentials or log presigned URLs, signatures, form fields, or
temporary credentials. Browser bucket CORS is separate from API CORS; neither
is authentication.

## Evolve an API for installed versions

Treat an old installed application as a current production client until a
reviewed support policy proves otherwise:

1. Make additive response fields optional to consumers.
2. Avoid changing an operation from optional to required in place.
3. Preserve stable Problem codes and request/correlation identity.
4. Announce a planned change with `Deprecation`, `Sunset`, and migration `Link`
   metadata only after dates and compatibility have been reviewed.
5. Test at least the oldest supported client contract against current server
   behavior.

The response helpers format and attach metadata; the application owns the
dates, semantic validation, migration policy, and compatibility window.

## Cost and trust boundaries

The cross-client additions introduce no worker, schedule, provisioned
concurrency, NAT Gateway, hosted control plane, or fixed application compute.
Provider identity, API Gateway, databases, storage, logs, transfer, and optional
attestation services can still incur their own usage or retained-resource cost.

App Attest and Play Integrity may help assess whether a sensitive request came
from a legitimate app instance. Verify assertions server-side, bind them to a
server challenge or stable request digest, prevent replay, and provide a safe
degradation/recovery path. These signals supplement TLS, OAuth, authorization,
idempotency, audit, and rate limits; they neither identify the user nor grant
business permission.

## Readiness checklist

A Minco API is ready for browser and native clients when:

- one canonical OpenAPI 3.1 contract covers every supported frontend;
- public clients use authorization code plus PKCE without an embedded secret;
- authorization remains in application use cases;
- unsafe retries use idempotency keys and conditional writes;
- failures use stable `application/problem+json` bodies and request IDs;
- temporary failures expose actionable retry timing;
- pagination and payload bounds work on slow or metered links;
- browser CORS is exact at both the runtime and authoritative ingress;
- large transfers use narrowly scoped object capabilities and an
  application-owned control flow;
- contract evolution accounts for delayed installed-client upgrades; and
- attestation, push, deep links, offline synchronization, and background
  transfer are added only when the product needs them.

Continue with [resource API conventions](./resource-api),
[identity and sessions](./identity-and-sessions),
[files and static sites](./files-and-static-sites), and
[AWS deployment planning](./deployment).
