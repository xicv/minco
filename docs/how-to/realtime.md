# Add minimal realtime invalidation

Minco's minimal realtime profile uses AppSync Events as an ephemeral wake-up
signal. The backend publishes; a visible browser subscribes; the application
then reloads authoritative state through its existing HTTP API. It is not an
SSE emulation, durable queue, presence service, or client publication channel.

## Select the plugin

Enable the static facade and configure the channel boundary in `minco.toml`:

```toml
[plugins]
enabled = ["health", "observability", "idempotency", "realtime"]

[plugins.configuration.realtime]
namespace = "orders"
max_event_bytes = 5120
subscriber_claim = "tenant_id"
```

Keep `max_event_bytes` at 5120 unless the application explicitly accepts
multiple AppSync billing units per inbound and delivered event. The namespace
is one 1-to-50-character portable segment. A publication channel has one to
four additional segments, such as `tenant-42/order-7`.

`subscriber_claim` is security policy, not a display label. The generated pure
AppSync `onSubscribe` handler requires the first channel segment after the
namespace to exactly equal that OIDC claim. For the example above, only a token
whose `tenant_id` is `tenant-42` may subscribe to
`/orders/tenant-42/order-7`. Use a stable tenant/user identifier that is already
present in the application's verified tokens.

The deployment plan requires the existing `jwt` auth profile. It renders:

- one `AWS::AppSync::Api` and one channel namespace;
- OIDC-only browser connection/subscription and IAM-only publication;
- the fail-closed claim/channel handler;
- exact namespace-scoped `appsync:EventPublish` Lambda IAM;
- HTTP and WebSocket endpoint outputs, with no API key, schedule, NAT Gateway,
  fixed compute, provisioned concurrency, or default AppSync payload logging.

## Publish from backend code

Application code depends on `RealtimePublisherService`, not AWS. Construct a
`RealtimePublication` after the authoritative state mutation has committed.
Envelope identifiers, event types, timestamps, payload shape, revisions, and
deduplication remain application-owned.

For local tests inject `MemoryRealtimePublisher`. In the AWS composition root,
enable `minco-aws-adapters/appsync-events` and inject
`AppSyncEventsPublisher` using the rendered HTTP endpoint, configured namespace,
Region, a `reqwest::Client`, and the runtime's normal shared AWS credentials
provider. The adapter makes one signed POST to the `/event` data plane. An HTTP
200 is accepted only when AppSync reports exactly that event as successful.

If publication must survive a process failure, first commit an event to the
existing transactional outbox and project that durable record into realtime.
Do not interpret AppSync acceptance as delivery to every browser.

## Subscribe from the browser

Serve or bundle `assets/realtime-client.mjs` from the plugin package:

```js
import { createRealtimeClient } from './realtime-client.mjs'

const realtime = createRealtimeClient({
  realtimeUrl: deployment.realtimeWebSocketEndpoint,
  httpUrl: deployment.realtimeHttpEndpoint,
  namespace: deployment.realtimeNamespace,
  channel: `${session.tenantId}/orders`,
  getToken: () => auth.getAccessToken(),
  resync: async () => orders.replace(await api.listOrders()),
  onEvent: () => orders.markStale(),
  onError: error => telemetry.reportSafe(error),
})

await realtime.start()
// Call realtime.stop() when the owning UI is destroyed.
```

The facade places OIDC authorization in the AppSync WebSocket subprotocol and
subscription message, never the URL or browser storage. It sends
`connection_init`, `subscribe`, and `unsubscribe`, but no client ping/pong.
AWS keepalives only reset a local stale-connection deadline. The facade closes
after a bounded hidden-page grace period, reconnects with bounded full jitter,
and reruns `resync` before releasing buffered live events.

The application should keep polling or manual refresh as a fallback for users
who block WebSockets or remain offline.

## Review cost and evidence separately

The plan exposes 5 KiB operation units and connection minutes. Estimate an
application scenario using all connect, subscribe, subscription-handler,
published-event, and delivered-event units; fanout is multiplicative. Identity,
data transfer, HTTP resync, and optional logging are separate costs.

Run the local gates without contacting AWS:

```bash
cargo test -p minco-plugin-realtime --all-features --locked
npm test --prefix plugins/minco-plugin-realtime
cargo test -p minco-aws-adapters --lib --features appsync-events --locked
cargo test -p minco-plan --locked
cargo minco plugin validate --json
```

These prove composition, protocol state transitions, signing shape, IAM/cost
rendering, and metadata consistency. CloudFormation/SAM validation, an applied
stack, browser delivery against that stack, and cleanup are distinct live
gates. Do not deploy solely because local checks passed.

The repository's bounded reference proof is
`tasks/M11/M11-T12-live-realtime-proof.md`. Its exact-source disposable AWS run
proves Cognito subscription, HTTP resynchronization before event release, the
real IAM publisher adapter, visibility reconnect/resynchronization,
mismatched-channel rejection, and verified stack/bucket cleanup. Treat that as
implementation evidence only: every future AWS run still requires its own exact
account, Region, role, source, resource, duration, spend, and cleanup authority.
