---
title: Local Development
description: Inspect and run the graph-selected local topology with SQLite, PostgreSQL, or Rustack.
---

# Local Development

`cargo minco dev` derives one `DevPlan` from the same application graph used by
inspection and deployment. It supervises declared services, lifecycle stages,
process groups, readiness probes, and shutdown; it does not infer hidden local
infrastructure.

## Preview first

```bash
cargo minco dev --dry-run --json
```

Review:

- the selected profile and environment;
- PostgreSQL, Rustack, or other declared services;
- migration and explicit seed stages;
- API, worker, and frontend process commands;
- ports, endpoint overrides, readiness, and shutdown behavior.

The dry run starts nothing and changes no data.

## SQLite loop

```bash
cargo minco dev --profile sqlite
```

This is the smallest persistent local profile and needs no Docker service. Use
it for contract, domain, application, HTTP, and SQLite adapter work.

## PostgreSQL and local AWS seams

```bash
cargo minco dev
```

The reference default selects PostgreSQL plus the Rustack services declared by
the graph. Standard AWS endpoint overrides let the same adapters talk to local
S3, SQS, SSM, and STS seams without contacting AWS.

Explicit port overrides make parallel workspaces possible:

```bash
cargo minco dev --port 31000 --rustack-port 4567
```

## Exercise one request

Development identity headers are accepted only when the application explicitly
enables them. A local Orders create request looks like:

```bash
curl --fail-with-body --silent \
  --request POST http://127.0.0.1:3000/orders \
  --header 'content-type: application/json' \
  --header 'idempotency-key: local-order-1' \
  --header 'x-minco-subject: local-user' \
  --header 'x-minco-permissions: orders.create,orders.read' \
  --data '{"customerReference":"LOCAL-001","lines":[{"sku":"MINCO-001","quantity":2}]}'
```

Repeat the same body and key to exercise idempotent replay. Reuse the key with a
different body to receive a conflict Problem response.

## Stop and recover

Ctrl-C stops the supervised process group and selected containers together. It
does not reset volumes. When a run fails, inspect the emitted process/service
identity and readiness reason, fix the declared input, then rerun the dry plan.

Local success proves only the selected local topology. Ignored provider tests,
hosted CI, AWS deployment, and runtime observation remain separate evidence.
