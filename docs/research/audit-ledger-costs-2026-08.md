# Audit ledger provider and cost review (2026-08-13)

This note records the external facts used by ADR-0040. Prices are evidence for
the decision, not constants for runtime code. Deployment plans must continue to
accept current Region-specific rates as explicit inputs.

## DynamoDB on-demand in Asia Pacific (Sydney)

The AWS Price List API offer for `AmazonDynamoDB`, Region `ap-southeast-2`,
reported on 2026-08-13:

| Dimension | Current USD price |
|---|---:|
| Standard read request units | $0.1425 per million |
| Standard write request units | $0.71 per million |
| Standard table storage | first 25 GB-month free, then $0.285 per GB-month |
| Point-in-time recovery storage | $0.228 per GB-month |

Source: [AWS Price List API offer](https://pricing.us-east-1.amazonaws.com/offers/v1.0/aws/AmazonDynamoDB/current/ap-southeast-2/index.json).

DynamoDB bills one standard write request unit per write of an item up to 1 KiB;
transactional writes consume two write request units. Therefore one million
sub-1-KiB canonical audit items added to existing transactions contribute about
two million write units, or **$1.42**, before indexes, storage, backups, reads,
data transfer and any increased cost of making the domain mutation
transactional. See [DynamoDB read/write operation billing](https://docs.aws.amazon.com/amazondynamodb/latest/developerguide/read-write-operations.html).

`TransactWriteItems` supports up to 100 actions and 4 MB across tables in one AWS
account and Region. An audit record consumes one action, so a domain mutation
plus audit record consumes at least two actions. A related-resource projection
or other index item consumes another action and write units. See [DynamoDB
transaction APIs](https://docs.aws.amazon.com/amazondynamodb/latest/developerguide/transaction-apis.html).

This cost is attractive for a low-idle AWS action ledger. It does not make
DynamoDB free or universally preferable. Large JSON changes round up by KiB,
global secondary indexes amplify storage and writes, strong reads cost more,
PITR is separately billed, and indefinite hot retention becomes a recurring
storage bill.

## Why no AWS relay is the default

A source outbox plus Lambda relay remains valid where stores cannot share a
transaction. When both source and ledger are DynamoDB, a direct cross-table
transaction has fewer states and lower marginal cost:

- no second outbox item;
- no Lambda invocation/duration;
- no delivery acknowledgement write/delete;
- no stream-retention recovery window; and
- no duplicate-delivery path between source commit and ledger acknowledgement.

DynamoDB Streams and Lambda remain at-least-once and consumers must be
idempotent. Streams retain change records for only 24 hours. Those properties
make a relay useful for downstream archive/export, not preferable for the
canonical audit commit. See [DynamoDB Streams](https://docs.aws.amazon.com/amazondynamodb/latest/developerguide/Streams.html) and [Lambda DynamoDB event-source settings](https://docs.aws.amazon.com/lambda/latest/dg/services-ddb-params.html).

## Growth model

DynamoDB has no application-visible table-size rotation requirement. Audit
queries must use a bounded partition key and sort key, never `Scan`. The initial
canonical access pattern is resource history:

```text
PK = tenant_scope#resource_type#resource_id
SK = occurred_at#event_id
```

Related-resource history requires explicit projection/index items and a bounded
relationship count. Tenant-wide chronological search is a separate, optional
time-bucketed access pattern; it must not add a default GSI merely for possible
future analytics.

For indefinite retention, keep a bounded hot horizon only when the application
needs it and export immutable batches to object storage. DynamoDB TTL is
asynchronous and must only be enabled after an archive receipt proves the batch
is durable. Legal hold or permanent online history may deliberately retain the
table and its cost.

SQLite is different: finite host disk and checkpoint/backup time make explicit
segment rotation valuable. PostgreSQL should normally partition by time rather
than create numbered databases at an arbitrary byte threshold. One query cursor
can span these physical layouts because it is based on record position rather
than provider segment identity.
