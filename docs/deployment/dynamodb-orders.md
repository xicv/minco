# DynamoDB Orders profile

The Orders DynamoDB profile is an access-pattern-specific implementation of
the same five application ports used by the SQLx and memory profiles. It is not
a generic repository and does not translate relational queries into DynamoDB.
The domain and application crates remain provider-free; the Orders adapter owns
the item and index model, while `minco-aws-dynamodb` owns validated AWS SDK
configuration and client construction.

## Selectable boundary

Enable `dynamodb` on `orders-service` or `orders-adapters` and set:

```text
DATABASE_KIND=dynamodb
DYNAMODB_TABLE_NAME=<injected table reference>
AUDIT_DYNAMODB_TABLE_NAME=<injected distinct audit table reference>
AWS_REGION=<selected deployment Region>
```

`DYNAMODB_ENDPOINT_URL` is optional. Remote overrides must use HTTPS. Plain
HTTP is accepted only for a loopback endpoint so the exact standard AWS SDK
client can be used against a disposable local emulator. Configuration and
provider errors redact the table and endpoint; credentials remain in the SDK
credential chain and are never part of Minco configuration.

## Item and access model

The table has `pk` and `sk` string keys. It stores two durable item kinds:

| Item | Key | Purpose |
| --- | --- | --- |
| Canonical order | `ORDER#<uuid>` / `ORDER` | Strong direct reads, revision-conditional update, and soft delete |
| Idempotency response | `IDEMPOTENCY#<sha256>` / `IDEMPOTENCY` | Immutable request fingerprint and original response snapshot |

`placeOrder` uses one `TransactWriteItems` call with conditional puts for the
two source items plus an immutable canonical audit event and its order-history
projection in the distinct audit table. `updateOrder` and `deleteOrder` use the
same cross-table transaction boundary. A revision-condition race therefore
commits neither a false audit event nor a partial source mutation. The raw
idempotency key is hashed before persistence and before deriving
the SDK client request token. Concurrent requests with the same key and
fingerprint commit one order and replay the immutable response; a different
fingerprint returns the stable application conflict and a replay creates no
second semantic action. The snapshot and audit history are retained after later
order update or deletion.

`getOrder` uses a strongly consistent table `GetItem`. `updateOrder` and
`deleteOrder` use a revision condition; the adapter then performs a strong read
to distinguish `PreconditionFailed` from `NotFound`. Delete is soft: the
canonical item and idempotency response remain, while all list-index attributes
are removed so the order disappears from query results.

Malformed stored items fail closed. Provider errors expose only stable
operation-level messages; they do not include item bodies, credentials,
endpoints, table names, or SDK response bodies.

## Bounded list queries

The adapter assigns each order deterministically to one of 16 calculated
shards. It queries at most eight shards concurrently and at most 128 DynamoDB
pages per shard. It never calls `Scan`. Three projected-`ALL` global secondary
indexes preserve every allowlisted Orders sort:

| Leading sort | Secondary direction | Index |
| --- | --- | --- |
| `createdAt` | same as `createdAt`, or absent | `orders-by-created-at` |
| `createdAt` | opposite to `createdAt` | `orders-by-created-at-inverted-id` |
| `id` | either `createdAt` direction, or absent | `orders-by-id` |

Each shard query applies the opaque cursor as an exclusive sort-key bound. The
adapter merges the bounded shard results using the application comparator and
returns the last visible order as the next cursor. Status filtering is explicit
and soft-deleted items have no index keys.

Global secondary index reads are eventually consistent by DynamoDB contract.
That means an accepted write can briefly be absent from `listOrders`, while a
direct `getOrder` remains strongly consistent. This profile is suitable only
when that list-read behavior is acceptable.

## Plan, SAM, IAM, and cost

[`minco.dynamodb.toml`](../../examples/orders/config/minco.dynamodb.toml)
declares both exact table key contracts, the three operational indexes,
point-in-time recovery, deletion protection, retention policy, current Sydney
rates, and consuming function. A DynamoDB cost-only plan without
that table contract still fails SAM rendering closed.

The explicit contract renders:

- two on-demand, server-side-encrypted DynamoDB tables with distinct names;
- point-in-time recovery, deletion protection and `Retain` replacement policy;
- `DATABASE_KIND`, `DYNAMODB_TABLE_NAME` and
  `AUDIT_DYNAMODB_TABLE_NAME` CloudFormation references;
- exact operational table and `/index/*` IAM for `DescribeTable`, `GetItem`,
  `Query`, `TransactWriteItems`, and `UpdateItem`; and
- separate audit-table IAM for `BatchGetItem`, `DescribeTable`, `Query`, and
  `TransactWriteItems`—never `dynamodb:*`, the unused `PutItem`, or
  `Resource: "*"`.

Cost output keeps request volume, transaction write amplification, three GSI
projections, audit canonical/projection fan-out, storage, PITR, and retained
table residual cost visible. The audit table has no practical table-size ceiling
but is not literally infinite or free; hot-query horizon, export/archive policy,
service quotas, throttling, legal retention and cost remain explicit operations.

## Local conformance and cleanup

Run:

```bash
scripts/dev/rustack-dynamodb-smoke.sh
```

The script starts the repository-pinned Rustack image with only DynamoDB and
STS, chooses a unique compose project, host port, and two tables, creates the
exact three-index Orders table plus the audit table, and exercises the Orders
ports and semantic history. It uses test-only credentials, deletes both tables,
polls until each is absent, and removes the
container and network through an exit trap. A failing test still runs cleanup.

This is emulator conformance, not AWS delivery proof. Rustack is pinned to
revision `ab8bc61a3e45058c7d42de8443f9d215cc110b18`; its upstream source is
reviewable at <https://github.com/tyrchen/rustack/tree/ab8bc61a3e45058c7d42de8443f9d215cc110b18>.
Any real-account table creation needs a new approval naming the exact account,
Region, resource prefix, duration, spend bound, and cleanup/absence procedure.
