---
title: Next
description: Unreleased Minco documentation.
---

# Next

This is the current-development manual for Minco: a contract-to-cloud Rust
framework for low-idle-cost web applications on AWS. It documents the source on
`main`; it never changes the meaning of the
[stable 0.6.0 documentation](/0.6.0/).

The manual is organized progressively: begin with one working application,
then reach for focused guides, the component catalog, practical recipes, or
exact reference when you need them.

<div class="doc-path-grid">
  <a class="doc-path-card" href="./getting-started/framework-tour">
    <strong>Build an application</strong>
    <span>Understand the contract-to-cloud path, project layers, and development loop.</span>
  </a>
  <a class="doc-path-card" href="./guides/resource-api">
    <strong>Use resource APIs</strong>
    <span>Implement create, list, read, update, and delete with one client-facing convention.</span>
  </a>
  <a class="doc-path-card" href="./guides/plugin-conformance">
    <strong>Author a plugin</strong>
    <span>Package a statically linked plugin and exercise the public conformance boundary.</span>
  </a>
  <a class="doc-path-card" href="./guides/deployment">
    <strong>Operate on AWS</strong>
    <span>Review Plan IR, residual cost, exact artifacts, and mutation evidence.</span>
  </a>
  <a class="doc-path-card" href="./features/">
    <strong>Browse all features</strong>
    <span>See the framework surface by contract, data, runtime, deployment, and evidence plane.</span>
  </a>
  <a class="doc-path-card" href="./plugins/">
    <strong>Choose built-in plugins</strong>
    <span>Compare all 16 plugins, adapters, and runtimes by purpose, stability, and cost behavior.</span>
  </a>
  <a class="doc-path-card" href="./cookbook/">
    <strong>Follow practical recipes</strong>
    <span>Combine the pieces for CRUD, background work, feedback, files, and safe AWS delivery.</span>
  </a>
</div>

## What Is Documented Here

| Area | Current development boundary |
|---|---|
| HTTP resources | JSON data envelopes, bounded cursor pages, strong entity tags, and Problem Details |
| Plugins | Archive-visible distribution metadata and public offline conformance reports |
| Testing | Domain, application, adapter, HTTP, plugin, deployment, and release evidence remain distinct |
| AWS | Zero provisioned application compute is enforced structurally; residual managed-service cost stays visible |

## The Golden Path

```text
new -> contract -> generate -> dev -> migrate -> seed -> test -> inspect
    -> package -> change set -> migrate target -> deploy -> verify
    -> promote exact artifact -> observe or compatibility-checked rollback
```

Each arrow is explicit and inspectable. Minco does not add a runtime service
locator, Active Record layer, hosted control plane, hidden scheduler, or
boot-time production migrations.

## Popular Topics

- [Install Minco](./getting-started/installation) and
  [build the first application](./getting-started/first-application).
- Use [typed configuration](./guides/configuration),
  [database lifecycle controls](./guides/database-lifecycle), and
  [graph-driven local development](./guides/local-development).
- Add [identity and sessions](./guides/identity-and-sessions),
  [events and notifications](./guides/events-and-notifications),
  [files and static sites](./guides/files-and-static-sites), or the
  [client feedback loop](./guides/feedback).
- Review the [complete feature catalog](./features/),
  [built-in component catalog](./plugins/), and
  [Cargo feature reference](./reference/feature-flags).

Start with the [framework tour](./getting-started/framework-tour), browse the
[exercised examples](./examples/), or go directly to the
[current CLI reference](./reference/cli).
