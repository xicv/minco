---
layout: home
title: One contract. Every runtime. Zero idle by default.

hero:
  name: Minco
  text: One contract. Every runtime. No idle application compute.
  tagline: Build contract-first Rust applications locally, inspect every AWS resource and wake source, then ship immutable artifacts with evidence attached.
  image:
    src: /minco-system.svg
    alt: Diagram showing a Minco contract flowing into local, HTTP API, and queue worker runtimes with deployment evidence
  actions:
    - theme: brand
      text: Read the 1.6.0 docs
      link: /1.6.0/
    - theme: alt
      text: Explore the production blueprint
      link: /1.6.0/cookbook/production-blueprint

features:
  - icon: SPEC
    title: Contract first
    details: OpenAPI defines the reviewed HTTP boundary before handlers, adapters, generated bindings, and tests.
  - icon: PLAN
    title: Cost and wake behavior are visible
    details: Plan IR exposes resources, IAM, connection pressure, wake sources, and residual managed-service cost before AWS mutation.
  - icon: RUN
    title: One application, explicit runtimes
    details: Compose ordinary Rust for local service, native Lambda HTTP, SQS workers, static delivery, and selected data adapters.
  - icon: PROVE
    title: Evidence follows the release
    details: Tests, source identity, immutable artifacts, change sets, verification receipts, and exact promotion remain connected.
---

<section class="home-section" aria-labelledby="operating-model-title">
  <div class="home-section-head">
    <div>
      <p class="home-kicker">The Minco operating model</p>
      <h2 id="operating-model-title">Keep the contract, application, infrastructure, and evidence connected.</h2>
    </div>
    <p>Minco is intentionally narrow. It standardizes the path from a reviewed API contract to a low-idle-cost AWS application without hiding Rust, Axum, SQLx, DynamoDB, Lambda, IAM, or the release artifact that reaches production.</p>
  </div>
  <ol class="home-flow">
    <li class="home-flow-step">
      <span class="home-flow-index">01 · SPEC</span>
      <strong>Define the boundary</strong>
      <p>Write operations, schemas, security, examples, idempotency, pagination, and failure bodies in OpenAPI.</p>
    </li>
    <li class="home-flow-step">
      <span class="home-flow-index">02 · GRAPH</span>
      <strong>Implement the use case</strong>
      <p>Keep domain rules pure, application ports owned by the use case, and provider code inside explicit adapters.</p>
    </li>
    <li class="home-flow-step">
      <span class="home-flow-index">03 · PLAN</span>
      <strong>Inspect before mutation</strong>
      <p>Review selected runtimes, resources, IAM, database pressure, wake sources, fixed cost, and deployment guards.</p>
    </li>
    <li class="home-flow-step">
      <span class="home-flow-index">04 · PROVE</span>
      <strong>Promote exact bytes</strong>
      <p>Bind source, tests, package digests, change sets, hosted verification, and rollback compatibility into one chain.</p>
    </li>
  </ol>
</section>

<section class="home-section home-scenario-panel" aria-labelledby="scenario-title">
  <div class="home-scenario-copy">
    <p class="home-kicker">Real-world reference path</p>
    <h2 id="scenario-title">A burst-ready Orders API you can inspect before AWS changes.</h2>
    <p>The reference application covers idempotent order placement, opaque cursor pagination, strong ETags, conditional updates, SQLite and PostgreSQL development, a DynamoDB access model, Lambda HTTP, optional queue workers, and exact release evidence.</p>
    <a class="home-text-link" href="./1.6.0/cookbook/production-blueprint">Read the production blueprint</a>
  </div>
  <ul class="home-scenario-facts">
    <li>
      <span>Retry safety</span>
      <strong>Same key, same result; conflicting payload, explicit problem</strong>
    </li>
    <li>
      <span>Concurrency</span>
      <strong>Strong ETags and required conditional mutation</strong>
    </li>
    <li>
      <span>Runtime</span>
      <strong>Local service, Lambda HTTP, and bounded SQS worker</strong>
    </li>
    <li>
      <span>Delivery</span>
      <strong>Plan, package, verify, promote, observe, or compatible rollback</strong>
    </li>
  </ul>
</section>

<section class="home-section home-zero-idle" aria-labelledby="zero-idle-title">
  <div class="home-zero-stat" aria-label="Zero provisioned application compute while idle">
    <span class="home-zero-value">0</span>
    <span class="home-zero-unit">provisioned application compute at rest</span>
  </div>
  <div class="home-zero-copy">
    <p class="home-kicker">Precise, not magical</p>
    <h2 id="zero-idle-title">Zero idle is a reviewable constraint, not a pricing slogan.</h2>
    <p>The minimal profile denies fixed application compute, NAT Gateway, provisioned concurrency, and scheduled wakeups. Storage, logs, data transfer, domains, and selected managed services can still cost money, so Minco keeps those residual classes visible in the plan.</p>
    <div class="home-zero-links">
      <a href="./1.6.0/explanation/zero-idle">Understand zero idle</a>
      <a href="./1.6.0/guides/deployment">Review deployment guards</a>
      <a href="./1.6.0/reference/testing">See the evidence model</a>
    </div>
  </div>
</section>

<section class="home-section home-cta" aria-labelledby="start-title">
  <div>
    <p class="home-kicker">Choose a path</p>
    <h2 id="start-title">Start small without creating a throwaway architecture.</h2>
    <p>Generate an SQLite-backed application with no AWS account, then move through the same contract, application, adapter, testing, planning, and release boundaries used by production profiles.</p>
  </div>
  <div class="home-cta-actions">
    <a href="./1.6.0/getting-started/first-application">Build the first application</a>
    <a href="./1.6.0/features/">Browse the feature map</a>
  </div>
</section>
