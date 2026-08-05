---
title: Add Realtime Invalidation
description: Publish ephemeral AppSync Events from the backend and resynchronize authoritative HTTP state in visible browser clients.
---

# Add Realtime Invalidation

Minco realtime is a wake-up signal, not a second data store. The backend
publishes an ephemeral event through AppSync Events, a visible browser marks
its view stale, and the application reloads authoritative state through its
existing HTTP API.

## Select the Static Plugin

```toml
[plugins]
enabled = ["health", "observability", "idempotency", "realtime"]

[plugins.configuration.realtime]
namespace = "orders"
max_event_bytes = 5120
subscriber_claim = "tenant_id"
```

The generated AppSync policy uses IAM for backend publication and OIDC for
browser connection and subscription. The first channel segment after the
namespace must equal the configured verified token claim. Minco creates no API
key, schedule, NAT Gateway, fixed compute, or provisioned concurrency.

## Publish After Authoritative State Commits

Application code depends on `RealtimePublisherService`, not the AWS SDK. In
local tests, inject `MemoryRealtimePublisher`. In the AWS composition root,
enable `minco-aws-adapters/appsync-events` and inject
`AppSyncEventsPublisher`. If delivery must survive process failure, first write
to the transactional outbox and project that durable record into realtime.

An accepted AppSync publication does not prove that every browser received the
event. Payloads should contain identifiers, revisions, and invalidation hints,
not the only copy of business state.

## Subscribe and Resynchronize

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
  onError: error => telemetry.reportSafe(error)
})

await realtime.start()
```

Authorization travels in the AppSync WebSocket protocol, never in the URL or
browser storage. The client closes after a bounded hidden-page grace period,
reconnects with bounded full jitter, and resynchronizes before releasing
buffered events. Keep polling or manual refresh as a fallback.

## Prove Each Boundary Separately

```bash
cargo test -p minco-plugin-realtime --all-features --locked
npm test --prefix plugins/minco-plugin-realtime
cargo test -p minco-aws-adapters --lib --features appsync-events --locked
proofs/realtime-appsync/scripts/test-local.sh
```

These local gates prove protocol transitions, signing shape, IAM/cost rendering
and the standalone consumer package. A CloudFormation validation, applied
stack, real browser delivery, cleanup, and later production observation remain
separate evidence.
