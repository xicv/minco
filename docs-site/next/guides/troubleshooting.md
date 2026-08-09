---
title: Troubleshoot a Minco Application
description: Diagnose common contract, local development, database, plugin, agent, packaging, and AWS planning failures from the nearest authoritative boundary.
---

# Troubleshoot a Minco Application

Minco failures are easiest to diagnose when you resist fixing the symptom at a
higher layer. Start at the nearest authoritative boundary, preserve the exact
error or diagnostic code, and only move outward after that layer is understood.

A useful first pass is:

```bash
cargo minco doctor
cargo minco contract check
cargo minco inspect --json
cargo minco check --with-cargo
```

For one operation, narrow the graph instead of reading the whole repository:

```bash
cargo minco explain <operationId> --json
```

## Contract or generated bindings are out of sync

**Symptoms**

- an OpenAPI operation is missing from generated bindings;
- a handler compiles against a shape that no longer matches the contract;
- `contract sync --check` reports drift.

**Check**

```bash
cargo minco contract check
cargo minco contract sync --check
```

Treat OpenAPI as canonical. Do not patch generated code by hand to make a test
pass. Update the reviewed contract, regenerate deterministically, then inspect
the resulting diff.

## `explain` cannot resolve an operation

**Symptoms**

- an operation has no handler/use-case link;
- multiple candidates appear plausible;
- evidence or adapter links are missing.

**Check**

```bash
cargo minco explain <operationId> --json
cargo minco inspect --json
```

Repair the project metadata or explicit operation mapping. Minco should not
infer a production path from naming similarity when the graph is incomplete.

## Local development starts but readiness fails

Liveness and readiness answer different questions. A live process can still be
unready because a selected dependency cannot serve traffic.

Check the process and then the dependency boundary:

```bash
curl --fail --silent http://127.0.0.1:3000/health/live
curl --fail --silent http://127.0.0.1:3000/health/ready
```

Then review the profile before starting anything else:

```bash
cargo minco dev --profile sqlite --dry-run --json
cargo minco dev --profile postgres --dry-run --json
```

Confirm the expected process, port, lifecycle stages, readiness probe, database
profile, and explicitly required services. A dry run is the safest place to
catch a wrong profile or hidden assumption.

## Rustack or a local port will not start

Preview the selected topology before changing containers or stored data:

```bash
cargo minco dev --dry-run --json
```

Compare the emitted service and process ports with listeners owned by other
workspaces. If the topology is correct but a port is occupied, choose explicit
workspace-local overrides:

```bash
cargo minco dev --port 31000 --rustack-port 4567
```

Also confirm that Docker is available for the selected PostgreSQL or Rustack
profile and that its declared services reached readiness. Ctrl-C stops the
supervised processes and selected containers; it does not reset volumes. Do
not delete local data merely to hide a port-ownership or readiness failure.

## Migration or seed execution is refused

Recreate the source plan and compare its digest with the approval you reviewed:

```bash
cargo minco db plan --set orders-postgres --json
cargo minco db migrate \
  --set orders-postgres \
  --database-url-env MINCO_DATABASE_URL \
  --expected-plan-digest REVIEWED_DIGEST \
  --receipt target/minco/orders-postgres-migration-receipt.json \
  --json
```

A changed SQL file or sidecar produces a new digest. A missing connection
variable, stale digest, incompatible history, reused receipt destination, or
destructive plan without explicit allowance should stop before mutation. Seed
runs add profile and environment allowlists and derive their approval digest
from the matching `--dry-run`. See [migrations and seeders](./database-lifecycle)
for the complete commands; do not bypass the refusal with an unrelated flag.

## PostgreSQL connections are exhausted or bursty

Do not treat a larger connection pool as the default fix for Lambda workloads.
Inspect:

- maximum Lambda or worker concurrency;
- pool size per execution environment;
- database/provider connection limit;
- whether a proxy is selected;
- transaction duration;
- worker batch and concurrency settings.

Then compare those assumptions with the plan and cost policy:

```bash
cargo minco deploy plan --json
cargo minco cost --json
```

The framework can make connection pressure visible; it cannot make an
oversubscribed database safe.

## SQLite behaves differently from the production adapter

SQLite is useful for the smallest local loop and real-engine tests, but it is
not proof of PostgreSQL- or DynamoDB-specific behavior. If the failure concerns
locking, SQL semantics, indexing, conditional writes, consistency, or provider
limits, exercise the actual selected adapter against its real engine/provider
boundary.

Use the [database lifecycle guide](./database-lifecycle) and
[DynamoDB guide](./dynamodb) to identify the correct evidence level.

## An idempotent create returns a conflict

A reused idempotency key is only replayable when the canonical request
fingerprint is the same. A changed payload with the same key should conflict.

Check:

1. the key value sent by the client;
2. the canonical request body and relevant operation identity;
3. idempotency retention relative to the retry window;
4. whether the storage claim and business mutation are atomic for the selected
   application design.

Do not convert a fingerprint conflict into a silent second create.

## Browser login or session requests fail

First identify which boundary rejected the request: provider authentication,
verified-claim mapping, application authorization, session lookup, or browser
transport. Minco does not supply the provider login UI.

Check the configured issuer and audience, claim-to-principal mapping, required
permission, session expiry/revocation, and cookie transport policy. For browser
failures, also compare the exact allowed origin, method, request headers,
exposed headers, credentials mode, and cookie `Secure`, `SameSite`, domain, and
path settings. Do not loosen CORS to a wildcard or enable development identity
headers in production to make the request pass. Local fake claims and a
successful browser preflight are not provider-authentication evidence.

## Update or delete returns `428` or `412`

- `428` means the required precondition is missing.
- `412` means the supplied strong ETag is stale or otherwise does not match the
  current representation.

Read the resource again, obtain the current `ETag`, and decide at the client or
business layer whether to retry or surface a concurrency conflict. Do not turn
conditional mutation into an unconditional last-write-wins path.

## A plugin is listed but does not behave as enabled

Catalog metadata describes capabilities; it does not enable executable code.
Check both distribution metadata and explicit composition:

```bash
cargo minco plugin list --json
cargo minco plugin validate --json
cargo minco plugin test --all --json
```

Then verify the Cargo feature/dependency and typed constructor registration in
the composition root. Passing offline conformance is not equivalent to provider
or production proof.

## Worker messages retry or reach the DLQ

Inspect the declared worker, queue, event-source mapping, retry and DLQ policy,
visibility timeout, batch size, concurrency, and database connection budget:

```bash
cargo minco inspect --json
cargo minco deploy plan --json
cargo minco perf --json
```

Confirm that `ReportBatchItemFailures` is enabled and that the worker returns
only failed message identifiers; otherwise successful records can be retried.
Queue visibility must exceed the function timeout with a reviewed margin, and
the maximum-receive policy determines when repeated failures reach the DLQ.
Use redacted message IDs and request/correlation IDs to separate a poison
message from a transient adapter failure. The application use case must still
be idempotent because delivery is at least once. Replaying or redriving a DLQ
is an explicit operational mutation, not a diagnostic step.

## Codex or Claude project skills are stale

Agent projections are version-matched and digest-bound. Plan first, synchronize
the exact plan, then diagnose:

```bash
cargo minco agent plan --target all --json
cargo minco agent sync --target all --expect-plan-digest <sha256> --json
cargo minco agent doctor --target all --json
cargo minco agent context --operation <operationId> --json
```

Do not hand-edit generated projections to bypass a digest mismatch. User-owned
instructions and client configuration are separate from Minco-generated files.

## AWS plan contains an unexpected resource or wake source

Stop before mutation. Inspect the selected runtime, plugins, deployment profile,
queue mappings, schedules, and static-site intent:

```bash
cargo minco inspect --json
cargo minco deploy plan --json
cargo minco cost --json
```

A minimal profile should make fixed compute, NAT Gateway, provisioned
concurrency, scheduled wakeups, connection pressure, and retained resources
visible. If the plan is surprising, the plan is doing its job: fix the input or
composition before creating a provider change set.

## Packaging succeeds but release verification fails

Treat the manifest mismatch as an identity problem, not a nuisance to suppress.
Rebuild from the intended clean source and verify the exact produced manifest:

```bash
cargo minco package
cargo minco release verify target/minco/release.json
```

Do not edit digests manually and do not rebuild during promotion. Exact
artifact reuse is part of the release contract.

## Hosted verification fails after local success

Local success proves a different boundary. Compare:

- exact source and artifact digest;
- account, Region, and environment identity;
- provider change set actually applied;
- target migration receipt;
- selected secret/config references;
- public ingress, IAM, CORS, DNS, and network behavior;
- readiness and representative business requests;
- logs/metrics using the same request ID or correlation identity.

A provider-backed failure should not be relabelled as a passing local test.

## When you still cannot isolate the problem

Collect the smallest useful evidence set:

```text
exact command
exact Minco and Rust versions
operationId or task ID
stable diagnostic/error code
redacted relevant JSON output
selected local/deployment profile
source and release digest when applicable
what evidence state has actually passed
```

Never include secret values, bearer tokens, signed URLs, provider credentials,
or unredacted customer data in an issue or agent prompt.

Next: [documentation map](../reference/documentation-map),
[testing and evidence](../reference/testing), or
[develop with coding agents](./agent-development).
