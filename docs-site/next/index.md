---
title: Next
description: Unreleased Minco documentation.
---

# Next

This area records only behavior merged after Minco 0.6.0. It never changes the
meaning of the [stable 0.6.0 documentation](/0.6.0/).

No post-0.6.0 compatibility change is claimed yet. These mutable pages retain
the current framework paths as the starting point for future development.
Choose the path that matches what you need to do.

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
</div>

## What Is Documented Here

| Area | Current development boundary |
|---|---|
| HTTP resources | JSON data envelopes, bounded cursor pages, strong entity tags, and Problem Details |
| Plugins | Archive-visible distribution metadata and public offline conformance reports |
| Testing | Domain, application, adapter, HTTP, plugin, deployment, and release evidence remain distinct |
| AWS | Zero provisioned application compute is enforced structurally; residual managed-service cost stays visible |

Start with the [framework tour](./getting-started/framework-tour), browse the
[exercised examples](./examples/), or go directly to the
[current CLI reference](./reference/cli).
