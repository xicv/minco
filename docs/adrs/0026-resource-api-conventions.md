# ADR 0026: OpenAPI-first resource API conventions

## Status

Accepted

## Context

Ordinary web applications repeatedly need create, list, read, update and delete
operations. Leaving every response shape, pagination scheme and concurrency
rule application-specific creates avoidable client and AI-agent work. Hiding
those choices behind a framework repository or ORM would conflict with Minco's
explicit application ports, visible SQL and contract-first architecture.

The convention follows the current semantics in
[RFC 9110](https://www.rfc-editor.org/rfc/rfc9110.html) for strong entity tags,
`If-Match` and `412 Precondition Failed`,
[RFC 6585](https://www.rfc-editor.org/rfc/rfc6585.html) for
`428 Precondition Required`,
[RFC 9457](https://www.rfc-editor.org/rfc/rfc9457.html) for problem details,
and the external contract remains OpenAPI 3.1.

## Decision

Minco defines a thin, opt-in resource convention in OpenAPI:

- every participating operation declares `x-minco-resource.name` and exactly
  one `action` from `create`, `list`, `read`, `update`, or `delete`;
- one resource family has one collection path, one direct member path and no
  duplicate action;
- create, read and update success bodies are `{ "data": ... }`;
- list success bodies are
  `{ "data": [...], "page": { "hasMore": bool, "nextCursor": string|null } }`;
- delete succeeds with `204` and no body;
- failures remain `application/problem+json` with stable codes and request IDs;
- create is idempotency-key protected, returns `201`, `Location` and a strong
  `ETag` (an idempotent replay may return `200` with the immutable original
  create representation, `Location` and `ETag`, even after later updates or
  deletion);
- read and update return a strong `ETag`;
- update and delete require exactly one strong `If-Match`; absence returns
  `428`, an invalid header returns `400`, and a valid but stale revision returns
  `412`;
- list metadata declares bounded `page[limit]`, opaque `page[after]`, stable
  default sort, allowlisted sort fields, allowlisted exact filters and cursor
  fields. Offset pagination is not the default.

The HTTP crate supplies envelope, query-policy and entity-tag primitives.
Application code still decides authorization and domain validation. Each use
case owns a purpose-specific port. Adapters implement bounded queries and
atomic revision predicates in visible SQL.

`cargo minco make resource <name>` selects an already valid, complete five
action family and creates the same failing application/HTTP specifications as
five reviewed operation generations. It does not edit OpenAPI, invent fields,
generate business behavior, choose deletion policy or generate persistence.

## Consequences

- Clients can share one response, pagination, error and concurrency model.
- OpenAPI and contract validation expose resource behavior before code exists.
- Strong tags plus atomic revision writes prevent lost updates.
- Create idempotency records retain the bounded original response snapshot
  instead of depending on the resource's later lifecycle.
- Cursor contents are opaque to clients and may bind the selected sort/filter.
- Adding this convention to an existing operation can be breaking when its
  response or precondition shape changes. Contract compatibility reports
  classify resource-policy changes explicitly.
- Different databases retain separate adapters and migrations. DynamoDB still
  needs access-pattern-specific ports rather than relational emulation.
- Bulk operations, relationships, field selection, search operators and
  asynchronous commands require later explicit conventions; they are not
  inferred from CRUD.

## Safety

Query names are parsed from a closed allowlist, bounds are enforced before the
application port, and user values never become SQL identifiers. Entity tags
contain no secret data and accept a restricted ASCII grammar. Authorization
and input validation happen before persistence. Delete policy, retention,
audit and restoration remain application decisions.
