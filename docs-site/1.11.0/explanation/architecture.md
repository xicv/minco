---
title: Understand Minco's Architecture
description: See how contracts, application code, adapters, runtimes, deployment plans, and evidence remain connected without hiding the underlying Rust or AWS model.
---

# Understand Minco's Architecture

Minco is a **contract-to-cloud** framework. Its purpose is not to replace Rust,
Axum, SQLx, DynamoDB, Lambda, or AWS with a proprietary runtime. It keeps those
parts explicit while giving them one inspectable application graph and one
repeatable delivery path.

The shortest useful mental model is:

```text
contract -> application -> adapters -> runtimes -> plan -> release evidence
```

Each stage has a different responsibility. Crossing those boundaries casually
is what Minco tries to prevent.

## The five project planes

`cargo minco inspect --json` projects the application across five connected
planes:

| Plane | Owns | Typical questions |
|---|---|---|
| Contract | OpenAPI operations, schemas, security, examples, Problems | What can a client send and receive? |
| Code | handlers, use cases, ports, adapters, composition | Which code implements this operation? |
| Capabilities | plugins and selected framework features | Which optional behavior is compiled into this app? |
| Resources | runtimes, queues, databases, storage, IAM, wake/cost intent | What must exist to run it? |
| Evidence | tests, digests, plans, provider receipts, release identity | What proves the claim we are making? |

The graph is useful to humans and coding agents because a question such as
"what implements `placeOrder`?" does not require guessing from filenames.
Use:

```bash
cargo minco explain placeOrder --json
```

Missing or ambiguous links should fail rather than silently selecting a
plausible implementation.

## Dependency direction

Application structure follows one simple direction:

```text
delivery -> application -> domain
                 ^
                 |
              adapters
```

- **Domain** contains invariants and domain values. It does not depend on Axum,
  SQLx, Lambda, AWS SDKs, or Minco deployment internals.
- **Application** contains use cases and owns the ports those use cases need.
  Authorization and transaction policy belong here when they are business
  rules.
- **Adapters** implement application-owned ports for PostgreSQL, SQLite,
  DynamoDB, S3, providers, clocks, queues, or other infrastructure.
- **Delivery** maps HTTP or worker input to one application use case and maps
  the result back out. Handlers should not contain SQL or business policy.
- **Composition** chooses concrete adapters and runtimes explicitly.

This is why Minco does not add Active Record, a generic CRUD repository, a
global service locator, runtime plugin discovery, or boot-time production
migrations.

## One application, explicit runtimes

The same application boundary can be composed for different execution models:

| Runtime | Wake source | Appropriate use |
|---|---|---|
| Local service | developer process | fast local development and integration testing |
| Lambda HTTP | API Gateway request | request-driven APIs with no provisioned application compute |
| SQS Lambda worker | queue message | bounded asynchronous work with retry/DLQ policy |
| Static delivery | asset request | separately published frontend assets through S3/CloudFront |

A runtime is not a second application architecture. The application use case
and contract stay authoritative; runtime-specific code remains at the edge.

## Plugins are static capabilities

Minco plugins are ordinary Rust crates with typed constructors and explicit
Cargo features. Their metadata can be inspected before linking, but metadata
never downloads or executes code.

That gives the framework an extension model without introducing a hidden
runtime container. Start with the [built-in catalog](../plugins/) and
[plugin composition guide](../plugins/using-plugins).

## Plan before mutation

Infrastructure intent is represented before AWS is changed. A plan can expose:

- selected functions, queues, tables, buckets, distributions, and roles;
- ingress and trigger relationships;
- IAM intent;
- reserved and worker concurrency;
- database connection pressure;
- wake sources;
- zero-compute, storage-only, usage-based, and fixed cost classes;
- target account, Region, environment, and deployment guards.

Use:

```bash
cargo minco deploy plan --json
cargo minco cost --json
```

The plan is evidence about intended topology. It is not proof that AWS was
mutated successfully. Provider-backed deployment and hosted verification remain
separate evidence states.

## Build once, then prove what moved

Minco treats source, package, deployment, and promotion identity as connected
claims:

```text
source -> tests -> package -> release manifest -> change set -> hosted verify
                                                        |
                                                        v
                                              promote exact artifact
```

Promotion must reuse the exact verified artifact rather than rebuilding source.
Rollback is also compatibility-checked rather than treated as "deploy an old
commit".

This distinction matters for coding-agent workflows too: inspection and local
plans can be read-only and deterministic without granting an agent production
credentials or mutation authority.

## Why the architecture is intentionally narrow

Minco is optimized for one problem: Rust web applications that benefit from a
deep AWS-native path and low idle application-compute cost. Narrowness allows
the framework to make stronger assumptions about planning, evidence, wake
behavior, and deployment than a provider-neutral general-purpose framework can.

It does **not** mean every AWS service is automatically supported. A public
type, enum variant, or adapter seam is not the same thing as a qualified
production profile. Supported runtime and deployment claims must have matching
implementation, cost, security, recovery, and provider evidence.

## Continue by intent

- Learn the working loop in the [framework tour](../getting-started/framework-tour).
- See the filesystem boundaries in [project structure](../getting-started/project-structure).
- Follow a complete system decision in the [production blueprint](../cookbook/production-blueprint).
- Understand the cost constraint in [Zero idle, precisely](./zero-idle).
- Review AWS mutation boundaries in [Plan an AWS deployment](../guides/deployment).
- See how claims are separated in [Testing and evidence](../reference/testing).
