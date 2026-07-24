# Architecture Overview

## Product boundary

Minco is an application kernel, official extension set, CLI, and deployment planner. It is
not an ORM, frontend framework, managed hosting platform, or general multi-cloud layer.

```text
OpenAPI contract
      |
      v
checked-in DTOs + operation metadata
      |
      v
Axum delivery -> application use cases -> domain rules
                      ^
                      |
          PostgreSQL / SQLite / AWS adapters

compiled module descriptors + operations + resources
      |
      v
application graph -> local driver / Plan IR / cost checks / SAM
```

## Dependency direction

```text
api/delivery --------> application --------> domain
                            ^
                            |
adapters ------------------+

composition root -> api + application + adapters
```

The domain has no knowledge of HTTP, SQL, AWS, or Minco deployment. The application layer
owns narrow infrastructure-facing ports. Adapters implement those ports. Delivery maps
transport DTOs to application commands and results.

## Horizontal layers, vertical features

The codebase enforces layers through Cargo crates and organizes code within each layer by
feature. A stable `operationId` is the trace key across contract, generated metadata,
handler, use case, adapter, tests, and deployment route.

## Static modules

A module descriptor declares:

- identity and version;
- operations;
- provided and required capabilities;
- migrations;
- health checks;
- resource intents and dependencies.

The graph builder rejects duplicates, missing capabilities, cycles, route conflicts, and
migration conflicts before serving traffic. Constructors remain ordinary Rust; there is
no runtime service lookup.

## Source-of-truth map

| Knowledge | Canonical source |
|---|---|
| HTTP behavior | OpenAPI document |
| Business invariants | Domain code and tests |
| Use-case orchestration | Application services |
| Persistence/provider behavior | Adapter crates |
| Schema | Ordered migration files |
| Composition | Explicit Rust composition root |
| Deployment resources | Compiled graph and Plan IR |
| Environment differences | Typed environment config and secret provider |
| Release identity | Release manifest |
| Work planning | Roadmap and task files |
