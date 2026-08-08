---
title: Documentation Map
description: Find every current Minco documentation area by task, concept, component, runtime, or reference need without falling into historical manuals.
---

# Documentation Map

Use this page when you know **what you are trying to accomplish** but not the
Minco term or page name. The current manual is organized by user intent rather
than by crate layout.

> Historical manuals remain available from the version selector, but current
> search intentionally prioritizes the latest stable manual and `Next` so old
> terminology does not crowd out current answers.

## Start and learn

| Need | Page | What you will get |
|---|---|---|
| Install the toolchain and CLI | [Installation](../getting-started/installation) | exact Rust/Minco versions and install commands |
| Build one application | [Build your first application](../getting-started/first-application) | SQLite-first working loop with production-shaped boundaries |
| Understand the development loop | [Framework tour](../getting-started/framework-tour) | contract-to-cloud sequence and project graph |
| Understand the filesystem | [Project structure](../getting-started/project-structure) | domain/application/adapter/API/service ownership |
| Understand why Minco is structured this way | [Architecture](../explanation/architecture) | planes, dependency direction, runtimes, plans, evidence |

## Build application behavior

| Need | Page | Search terms |
|---|---|---|
| CRUD/resource conventions | [Build a resource API](../guides/resource-api) | create, list, cursor, ETag, If-Match, Problem Details |
| Exact HTTP resource shapes | [Resource API reference](./resource-api) | envelope, pagination, filter, sort, precondition |
| Configuration and secrets | [Configuration](../guides/configuration) | precedence, environment, secret reference, redaction |
| PostgreSQL/SQLite lifecycle | [Migrations and seeders](../guides/database-lifecycle) | SQLx, migration, seed, plan, verify |
| Identity and sessions | [Identity and sessions](../guides/identity-and-sessions) | claims, principal, permission, session, revocation |
| Files and frontend assets | [Files and static sites](../guides/files-and-static-sites) | object storage, S3, signed access, CloudFront, SPA |
| Events and delivery intent | [Events and notifications](../guides/events-and-notifications) | outbox, email, webhook, notification |
| Background execution | [Queues and workers](../guides/background-work) | SQS, worker, partial batch, FIFO, DLQ |
| Realtime refresh signals | [Realtime subscriptions](../guides/realtime) | AppSync, subscription, invalidation, resync |
| Client/developer review loop | [Client feedback loop](../guides/feedback) | feedback, attachment, transcription, AI context |

## Local development and AI-native inspection

| Need | Page | Search terms |
|---|---|---|
| Run local dependencies and processes | [Local development](../guides/local-development) | dev profile, supervisor, readiness, Rustack |
| Inspect the application graph | [ProjectView, MCP, and workbench](../guides/project-view) | project view, graph, MCP, workbench, evidence lane |
| Develop with coding agents | [Codex and Claude Code](../guides/agent-development) | agent plan, sync, doctor, context, eval |
| Diagnose a failure | [Troubleshooting](../guides/troubleshooting) | doctor, drift, readiness, stale ETag, packaging, AWS plan |

## Plugins and extension points

| Need | Page | Search terms |
|---|---|---|
| See official components | [Built-in plugins and adapters](../plugins/) | health, observability, storage, feedback, runtime, database |
| Add a component to an application | [Install and compose plugins](../plugins/using-plugins) | Cargo feature, typed constructor, composition |
| Test plugin distribution and behavior | [Plugin conformance guide](../guides/plugin-conformance) | metadata, archive, offline conformance |
| Look up exact conformance rules | [Plugin conformance reference](./plugin-conformance) | report, fixture, capability, evidence |

## AWS, cost, and operation

| Need | Page | Search terms |
|---|---|---|
| Review and apply AWS topology | [Plan an AWS deployment](../guides/deployment) | Plan IR, SAM, CloudFormation, IAM, change set, promote, rollback |
| Use the Orders DynamoDB access model | [DynamoDB adapter](../guides/dynamodb) | conditional write, index, consistency, no scan |
| Understand the idle-cost promise | [Zero idle, precisely](../explanation/zero-idle) | provisioned compute, wake source, residual cost |
| See one production decision end to end | [Production blueprint](../cookbook/production-blueprint) | burst traffic, Lambda, PostgreSQL, DynamoDB, SQS, recovery |

## Recipes and exercised examples

| Need | Page | What it demonstrates |
|---|---|---|
| Choose a practical composition | [Practical recipes](../cookbook/) | CRUD, auth, uploads, notifications, review environments, frontend/API |
| Trace the reference API completely | [Orders API end to end](../cookbook/orders-api) | contract, CRUD, adapters, HTTP behavior, planning |
| See what the repository actually exercises | [Exercised examples](../examples/) | reference applications and evidence boundaries |
| Browse everything Minco claims today | [Feature catalog](../features/) | contract, data, services, workers, AWS, evidence, non-features |

## Exact reference

Use reference when you need facts rather than a walkthrough:

- [CLI commands](./cli)
- [Cargo feature flags](./feature-flags)
- [Resource API conventions](./resource-api)
- [Plugin conformance](./plugin-conformance)
- [Testing and evidence](./testing)

For Rust type and function signatures, use the version-matched `docs.rs` API
reference linked from the site navigation. Repository-generated package,
plugin, schema, diagnostic, and release reference remains under
`docs/reference/generated/` in the source repository.

## Common search vocabulary

Minco deliberately uses precise terms. These aliases can help when searching:

| If you are thinking… | Search for… |
|---|---|
| controller / route handler | `handler`, `operationId`, `resource API` |
| service layer | `application use case`, `application port` |
| repository / DAO | `adapter`, `port`, `SQLx`, `DynamoDB` |
| cron / scheduled job | `schedule`, `wake source`, `worker` |
| background job | `SQS`, `worker`, `partial batch`, `DLQ` |
| optimistic locking | `ETag`, `If-Match`, `412`, `revision` |
| retry-safe POST | `idempotency`, `Idempotency-Key`, `replay` |
| deploy preview | `Plan IR`, `change set`, `deploy plan` |
| zero server cost | `zero idle`, `residual cost`, `wake source` |
| AI project context | `ProjectView`, `MCP`, `agent context` |

## Version and freshness rules

The **stable manual** describes the published release. **Next** describes source
on `main` that has not yet been released. A pull request, prototype, enum
variant, or provider seam is not documented as shipped product behavior until
it is part of the corresponding source/release boundary.

This matters when multiple framework changes are being developed in parallel:
unmerged work can inform future documentation, but it must not silently rewrite
the meaning of a frozen stable manual.

If you still cannot find the right page, start with
[Troubleshooting](../guides/troubleshooting) for failures or the
[Feature catalog](../features/) for capability discovery.
