# Inspect the zero-idle AWS profile

Minco's AWS default targets zero provisioned application compute: one native
ARM64 Lambda ZIP behind API Gateway HTTP API, with no NAT Gateway, provisioned
concurrency, fixed application compute, or schedule.

## Features

Enable `aws-lambda`, `aws-adapters`, and `plan`. Select database and plugin
features explicitly from the generated feature reference.

## Provider assumptions

Plan and cost commands are local and do not call AWS. Deployment later requires
an exact target, current account/role proof, reviewed change set, migrations,
artifact/manifest digests, and explicit apply approval.

## Cost and wake behavior

Lambda compute can be `zero_compute`; API/Lambda invocations are
`request_only`; database/log/static assets can be `storage_only`. HTTP requests
are wake sources. Zero provisioned compute is not a zero-bill guarantee.

```bash
cargo minco deploy plan --config examples/orders/config/minco.dev.toml --stdout --json
cargo minco cost --config examples/orders/config/minco.neon-launch.toml --json
cargo minco cost --config examples/orders/config/minco.aurora-serverless-v2.toml --json
```

Review every missing regional rate and pricing-confidence label. Keep fixed RDS,
self-hosted hosts, retained logs, DNS, secrets, storage, and schedules visible
when selected.

## Verification

The recipe runner executes `zero-idle-plan`, `cost-neon`, and `cost-aurora`.

## Unsupported gates

Planning is not permission to package, migrate, deploy, promote, roll back, or
publish. Account eligibility, quotas, Region support, current pricing, and live
runtime behavior remain separate evidence.
