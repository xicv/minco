---
title: Exercised Examples
description: Find current examples by application, plugin, runtime, database, and evidence boundary.
---

# Exercised Examples

These examples already exist in the repository and run in current quality
gates. For outcome-oriented walkthroughs, start with the
[practical cookbook](../cookbook/) or the complete
[Orders API recipe](../cookbook/orders-api).

## Orders Application

[`examples/orders`](https://github.com/xicv/minco/tree/main/examples/orders)
is the reference contract-to-cloud application.

| Slice | What it demonstrates | Evidence |
|---|---|---|
| OpenAPI | health plus complete order resource family | contract validation and deterministic sync |
| Domain | order invariants and revision transitions | pure unit tests |
| Application | authorization and validation before ports | fake-port tests |
| Memory adapter | idempotent replay and conflict behavior | local behavior tests |
| SQLite adapter | migrations, persistence, cursors, atomic update/delete | real SQLite tests |
| PostgreSQL adapter | the same behavioral contract | compiled; requires configured ignored tests |
| Axum API | envelopes, pagination, ETags, auth, Problem responses | in-process router tests |
| Lambda and worker | native runtime entry points | local build/release qualification |

Start with the
[`openapi.yaml`](https://github.com/xicv/minco/blob/main/examples/orders/openapi/openapi.yaml),
then follow one operation through `domain`, `application`, `adapters`, `api`,
and `service`.

## Third-Party-Style Plugin

[`examples/plugins/third-party-minimal`](https://github.com/xicv/minco/tree/main/examples/plugins/third-party-minimal)
is a standalone Cargo workspace with versioned dependencies and source path
overrides.

```bash
cargo test \
  --manifest-path examples/plugins/third-party-minimal/Cargo.toml \
  --all-features --locked
```

It proves the public package API, strict distribution record, concrete plugin
lifecycle, deterministic registration provenance, and explicit `provider_live:
not_run` boundary.

## Generated PostgreSQL and SQLite Applications

`scripts/test/generated_apps.sh` creates fresh applications for both database
profiles, compiles and tests their initial vertical slices, exercises module,
migration, seeder, worker, adapter, operation, and plugin generators, then
confirms generated TODO specifications fail visibly.

The script proves generation and compiler integration. It does not connect to
a configured PostgreSQL server or deploy either application.

## Feedback Plugin

[`plugins/minco-plugin-feedback`](https://github.com/xicv/minco/tree/main/plugins/minco-plugin-feedback)
demonstrates a larger first-party plugin: contract routes, a framework-free
widget, attachments, transcription boundaries, events, audit, notifications,
client tokens, persistence, and browser tests.

Its browser matrix is local UI evidence. Provider transcription, production
storage, and live notification delivery remain separate.

## Worker and AWS Adapter Examples

The `minco-aws-worker` example exercises partial SQS batch failure, FIFO
fail-forward behavior, bounded concurrency, and redacted message diagnostics.
AWS adapters include ignored Rustack and bounded real-AWS tests. Ignored tests
are compiled evidence, not a claim that the provider ran.

## Pick an Example by Goal

| Goal | Example or guide | Strongest default evidence |
|---|---|---|
| Standard HTTP CRUD | [Orders API end to end](../cookbook/orders-api) | local contract/domain/application/SQLite/HTTP tests |
| Generated application | [Build your first application](../getting-started/first-application) | compiler and local profile |
| External plugin package | [Test a Plugin](../guides/plugin-conformance) | public offline conformance |
| SQS partial batches | [Queues and workers](../guides/background-work) | local runtime and Plan IR tests |
| Client review loop | [Feedback](../guides/feedback) | local persistence/API/widget/browser tests |
| Deployment review | [Plan an AWS Deployment](../guides/deployment) | deterministic offline plan/package evidence |
| Evidence interpretation | [Testing and Evidence](../reference/testing) | explicit boundary vocabulary |
