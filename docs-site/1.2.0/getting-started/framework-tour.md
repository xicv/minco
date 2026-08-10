---
title: Framework Tour
description: Follow Minco from an OpenAPI contract to tested Rust, an AWS plan, and verifiable evidence.
---

# Framework Tour

Minco is a narrow Rust framework for web applications that should cost very
little while idle on AWS. It standardizes the path around your domain code
rather than replacing Rust, Axum, SQLx, or AWS services.

```text
OpenAPI → domain and use cases → static composition → Plan IR → exact evidence
```

## The Five Planes

### 1. Contract

OpenAPI 3.1 is the external HTTP source of truth. Define operation IDs,
security, schemas, examples, success responses, and Problem responses before
writing the handler.

```bash
cargo minco contract check
cargo minco contract sync --check
```

Generated files are deterministic projections of that contract. Do not edit a
file marked `@generated` manually.

### 2. Code

Business behavior stays in ordinary Rust with inward dependencies:

```text
HTTP or worker delivery → application use case → domain
                               ↑
                         adapter implements port
```

The HTTP handler extracts and maps input, calls one use case, and maps the
result. It contains no SQL. Application ports describe a use case instead of a
generic CRUD repository.

### 3. Capabilities

Plugins are statically linked and explicitly selected. Typed services and
contributions compose deterministically; there is no runtime directory scan,
dynamic-library loading, or global service locator.

```bash
cargo minco plugin list --json
cargo minco plugin validate --json
cargo minco inspect --json
```

### 4. Resources

Plan IR projects the selected graph into functions, triggers, queues,
databases, IAM, wake sources, connection pressure, performance assumptions,
and cost intent.

```bash
cargo minco deploy plan --stdout --json
cargo minco cost --json
cargo minco perf --json
```

Planning is local and non-contacting. It does not authorize an AWS change.

### 5. Evidence

Minco keeps source checks, compiler proof, provider observations, release
artifacts, deployment receipts, promotion, and production runtime as separate
facts. A green unit test never silently becomes live AWS proof.

## The Development Loop

Use one vertical slice at a time:

1. change OpenAPI and add examples;
2. run contract validation and deterministic sync;
3. add one failing application test through a public use-case interface;
4. implement the domain rule and use case;
5. add the selected adapter and migration only when persistence is required;
6. test the real Axum router in process;
7. inspect the operation, Plan IR, IAM, wake, and cost projection;
8. run the focused checks and then the complete local quality gate;
9. deploy the same exact artifact only under a separately reviewed boundary.

```bash
cargo minco explain placeOrder --json
cargo minco check --with-cargo
./scripts/quality.sh
```

## Choose the Next Detail

- Learn the [project structure](./project-structure).
- Implement the complete [resource API workflow](../guides/resource-api).
- Review [testing and evidence boundaries](../reference/testing).
- Understand [zero idle precisely](../explanation/zero-idle).
