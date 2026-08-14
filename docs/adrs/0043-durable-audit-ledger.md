# ADR 0043: Use semantic actions and a separate durable audit ledger

## Status

Accepted.

## Context

Minco's beta audit plugin exposes a single append-only event and SQL adapters
currently persist it inside the operational plugin schema. That is useful for
bounded security events but cannot be the long-term action-history contract:
audit data is append-only, commonly outlives source rows and actors, may contain
more sensitive data than the source, and grows without a natural upper bound.

Minco has no ORM lifecycle to observe. Database hooks can describe a row change,
but do not reliably retain the authenticated actor, authorization decision,
OpenAPI operation, idempotency identity or business reason. PostgreSQL triggers
and logical decoding, SQLite triggers and sessions, and DynamoDB Streams also
have different ordering, retention and failure behavior.

A second durable store introduces a dual-write problem. PostgreSQL cannot commit
to an unrelated audit database in the same local transaction. SQLite WAL does
not make a commit across two attached database files crash-atomic. DynamoDB can
atomically transact across tables in one account and Region.

## Decision

Minco auditing records semantic application actions as the authoritative source.
An action is created only after authorization and validation and is committed by
a use-case-shaped adapter together with the domain mutation.

Permanent audit history is physically separate from operational data and has no
foreign key to an operational table. Records identify a primary resource and
bounded explicit related resources by tenant scope, resource type and opaque
resource ID. An authorized application use case retrieves history through an
`AuditReader`; the domain row does not join or reach into the ledger.

PostgreSQL and SQLite use a bounded transactional intent journal in the source
store. A selected, explicit relay delivers committed intents idempotently in
batches to a separate PostgreSQL database or SQLite file and acknowledges them
only after ledger commit. The journal is coordination state, not permanent audit
history, and is pruned after acknowledgement. No relay or recovery schedule is
created implicitly.

The AWS profile uses a separate DynamoDB on-demand audit table by default. When
the source of truth is also DynamoDB in the same account and Region, the domain
mutation and canonical audit item use one `TransactWriteItems` call. This avoids
a second journal table, Lambda relay, delivery lag and duplicate-delivery state.
Provider limits still bound each application transaction; bulk operations must
be explicitly chunked and cannot claim cross-chunk atomicity.

The generated table-scoped IAM authorizes both `TransactWriteItems` and the
dependent item actions used by its transaction members. A transactional audit
`Put` therefore requires `dynamodb:PutItem` even though the adapter never calls
the standalone `PutItem` API.

DynamoDB is not the global default. Local SQLite and self-hosted PostgreSQL
profiles do not acquire an AWS dependency, credentials, network path or cloud
cost merely by selecting auditing. Explicit plugin composition and Plan IR
continue to expose the selected provider, resources, IAM, wake sources,
retention, recovery and cost assumptions.

The ledger contract is additive V2. Existing `AuditEvent`, `AuditSink`,
`AuditService`, serialized fields and `audit.append` capability remain
compatible. New batch, query and health contracts use new Rust types and
capability versions.

Every V2 record is bounded before provider contact. Changed fields are
allowlisted and values may be literal, redacted, digested or omitted. Raw
credentials, tokens and unbounded blobs are never valid defaults. Event IDs are
globally unique and append is idempotent: the same ID and content is a duplicate;
the same ID with different content is corruption and fails closed.

Storage lifecycle is provider-specific behind one inspectable policy:

- SQLite rotates sealed segments by configured bytes and optionally age;
- PostgreSQL uses time partitions and/or explicit hot-retention boundaries;
- DynamoDB uses an unbounded logical table with a bounded hot-query horizon and
  optional archive-after watermark rather than file-size rotation; and
- deletion after archive requires a durable archive receipt and explicit
  retention policy. Nothing is deleted merely because it is old.

The reader merges hot and archived/segmented sources behind a stable cursor.
Resource revision and transaction ordinal express causal order when available;
timestamps remain descriptive and are not treated as a cross-host lock.

Database-level change capture may later contribute records with an explicit
`database_evidence` origin and an unknown or database-principal actor. It is a
bypass-detection signal, not a substitute for semantic action auditing.

## Size and operational policy

A universal 50 or 100 MB limit is rejected. Those values are reasonable initial
SQLite segment targets, but PostgreSQL partitions and DynamoDB tables have
different failure and cost boundaries. Each finite-disk profile declares warn,
rotate and reject thresholds plus a minimum free-disk reserve. Health exposes
active segment bytes, total hot bytes, free bytes, pending journal count/bytes,
oldest pending age, archive watermark and failed/quarantined records.

The initial SQLite recommendation is a 100 MiB target segment with warning and
hard thresholds derived from deployment capacity, not a silent fixed default.
Queries continue across sealed segments because cursors identify record order,
not a filename or physical table.

## Consequences

- Action history remains attributable and authorization-aware without an ORM.
- Permanent growth cannot exhaust an operational SQL database or SQLite file.
- SQL profiles accept a small bounded source journal to preserve atomicity.
- DynamoDB writes are low-idle-cost and immediately durable but consume
  transactional write units and require access-pattern-specific keys.
- Related-resource gathering requires explicit index entries and therefore has
  visible write/storage amplification.
- Hot retention, archive, restore, legal hold and erasure policies remain
  application/deployment decisions rather than framework guesses.
- Later tasks must prove SQL crash recovery, DynamoDB/Rustack transaction
  behavior, Plan/SAM/IAM/cost rendering and one complete Orders slice.

## Alternatives rejected

### Audit every table through database hooks

This loses application actor and business semantics, differs materially by
database, and encourages a generic persistence abstraction forbidden by
ADR-0004 and ADR-0032.

### Write directly to a second SQL database in the request

This has an unavoidable commit gap and turns a ledger outage into ambiguous
domain state. Distributed transactions are not part of the supported profiles.

### Put all audit records in the operational database forever

This couples backup, restore, vacuum/checkpoint, disk exhaustion and retention
of unrelated workloads and contradicts the separation requirement.

### Make DynamoDB mandatory everywhere

This would hide an AWS dependency in local and self-hosted profiles and break
Minco's explicit static composition and local-first operating boundary.

## Compatibility

The provider-neutral Rust API is additive. Existing V1 readers and writers keep
their exact behavior. No existing serialized schema is extended with required
fields. Provider and Plan changes are deferred to separately versioned tasks.

## Safety

This decision authorizes no provider contact or production mutation. Provider
credentials, table contents, database URLs and secret values are excluded from
records and evidence. Audit reads and exports require their own application
authorization and must not recursively audit themselves by default.
