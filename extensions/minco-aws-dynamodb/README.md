# minco-aws-dynamodb

Validated AWS SDK client, table configuration, readiness, resource intent, and
redacted provider errors for application-owned DynamoDB access models. This
crate intentionally does not expose a generic CRUD repository.

## Durable audit ledger

`audit_v2::DynamoDbAuditLedger` implements Minco's AWS-default audit profile on
a distinct table with string `pk`/`sk` keys. Each semantic action stores one
canonical immutable item and one projection per unique direct or explicitly
related resource. Resource identities are length-prefixed and SHA-256 hashed in
keys; the validated V2 record remains the self-contained query payload.

`transact_items` returns bounded conditional puts that an application-owned
access model combines with its source mutation in one `TransactWriteItems`
request. Generic `append_batch` uses strongly consistent canonical lookups to
classify same-ID/same-content retries and rejects conflicting content. Provider
errors and table names are redacted.

The table uses no scan or default GSI. Resource history is a strongly consistent
partition query with opaque `(occurred_at, event_id)` cursors. Health uses
`DescribeTable` size and item-count estimates; AWS updates these estimates only
periodically, so they are a cost/capacity signal rather than an exact write
counter.

DynamoDB is the default audit ledger only for the AWS profile, especially when
the operational mutation is already in DynamoDB in the same account and Region.
SQL profiles retain their transactional source journal and physically separate
SQL ledger. Retention, PITR export and archive deletion remain explicit release
operations—there is no hidden TTL, Stream, Lambda relay or scheduler.
