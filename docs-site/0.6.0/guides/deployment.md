---
title: Plan an AWS Deployment
description: Move from a local graph to an exact reviewable artifact without collapsing plan, apply, verification, and promotion.
---

# Plan an AWS Deployment

The minimal AWS profile targets native ARM64 Lambda behind API Gateway HTTP
API. Plan and cost inspection are local; provider mutations require a separate
reviewed target and authorization.

## 1. Validate the Application Graph

```bash
cargo minco config check --environment staging
cargo minco inspect --json
cargo minco deploy plan --config infra/aws/staging.toml --stdout --json
cargo minco cost --config infra/aws/staging.toml --json
cargo minco perf --config infra/aws/staging.toml --json
```

Review every function, trigger, queue, schedule, database, retained resource,
IAM action, connection budget, wake source, and pricing-confidence field.

## 2. Confirm the Minimal-Idle Policy

The structural gate rejects these from the minimal profile:

- NAT Gateway;
- fixed application compute;
- undeclared or unbounded schedules;
- provisioned concurrency;
- hidden pollers or background work.

Managed services can still retain data or charge per request. Read
[Zero Idle, Precisely](../explanation/zero-idle) before describing cost to a
client.

## 3. Build Once

```bash
cargo minco package --environment staging
cargo minco release verify target/minco/release.json
```

The manifest binds source, contract, configuration, Plan IR, migrations,
seeds, lockfile, toolchain, and artifact digests. Later stages must consume that
artifact; they must not rebuild source.

`package` builds the configured artifacts and seals the release. Use the lower-
level `release create --artifact PATH` command only when a reviewed external
build already produced one function artifact plus the exact Plan and template;
it is an alternative sealing path, not a second step after `package`.

## 4. Keep Mutation Stages Separate

```text
local plan
  → unexecuted CloudFormation change set
  → explicit migration receipt
  → exact change-set apply
  → candidate hosted verification
  → routing-only promotion
  → separate production observation
```

Each arrow requires current identity, target, drift, digest, and terminal
receipt checks. A dry run is review evidence, not authorization.

## 5. Verify What Actually Ran

Hosted verification binds request IDs, status codes, readiness,
authentication, smoke results, candidate version, and provider artifact digest
to the release manifest. Promotion can change only the guarded live routing
boundary.

| Evidence | Proves | Does not prove |
|---|---|---|
| Local quality | source behavior and structural plans | hosted runtime |
| Apply receipt | reviewed infrastructure mutation completed | candidate acceptance |
| Hosted verification | exact candidate responded as required | live production traffic |
| Promotion receipt | exact routing-only change completed | ongoing production health |
| Production observation | bounded live behavior at one time | future release correctness |

Never copy an account, role, stack, database URL, or approval digest from a
historical example into a new environment.
