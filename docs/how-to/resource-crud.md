# Build a standardized resource API

Minco's resource conventions standardize external HTTP behavior while keeping
application ports use-case-shaped. The Orders reference implements create,
read, list, update, and delete without adding an ORM or generic repository.

## Features

Enable `contract`, `http`, and `test`. Add a database adapter separately only
when the operation needs persistence.

## Provider assumptions

OpenAPI validation and in-process Axum `oneshot` tests are local. The memory
adapter proves the portable behavior without pretending to prove PostgreSQL,
SQLite, or DynamoDB semantics.

## Cost and wake behavior

The contract and in-process tests have `zero_compute` idle cost and no wake
source. A deployed HTTP API wakes on requests; its selected database retains its
own storage, request, or fixed-cost dimensions.

## Contract shape

Use `x-minco-resource` on each operation. Success bodies are stable resource
documents, list operations return an opaque bounded cursor page, failures use
`application/problem+json`, and updates/deletes require a strong `If-Match`
precondition derived from the current `ETag`.

The reference contract covers:

- `POST /orders` with an idempotency key;
- `GET /orders/{orderId}`;
- `GET /orders?page[limit]=...&page[after]=...` with an allowlisted bounded query;
- `PATCH /orders/{orderId}` with `If-Match`;
- `DELETE /orders/{orderId}` with `If-Match`.

Check the external contract and real router behavior:

```bash
cargo minco contract check --json
cargo test --locked -p orders-application -p orders-adapters -p orders-api --all-features
```

Authorization and validation stay in the application use case and fail before
persistence. HTTP handlers extract/map, call one use case, and map a response.

## Verification

The recipe runner binds this page to `orders-contract` and
`orders-resource-api`. Those checks cover status, media types, stable problem
codes, ETags, preconditions, cursor shape, and adapter concurrency behavior.

## Unsupported gates

The convention does not supply business authorization, an Active Record model,
a generic CRUD repository, or relational semantics for DynamoDB. DynamoDB needs
access-pattern-specific ports and adapters.
