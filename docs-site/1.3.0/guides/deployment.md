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

## 6. Create Review Environments with an Exit

Minco applies the useful preview-environment idea without adding a hosted Minco
control plane or an implicit cleanup worker:

```bash
cargo minco --json deploy plan --environment preview --stdout
cargo minco --json deploy review --environment preview --dry-run
cargo minco --json destroy --environment preview --dry-run
```

A preview plan names its owner, TTL, expected account and Region, exact source
and release, Feedback linkage, resources, retention, wake sources, incomplete
pricing, and cleanup policy. Review creation is a separately authorized,
read-only provider inspection after exact deployment and hosted verification.
Destroy requires the resulting review digest as a new approval, rechecks the
stack and retention boundary, uses standard CloudFormation deletion, and
records success only after absence is observed.

Expiry is visible metadata, not deletion. An optional one-time external
schedule is explicit cost and wake behavior; manual cleanup remains the safe
default. Production and persistent staging targets cannot use preview destroy.

## 7. Publish Static Bytes as Part of the Exact Release

When `static-site` is enabled, packaging binds every normalized asset path,
size, media type, cache policy, and SHA-256 digest. Publication stays separate:

```bash
cargo minco deploy static-site plan
cargo minco deploy static-site apply --approve-release-digest RELEASE_DIGEST
cargo minco deploy verify --static-site
```

The AWS plan uses a private encrypted S3 bucket, CloudFront Origin Access
Control, explicit cache policy, and optional pre-existing certificate/hosted
zone inputs. Apply serializes publishers with a conditional lock, verifies
every uploaded checksum and metadata value before deleting stale objects,
waits for invalidation completion, and writes an immutable receipt. A failed
publication leaves its lock for explicit reviewed recovery rather than
guessing that provider state is safe.

## 8. Roll Back by Compatibility, Not by Tag Name

```bash
cargo minco rollback --dry-run --json
cargo minco promote --canary --dry-run --json
```

Rollback compares the complete receipt chains of the current and target
releases across contract, configuration, resources, migrations, seeds,
persisted-data review, API routing, and worker artifacts. A compatible result
still performs no mutation: redeploy the exact older artifact as a new
candidate without rebuilding, repeat hosted verification, then use ordinary
promotion.

Canary is opt-in and API-only. It requires one to five reviewed CloudWatch
metric alarms, a bounded monitoring window, the exact hosted candidate, and a
routing-only alias change set. Workers remain unchanged. Minco restores the
prior unweighted route before completing ordinary full promotion, and it adds
no provisioned concurrency or fixed compute.
