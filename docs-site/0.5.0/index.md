---
title: Minco 0.5.0
description: Start building low-idle-cost Rust web applications with Minco.
---

# Minco 0.5.0

<div class="version-banner">
  <span><strong>Latest stable release.</strong> These pages target Minco 0.5.0 and Rust 1.97.1.</span>
  <a href="../versions">View all versions</a>
</div>

Minco is a contract-to-cloud framework for building, operating, and evolving
low-idle-cost Rust web applications through one inspectable application graph.
It gives you a deliberately narrow path:

```text
OpenAPI → ordinary Rust → static capabilities → AWS resources → evidence
```

The minimal AWS profile uses a native ARM64 Lambda and API Gateway HTTP API. It
has no NAT Gateway, provisioned concurrency, scheduled poller, or always-on
application compute.

## Choose a path

- New to Minco? [Build your first API](./tutorials/first-api).
- Adding standard CRUD operations? [Build a resource API](./how-to/resource-api).
- Preparing infrastructure? [Plan a deployment](./how-to/plan-deployment).
- Evaluating the architecture? Read [Contract to cloud](./explanation/architecture)
  and [Zero idle, precisely](./explanation/zero-idle).

## What Minco standardizes

Minco standardizes contract policy, application structure, static plugin
composition, HTTP conventions, deployment plans, and evidence. It does not hide
your business rules behind an ORM, Active Record model, runtime container, or
hosted control plane.

The source of truth remains inspectable:

```bash
cargo minco inspect --json
cargo minco explain <operationId> --json
cargo minco deploy plan --stdout --json
```

## Release evidence

The coordinated 28-crate family is published from immutable tag `v0.5.0`.
Registry ownership, archive checksums, external consumers, the CLI install, and
all exact docs.rs routes were independently verified. This is publication
evidence—not proof that a particular application is deployed or healthy.
