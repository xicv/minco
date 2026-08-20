---
title: Zero Idle, Precisely
description: Understand which Minco resources scale to zero, which retain cost, and what local policy can and cannot prove.
---

# Zero Idle, Precisely

Minco’s core promise is zero provisioned **application compute** while an
application is idle. It is not a promise that every AWS bill reaches zero.

## Structural Minimal Profile

The default AWS plan uses native ARM64 Lambda plus API Gateway HTTP API and
rejects:

- NAT Gateway;
- fixed application compute;
- provisioned concurrency;
- undeclared schedules;
- hidden polling or background work.

Those rules are deterministic and testable before contacting AWS.

## Residual Cost Still Exists

| Resource | Idle behavior | Residual dimensions |
|---|---|---|
| Lambda | no invocation compute | logs, artifact storage, optional concurrency settings |
| API Gateway HTTP API | request-priced | custom domain and related services when selected |
| S3 | no request charge without traffic | retained bytes, versions, replication, transfer |
| SQS | no polling when no worker is invoked | requests and retained messages |
| DynamoDB on-demand | no throughput charge at zero traffic | storage, backups, streams, indexes, transfer |
| Aurora Serverless v2 at 0 ACU | compute can pause when eligible | storage, I/O, backup, logs, resume latency |
| Neon | compute can suspend | retained storage/history and provider-plan policy |
| CloudFront | request-priced profile | transfer, invalidations, logs, optional fixed commercial plan |

Every selected database profile must declare correctness, connection, wake,
cost, and evidence assumptions. A free allowance is eligibility-dependent, not
a complete zero-cost estimate.

## Feedback Loop Value

Low idle cost makes early deployment economical. That shortens the loop:

```text
contract → failing test → implementation → exact artifact → preview → feedback
    ↑                                                               ↓
    └──────────────────── reviewed requirement change ──────────────┘
```

The loop only stays safe when tests, source identity, migration policy,
deployment receipts, feedback provenance, and production observations remain
visible.

## Local Versus Live Proof

Local policy can prove the absence of forbidden resources, declared wake
sources, deterministic IAM intent, connection arithmetic, and cost confidence.
It cannot prove current AWS prices, account eligibility, actual database pause,
cold-start latency, completed cleanup, or the final bill.

Tie live claims to the exact account, Region, artifact, provider response, and
observation time. Recheck them when any of those inputs changes.
