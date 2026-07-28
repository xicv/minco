# ADR 0022: Make seed data classified, preservation-aware and verifiable

## Status

Accepted.

## Context

Unclassified SQL seed scripts hide whether their data is suitable for
production, which rows they may replace, whether reruns are safe and how an
operator can prove the intended result. Treating backfills as seed scripts also
loses the ordered, attributable history already provided by migrations.

Minco supports PostgreSQL and SQLite without an ORM and does not force their SQL
semantics into a least-common-denominator abstraction.

## Decision

Every configured seed root contains a strict `.minco-seeds.toml` sidecar. A seed
declares a stable ID and version, application or plugin owner, backend, class,
source and verification files, dependencies, environment allowlist,
idempotency strategy, mutable-state ownership, destructive risk, transaction
behavior and preservation contract. Source and verification SHA-256 digests
are part of the deterministic catalog and plan.

The four classes are:

- `reference`: deterministic data allowed only in declared environments;
- `demo`: sample data for local and development environments;
- `test`: data for disposable test environments;
- `bootstrap`: initial identities or state requiring explicit environment
  acknowledgement.

Production rejects every `demo` or `test` seed in the complete dependency
closure, even when a `reference` or `bootstrap` root pulls it indirectly.
Every dependency must allow the selected environment and use the same backend.
One executable plan cannot mix transaction behaviors.

Planning, source verification and target execution are distinct:

- `db seed --profile <class> --dry-run` emits a deterministic plan and makes no
  connection;
- `db seed --verify` verifies source metadata and digests without a target;
- target verification requires an explicit profile, environment, set and
  database URL environment-variable name;
- execution additionally requires the exact reviewed plan digest and a new
  project-contained receipt path.

The adapters re-resolve and hash every SQL file before execution. A
`transaction = "required"` plan executes in one backend transaction; an
`autocommit` plan explicitly has no whole-plan rollback promise. PostgreSQL and
SQLite verification run under database-enforced read-only modes and each check
must return exactly one boolean row.

`risk = "destructive"` requires `--allow-destructive`. Any execution whose
dependency closure contains a bootstrap seed requires
`--authorize-bootstrap <environment>` matching the selected environment. This
acknowledgement supplements rather than replaces external credential and
operator authorization.

The receipt is reserved with create-new semantics before mutation and records
the source change, catalog and plan digests, class, environment, backend,
transaction behavior, authorization flags, seed IDs, verification and outcome.
Database URL values are never serialized.

Deterministic fixture identities live in `minco-test` and depend only on an
explicit namespace, kind and ordinal. They do not depend on an ORM, wall clock,
random-number generator or global state.

Data backfills remain migrations. Seeders must not be used to hide schema/data
evolution that needs ordered release history.

## Consequences

- Seed SQL remains backend-specific and reviewable while lifecycle metadata is
  provider-neutral.
- Preservation and idempotency claims are explicit evidence, not inferred from
  arbitrary SQL; authors and reviewers remain responsible for matching SQL to
  those claims.
- Independently targeted databases never claim to share one transaction.
- Generated applications include a local/development demo seed, but no
  production sample data or implicit startup seeding.

## Compatibility

The seed sidecar, catalog/plan JSON and receipt JSON begin at schema version 1.
The CLI and serialized schemas are intended for the documented 0.4.0
compatibility boundary.

## Safety

Planning and source-only verification authorize no database or cloud mutation.
Execution requires an out-of-band direct credential, exact plan digest and
unused bounded receipt path. This decision authorizes no deployment, AWS
mutation, release, tag or crate publication.
