# ADR 0021: Make database migration lifecycle explicit and attributable

## Status

Accepted.

## Context

Minco already keeps production migration separate from application startup and
uses dedicated SQLx history tables for independently versioned migration
directories. The remaining operator path delegated to an application-specific
shell command. It did not provide one deterministic source plan, target drift
inspection, destructive-risk classification, exact-plan acknowledgement,
post-migration verification, or a durable result receipt.

Changing an existing SQL migration merely to add lifecycle metadata would also
change SQLx's checksum for release state that may already be applied.

## Decision

Each configured migration root has an immutable
`.minco-migrations.toml` sidecar that declares:

- a stable set ID and application or plugin owner;
- its PostgreSQL or SQLite backend and unique history table;
- dependencies on other sets for the same backend;
- tables that prove the set's expected schema exists;
- an explicit risk and reversibility claim for every SQL migration.

`minco-db` resolves these manifests and SQL files into a deterministic catalog
and dependency-ordered plan. The plan binds source paths, SHA-256 source
digests, SQLx SHA-384 checksums, ownership, risk, ordering and verification
metadata.

Database lifecycle commands are separate:

- `db plan` and source-only `db status`/`db verify` never connect or mutate;
- target `db status` reports pending, applied, dirty, drift and missing-source
  state;
- target `db verify` additionally proves declared tables exist;
- `db migrate` requires one selected set, a database URL environment-variable
  name, the exact plan digest and a new receipt path.

Database URL values are never command-line arguments or receipt fields.
PostgreSQL execution retains SQLx's advisory lock for the full migration run.
SQLite execution requires a file-backed database and holds an adjacent
cross-process file lock because SQLx's SQLite migration lock is a no-op.
Adapters revalidate identifiers, source containment, versions and checksums
immediately before execution.

Data-rewrite and destructive pending migrations require an explicit
`--allow-destructive` acknowledgement. Irreversible migrations remain labeled
irreversible; Minco does not invent rollback SQL.

The receipt is reserved with create-new semantics before mutation and records
the source change, catalog and plan digests, before/after target state, newly
observed applied versions, verification and outcome. A retry uses a new receipt
path.

## Consequences

- Plugin and application histories cannot silently share one history table for
  the same backend.
- Plans and receipts are inspectable evidence for later deployment-controller
  work.
- PostgreSQL and SQLite retain separate operational adapters; no generic CRUD
  or least-common-denominator database layer is introduced.
- Production application startup remains migration-free.
- A source-only verification result explicitly reports that no target was
  inspected.

## Compatibility

The migration sidecar schema, catalog/plan JSON and receipt JSON begin at schema
version 1. The `db migrate` CLI now requires explicit target, digest and receipt
arguments instead of delegating to `commands.database_migrate`. This is a
public CLI and serialized-schema change intended for the documented 0.4.0
compatibility boundary. Existing SQL migration contents and checksums are
unchanged.

## Safety

Planning and source verification authorize no database or cloud mutation.
Migration requires a direct credential supplied out of band, an exact digest
acknowledgement and an unused project-contained receipt path. This decision
authorizes no deployment, AWS mutation, release, tag or crate publication.
