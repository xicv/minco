---
title: Orders API End to End
description: Follow the reference application from OpenAPI through CRUD, persistence, HTTP behavior, runtime planning, and release evidence.
---

# Orders API End to End

`examples/orders` is Minco's reference contract-to-cloud application. It is not
only a CRUD sample: it exercises duplicate-safe commands, bounded collection
queries, optimistic concurrency, domain and application boundaries,
memory/SQLite/PostgreSQL/DynamoDB adapters, Axum, local service, Lambda HTTP,
worker composition, Plan IR, and release packaging.

<div class="scenario-fact-grid">
  <div class="scenario-fact">
    <span>Traffic shape</span>
    <strong>Low baseline, short bursts</strong>
    <p>A common internal, partner, or mobile ordering pattern where fixed application compute is difficult to justify.</p>
  </div>
  <div class="scenario-fact">
    <span>Client safety</span>
    <strong>Retries and concurrent edits</strong>
    <p>Idempotency keys protect create; strong ETags protect update and delete.</p>
  </div>
  <div class="scenario-fact">
    <span>Data choices</span>
    <strong>SQLite, PostgreSQL, or DynamoDB</strong>
    <p>Each adapter implements the same application-owned ports with different operational tradeoffs.</p>
  </div>
  <div class="scenario-fact">
    <span>Delivery claim</span>
    <strong>Exact artifact, explicit evidence</strong>
    <p>Local tests, a plan, and a package remain weaker claims than hosted verification and production observation.</p>
  </div>
</div>

## The system you are building

<div class="topology-strip" role="img" aria-label="Clients call API Gateway and a Lambda HTTP runtime, which invokes the Orders application and a selected data adapter">
  <div class="topology-node">
    <small>Clients</small>
    <strong>Web, mobile, partner</strong>
    <span>Retry-safe commands and conditional writes</span>
  </div>
  <span class="topology-arrow" aria-hidden="true">→</span>
  <div class="topology-node">
    <small>Ingress</small>
    <strong>API Gateway HTTP API</strong>
    <span>Reviewed methods, paths, headers, limits, and identity boundary</span>
  </div>
  <span class="topology-arrow" aria-hidden="true">→</span>
  <div class="topology-node">
    <small>Compute</small>
    <strong>Native Lambda HTTP</strong>
    <span>The same Axum router used by local development</span>
  </div>
  <span class="topology-arrow" aria-hidden="true">→</span>
  <div class="topology-node">
    <small>Application</small>
    <strong>Orders use cases + adapter</strong>
    <span>PostgreSQL or access-pattern-specific DynamoDB</span>
  </div>
</div>

Asynchronous fulfillment or notifications can be added as an explicit event,
queue, and `aws-worker` composition. They are not hidden inside the request
handler or activated by catalog metadata.

## 1. Read the contract first

The canonical source is
[`examples/orders/openapi/openapi.yaml`](https://github.com/xicv/minco/blob/main/examples/orders/openapi/openapi.yaml).
It declares:

| Method | Path | Operation | Client guarantee |
|---|---|---|---|
| `POST` | `/orders` | `placeOrder` | Required `Idempotency-Key`; new result or immutable replay |
| `GET` | `/orders` | `listOrders` | Bounded allowlisted sort/filter and opaque cursor pagination |
| `GET` | `/orders/{orderId}` | `getOrder` | Strong `ETag` on the current representation |
| `PATCH` | `/orders/{orderId}` | `updateOrder` | Required `If-Match`; stale writes fail |
| `DELETE` | `/orders/{orderId}` | `deleteOrder` | Required `If-Match`; stale deletes fail |

Validate the contract and deterministic bindings:

```bash
cargo minco contract check
cargo minco contract sync --check
```

## 2. Trace one operation through the graph

```bash
cargo minco explain placeOrder --json
cargo minco explain updateOrder --json
```

The trace links each OpenAPI operation to its delivery handler, application use
case, adapters, tests, capabilities, resources, and evidence. Missing or
ambiguous links fail instead of being inferred.

For `placeOrder`, the repository manifest identifies memory, PostgreSQL, SQLite,
and DynamoDB adapters. For `updateOrder`, the evidence links include
application validation, the real Axum router, and revision-safe adapter tests.

## 3. Read the layers

```text
examples/orders/domain       pure invariants and revision transitions
examples/orders/application  use cases and owned ports
examples/orders/adapters     memory, SQLite, PostgreSQL, and DynamoDB implementations
examples/orders/api          generated contract types and Axum mapping
examples/orders/service      composition, local, Lambda, migration, and worker entrypoints
```

For `placeOrder`, look for authorization and validation before the persistence
port, then the atomic idempotency claim and immutable replay result. For
`updateOrder` and `deleteOrder`, look for the expected revision in the adapter's
write predicate.

## 4. Run nearest-boundary tests

```bash
cargo test -p orders-domain -p orders-application
cargo test -p orders-adapters -p orders-api
```

The SQLite adapter runs against a real local engine. PostgreSQL tests compile
but require an explicitly configured real engine; an ignored test is not a
provider pass. DynamoDB conformance is separate from proving a deployed table
and IAM policy.

## 5. Start the SQLite profile

```bash
cargo minco dev --profile sqlite --dry-run --json
cargo minco dev --profile sqlite
```

The first command lets you review the lifecycle and process plan. The second
applies the selected local lifecycle and starts the supervised service.

## 6. Place an order exactly once

The reference local ingress accepts bounded subject and permission headers so
the complete authorization path can be exercised without pretending that a
production identity provider has been verified.

```bash
curl --fail-with-body --silent \
  --request POST http://127.0.0.1:3000/orders \
  --header 'content-type: application/json' \
  --header 'idempotency-key: cookbook-order-1' \
  --header 'x-minco-subject: cookbook-user' \
  --header 'x-minco-permissions: orders.create,orders.read,orders.update,orders.delete' \
  --data '{"customerReference":"COOKBOOK-001","lines":[{"sku":"MINCO-BOOK","quantity":1}]}'
```

A new command returns `201 Created`, a canonical `Location`, a strong `ETag`,
and the documented envelope:

```http
HTTP/1.1 201 Created
Location: /orders/<uuid>
ETag: "<strong-etag>"
content-type: application/json

{
  "data": {
    "id": "<uuid>",
    "customerReference": "COOKBOOK-001",
    "lines": [
      { "sku": "MINCO-BOOK", "quantity": 1 }
    ],
    "revision": 1,
    "status": "accepted",
    "createdAt": "<RFC3339 timestamp>",
    "updatedAt": "<RFC3339 timestamp>"
  }
}
```

The placeholders show contract shape rather than a fabricated live receipt.

| Retry | Result |
|---|---|
| Same idempotency key and same canonical payload | `200 OK` with the immutable original result |
| Same idempotency key and a different canonical payload | Explicit conflict Problem; no second order |
| New idempotency key | A separate create command |

## 7. List with an opaque cursor

```bash
curl --fail-with-body --silent \
  'http://127.0.0.1:3000/orders?page%5Blimit%5D=20&sort=-createdAt,-id' \
  --header 'x-minco-subject: cookbook-user' \
  --header 'x-minco-permissions: orders.read'
```

The response contains `data` plus:

```json
{
  "page": {
    "hasMore": true,
    "nextCursor": "<opaque-cursor>"
  }
}
```

Pass the returned cursor back unchanged:

```bash
curl --fail-with-body --silent \
  'http://127.0.0.1:3000/orders?page%5Blimit%5D=20&page%5Bafter%5D=<opaque-cursor>&sort=-createdAt,-id' \
  --header 'x-minco-subject: cookbook-user' \
  --header 'x-minco-permissions: orders.read'
```

Clients do not parse, construct, or depend on the cursor's internal encoding.
The contract bounds page size, sort fields, filter fields, and cursor length.

## 8. Update with the current ETag

Read one order and retain the exact strong `ETag` response header. Send it in
`If-Match` for update or delete:

```bash
curl --fail-with-body --silent \
  --request PATCH http://127.0.0.1:3000/orders/<uuid> \
  --header 'content-type: application/json' \
  --header 'if-match: "<strong-etag>"' \
  --header 'x-minco-subject: cookbook-user' \
  --header 'x-minco-permissions: orders.update' \
  --data '{"customerReference":"COOKBOOK-001-REVISED"}'
```

| Condition | HTTP behavior |
|---|---|
| Current strong ETag | Mutation succeeds and returns the next representation and ETag |
| Missing `If-Match` | `428 Precondition Required` Problem |
| Valid but stale ETag | `412 Precondition Failed` Problem |
| Empty update document | Validation fails before the update port is called |

This is optimistic concurrency enforced by the write predicate, not a
read-then-write convention.

## 9. Choose the data profile deliberately

| Profile | Best fit | Operational consequence |
|---|---|---|
| SQLite | Local development, desktop, or explicitly single-process durable use | Smallest local stack; not a hidden multi-process production database |
| PostgreSQL | Relational queries, transactions, reporting, and an existing PostgreSQL operating model | Connection pressure, provider availability, and database cost must be planned |
| DynamoDB | Known key/index access patterns, conditional writes, and on-demand AWS-native operation | No generic repository or scan; access patterns and eventual consistency stay explicit |

The same application-owned ports preserve idempotency, ordering, cursor,
revision, and soft-delete behavior across the exercised adapters. That does not
make their latency, query flexibility, consistency, or cost models identical.

## 10. Inspect deployment without AWS mutation

```bash
cargo minco inspect --json
cargo minco deploy plan --json
cargo minco cost --json
cargo minco package
cargo minco release verify target/minco/release.json
```

These commands prove graph structure, deterministic planning, cost
classification, and local package identity. They do not create a change set,
migrate a target database, deploy, verify a live endpoint, or promote a release.

The project cost policy currently denies fixed compute, NAT Gateway,
provisioned concurrency, and scheduled wakeups, while bounding reserved
concurrency and database connections. A deployment plan that violates those
constraints must fail before provider mutation.

## 11. Review failure and evidence together

| Event | Expected behavior | Strongest relevant evidence |
|---|---|---|
| Client retries a timed-out create | Immutable replay or explicit idempotency conflict | Application, adapter, and HTTP tests |
| Two clients edit the same revision | One succeeds; the stale mutation receives `412` | Adapter predicate and real-router tests |
| Database is unavailable | Readiness reports unavailable; public detail remains bounded | Dependency test plus hosted readiness verification |
| One SQS record fails | Worker reports partial failure so successful records are not retried | Worker mapping test and hosted queue verification |
| Deployment inputs drift | Guarded mutation stops on account, Region, environment, digest, or change-set mismatch | Plan and deployment receipts |
| A release must be reversed | Compatibility is checked before rollback; exact prior bytes are reused | Release manifest, migration compatibility, and rollback receipt |

## 12. Move from recipe to production design

The [production blueprint](./production-blueprint) turns this vertical slice
into an operational decision: traffic pattern, persistence choice, optional
worker, wake sources, residual cost, rollout, observation, and recovery. Use it
before copying the reference composition unchanged.

Next: [resource API details](../guides/resource-api),
[testing evidence](../reference/testing), or
[safe deployment](../guides/deployment).
