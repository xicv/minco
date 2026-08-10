---
title: Use the DynamoDB Orders Adapter
description: Select the access-pattern-specific DynamoDB profile with conditional writes, bounded queries, exact IAM, and explicit consistency tradeoffs.
---

# Use the DynamoDB Orders Adapter

The Orders DynamoDB profile implements the same application ports as the SQLx
and memory profiles, but it owns a DynamoDB-specific item and index model. It
is not a generic repository and never emulates relational SQL semantics.

## Select the Profile

Enable the `dynamodb` feature and configure:

```text
DATABASE_KIND=dynamodb
DYNAMODB_TABLE_NAME=<injected table reference>
AWS_REGION=<selected deployment Region>
```

`DYNAMODB_ENDPOINT_URL` is optional. Remote overrides require HTTPS; plain HTTP
is accepted only for loopback emulator conformance. Credentials remain in the
standard AWS SDK chain and errors redact tables, endpoints, item bodies, and
provider responses.

## Understand the Access Model

`placeOrder` performs one conditional `TransactWriteItems` operation for the
canonical order and immutable idempotency response. Direct reads are strongly
consistent. Update and soft delete use revision conditions, while a follow-up
strong read distinguishes not-found from precondition failure.

List operations never call `Scan`. Orders are assigned to 16 deterministic
shards and queried through three projected-`ALL` indexes with bounded
concurrency and page counts. Global-secondary-index list reads are eventually
consistent, so a newly accepted write can briefly be absent from a list while
`getOrder` already returns it. Select this profile only when that behavior is
acceptable.

## Review Plan, IAM, and Residual Cost

The explicit Orders table contract renders an on-demand encrypted table, point-
in-time recovery, retained deletion/replacement policy, exact table/index IAM,
and the selected function environment. It never grants `dynamodb:*`,
`PutItem`, or `Resource: "*"`.

Cost output retains request volume, transactional write amplification, three
index projections, storage, recovery, backup, and retained-table cost.
Request cost may reach zero at zero traffic; retained storage and backups do
not become a complete zero-dollar claim.

## Run Disposable Local Conformance

```bash
scripts/dev/rustack-dynamodb-smoke.sh
```

The script creates the exact three-index table in the pinned Rustack emulator,
exercises all five Orders ports, deletes the table, proves absence, and removes
its container and network through an exit trap. This is emulator conformance,
not permission to create a real AWS table.
