---
title: Build Your First Application
description: Generate, inspect, run, and exercise a Minco application without deploying it.
---

# Build Your First Application

This path uses SQLite so the first request needs no database container or AWS
account. It still uses the same contract, application, adapter, HTTP, and
evidence boundaries as a larger deployment.

## 1. Generate the project

```bash
cargo minco new hello-minco --database sqlite
cd hello-minco
cp .env.example .env
```

The generator creates ordinary Rust, TOML, SQL, and OpenAPI source. Domain and
application crates remain independent of Axum, SQLx, Lambda, and AWS SDKs.

## 2. Inspect before running

```bash
cargo minco doctor
cargo minco contract check
cargo minco inspect --json
```

`inspect` projects the application graph across five planes: contract, code,
capabilities, resources, and evidence. Use `explain` when you need the complete
trace for one OpenAPI operation:

```bash
cargo minco explain getPlatform --json
```

## 3. Review the local plan

```bash
cargo minco dev --profile sqlite --dry-run --json
```

The plan shows the selected services, lifecycle stages, processes, ports, and
readiness probes. A dry run starts nothing.

## 4. Start the application

```bash
cargo minco dev --profile sqlite
```

In another terminal, exercise the health contract:

```bash
curl --fail --silent http://127.0.0.1:3000/health/live
curl --fail --silent http://127.0.0.1:3000/health/ready
```

Ctrl-C stops the supervised process group. It does not silently reset durable
data.

## 5. Add a resource deliberately

Preview the generated vertical slice before writing files:

```bash
cargo minco make resource widget --dry-run --json
```

Then follow the required order:

1. define OpenAPI requests, responses, security, examples, and Problem bodies;
2. sync deterministic bindings;
3. write a failing application test with fake ports;
4. implement domain rules and one use case;
5. add a persistence adapter only when needed;
6. test the Axum boundary with an in-process router;
7. inspect resource, IAM, cost, and wake implications.

The [resource API guide](../guides/resource-api) supplies the standard create,
list, read, update, and delete shapes without introducing a generic repository
or ORM.

## 6. Run the local gates

```bash
cargo minco test unit
cargo minco test feature
cargo minco check --with-cargo
```

Local success is not deployment proof. Packaging, hosted CI, provider-backed
verification, promotion, and production observation remain separate evidence.

Continue with the [framework tour](./framework-tour), the
[feature catalog](../features/), or the [Orders recipe](../cookbook/orders-api).
