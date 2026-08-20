---
title: Production Blueprint for a Burst-Ready Orders Service
description: Design a real Minco service from traffic pattern through contract, runtime, persistence, cost, failure, delivery evidence, and recovery.
---

# Production Blueprint for a Burst-Ready Orders Service

This blueprint shows how to turn the reference Orders vertical slice into a
production decision. It is intentionally concrete: a web, mobile, or partner
client places and edits orders; traffic is quiet for long periods and bursts
during ordering windows; duplicate submissions and concurrent edits must be
safe; background fulfillment may be added later; and fixed application compute
is not justified.

It is not a claim that one topology fits every system. Minco keeps the
alternatives visible so the team can choose the smallest composition that
satisfies the access pattern, reliability target, and operating model.

<div class="scenario-fact-grid">
  <div class="scenario-fact">
    <span>Users</span>
    <strong>Mobile, web, and partner clients</strong>
    <p>All clients share one reviewed HTTP contract and receive the same Problem shapes.</p>
  </div>
  <div class="scenario-fact">
    <span>Load</span>
    <strong>Long idle periods, short bursts</strong>
    <p>Request-driven Lambda and optional queue-driven workers avoid always-on application processes.</p>
  </div>
  <div class="scenario-fact">
    <span>Correctness</span>
    <strong>Retry-safe and revision-safe</strong>
    <p>Idempotency protects create; strong ETags and conditional writes protect update and delete.</p>
  </div>
  <div class="scenario-fact">
    <span>Operations</span>
    <strong>Review before mutation</strong>
    <p>Topology, IAM, wake sources, connection pressure, cost classes, and artifact identity remain inspectable.</p>
  </div>
</div>

## Design from the traffic pattern

Begin with behavior and constraints instead of selecting AWS services first.

| Requirement | Design response | Why it matters |
|---|---|---|
| Clients may retry after timeouts | Require `Idempotency-Key` for `POST /orders` and retain the immutable original result | A timeout does not become a duplicate order |
| Multiple clients may edit the same order | Return a strong `ETag`; require `If-Match` for update and delete | Stale writes fail explicitly instead of overwriting newer state |
| List traffic can spike | Bound page size, sort, filters, and cursor length | Work remains predictable for clients and adapters |
| Traffic is often zero | Use request-driven Lambda HTTP without provisioned concurrency | Application compute is not reserved while idle |
| Fulfillment may be asynchronous | Add an explicit event/outbox, SQS queue, and `aws-worker` only when required | Background work is visible, retryable, and independently bounded |
| Data access is known | Select PostgreSQL or an access-pattern-specific DynamoDB adapter | Persistence follows query and consistency needs, not a generic repository |
| Production changes require review | Generate Plan IR and an immutable change set before apply | IAM, resources, wake sources, and cost can be inspected before mutation |
| Releases must be attributable | Package once and bind source, configuration, and digests into the release manifest | Promotion and compatible rollback reuse exact bytes |

## Production shape

<div class="topology-strip" role="img" aria-label="Clients call API Gateway and Lambda HTTP, which invokes Orders use cases and a selected database">
  <div class="topology-node">
    <small>Clients</small>
    <strong>Web · mobile · partner</strong>
    <span>OpenAPI-generated or contract-tested clients</span>
  </div>
  <span class="topology-arrow" aria-hidden="true">→</span>
  <div class="topology-node">
    <small>Ingress</small>
    <strong>API Gateway HTTP API</strong>
    <span>Exact routes, limits, CORS, identity, and public error policy</span>
  </div>
  <span class="topology-arrow" aria-hidden="true">→</span>
  <div class="topology-node">
    <small>Compute</small>
    <strong>ARM64 Lambda HTTP</strong>
    <span>Same Axum router as local service; no provisioned concurrency</span>
  </div>
  <span class="topology-arrow" aria-hidden="true">→</span>
  <div class="topology-node">
    <small>State</small>
    <strong>PostgreSQL or DynamoDB</strong>
    <span>Selected from transactions, queries, consistency, connections, and cost</span>
  </div>
</div>

The minimal synchronous path ends at the selected data adapter. Add this
explicit branch only when the product needs asynchronous fulfillment,
notifications, imports, or other deferred work:

```text
successful transaction
    -> durable outbox/event intent
    -> bounded dispatcher
    -> SQS queue + DLQ
    -> Lambda worker with partial-batch response
    -> application use case + provider adapter
```

The event is not durable business truth by itself. The authoritative state
remains behind the application and HTTP read boundary.

## Keep the contract executable

The reference contract already describes the critical mobile and partner
behaviors:

```text
POST   /orders             place exactly once
GET    /orders             list with bounded opaque cursor
GET    /orders/{orderId}   read and receive the current ETag
PATCH  /orders/{orderId}   update only with current If-Match
DELETE /orders/{orderId}   delete only with current If-Match
```

Before implementation changes, run:

```bash
cargo minco contract check
cargo minco contract sync --check
cargo minco explain placeOrder --json
cargo minco explain updateOrder --json
```

Review that each operation resolves to one delivery handler, one application
use case, its allowed adapters, and relevant evidence. The graph should fail on
missing or ambiguous identity rather than choosing a plausible path.

## Choose persistence from the access pattern

### PostgreSQL profile

Choose PostgreSQL when the application needs relational joins, flexible
reporting, multi-entity transactions, mature SQL operations, or already has a
PostgreSQL operating model.

Review:

- maximum concurrent Lambda executions that can reach the database;
- pool size per execution environment;
- provider connection limits and any proxy;
- migration ownership and rollback compatibility;
- backups, retention, Region, and availability;
- whether the selected provider has its own baseline or idle cost.

Minco's bounded pool and `max_database_connections` policy make connection
pressure visible; they do not make an unsuitable database capacity plan safe.

### DynamoDB profile

Choose DynamoDB when order access is defined by known keys and indexes, the
conditional-write model fits, and on-demand AWS-native operation is more
important than ad hoc relational queries.

The reference adapter uses conditional transactions, strong point reads, and
bounded indexed list queries. It deliberately avoids table scans and a generic
CRUD repository. Review:

- partition and sort keys for each operation;
- index projection and list ordering;
- conditional expressions for revision and idempotency;
- which reads are strong or eventually consistent;
- item growth, retention, backup, and encryption;
- exact table and index IAM actions.

### SQLite profile

Keep SQLite for the first application, local tests, desktop use, and other
explicitly single-process durability profiles. Do not present it as a hidden
drop-in substitute for a horizontally scaled production database.

## Make low idle cost a checked policy

The reference repository encodes the intended minimal profile:

```toml
[cost_policy]
deny_fixed_compute = true
deny_nat_gateway = true
deny_provisioned_concurrency = true
deny_scheduled_wakeups = true
max_reserved_concurrency = 5
max_database_connections = 20
```

These controls turn architectural intent into plan validation:

- `deny_fixed_compute` rejects always-on application compute in the selected
  profile;
- `deny_nat_gateway` prevents an easy-to-miss fixed network charge in the
  minimal topology;
- `deny_provisioned_concurrency` preserves request-driven Lambda capacity;
- `deny_scheduled_wakeups` stops background timers from quietly defeating the
  idle model;
- concurrency and connection bounds cap one class of load amplification.

They do not make the total bill zero. The selected database, DynamoDB storage
and requests, S3, CloudFront, API Gateway, queues, logs, metrics, data transfer,
domains, certificates, and retained artifacts can still create usage or
storage cost.

## Develop with the production boundaries intact

Use SQLite for the smallest local loop:

```bash
cargo minco dev --profile sqlite --dry-run --json
cargo minco dev --profile sqlite
```

Use the PostgreSQL profile when the production adapter needs real relational
behavior:

```bash
cargo minco dev --profile postgres --dry-run --json
cargo minco dev --profile postgres
```

Then exercise the client guarantees, not only happy-path CRUD:

<ol class="workflow-rail">
  <li>
    <strong>Retry one create with the same key and payload.</strong>
    <p>Expect the immutable original order result, not a second order.</p>
  </li>
  <li>
    <strong>Retry the key with a different payload.</strong>
    <p>Expect an explicit conflict Problem and no additional mutation.</p>
  </li>
  <li>
    <strong>List through more than one page.</strong>
    <p>Pass the opaque cursor back unchanged and verify stable ordering.</p>
  </li>
  <li>
    <strong>Race two updates from the same ETag.</strong>
    <p>Only the first current conditional write may succeed; the stale request receives `412`.</p>
  </li>
  <li>
    <strong>Make the selected dependency unavailable.</strong>
    <p>Readiness should fail with bounded public detail while liveness continues to describe the process.</p>
  </li>
  <li>
    <strong>Stop the supervisor.</strong>
    <p>Processes should terminate cleanly without silently deleting durable data.</p>
  </li>
</ol>

## Plan the AWS change before mutation

Run the non-mutating sequence first:

```bash
cargo minco inspect --json
cargo minco deploy plan --json
cargo minco cost --json
cargo minco package
cargo minco release verify target/minco/release.json
```

Review at least:

| Plane | Questions |
|---|---|
| Contract | Are operations, schemas, examples, authentication, and Problem bodies complete? |
| Code | Does every operation resolve to the intended use case, adapter, and runtime? |
| Resources | Which APIs, functions, queues, tables, buckets, distributions, roles, and policies appear? |
| Wake | What can start compute: HTTP request, queue message, or an explicitly approved schedule? |
| Cost | Which resources are zero-compute, storage-only, usage-based, or fixed? |
| Capacity | What are reserved concurrency, worker concurrency, batch size, and database connection pressure? |
| Evidence | Which tests, source identities, manifests, digests, and provider receipts support the proposed claim? |

Only after the plan and package are accepted should the delivery workflow
create and review a provider change set, apply target migrations under the
correct environment guard, deploy, and run hosted verification.

## Wake and residual cost model

| Component | Wake source | Idle application compute | Residual considerations |
|---|---|---:|---|
| Lambda HTTP | API Gateway request | Zero when provisioned concurrency is denied | Requests, duration, logs, API Gateway, data transfer |
| Lambda worker | SQS message | Zero between messages | Queue requests/storage, retries, DLQ retention, duration, logs |
| DynamoDB | Application request | Not application compute | Storage, reads/writes, backups, streams if selected |
| Serverless PostgreSQL profile | Application request and provider policy | Depends on provider profile | Storage, minimum capacity, connection proxy, backups, transfer |
| S3/static assets | HTTP request or deployment publish | Zero | Stored bytes, requests, CloudFront, invalidations, transfer |
| Observability | Runtime activity | Zero between activity | Log ingestion/retention, metrics, traces, alarms |
| Domain/DNS/certificates | External lifecycle | Zero | Registration and selected hosted-zone or certificate services |

The plan should identify the actual selected resources and cost classes. This
table is a review frame, not a provider quote.

## Failure and recovery design

### Duplicate or delayed client request

The idempotency store must claim the key and canonical request fingerprint
atomically. A replay returns the original immutable result; a changed request
with the same key conflicts. Retention must be longer than the client retry
window and documented as application policy.

### Stale concurrent mutation

The adapter includes the expected revision in the write predicate. Missing
preconditions return `428`; stale preconditions return `412`. The client reads
the current representation before deciding whether to retry or ask a user to
resolve the conflict.

### Partial worker failure

The SQS runtime reports failed records without retrying successful records.
FIFO fail-forward, batch size, visibility timeout, maximum receives, DLQ,
concurrency, idempotency, and database connections remain explicit inputs.

### Dependency outage

Liveness answers whether the process can execute. Readiness answers whether the
selected dependencies can serve traffic. Public health bodies stay bounded;
provider payloads and secrets remain in protected diagnostics.

### Failed deployment

Account, Region, environment, source digest, artifact digest, migration state,
change-set identity, and drift checks fail closed. Do not rebuild during
promotion or rollback. Reuse the exact verified artifact and require a
compatibility check before reversing application or schema versions.

## Evidence required for each claim

<div class="evidence-grid">
  <div class="evidence-card">
    <span>Local behavior</span>
    <strong>Nearest-boundary tests</strong>
    <p>Pure domain, fake-port application, real-engine adapter, and in-process HTTP tests.</p>
  </div>
  <div class="evidence-card">
    <span>Topology</span>
    <strong>Plan and structure checks</strong>
    <p>Resources, IAM, triggers, cost, wake, capacity, and generated provider structure.</p>
  </div>
  <div class="evidence-card">
    <span>Artifact</span>
    <strong>Immutable package identity</strong>
    <p>Source identity, configuration projection, binary and asset digests, and release verification.</p>
  </div>
  <div class="evidence-card">
    <span>Environment</span>
    <strong>Hosted verification and observation</strong>
    <p>Exact deployed identity, live endpoints, selected provider behavior, alarms, and production evidence.</p>
  </div>
</div>

A useful release checklist is:

```text
contract checked
bindings synchronized
domain and application tests passed
selected adapters exercised against real engines
HTTP boundary exercised in process
plugin graph and conformance checked
Plan IR reviewed for resource, IAM, wake, cost, and capacity
package and release manifest verified
provider change set reviewed
target migration applied and verified
hosted API and worker behavior verified
exact artifact promoted
observation and compatible rollback path confirmed
```

## Extend only when the requirement appears

Add capabilities deliberately:

- [identity and sessions](../guides/identity-and-sessions) for verified users
  and revocable application sessions;
- [events and notifications](../guides/events-and-notifications) plus
  [queues and workers](../guides/background-work) for asynchronous delivery;
- [files and static sites](../guides/files-and-static-sites) for uploads or a
  separately published frontend;
- [realtime invalidation](../guides/realtime) when subscribed clients benefit
  from a refresh signal but HTTP remains authoritative;
- [client feedback](../guides/feedback) for review environments and bounded
  development-ready context.

Continue with the [Orders API end-to-end recipe](./orders-api), the
[deployment guide](../guides/deployment), or
[testing and evidence](../reference/testing).
