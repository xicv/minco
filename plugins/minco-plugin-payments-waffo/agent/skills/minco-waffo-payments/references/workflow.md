# Waffo payment workflow

## Hosted checkout

Create the pending order in application state first. Use an explicit provider
idempotency key for each typed request and reuse it only when retrying the same
request. An authenticated checkout uses separate token and checkout keys. The
short-lived token may exist in memory and in the URL fragment returned to the
caller, but it must never be logged or persisted by Minco.

The return URL is navigation, not payment proof. Advance application-owned
payment or entitlement state only after a verified webhook or an explicitly
authorized provider query.

## Webhooks

1. Preserve the exact body and signature header.
2. Enforce the configured byte bound.
3. Verify signature and the asymmetric timestamp tolerance before JSON decode.
4. Reject an unexpected Waffo environment or store.
5. Atomically claim the provider, mode, store and delivery identity.
6. Apply one idempotent application-owned projection or command.
7. Return success only after the chosen durability boundary.

## Offline tests and evidence

Use exact Waffo short-ID fixtures and ephemeral RSA keys. Queue responses on
`FakeWaffoTransport` and assert the exact path, idempotency key and body. Cover
tampering, stale timestamps, environment/store mismatch, duplicates,
same-key/different-body conflict, redirect refusal, unsafe checkout URLs,
ephemeral token handling, malformed responses and size limits.

Custom API origins are an explicit trusted-operator test seam. They require an
opt-in flag and cannot be combined with production mode. Local conformance does
not prove a Waffo sandbox, production account, delivery, cleanup or SLO.

At a maintenance release boundary, re-check version-matched documentation,
exact package/tool pins, public-contract compatibility and lane-specific evidence.

At the 1.5 assurance release boundary, keep Waffo offline fakes and measured
local gates separate from a live Waffo account, payment, webhook delivery,
cleanup or production outcome.
