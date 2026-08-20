---
title: Resource API
description: Current resource HTTP shapes, query rules, strong entity tags, and stable Problem Details.
---

# Resource API

The current resource boundary lives in OpenAPI policy plus the public
`minco-http` types. It standardizes transport behavior without creating an ORM
or repository abstraction.

## Action Matrix

| Action | Method pattern | Success | Required controls |
|---|---|---:|---|
| Create | `POST /resources` | `201` or replay `200` | `Idempotency-Key`, `Location`, strong `ETag` |
| List | `GET /resources` | `200` | bounded cursor, allowlisted sort and filters |
| Read | `GET /resources/{id}` | `200` | strong `ETag` |
| Update | `PATCH /resources/{id}` | `200` | exactly one strong `If-Match`, new `ETag` |
| Delete | `DELETE /resources/{id}` | `204` | exactly one strong `If-Match`, empty body |

## Success Documents

Single-resource responses use `ResourceDocument<T>`:

```json
{
  "data": {
    "id": "018f9f9d-a8a2-7e04-9d66-cf5d9e521d71",
    "revision": 4
  }
}
```

Collection responses use `ResourceCollection<T>` and `CursorPageInfo`:

```json
{
  "data": [
    {
      "id": "018f9f9d-a8a2-7e04-9d66-cf5d9e521d71",
      "revision": 4
    }
  ],
  "page": {
    "hasMore": true,
    "nextCursor": "eyJjcmVhdGVkQXQiOiIyMDI2LTA4LTAxIn0"
  }
}
```

## List Query

| Parameter | Rule |
|---|---|
| `page[limit]` | integer from `1` through the declared maximum |
| `page[after]` | opaque URL-safe token, 1–512 bytes |
| `sort` | comma-separated allowlisted fields; `-` means descending |
| `filter[field]` | one declared field and one bounded value |

Unknown or repeated parameters fail. User values never become SQL identifiers.
The application port receives a parsed limit, optional cursor, ordered sort
terms, and a map of allowlisted filters.

## Strong Entity Tags

`StrongEntityTag::for_resource(resource, id, revision)` emits a quoted value
whose opaque portion is bounded and contains only a restricted ASCII grammar.
Revision `0`, weak tags, comma-separated alternatives, repeated headers, and
unquoted values fail closed.

```text
"order:018f9f9d-a8a2-7e04-9d66-cf5d9e521d71:4"
```

`parse_if_match` distinguishes a missing header from a malformed header. The
application or adapter separately distinguishes a well-formed but stale
revision.

## Problem Details

Failures use `application/problem+json`, an `x-request-id` response header, and
a matching public request ID in the body.

```json
{
  "type": "https://minco.dev/problems/precondition_failed",
  "title": "Precondition failed",
  "status": 412,
  "detail": "The resource changed after it was read. Fetch the current representation and retry.",
  "code": "precondition_failed",
  "requestId": "request-01J4M6GAF3JQZ8J7VV8W2W7KZD"
}
```

| Status | Meaning | Client action |
|---:|---|---|
| `400` | malformed query or `If-Match` | correct the request shape |
| `404` | resource is not visible or absent | do not infer hidden resource existence |
| `409` | command conflicts with current state | refresh application state |
| `412` | supplied revision is stale | fetch, reconcile, and retry |
| `422` | field or domain validation failed | use the public `errors` map |
| `428` | `If-Match` is required | retry with the current strong tag |

## Public Rust Surface

The `minco-http` facade exports:

```rust
pub use resource::{
    Cursor, CursorPageInfo, EntityTagError, ResourceCollection, ResourceDocument,
    ResourceListPolicy, ResourceListQuery, ResourceQueryError, SortDirection,
    SortTerm, StrongEntityTag, parse_if_match, parse_resource_list_query,
};
```

The authoritative contract remains
[`examples/orders/openapi/openapi.yaml`](https://github.com/xicv/minco/blob/main/examples/orders/openapi/openapi.yaml).
