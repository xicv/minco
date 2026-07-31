---
title: Contract-to-cloud architecture
description: How Minco connects contract, code, capabilities, resources, and evidence.
---

# Contract-to-cloud architecture

Minco uses one application graph viewed through five planes:

```text
Contract
  ↓
Code
  ↓
Capabilities
  ↓
Resources
  ↓
Evidence
```

## Contract

OpenAPI 3.1 is the reviewed external HTTP source of truth. Operations, schemas,
security, examples, success responses, and Problem responses exist before
handler implementation.

## Code

Business logic remains ordinary Rust. Dependencies point inward:
`delivery → application → domain`. Application ports are use-case-shaped.
Adapters implement those ports; HTTP handlers and the composition root remain
thin.

## Capabilities

Plugins are statically linked, typed, and explicitly selected. This avoids
runtime discovery, hidden service locators, and boot-time network behavior.

## Resources

Plan IR projects selected capabilities into functions, triggers, queues,
databases, IAM, performance, and cost. The composition root is the only place
that chooses concrete adapters and runtimes.

## Evidence

Tests, source identity, immutable artifacts, migrations, deployment receipts,
hosted observations, and promotion digests show what happened without relying
on a hosted Minco control plane.

The result is deliberately deeper than a broad general-purpose framework: one
AWS-native path from reviewed contract to inspectable operation.
