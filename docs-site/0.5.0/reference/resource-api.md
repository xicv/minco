---
title: Resource API reference
description: Exact Minco 0.5.0 success, pagination, idempotency, and concurrency conventions.
---

# Resource API reference

The resource convention is a thin OpenAPI and HTTP layer. It is opt-in and does
not create a generic repository or ORM.

| Action | Success | Required control |
|---|---|---|
| Create | `201` with `{ "data": resource }` | `Idempotency-Key`, `Location`, strong `ETag` |
| List | `200` with `data` and `page` | bounded opaque cursor and allowlisted sort/filter |
| Read | `200` with `{ "data": resource }` | strong `ETag` |
| Update | `200` with `{ "data": resource }` | exactly one strong `If-Match` |
| Delete | `204` with no body | exactly one strong `If-Match` |

## Collection page

```json
{
  "data": [],
  "page": {
    "hasMore": false,
    "nextCursor": null
  }
}
```

Clients treat cursors as opaque and follow only a returned `nextCursor`.
Offset pagination is not the default.

## Failure semantics

Failures use `application/problem+json` with stable codes and request IDs.
Missing `If-Match` returns `428`; an invalid header returns `400`; a valid but
stale revision returns `412`.

Application use cases retain authorization, field validation, domain
invariants, audit, retention, deletion policy, and transaction boundaries.
DynamoDB uses access-pattern-specific ports rather than relational emulation.
