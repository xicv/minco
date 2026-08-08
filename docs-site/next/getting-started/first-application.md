---
title: Build Your First Application
description: Generate, inspect, run, and exercise a Minco application locally without deploying it.
---

# Build Your First Application

This path uses SQLite, so the first request needs no database container or AWS
account. It still follows the same contract, application, adapter, HTTP,
planning, and evidence boundaries used by a larger deployment.

<div class="doc-summary-grid">
  <div class="doc-summary-card">
    <span>Runtime</span>
    <strong>One local Rust process</strong>
    <p>Axum serves the generated API while SQLite provides a real persistence engine.</p>
  </div>
  <div class="doc-summary-card">
    <span>Cloud changes</span>
    <strong>None</strong>
    <p>Every command in this guide is local or a dry run; no AWS account is required.</p>
  </div>
  <div class="doc-summary-card">
    <span>Architecture</span>
    <strong>Production-shaped</strong>
    <p>Domain, application, adapter, delivery, configuration, and evidence remain separate.</p>
  </div>
  <div class="doc-summary-card">
    <span>Outcome</span>
    <strong>An inspectable vertical slice</strong>
    <p>You can run it, call it, add a resource, and trace the result through the project graph.</p>
  </div>
</div>

## 1. Generate the project

```bash
cargo minco new hello-minco --database sqlite
cd hello-minco
cp .env.example .env
```

The generator creates ordinary Rust, TOML, SQL, and OpenAPI source. Domain and
application crates remain independent of Axum, SQLx, Lambda, and AWS SDKs. You
can inspect or change every generated file; there is no remote control plane
that must remain available for the application to run.

<div class="expected-output">
  <strong>What to inspect</strong>
  <p>Confirm that the project contains an OpenAPI contract, domain and application boundaries, a SQLite adapter, an HTTP delivery crate, environment configuration, and explicit quality commands.</p>
</div>

## 2. Inspect before running

```bash
cargo minco doctor
cargo minco contract check
cargo minco inspect --json
```

These commands answer different questions:

| Command | Question it answers | What it does not prove |
|---|---|---|
| `doctor` | Are the required local tools and project inputs available? | That the application behavior is correct |
| `contract check` | Is the OpenAPI document valid for Minco's reviewed boundary? | That implementation and contract are synchronized |
| `inspect --json` | How do contract, code, capabilities, resources, and evidence connect? | That any provider resource exists |

Use `explain` when you need the complete trace for one OpenAPI operation:

```bash
cargo minco explain getPlatform --json
```

A successful trace should identify the contract operation and its known
implementation, capability, resource, and evidence links. Missing or ambiguous
links remain diagnostics instead of being silently guessed.

## 3. Review the local plan

```bash
cargo minco dev --profile sqlite --dry-run --json
```

The dry run projects the graph into a local execution plan. Review the selected
environment, lifecycle stages, process command, ports, migration step, and
readiness probe before anything starts.

<div class="expected-output">
  <strong>Dry-run boundary</strong>
  <p>A dry run can prove that Minco built a coherent plan. It cannot prove that the process starts, the database opens, migrations apply, or the HTTP boundary responds.</p>
</div>

## 4. Start and exercise the application

```bash
cargo minco dev --profile sqlite
```

In another terminal, exercise the health contract:

```bash
curl --fail --silent http://127.0.0.1:3000/health/live
curl --fail --silent http://127.0.0.1:3000/health/ready
```

The liveness response follows the generated contract and includes whether the
process is live plus the generated service name. Readiness evaluates selected
dependencies without exposing provider credentials or internal diagnostics.

Ctrl-C stops the supervised process group. It does not silently reset durable
data.

## 5. Understand the ownership boundaries

| Layer | Owns | Must not own |
|---|---|---|
| Contract | Client-visible operations, schemas, examples, security, and public failures | SQL, provider clients, or business implementation |
| Domain | Invariants and state transitions | Axum extractors, SQLx queries, or AWS types |
| Application | One use case, authorization, transaction intent, and owned ports | Provider-specific persistence or transport mapping |
| Adapter | SQLite, PostgreSQL, DynamoDB, or provider implementation details | Business policy that should be portable |
| HTTP | Extraction, validation mapping, one use-case call, and response mapping | Database queries or hidden orchestration |
| Evidence | What was checked, exercised, packaged, deployed, or observed | Claims stronger than the underlying proof |

This separation is what lets the same use case run behind a local Axum service
or a native Lambda HTTP runtime without moving business rules into the
transport.

## 6. Add a resource deliberately

Preview the generated vertical slice before writing files:

```bash
cargo minco make resource widget --dry-run --json
```

Then follow the required order:

<ol class="workflow-rail">
  <li>
    <strong>Define the client boundary.</strong>
    <p>Add OpenAPI requests, responses, security, examples, idempotency, pagination, conditional mutation, and Problem bodies.</p>
  </li>
  <li>
    <strong>Synchronize deterministic bindings.</strong>
    <p>Review generated changes instead of hand-maintaining a second schema model.</p>
  </li>
  <li>
    <strong>Write the nearest failing test.</strong>
    <p>Start with a pure domain test or an application test using fake ports.</p>
  </li>
  <li>
    <strong>Implement one use case.</strong>
    <p>Keep authorization and business decisions in the application boundary.</p>
  </li>
  <li>
    <strong>Add persistence only when needed.</strong>
    <p>Implement the application-owned port with SQLite first, then add another adapter for a justified production access pattern.</p>
  </li>
  <li>
    <strong>Exercise the real HTTP boundary.</strong>
    <p>Use an in-process Axum router so extraction, headers, status codes, and Problem mapping are covered together.</p>
  </li>
  <li>
    <strong>Inspect operational consequences.</strong>
    <p>Review resources, IAM, connection pressure, cost classes, wake sources, and evidence before selecting an AWS profile.</p>
  </li>
</ol>

The [resource API guide](../guides/resource-api) supplies standard create, list,
read, update, and delete shapes without introducing a generic repository or
ORM.

## 7. Run the local gates

```bash
cargo minco test unit
cargo minco test feature
cargo minco check --with-cargo
```

Local success is not deployment proof. Packaging, hosted CI, provider-backed
verification, promotion, and production observation remain separate evidence.

## 8. Compare with a complete application

The first project proves the development loop. The
[Orders API end-to-end recipe](../cookbook/orders-api) adds idempotent create,
opaque cursor pagination, strong ETags, conditional updates and deletes, real
persistence adapters, and release planning. The
[production blueprint](../cookbook/production-blueprint) then explains how
traffic, failure behavior, wake sources, residual cost, and recovery affect the
runtime choice.

Continue with the [framework tour](./framework-tour), the
[feature catalog](../features/), or the [Orders recipe](../cookbook/orders-api).
