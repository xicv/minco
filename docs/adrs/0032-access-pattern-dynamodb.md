# ADR 0032: Keep DynamoDB access patterns application-owned and deployment explicit

## Status

Accepted.

## Context

ADR-0012 rejects a generic database abstraction for DynamoDB. The original
M6-T01 task nevertheless predates the complete Orders resource contract: it
named only create and direct read, while the selectable Orders store now also
requires bounded listing, revision-conditional update and soft deletion.

Plan IR already classifies DynamoDB on-demand cost and local service needs, but
generic SAM intentionally fails closed because it has no table key/index
contract or exact IAM resource. Making the renderer guess those details would
turn access-pattern correctness into hidden infrastructure policy.

## Decision

The Orders application owns its DynamoDB access model and implements its five
use-case-shaped application ports. Domain and application crates remain free of
AWS SDK, Plan and HTTP dependencies.

The access model uses one canonical order item, one immutable idempotency item
and explicit secondary-index keys. Create uses an atomic transaction with
conditional puts. Direct reads and post-condition classification use strong
table reads. List uses bounded index `Query` operations, never `Scan`, and its
eventual-consistency behavior is documented. Updates and soft deletes use
revision conditions on the canonical item; soft deletion removes list-index
attributes while retaining the canonical and idempotency records.

`minco-aws-dynamodb` is an official provider extension, not a repository. It
owns validated table/client configuration, standard SDK endpoint selection, a
provider descriptor and reusable AWS resource support. It contains no Orders
types or business behavior.

Plan IR gains an optional, schema-closed DynamoDB table contract containing the
logical table identity, scalar key attributes, secondary indexes, on-demand
billing, recovery/retention intent and the function that needs access. Existing
DynamoDB cost-only inputs remain readable and continue to fail generic SAM.
Only an input with the explicit table contract may render a table, runtime table
reference and least-privilege IAM. The renderer supports schema declarations;
it does not infer queries or emulate relational constraints.

The implementation is a descendant of the exact M12-T06 1.0 candidate. The
candidate's commit and evidence remain immutable. The new public package and
serialized optional Plan fields receive separate post-1.0 compatibility,
packaging and qualification evidence.

## Consequences

- A DynamoDB profile is selectable only when all current application ports and
  deployment obligations are implemented.
- Table keys, indexes, consistency, conditional semantics, IAM and residual
  cost are reviewable before provider contact.
- Adding another application's DynamoDB adapter requires its own access model;
  it may reuse provider and renderer primitives without inheriting Orders
  concepts.
- Strong direct reads cost more than eventual reads; secondary-index listing is
  eventually consistent and must be acceptable to the application contract.
- Idempotency response snapshots survive later update and deletion.
- On-demand mode removes provisioned throughput, not storage, backup, index,
  transfer or request costs.

## Compatibility

The table contract is optional in the existing Plan schema. Inputs without it
retain their current cost/diagnostic representation and their intentional
generic-SAM rejection. The new official crate and application feature are
additive post-1.0 surfaces. Generated references and repository truth must be
updated with their exact package and schema inventory.

## Safety

Table names, endpoints and item bodies are never retained in public evidence.
Custom non-loopback endpoints require HTTPS; loopback HTTP is local-only.
Runtime IAM names exact table and index ARNs and never widens to `*`. Tests use
unique Rustack-local resources and cleanup traps. Real AWS creation requires a
separate, exact, time- and spend-bounded approval with absence-verified cleanup.
