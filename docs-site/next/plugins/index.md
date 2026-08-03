---
title: Built-in Plugins and Adapters
description: Compare all official Minco plugins, adapters, and runtimes before compiling them into an application.
---

# Built-in Plugins and Adapters

Minco ships **16 built-in components** in its checked catalog: 11 plugins, two
database adapters, two AWS runtimes, and one AWS adapter bundle. Every component
is an ordinary Rust crate with archive-visible distribution metadata and an
explicit facade feature.

```bash
cargo minco plugin list --json
cargo minco plugin validate --json
```

Only health, observability, and idempotency are enabled by the default facade.
Everything else is opt-in. Catalog metadata never downloads, constructs, or
loads executable code.

## Health

<span class="doc-badge doc-badge-stable">stable</span>
<span class="doc-badge">default</span>

`minco-plugin-health` provides liveness, readiness, and dependency health
registration. Use liveness for process health and readiness for whether the
selected dependencies can serve traffic; do not turn deep provider diagnostics
into public response bodies.

Feature: `plugin-health` · Runtime: native

## Observability

<span class="doc-badge doc-badge-stable">stable</span>
<span class="doc-badge">default</span>

`minco-plugin-observability` configures structured tracing and
CloudWatch-compatible logging. Request IDs and stable error codes cross
boundaries; sensitive headers, secret values, provider payloads, and customer
data remain redacted.

Feature: `plugin-observability` · Runtime: native

## Idempotency

<span class="doc-badge doc-badge-stable">stable</span>
<span class="doc-badge">default</span>

`minco-plugin-idempotency` defines idempotency keys, canonical request
fingerprints, replay/conflict behavior, and a storage port. It supports the
resource create convention without prescribing the application's persistence
adapter.

Feature: `plugin-idempotency` · Runtime: native

## Identity

<span class="doc-badge">beta</span>

`minco-plugin-identity` maps already-verified claims into provider-neutral
identities, scopes, and permissions. Credential verification remains an
explicit ingress/provider responsibility; business authorization remains in
application use cases.

Feature: `plugin-identity` · Guide: [Identity and sessions](../guides/identity-and-sessions)

## Sessions

<span class="doc-badge">beta</span>

`minco-plugin-sessions` defines session issuance, lookup, expiry, and revocation
with injected stores and clocks. Applications own cookie/token policy,
retention, and the chosen persistence profile.

Feature: `plugin-sessions` · Guide: [Identity and sessions](../guides/identity-and-sessions)

## Object Storage

<span class="doc-badge">beta</span>

`minco-plugin-object-storage` supplies provider-neutral storage ports for
uploads, exports, and feedback attachments. Application policy owns media,
size, encryption, retention, scanning, and tenant/object-key boundaries.

Feature: `plugin-object-storage` · Guide: [Files and static sites](../guides/files-and-static-sites)

## Events

<span class="doc-badge">beta</span>

`minco-plugin-events` supplies domain-event and transactional-outbox ports. It
installs no scheduler or polling service; request-assisted dispatch or an
explicit worker owns delivery.

Feature: `plugin-events` · Guide: [Events and notifications](../guides/events-and-notifications)

## Notifications

<span class="doc-badge">beta</span>

`minco-plugin-notifications` models email, webhook, in-app, and developer
notification intent. Applications inject provider adapters and own recipients,
consent, templates, rate limits, and failure policy.

Feature: `plugin-notifications` · Guide: [Events and notifications](../guides/events-and-notifications)

## Audit

<span class="doc-badge">beta</span>

`minco-plugin-audit` records append-only business history independently of
operational logs. Applications own retention, access, actor/subject policy, and
the durable adapter.

Feature: `plugin-audit` · Runtime: native

## Feedback

<span class="doc-badge doc-badge-stable">stable</span>

`minco-plugin-feedback` provides the client review loop: threads, attachments,
optional voice transcription, discussion, optimistic status changes,
notifications, audit, and deterministic AI context. It supports PostgreSQL and
SQLite through injected ports.

Feature: `plugin-feedback` · Guide: [Client feedback loop](../guides/feedback)

## Static Site

<span class="doc-badge">beta</span>

`minco-plugin-static-site` declares private static assets, CDN caching, SPA
fallback, and optional custom-domain deployment intent. Exact source bytes are
bound into the release and published separately from infrastructure apply.

Feature: `plugin-static-site` · Guide: [Files and static sites](../guides/files-and-static-sites)

## SQLx PostgreSQL

<span class="doc-badge">beta adapter</span>

`minco-sqlx-postgres` supplies bounded PostgreSQL pools and explicit migration
support. Neon, self-hosted PostgreSQL, RDS, and Aurora remain distinct
deployment profiles with visible connection and cost assumptions.

Feature: `sqlx-postgres` · Cost intent: provider managed

## SQLx SQLite

<span class="doc-badge">beta adapter</span>

`minco-sqlx-sqlite` supplies SQLite pools for local, desktop, and persistent
single-process use. Durability and multi-process constraints remain explicit.

Feature: `sqlx-sqlite` · Cost intent: storage only

## AWS Adapters

<span class="doc-badge">beta adapter</span>

`minco-aws-adapters` contains explicit opt-in AWS service adapters, including
the local Rustack seams used for S3, SQS, SSM, and STS conformance. Provider
credentials, endpoints, resource names, IAM, and retention are
application/deployment inputs.

Feature: `aws-adapters` · Cost intent: provider managed and storage only

## AWS Lambda

<span class="doc-badge">beta runtime</span>

`minco-aws-lambda` runs the same Axum router as the local entry point in a
native Lambda HTTP function and can load explicitly referenced SSM
configuration. The minimal profile uses no provisioned concurrency.

Feature: `aws-lambda` · Idle class: zero compute · Wake source: HTTP request

## AWS Worker

<span class="doc-badge">beta runtime</span>

`minco-aws-worker` maps SQS Lambda batches with partial-failure, FIFO,
concurrency, and diagnostic rules. Queue, mapping, DLQ, IAM, and retry policy
remain explicit.

Feature: `aws-worker` · Idle classes: zero compute and storage only · Wake source: queue message · Guide: [Queues and workers](../guides/background-work)

## Read the Authority

The generated
[`plugins.md`](https://github.com/xicv/minco/blob/main/docs/reference/generated/plugins.md)
is derived from `plugins/catalog.toml`, package-root `minco-plugin.json` files,
and the linked descriptors. It includes runtimes, databases, resources, wake
sources, cost intent, configuration fields, operations, and metadata digests.

Next: [install and compose plugins](./using-plugins) or
[test a plugin](../guides/plugin-conformance).
