# ADR 0031: Subscriber-only realtime with AppSync Events

## Status

Accepted.

## Context

Minco applications need low-cost realtime invalidation and review updates
without adding fixed compute, a NAT Gateway, a connection registry or a second
authoritative data store. Research and an executable Pusher Protocol v7 proof
on 2026-08-04 showed that a Rust Pusher-compatible server is feasible, but its
minimal AWS profile requires API Gateway WebSockets, callback Lambda logic,
DynamoDB connection state and application heartbeat traffic.

AWS AppSync Events owns WebSocket connection state, keep-alives, subscription
routing and fanout. Browser subscribers still require an open transport while
they need immediate delivery; no browser technology can receive an unsolicited
event without an open connection or an operating-system push service. AppSync
channels are ephemeral, so realtime cannot replace the application's durable
HTTP and persistence contracts.

The existing `minco-plugin-events` contract represents durable domain events
and transactional outbox dispatch. A UI notification has different targeting,
payload, delivery and recovery semantics and must not weaken that contract.

## Decision

1. Add a separate, statically linked `minco-plugin-realtime` facade with one
   backend operation: publish a bounded JSON envelope to one channel.
2. Prove the facade with two implementations before exposing it: deterministic
   memory publication for local/tests and an IAM-signed AppSync Events HTTP
   publisher in `minco-aws-adapters`.
3. Configure one AppSync namespace segment and use a conservative portable
   channel grammar of one to four additional slash-separated segments. Each is
   1 to 50 ASCII alphanumeric or hyphen characters and starts and ends with an
   alphanumeric character.
4. Default to a 5 KiB serialized envelope limit. Larger limits require explicit
   application configuration and cost review because AppSync meters inbound and
   outbound events in 5 KiB units.
5. The AWS profile uses `AWS_IAM` for backend publication and the application's
   existing JWT issuer/audiences through `OPENID_CONNECT` for browser connection
   and subscription. API keys and browser publication are not supported by the
   minimal profile.
6. Plan IR carries optional typed realtime intent. SAM emits one
   `AWS::AppSync::Api`, one `AWS::AppSync::ChannelNamespace`, exact Lambda
   `appsync:EventPublish` IAM and HTTP/realtime endpoint outputs. Provider
   logging is disabled by default to avoid payload leakage and optional log
   cost. The resource is request-driven and adds no schedules or fixed compute.
7. The browser facade exposes subscription only. It connects while the document
   is visible, disconnects after a bounded hidden grace period, retries with
   bounded jitter, and invokes an application-supplied HTTP resynchronization
   callback after every successful initial or replacement subscription.
8. Live events received during resynchronization are buffered and released only
   after the authoritative HTTP callback completes. Application revisions and
   idempotency remain application-owned; the facade never guesses ordering or
   persists tokens, cursors or payloads.
9. The Rust publisher treats successful AppSync acceptance as ephemeral
   notification acceptance, not durable delivery. Applications that require
   retry durability continue to use the explicit domain-event outbox and may
   project an outbox record into realtime publication.
10. A pure AppSync `onSubscribe` handler fails closed unless the first channel
    segment after the namespace exactly equals the configured OIDC claim
    (default `sub`; tenant applications normally select a tenant claim).

## Cost and operational contract

AppSync connection minutes, connection/subscription requests, inbound events,
outbound fanout and handler invocations are request-priced. The plan reports
connection and 5 KiB operation dimensions without inventing application fanout.
CloudWatch logs, identity, data transfer and any selected handler data source
remain separate dimensions. Zero provisioned compute is not a zero-bill claim.

The browser receives AWS-managed keep-alives and does not send application
ping/pong messages. A connection may be retired by AWS or the network at any
time; reconnect always performs HTTP resynchronization before buffered live
events are released.

## Security

Channel authorization remains application policy expressed by the selected
OIDC claim and the generated fail-closed AppSync namespace handler. Tokens are
obtained from a callback, used only to authorize a connection/subscription and
never written to logs, URLs or browser storage by the facade. Backend
credentials come from the normal AWS credentials provider and SigV4;
configuration contains endpoints and secret names only, never credentials.

Endpoint validation requires HTTPS, the configured Region and an AppSync API
host unless an explicit loopback test endpoint is supplied. Provider response
bodies and credential material never appear in public errors.

## Consequences

- Minco gains realtime invalidation without owning connection infrastructure.
- Frontends replace Pusher-specific concepts with a small Minco subscription
  contract and an explicit resync boundary.
- The minimal profile is AWS-specific at deployment, while the publisher port
  and browser lifecycle contract remain provider-neutral.
- Realtime is deliberately unsuitable for durable audit, workflow state,
  guaranteed delivery, presence, offline notification and large payloads.
- Local composition and conformance make no provider call. Template validation,
  provider deployment, browser runtime and cleanup remain separate evidence.

## Alternatives rejected

- Pusher Protocol v7 on API Gateway: portable and compatible, but materially
  more backend state, callback logic and heartbeat cost for the selected AWS
  profile.
- SSE as the general transport: one-way but still connection-oriented, with
  AWS response-streaming duration constraints and no background delivery.
- Polling only: valid for non-urgent state, but adds latency and repeated reads;
  it remains the reconnect/fallback recovery mechanism.
- API keys in the browser: minimal setup but insufficient for private or
  tenant-scoped channels.
