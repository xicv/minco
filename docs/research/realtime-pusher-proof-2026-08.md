# Realtime transport proof: Pusher protocol, WebSockets, SSE and AWS

Research snapshot: 2026-08-04, Australia/Adelaide. This is a bounded proof,
not a production plugin decision or live AWS qualification.

## Decision

If preserving `pusher-js` is optional, prefer AWS AppSync Events for Minco's
minimal AWS profile and put a small provider-neutral Minco TypeScript facade in
front of the browser transport. This removes more backend infrastructure and
has lower Sydney connection and operation rates. Keep the Pusher-compatible
option only where Laravel Echo/Pusher ecosystem compatibility is itself a
product requirement or where the same wire protocol must run outside AWS.

The trade is concentrated in the frontend: replace `pusher-js` once, preserve
its useful reconnect/subscription ergonomics in the Minco facade, and test that
facade against the AWS transport. AWS documents an Amplify Events client using
`events.connect(...)`; a direct protocol client is also possible but should not
become application-level code. See the
[Amplify Events client guide](https://docs.aws.amazon.com/appsync/latest/eventapi/build-amplify-app.html).

For an AppSync implementation, Minco's Rust backend publishes through an
IAM-authorized adapter while AppSync owns connection state, subscriptions,
heartbeats and broadcast fanout. AppSync can enforce Cognito, OIDC, IAM, API-key
or Lambda authorization and namespace-level publish/subscribe rules. See
[Event API authorization](https://docs.aws.amazon.com/appsync/latest/eventapi/configure-event-api-auth.html)
and [event handlers](https://docs.aws.amazon.com/appsync/latest/eventapi/writing-event-handlers.html).
The current `aws-sdk-appsync` 1.119.0 Rust crate exposes the AppSync control
plane, not an Event API publish data-plane operation, so the Rust adapter still
needs a small SigV4-signed HTTPS publisher (or a separately justified Lambda
integration). This is substantially less backend code than owning connections
and fanout, but it is not zero integration work.

If Pusher compatibility is retained, proceed toward a statically linked Minco
`realtime.pusher` plugin with two explicit implementations behind a
use-case-shaped publisher port:

1. native Axum WebSockets for local and fixed-compute profiles; and
2. API Gateway WebSocket + Lambda + DynamoDB for the minimal AWS profile.

Keep the installed `pusher-js` client. Implement only a named, tested subset of
Pusher Protocol v7 at first: connection establishment, public/private channel
authorization, subscribe/unsubscribe, application ping/pong, server events,
sender exclusion and reconnect/resubscribe. Presence, encrypted channels,
client events, cache channels, user sign-in, webhooks and HTTP fallbacks remain
unsupported until independently justified.

Do not make SSE the general realtime transport. SSE remains useful for bounded,
one-request progress feeds, but it is one-way, does not reuse `pusher-js`, and
API Gateway response streaming is REST-only with a 15-minute maximum. A Lambda
integration remains active for the stream duration. That is a poor fit for
long-lived pub/sub connections and bidirectional protocol liveness.

The two branches compare as follows:

| Concern | AppSync Events + Minco client | Pusher v7 + API Gateway |
|---|---|---|
| Frontend change | One-time replacement and facade | Keep current `pusher-js` |
| Connection/fanout backend | AWS-managed | Minco Lambda/DynamoDB callbacks |
| Authorization | Managed modes plus namespace handlers | Minco auth endpoint and channel rules |
| Sydney connection rate | USD 0.08/million minutes | USD 0.325/million minutes |
| Sydney operation/message rate | USD 1.00/million, 5 KiB units | USD 1.30/million, 32 KiB units below 1B |
| Portability | AWS-specific adapter/client driver | Protocol works on Axum, AWS or fixed compute |
| Local fidelity | Requires a fake/contract harness | Native Axum server exercises same protocol |

The AppSync price advantage assumes small realtime events. AppSync meters in 5
KiB units, while API Gateway meters in 32 KiB units; larger payloads can make
AppSync's per-event cost higher even though its connection minutes remain much
cheaper. Measure the actual payload histogram before choosing on cost alone.

## Current primary-source findings

- Pusher's current documented wire protocol remains version 7. It requires the
  `/app/<key>` path, a double-encoded `pusher:connection_established` data
  object, application-level ping/pong compatibility, signed private channel
  authorization and subscription acknowledgement. See the
  [Pusher Channels protocol](https://pusher.com/docs/channels/library_auth_reference/pusher-websockets-protocol/)
  and [authorization signatures](https://pusher.com/docs/channels/library_auth_reference/auth-signatures/).
- The exact client proved here is
  [`pusher-js` v8.6.0](https://github.com/pusher/pusher-js/releases/tag/v8.6.0),
  released 2026-07-23. The comparison baselines were
  [Laravel Reverb v1.11.0](https://github.com/laravel/reverb/releases/tag/v1.11.0)
  and [Sockudo v4.7.0](https://github.com/sockudo/sockudo/releases/tag/v4.7.0).
- API Gateway does not establish the connection until the `$connect`
  integration completes. A callback handshake therefore has to run after
  `$connect`, probe `GetConnection`, and treat a later `GoneException` as stale.
  See AWS's [`$connect` lifecycle](https://docs.aws.amazon.com/apigateway/latest/developerguide/apigateway-websocket-api-route-keys-connect-disconnect.html)
  and [WebSocket integration event shape](https://docs.aws.amazon.com/apigateway/latest/developerguide/apigateway-websocket-api-integration-requests.html).
- API Gateway WebSocket access logs depend on a Region/account-level
  CloudWatch role. This bounded proof does not create or overwrite that global
  setting; it keeps explicit one-day Lambda logs and detailed API metrics.
  See AWS's [WebSocket logging guide](https://docs.aws.amazon.com/apigateway/latest/developerguide/websocket-api-logging.html)
  and [`AWS::ApiGateway::Account`](https://docs.aws.amazon.com/AWSCloudFormation/latest/TemplateReference/aws-resource-apigateway-account.html).
- API Gateway WebSockets have a 10-minute idle timeout, two-hour maximum
  connection duration, 32 KiB frame limit and 128 KiB message limit. The
  Pusher client must reconnect and resubscribe after gateway retirement. See
  [AWS WebSocket quotas](https://docs.aws.amazon.com/apigateway/latest/developerguide/apigateway-execution-service-websocket-limits-table.html).
- API Gateway meters data messages in 32 KiB increments and connection minutes;
  WebSocket control-frame ping/pong is free, but Pusher's JSON `pusher:ping` and
  `pusher:pong` are data messages. See
  [API Gateway pricing](https://aws.amazon.com/api-gateway/pricing/).
- API Gateway response streaming supports SSE only on REST APIs, for at most 15
  minutes, and has endpoint-specific idle limits. See
  [AWS response-streaming considerations](https://docs.aws.amazon.com/apigateway/latest/developerguide/response-transfer-mode.html).
- AppSync Events is serverless managed pub/sub with its own WebSocket protocol.
  Its published price is USD 1 per million Event API operations and USD 0.08
  per million connection minutes. See
  [AppSync Events](https://docs.aws.amazon.com/appsync/latest/eventapi/event-api-welcome.html)
  and [AppSync pricing](https://aws.amazon.com/appsync/pricing/).

Exact version pins used by the proof are in both Cargo lockfiles and the npm
lockfile. The AWS Rust package pins include `aws-config` 1.10.1,
`aws-sdk-apigatewaymanagement` 1.106.0, `aws-sdk-dynamodb` 1.119.0,
`aws-sdk-lambda` 1.138.0 and `lambda_runtime` 1.3.0. The local server pins Axum
0.8.9 and Tokio 1.53.1. The AWS packages disable legacy default TLS features
and select `default-https-client` plus Tokio explicitly; both Cargo lockfiles
pass `cargo audit`. All packages require no newer than the repository's Rust
1.97.1.

## Cost answer

It is cost-effective for a low-to-moderate, bursty Minco workload because the
minimal AWS shape has no NAT Gateway, provisioned concurrency or fixed compute.
It is not automatically cost-effective for a large, mostly idle, permanently
connected fleet: connection minutes and application heartbeat messages scale
linearly.

The AWS Price List API was queried on 2026-08-04 for `ap-southeast-2`. It
returned USD 0.325 per million API Gateway WebSocket connection minutes and USD
1.30 per million messages for the first billion messages each month. The
corresponding AppSync Events entries were USD 0.08 per million connection
minutes and USD 1.00 per million Event API operations.

Illustrative API Gateway baseline, before Lambda duration/requests, DynamoDB,
logs and data transfer, assuming clients are otherwise idle and therefore
exchange two metered Pusher JSON heartbeat messages every 300 seconds:

| Concurrent clients | Connected pattern | Connection cost | Heartbeat messages | API Gateway subtotal |
|---:|---|---:|---:|---:|
| 100 | 24x7 for 30 days | USD 1.40 | USD 2.25 | USD 3.65/month |
| 1,000 | 24x7 for 30 days | USD 14.04 | USD 22.46 | USD 36.50/month |
| 10,000 | 24x7 for 30 days | USD 140.40 | USD 224.64 | USD 365.04/month |
| 10,000 | 8 hours/day for 30 days | USD 46.80 | USD 74.88 | USD 121.68/month |

Normal application messages reset the Pusher activity timer, so they can reduce
heartbeat traffic, but those application messages are themselves metered. At
10,000 always-connected clients, AppSync Events would reduce the connection
portion from USD 140.40 to USD 34.56 and has a lower operation rate; that saving
must be weighed against replacing `pusher-js` and implementing a different
client contract.

Required production cost gates:

- measure connected-client minutes, messages per publish, fanout distribution,
  payload-size buckets, reconnect rate, Lambda duration and DynamoDB operations;
- model API Gateway/Lambda/DynamoDB against AppSync Events and a small Rust
  fixed-compute service at the observed p50/p95 workload;
- cap channel fanout and publisher batch size, and alarm on stale connection
  cleanup, callback throttling and reconnect storms;
- treat Sydney price-list rates as dated input, not a permanent contract.

## Proof architecture and boundaries

The local proof is a real Axum WebSocket server exercised by the unmodified
browser distribution from the pinned `pusher-js` package. Browser-visible tests
cover connected state, public subscription, typed event routing, valid private
authorization through HTTP, invalid signature rejection, application
ping/pong, two-client sender exclusion, and reconnect/resubscribe with a new
socket ID.

The AWS proof is deliberately smaller. One Rust Lambda handles API Gateway
routes. `$connect` first stores a TTL-bounded connection record, then invokes a
child event and returns. The child probes connection visibility before posting
the double-encoded Pusher handshake, retries throttling or pre-establishment
visibility for at most ten seconds, and abandons a stale connection. The
CloudFormation stack uses on-demand DynamoDB, arm64 Lambda, reserved concurrency
as a blast-radius cap, one-day Lambda logs, detailed API metrics, explicit
throttles, narrow IAM, and delete policies. It intentionally avoids an API
access-log setting because that would require account-global role state. A live
script creates an exact artifact bucket and stack and removes both through an
exit trap.

This does not prove multi-connection AWS fanout, presence, authorization at the
gateway, ordering, delivery guarantees or production scale. It proves the
client protocol slice and deployable AWS boundary needed to justify an ADR.

## Live AWS gate

No provider-capable run was performed for this task slice, and no resources
were created. Exact authority for a non-root IAM role, clean source commit,
stack name, duration, spend and whole-run cleanup scope was not supplied. A
live run requires all of those values, rejects root and non-role callers, and
refuses to contact STS when any authority field is absent:

```bash
AWS_REGION=ap-southeast-2 \
MINCO_REALTIME_PROOF_PROFILE=<profile> \
MINCO_REALTIME_PROOF_ALLOW_ACCOUNT=<account-id> \
MINCO_REALTIME_PROOF_EXPECTED_ROLE_ARN=<iam-role-arn> \
MINCO_REALTIME_PROOF_STACK=<new-explicit-stack-name> \
MINCO_REALTIME_PROOF_SOURCE_SHA=<exact-clean-git-or-jj-commit> \
MINCO_REALTIME_PROOF_MAX_DURATION_MINUTES=30 \
MINCO_REALTIME_PROOF_MAX_SPEND_USD=5 \
MINCO_REALTIME_PROOF_CLEANUP='delete-stack:<stack>;delete-bucket:<stack>-artifacts-<account-id>' \
proofs/realtime-pusher/scripts/run-live-aws.sh
```

The runner rejects a pre-existing stack or bucket, uploads a checksum-verified
versioned artifact, deletes only the returned stack identity and exact object
version, and refuses broad bucket cleanup if unexpected objects exist. Provider
deployment, browser runtime and cleanup must remain separate results.

## Minco production decision gates

Before adding a public extension point, write an ADR that defines a small typed
publisher/subscription contract, supported Pusher subset, delivery semantics,
ordering scope, authentication boundary, observability, quotas, cost model and
fallback behavior. Prove the extension with both the Axum and AWS
implementations. Do not expose API Gateway, Lambda, DynamoDB, Axum or Pusher
types to domain/application crates; select the concrete implementation only in
the composition root.

AsyncAPI 3.1.0 may document channel/message contracts, but the repository's
OpenAPI remains the source of truth for HTTP publication and authorization
operations. AsyncAPI adoption is a separate decision, not a prerequisite for
this proof.
