---
title: Durable Action Auditing
description: Record semantic business actions in a separate append-only ledger with atomic writes, bounded queries, explicit retention, and visible cost.
---

# Durable Action Auditing

Minco records **semantic application actions**, not arbitrary ORM row hooks.
The application use case knows whether a change means `order.created`,
`order.updated`, or `order.deleted`; the persistence adapter guarantees that the
domain mutation cannot succeed without its audit intent being durably accepted.

This separation keeps operational logging, domain storage, and durable audit
history honest:

```text
request -> application use case -> domain mutation + audit intent
                              |-> operational store
                              `-> physically separate audit ledger
```

The V2 record is schema-agnostic. It stores stable tenant, resource and action
identity, actor, occurrence/recording time, correlation and operation IDs,
optional revision, privacy-aware changes, labels and a bounded list of related
resources. It does not require a foreign key into the operational schema.

## Preserve atomicity

Do not write the domain row and then independently attempt an audit write.
A crash between those steps loses history.

For SQLite and PostgreSQL, the source transaction writes the domain mutation
and a journal entry together. A bounded relay claims journal entries with a
lease, appends them idempotently to the separate ledger, and acknowledges only
after success. Retried or concurrent workers cannot create conflicting events.

For the Orders DynamoDB profile, the domain item, idempotency item and audit
items are written with one conditional `TransactWriteItems` operation across
the operational and audit tables. This avoids a default stream or Lambda relay.
Transaction size, action count and relationship projections remain bounded.

## Configure a distinct ledger

The Orders service fails closed if its source and audit storage are the same:

```text
# SQLite
DATABASE_KIND=sqlite
SQLITE_PATH=var/orders.db
AUDIT_SQLITE_PATH=var/orders-audit.db

# PostgreSQL
DATABASE_KIND=postgres
DATABASE_URL=<operational secret>
AUDIT_DATABASE_URL=<distinct audit secret>

# DynamoDB
DATABASE_KIND=dynamodb
DYNAMODB_TABLE_NAME=<orders table>
AUDIT_DYNAMODB_TABLE_NAME=<distinct audit table>
```

Production migrations remain explicit release operations. The SQL service does
not migrate its audit ledger during Lambda startup.

## Query resource history

The reference contract exposes:

```http
GET /orders/{orderId}/audit?page[limit]=50&sort=-occurredAt,-eventId
```

The application use case requires `orders.audit.read`, caps pages at 100, and
uses an opaque cursor over occurrence time and event ID. The history remains
available after an order is soft-deleted because it is not joined through a
foreign key. Related-resource history exists only for explicit bounded
projections; a generic cross-ledger scan is not an API.

Values may be literal, SHA-256 digests, redacted, or omitted. Choose the data
class before persistence, never after disclosure. Audit access is itself a
security boundary and should have its own authorization and operational log.

## Plan for indefinite growth

There is no universal 50 or 100 MB rotation rule:

| Store | Recommended lifecycle | Why |
|---|---|---|
| SQLite | Explicitly seal bounded segments and monitor host disk | Finite disk, backup and checkpoint time are hard limits |
| PostgreSQL | Separate database plus time partitioning | Time-based pruning/archive is operationally predictable |
| DynamoDB | Retain or archive a reviewed hot horizon | No practical table-size rotation need, but storage and PITR are billable |

Logical cursors do not expose segment or partition identity, so the application
query contract can span lifecycle changes. Archival still needs an immutable
batch manifest and completion receipt. Never enable TTL or delete a segment
before archive proof and legal-hold checks.

The plan reports audit storage and transactional-write amplification. Regional
DynamoDB request, storage and PITR prices require explicit inputs; an incomplete
price is reported as incomplete, not zero.

## Operate and test it

Test at the transaction boundary:

- source failure writes neither domain nor audit intent;
- audit/journal failure rolls back the domain mutation;
- duplicate request and relay retry produce one canonical event;
- concurrent revisions preserve the winner's semantic action only;
- relay claim expiry and worker races are recoverable;
- redacted/omitted fields never leak through errors or diagnostics;
- history ordering and cursor continuity survive deletion and segment changes;
- storage warning, archive and legal-hold states are observable.

Run the repository's SQL adapter tests and disposable DynamoDB conformance before
claiming an implementation is qualified. Local emulators prove contracts, not
AWS permissions, pricing, deployment or production behavior.
