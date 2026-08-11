# ADR-0039: Keep Waffo payments provider-specific and application-owned

- Status: Accepted
- Date: 2026-08-11

## Context

Applications need a small, reviewable route to Waffo Pancake hosted checkout,
signed server actions, read-only queries and authenticated webhook delivery.
Those mechanisms do not define an application's order, invoice, subscription,
entitlement, tax, refund or access-control model. Generalising them into a
framework billing domain from one provider would freeze provider vocabulary as
product policy and conceal important authority boundaries.

The provider contract was reviewed against the official Waffo Pancake Go SDK
`v0.9.0` at exact revision
`799135cbe07c45819da0ab4bf777c64fcc956220`. That source review is reference
evidence, not a live-provider qualification.

## Decision

Minco ships `minco-plugin-payments-waffo` as an opt-in beta static plugin. It
provides typed signed actions, guest and authenticated hosted checkout,
read-only GraphQL, standard HTTP webhook verification, a bounded JSON CLI and
deterministic offline fakes. Applications retain authoritative payment and
entitlement state and decide how one verified provider event maps to one
idempotent application command or projection.

The integration preserves these security boundaries:

- configuration is strict and binds the Minco environment class to the Waffo
  test or production mode before resolving a secret;
- private and webhook keys remain opaque references until an explicit client or
  verifier operation needs them;
- production mutations require the persisted production-write guard;
- generic path-selected actions are test-only, while production exposes only
  reviewed typed methods;
- every mutating action uses a caller-supplied provider idempotency key and a
  Minco claim bound to provider mode, origin, merchant, path and body digest;
- short-lived session bearer tokens are redacted, zeroized and never completed
  into generic idempotency storage;
- signed requests use one bounded attempt and never follow redirects;
- a provider checkout URL must be absolute HTTPS without credentials or an
  existing fragment before Minco appends the session token fragment;
- a custom HTTPS API origin is an explicit trusted-operator test seam and is
  rejected with production credentials;
- webhook signatures cover the untouched bounded request bytes and are checked
  before JSON decoding, environment/store binding and durable deduplication;
  and
- provider `aiHint` values are typed as untrusted data and never interpreted as
  agent instructions.

The plugin descriptor declares provider-managed residual cost and HTTP request
wake sources. Static composition creates no request, queue, worker, schedule,
database, fixed compute, AWS resource, retry loop, poller or always-on Minco
control plane. Waffo account fees and application persistence remain outside
Minco's topology estimate and require their own evidence.

The public transport seam exposes the exact prepared URL, canonical path,
merchant, timestamp, signature, optional idempotency key, body and response
bound needed by an application-owned implementation. Its Debug contract
redacts the signature, and retained fake evidence omits signature and URL
secrets.

## Compatibility

The plugin, facade feature, CLI and Waffo Agent Skill are additive in the
lock-step `1.3.0` minor line. The plugin is disabled by default and requires the
official idempotency capability. Official descriptors advance to `^1.3.0` so
checked source and compiled descriptors agree. Existing 1.2 applications and
third-party plugins remain unchanged until they intentionally adopt the 1.3
line.

## Evidence, recovery and rollback

Offline tests prove signing, validation, idempotency behavior, redirect
refusal, response bounds, verified webhook parsing and deterministic fakes.
They do not prove Waffo sandbox behavior, a merchant account, payment success,
settlement, webhook delivery, production readiness or an SLO. Live-provider
evidence remains `NOT RUN` until separately authorised and recorded.

Rollback removes the opt-in facade feature and provider-specific application
composition. Application payment state remains application-owned; Minco does
not delete or rewrite it. A provider action with an ambiguous transport outcome
must be reconciled with the same provider idempotency key or an authorised
query, never retried automatically.

## Alternatives rejected

- A provider-neutral payment, subscription or entitlement abstraction: one
  provider does not establish a stable cross-provider domain.
- Automatic retries, polling or reconciliation workers: they add hidden wake
  sources and can duplicate external mutations after ambiguous outcomes.
- Treating checkout redirects or client returns as payment authority: only a
  verified webhook or explicitly authorised provider query can inform
  application state.
- A remote plugin loader or hosted Minco payment control plane: static typed
  composition and application-owned credentials remain the narrower boundary.

## References

- [Waffo Pancake Go SDK v0.9.0](https://github.com/waffo-com/waffo-pancake-sdk-go/tree/799135cbe07c45819da0ab4bf777c64fcc956220)
- [ADR-0005 static plugins](0005-static-plugins.md)
- [ADR-0033 agent-native development](0033-agent-native-development.md)
- [ADR-0038 local-first Actions boundary](0038-local-first-actions-boundary.md)
