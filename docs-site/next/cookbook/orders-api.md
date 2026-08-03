---
title: Orders API End to End
description: Follow the reference application from OpenAPI through CRUD, persistence, HTTP tests, and deployment planning.
---

# Orders API End to End

`examples/orders` is the reference contract-to-cloud application. It exercises
health and a complete Orders resource family across domain, application,
memory/SQLite/PostgreSQL adapters, Axum, local service, Lambda, worker, Plan IR,
and release packaging.

## 1. Read the Contract First

The canonical source is
[`examples/orders/openapi/openapi.yaml`](https://github.com/xicv/minco/blob/main/examples/orders/openapi/openapi.yaml).
It declares:

| Method | Path | Operation |
|---|---|---|
| `POST` | `/orders` | `placeOrder` |
| `GET` | `/orders` | `listOrders` |
| `GET` | `/orders/{orderId}` | `getOrder` |
| `PATCH` | `/orders/{orderId}` | `updateOrder` |
| `DELETE` | `/orders/{orderId}` | `deleteOrder` |

Validate the contract and deterministic bindings:

```bash
cargo minco contract check
cargo minco contract sync --check
```

## 2. Trace One Operation

```bash
cargo minco explain placeOrder --json
cargo minco explain updateOrder --json
```

The trace links the OpenAPI operation to its delivery handler, application use
case, adapters, tests, capabilities, resources, and evidence. Missing or
ambiguous links fail rather than being inferred.

## 3. Read the Layers

```text
examples/orders/domain       pure invariants and revision transitions
examples/orders/application  use cases and owned ports
examples/orders/adapters     memory, SQLite, and PostgreSQL implementations
examples/orders/api          generated contract types and Axum mapping
examples/orders/service      composition, local, Lambda, and worker entrypoints
```

For `placeOrder`, look for authorization and validation before the persistence
port, then the atomic idempotency claim and immutable replay result. For
`updateOrder` and `deleteOrder`, look for the expected revision in the adapter's
write predicate.

## 4. Run Nearest-Boundary Tests

```bash
cargo test -p orders-domain -p orders-application
cargo test -p orders-adapters -p orders-api
```

The SQLite adapter runs against a real local engine. PostgreSQL tests compile
but require an explicitly configured real engine; an ignored test is not a
provider pass.

## 5. Start the SQLite Profile

```bash
cargo minco dev --profile sqlite --dry-run --json
cargo minco dev --profile sqlite
```

Create an order in another terminal:

```bash
curl --fail-with-body --silent \
  --request POST http://127.0.0.1:3000/orders \
  --header 'content-type: application/json' \
  --header 'idempotency-key: cookbook-order-1' \
  --header 'x-minco-subject: cookbook-user' \
  --header 'x-minco-permissions: orders.create,orders.read,orders.update,orders.delete' \
  --data '{"customerReference":"COOKBOOK-001","lines":[{"sku":"MINCO-BOOK","quantity":1}]}'
```

Repeat that exact request to receive the original result. A different payload
with the same idempotency key is a conflict.

## 6. List with an Opaque Cursor

```bash
curl --fail-with-body --silent \
  'http://127.0.0.1:3000/orders?page%5Blimit%5D=20&sort=-createdAt,-id' \
  --header 'x-minco-subject: cookbook-user' \
  --header 'x-minco-permissions: orders.read'
```

Pass `page.nextCursor` back unchanged. Clients do not parse or construct cursor
contents.

## 7. Update with the Current ETag

Read returns a strong `ETag`. Send exactly that value in `If-Match` for update
or delete. Missing headers return `428`; stale but valid tags return `412`.
This is optimistic concurrency, not a read-then-write convention.

## 8. Inspect Deployment without AWS Mutation

```bash
cargo minco inspect --json
cargo minco deploy plan --json
cargo minco cost --json
cargo minco package
cargo minco release verify target/minco/release.json
```

These commands prove graph structure, deterministic planning, and local package
identity. They do not create a change set, migrate a target database, deploy,
verify a live endpoint, or promote a release.

Next: [resource API details](../guides/resource-api),
[testing evidence](../reference/testing), or
[safe deployment](../guides/deployment).
