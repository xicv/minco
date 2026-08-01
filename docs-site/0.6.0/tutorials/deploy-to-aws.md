---
title: Deploy to AWS
description: Move one exact Minco 0.6.0 artifact through plan, review, apply, verify, and promotion.
minco_version: 0.6.0
rust_version: 1.97.1
---

# Deploy to AWS

This tutorial explains the guarded deployment journey. The local steps are safe
to run without AWS credentials. Provider mutations require a reviewed account,
Region, environment, exact digests, and separate authorization.

## 1. Inspect the plan locally

```bash
cargo minco config check --environment staging
cargo minco deploy plan --config infra/aws/staging.toml
cargo minco cost --config infra/aws/staging.toml
cargo minco perf --config infra/aws/staging.toml
```

Review every wake source, database profile, IAM intent, retained resource, and
cost confidence. “Zero idle” means zero provisioned application compute—not a
promise of a zero AWS bill.

## 2. Build once

```bash
cargo minco package --environment staging
cargo minco release create --artifact target/minco/orders-lambda.zip
```

The release manifest binds source, contract, plan, configuration, migrations,
seeds, lockfile, toolchain, and artifact digests. Promotion later uses this
exact artifact; it never rebuilds source.

## 3. Review the mutation boundary

```text
plan → unexecuted change set → explicit migration → apply exact change set
     → hosted verification → promote exact artifact
```

Each arrow has its own receipt and digest approval. A dry run is evidence for
review, not authorization to mutate AWS.

## 4. Apply only with explicit authority

When the exact target and digests have been reviewed, follow the complete
[deployment lifecycle reference](https://github.com/xicv/minco/blob/v0.6.0/docs/deployment/dev-to-deploy.md).
Do not copy historical account, role, stack, database, or approval values into
a new environment.

## 5. Verify before promotion

Hosted verification must bind the observed endpoint and provider artifact to
the same immutable release. Promotion modifies only the guarded live alias
routing boundary after contract, readiness, authentication, smoke, and artifact
identity checks pass.
