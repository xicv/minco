---
title: Minco 1.7.0
description: Stable documentation for Apple-first local services and contract-to-cloud Rust application development.
---

# Minco 1.7.0

<p class="doc-kicker">Stable release manual</p>

<div class="version-banner">
  <span><strong>Latest stable release.</strong> These pages target Minco 1.7.0 and Rust 1.97.1.</span>
  <a href="../versions">View all versions</a>
</div>

<p class="doc-lead">This manual describes the published 1.7.0 source. It develops the same contract-to-cloud model as the frozen <a href="../1.6.0/">1.6.0 manual</a>, while keeping later unreleased behavior visibly separate in <a href="../next/">Next</a>.</p>

The 1.7 release makes [Apple Container the fresh local-service default](./guides/local-development)
on qualified Apple silicon hosts while retaining lifecycle receipts, exact
owned-resource recovery, explicit runtime selection and Docker fallback.

## Start with the outcome

The documentation is organized progressively. Begin with one working application,
then move to a focused guide, a real scenario, the component catalog, or exact
reference when the next decision requires it.

<div class="doc-path-grid">
  <a class="doc-path-card" data-index="01 · START" href="./getting-started/framework-tour">
    <strong>Build an application</strong>
    <span>Understand the contract-to-cloud path, project layers, and development loop.</span>
  </a>
  <a class="doc-path-card" data-index="02 · HTTP" href="./guides/resource-api">
    <strong>Use resource APIs</strong>
    <span>Implement create, list, read, update, and delete with one client-facing convention.</span>
  </a>
  <a class="doc-path-card" data-index="03 · CLIENTS" href="./guides/mobile-api">
    <strong>Serve browser and native clients</strong>
    <span>Use one API with explicit PKCE, CORS, retry, compatibility, and device-trust boundaries.</span>
  </a>
  <a class="doc-path-card" data-index="04 · EXTEND" href="./guides/plugin-conformance">
    <strong>Author a plugin</strong>
    <span>Package a statically linked plugin and exercise the public conformance boundary.</span>
  </a>
  <a class="doc-path-card" data-index="05 · AGENTS" href="./guides/agent-development">
    <strong>Develop with coding agents</strong>
    <span>Install version-matched Codex and Claude skills and inspect bounded project context.</span>
  </a>
  <a class="doc-path-card" data-index="06 · AWS" href="./guides/deployment">
    <strong>Operate on AWS</strong>
    <span>Review Plan IR, residual cost, exact artifacts, guarded mutation, and recovery evidence.</span>
  </a>
  <a class="doc-path-card" data-index="07 · MAP" href="./features/">
    <strong>Browse all features</strong>
    <span>See the framework surface by contract, data, runtime, deployment, and evidence plane.</span>
  </a>
  <a class="doc-path-card" data-index="08 · COMPOSE" href="./plugins/">
    <strong>Choose built-in plugins</strong>
    <span>Compare all 19 plugins, adapters, and runtimes by purpose, stability, and cost behavior.</span>
  </a>
  <a class="doc-path-card" data-index="09 · RECIPES" href="./cookbook/">
    <strong>Follow practical recipes</strong>
    <span>Combine the pieces for CRUD, background work, feedback, files, and safe AWS delivery.</span>
  </a>
  <a class="doc-path-card" data-index="10 · BLUEPRINT" href="./cookbook/production-blueprint">
    <strong>Follow a production blueprint</strong>
    <span>Design a burst-ready Orders system from traffic pattern through cost, failure, and evidence.</span>
  </a>
</div>

## See the whole system

Minco keeps five concerns distinct but traceable. That separation lets a person or
coding agent answer “what changes if this operation changes?” without relying on a
runtime service locator or an undocumented control plane.

<div class="framework-plane-grid">
  <div class="framework-plane">
    <span>01 · Contract</span>
    <strong>What clients may do</strong>
    <p>OpenAPI operations, schemas, examples, authentication, pagination, idempotency, and public failures.</p>
  </div>
  <div class="framework-plane">
    <span>02 · Code</span>
    <strong>What the business owns</strong>
    <p>Pure domain rules, use cases, owned ports, authorization decisions, and explicit adapter boundaries.</p>
  </div>
  <div class="framework-plane">
    <span>03 · Capabilities</span>
    <strong>What the application selects</strong>
    <p>Statically linked plugins, typed services, facade features, declared dependencies, and explicit composition.</p>
  </div>
  <div class="framework-plane">
    <span>04 · Resources</span>
    <strong>Where code executes</strong>
    <p>Local processes, Axum HTTP, Lambda HTTP, SQS workers, static delivery, databases, and provider topology.</p>
  </div>
  <div class="framework-plane">
    <span>05 · Evidence</span>
    <strong>What can be claimed</strong>
    <p>Nearest-boundary tests, Plan IR, artifact digests, change sets, hosted verification, promotion, and rollback.</p>
  </div>
</div>

## Real-world path: burst-ready orders

<div class="scenario-panel">
  <div class="scenario-panel-copy">
    <p class="scenario-kicker">Reference architecture</p>
    <h3>Start with a mobile-friendly Orders API, then add only the runtime you can justify.</h3>
    <p>The blueprint connects duplicate-safe order placement, bounded list queries, optimistic concurrency, local SQLite or PostgreSQL, production data choices, optional asynchronous fulfillment, zero provisioned application compute, and exact release evidence.</p>
    <a class="scenario-link" href="./cookbook/production-blueprint">Open the production blueprint</a>
  </div>
  <ul class="scenario-panel-list">
    <li>
      <span>Traffic</span>
      <strong>Quiet most of the day, bursty during ordering windows</strong>
    </li>
    <li>
      <span>Safety</span>
      <strong>Idempotency keys and required ETags at the HTTP boundary</strong>
    </li>
    <li>
      <span>Cost</span>
      <strong>No fixed application compute in the minimal AWS profile</strong>
    </li>
    <li>
      <span>Proof</span>
      <strong>Source, tests, plan, artifact, deployment, and observation stay distinct</strong>
    </li>
  </ul>
</div>

## What is documented here

| Area | Shipped boundary | Use it to answer |
|---|---|---|
| HTTP resources | JSON data envelopes, bounded cursor pages, strong entity tags, idempotent create, and Problem Details | What must every client implement consistently? |
| Browser and native clients | Exact CORS inventories, response metadata, PKCE guidance, bounded retry, and installed-client compatibility | How does one API serve web, iOS, Android, desktop, and automation safely? |
| Plugins | Archive-visible metadata, static composition, facade features, and public offline conformance reports | What can be added without runtime scanning? |
| Data | Explicit SQLite, PostgreSQL, and access-pattern-specific DynamoDB adapters | Which persistence model matches the query and cost profile? |
| Realtime | Subscriber-only AppSync invalidation with authoritative HTTP resynchronization | How can clients refresh without treating events as durable truth? |
| Local and AI | Graph-derived development, bounded ProjectView, read-only MCP, and accessible workbench projections | What context can a person or agent inspect safely? |
| Testing | Domain, application, adapter, HTTP, plugin, deployment, and release evidence remain independent | Which claim has actually been exercised? |
| AWS | Zero provisioned application compute, explicit IAM and wake sources, guarded mutation, and visible residual cost | What will wake, persist, or cost money after deployment? |

## The golden path

```text
new -> contract -> generate -> dev -> migrate -> seed -> test -> inspect
    -> package -> change set -> migrate target -> deploy -> verify
    -> promote exact artifact -> observe or compatibility-checked rollback
```

Each arrow is explicit and inspectable. Minco does not add a runtime service
locator, Active Record layer, hosted control plane, hidden scheduler, or
boot-time production migrations.

## Popular topics

- [Install Minco](./getting-started/installation) and
  [build the first application](./getting-started/first-application).
- Use [typed configuration](./guides/configuration),
  [database lifecycle controls](./guides/database-lifecycle), and
  [graph-driven local development](./guides/local-development).
- Add [identity and sessions](./guides/identity-and-sessions),
  [browser and native client guidance](./guides/mobile-api),
  [events and notifications](./guides/events-and-notifications),
  [files and static sites](./guides/files-and-static-sites), the
  [realtime invalidation path](./guides/realtime), or the
  [client feedback loop](./guides/feedback).
- Inspect the same bounded project model through
  [ProjectView, MCP, and the local workbench](./guides/project-view), or select
  the [DynamoDB Orders adapter](./guides/dynamodb).
- Set up [Codex and Claude Code](./guides/agent-development) with project-local,
  version-matched Minco workflows.
- Review the [complete feature catalog](./features/),
  [built-in component catalog](./plugins/), and
  [Cargo feature reference](./reference/feature-flags).

Continue with the [framework tour](./getting-started/framework-tour), browse the
[exercised examples](./examples/), or use the
[current CLI reference](./reference/cli) when you need exact command behavior.
