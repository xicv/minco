# `minco-plugin-realtime`

Provider-neutral, subscriber-only realtime invalidation for Minco. Backend code
publishes bounded JSON envelopes through a use-case-shaped port; browser code
can only subscribe and always resynchronizes authoritative state over HTTP.

The crate includes two distinct seams:

- `RealtimePublisherService`, with strict channel and payload bounds plus a
  deterministic `MemoryRealtimePublisher` for local tests;
- `REALTIME_CLIENT_MODULE`, the dependency-free browser facade also packaged as
  `assets/realtime-client.mjs`.

The selected AWS implementation lives in `minco-aws-adapters` behind its
`appsync-events` feature. It signs backend HTTP publication with SigV4. The
browser facade uses OIDC only for connect and subscribe; it has no publish API,
does not persist a token, sends no application keepalive, disconnects while the
UI is hidden, and buffers live invalidations until the application's HTTP
resync completes.

```toml
[plugins]
enabled = ["health", "observability", "idempotency", "realtime"]

[plugins.configuration.realtime]
namespace = "orders"
max_event_bytes = 5120
subscriber_claim = "tenant_id"
```

Channels supplied to the service omit the configured namespace. With the
configuration above, `tenant-42/order-7` becomes
`/orders/tenant-42/order-7`. The generated AppSync handler authorizes the
subscription only when the token's `tenant_id` claim is `tenant-42`.

Realtime delivery is ephemeral. A successful publish is not durable delivery;
use the domain-event outbox when retries or auditability are required.
