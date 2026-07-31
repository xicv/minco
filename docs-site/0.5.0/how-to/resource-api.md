---
title: Build a resource API
description: Add a complete OpenAPI-first CRUD family without introducing a generic repository.
---

# Build a resource API

Use Minco’s opt-in resource convention when a domain concept needs standard
create, list, read, update, and delete operations.

## 1. Define the reviewed contract

Add all five operations to OpenAPI. Every operation declares the same
`x-minco-resource.name` and one unique action. Include examples, security,
success envelopes, pagination parameters, precondition headers, and stable
Problem responses.

```yaml
x-minco-resource:
  name: order
  action: create
```

Run the contract gates:

```bash
cargo minco contract check
cargo minco contract sync --check
```

## 2. Preview the generated specifications

```bash
cargo minco make resource order --dry-run --json
cargo minco make resource order
```

The command only selects an already valid, complete family. It generates
failing application and HTTP specifications plus operation traces. It does not
invent domain rules, persistence, deletion policy, or successful behavior.

## 3. Implement inward

For each operation:

1. add a failing application test with fake use-case-shaped ports;
2. implement domain invariants and one application use case;
3. implement the port in memory and the selected real database;
4. add in-process Axum contract tests;
5. confirm `cargo minco explain <operationId> --json` traces the slice.

HTTP handlers should extract and map, call one use case, and map the response.
They contain no SQL. Application and domain crates do not depend on Axum, SQLx,
Lambda, or AWS SDKs.

## 4. Verify concurrency and idempotency

Create requires `Idempotency-Key`. Update and delete require a strong
`If-Match`. Real adapters must make revision predicates atomic and retain the
original create replay even if the resource later changes or is deleted.

```bash
cargo minco test unit
cargo minco test feature
./scripts/quality.sh
```

See the exact [resource API reference](../reference/resource-api).
