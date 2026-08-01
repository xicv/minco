---
title: Build a Resource API
description: Implement standard create, list, read, update, and delete behavior without introducing a generic repository.
---

# Build a Resource API

Use the resource convention when clients benefit from one predictable set of
JSON, pagination, error, idempotency, and concurrency rules. The convention is
opt-in; your application still owns every business decision.

## 1. Define the Complete Contract

Declare all five operations with the same resource name and one unique action.
The list action also declares bounded sort, filter, and cursor fields.

```yaml
x-minco-resource:
  name: order
  action: list
  defaultLimit: 20
  maxLimit: 100
  defaultSort: [-createdAt, -id]
  sortFields: [createdAt, id]
  filterFields: [status]
  cursorFields: [createdAt, id]
```

Create must declare `Idempotency-Key`. Update and delete must declare
`If-Match`, plus explicit `412` and `428` Problem responses. Create, read, and
update return a JSON data envelope and strong `ETag`; create also returns
`Location`.

```bash
cargo minco contract check
cargo minco contract sync --check
```

## 2. Preview the Specification Files

```bash
cargo minco make resource order --dry-run --json
```

The generator refuses an incomplete or inconsistent family. When the family is
valid, apply the plan:

```bash
cargo minco make resource order
```

The output is intentionally incomplete: it creates failing application and
HTTP specifications and operation traces. It does not choose fields, write SQL,
or invent successful business behavior.

## Complete Request Flow

Implement one vertical slice at a time.

### Create

1. Parse a bounded `Idempotency-Key`.
2. Authorize and validate before persistence.
3. Atomically claim the key with a fingerprint of the command.
4. Commit the resource and immutable replay snapshot together.
5. Return `201`; return the original result on an identical retry.
6. Reject the same key with a different fingerprint.

```json
{
  "data": {
    "id": "018f9f9d-a8a2-7e04-9d66-cf5d9e521d71",
    "customerReference": "PO-1042",
    "lines": [{ "sku": "MINCO-BOOK", "quantity": 1 }],
    "revision": 1,
    "status": "accepted",
    "createdAt": "2026-08-01T02:00:00Z",
    "updatedAt": "2026-08-01T02:00:00Z"
  }
}
```

### List

Parse query fields through the allowlist before calling the application port:

```text
GET /orders?page[limit]=20&sort=-createdAt,-id&filter[status]=accepted
```

Return a bounded collection. Clients treat `nextCursor` as opaque.

```json
{
  "data": [],
  "page": {
    "hasMore": false,
    "nextCursor": null
  }
}
```

### Read, Update, and Delete

Read returns the current strong entity tag. Send exactly that value in
`If-Match` for update or delete.

```text
ETag: "order:018f9f9d-a8a2-7e04-9d66-cf5d9e521d71:1"
If-Match: "order:018f9f9d-a8a2-7e04-9d66-cf5d9e521d71:1"
```

The adapter must include the expected revision in the write predicate. A
read-then-write sequence without an atomic predicate can still lose updates.

| Condition | HTTP status | Stable code |
|---|---:|---|
| Header absent | `428` | `precondition_required` |
| Weak, repeated, or malformed tag | `400` | `invalid_if_match` |
| Valid but stale tag | `412` | `precondition_failed` |

## 3. Test Every Boundary

For each action, add tests in this order:

1. application test proving authorization and validation fail before the port;
2. domain test for each invariant or transition;
3. real adapter test for transaction, idempotency, and atomic revision behavior;
4. Axum `oneshot` test for status, media type, headers, request ID, and body;
5. contract and operation trace checks.

```bash
cargo test -p orders-domain -p orders-application
cargo test -p orders-adapters -p orders-api
cargo minco explain updateOrder --json
```

## 4. Keep Policy in the Application

Minco does not decide who may update, which fields are mutable, whether delete
is soft or hard, how long data is retained, or what must be audited. DynamoDB
uses access-pattern-specific ports rather than pretending to be a relational
repository.

Use the [Resource API reference](../reference/resource-api) for exact shapes and
the [orders example](../examples/) for exercised source.

