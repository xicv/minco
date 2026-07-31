---
title: Plan a deployment
description: Inspect AWS resources, IAM, cost, and wake sources without mutating a provider.
---

# Plan a deployment

Deployment planning is deterministic and non-contacting. It does not require
AWS credentials.

```bash
cargo minco deploy plan \
  --config infra/aws/staging.toml \
  --output target/minco/staging-plan.json

cargo minco deploy render-sam \
  --config infra/aws/staging.toml \
  --output target/minco/staging-template.yaml

cargo minco cost --config infra/aws/staging.toml
cargo minco perf --config infra/aws/staging.toml
```

## Review the plan

Confirm:

- each operation maps to the intended function and trigger;
- there is no unreviewed NAT Gateway, fixed compute, schedule, or provisioned
  concurrency;
- database correctness, wake-source, connection, and cost assumptions are
  explicit;
- IAM resources are exact and capabilities are selected deliberately;
- every artifact path, byte size, and digest is present.

## Explain one operation

```bash
cargo minco explain placeOrder --json
```

The trace connects the OpenAPI operation to code ownership, capabilities,
resources, cost, and evidence.

Planning does not authorize a change set or deployment. Use the guarded
controller only after the target, account, Region, environment, source, and
artifact digests are independently reviewed.
