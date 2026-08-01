---
title: Minco 0.6.0
description: Stable documentation for Minco 0.6.0, the AWS-native Rust framework for low-idle-cost web applications.
---

# Minco 0.6.0

Versioned documentation for the `0.6.0` release line. The site banner records
whether the exact package family is still a candidate or is published.

Minco turns a reviewed OpenAPI contract into layered Rust, deterministic
application and plugin graphs, explicit AWS infrastructure, and exact release
evidence. Version 0.6.0 adds archive-visible plugin distribution metadata, one
public offline conformance kit, and the detailed documentation you are reading.

## Choose a Path

<div class="doc-path-grid">
  <a class="doc-path-card" href="./tutorials/first-api">
    <strong>Build your first API</strong>
    <span>Generate, run, and test a contract-first application locally.</span>
  </a>
  <a class="doc-path-card" href="./guides/resource-api">
    <strong>Standardize CRUD</strong>
    <span>Use envelopes, cursor pages, ETags, conditional writes, and Problem Details.</span>
  </a>
  <a class="doc-path-card" href="./guides/plugin-conformance">
    <strong>Author a plugin</strong>
    <span>Ship inspectable static metadata and run the public conformance boundary.</span>
  </a>
  <a class="doc-path-card" href="./guides/deployment">
    <strong>Operate on AWS</strong>
    <span>Review Plan IR, residual cost, exact artifacts, and mutation evidence.</span>
  </a>
</div>

## What 0.6.0 Standardizes

| Surface | Stable contract |
|---|---|
| Resource APIs | OpenAPI-first five-action families, data/page envelopes, bounded opaque cursors, strong ETags and RFC 9457 problems |
| Plugins | Static Cargo composition plus strict `minco-plugin.json` records in published archives |
| Conformance | One public `minco-test` report API for official and third-party-style packages |
| AWS | Zero provisioned application compute in the minimal profile, with residual managed-service cost kept explicit |
| Evidence | Local, hosted, package, provider, deployment and production claims remain separate |

Start with [installation](./installation), take the [framework tour](./getting-started/framework-tour),
or browse the [exercised examples](./examples/).
