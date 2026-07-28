# ADR 0023: Graph-driven local development supervision

## Status

Accepted

## Context

Minco already exposes application/plugin registration, deployment Plan IR,
typed runtime configuration, explicit migration sets, classified seed sets and
graph-derived local Compose topology. The previous local workflow still
required separate service, migration and application scripts. Those scripts
could drift in profile selection and did not own the lifecycle of every child
or container as one operation.

Local convenience must not weaken Minco's explicit boundaries. In particular,
a development command must not infer undeclared workers, run schedules or
seeds, resolve production secrets, contact AWS, or report successful shutdown
while a child process or selected Compose service remains active.

## Decision

`minco-dev` is a publishable, provider-neutral crate owning serialized
`DevPlan` derivation, readiness events, labelled log events and coordinated
process supervision. `cargo minco dev` is the composition root that combines:

1. the selected deployment Plan IR for the application, database kind, workers,
   schedules and required local AWS services;
2. the typed runtime environment classification;
3. manifest-declared process commands, migration/seed commands, frontend and
   named development profiles.

`cargo minco dev --dry-run --json` performs no startup and no secret
resolution. It emits the complete deterministic service, lifecycle and process
plan. Command environment keys whose names can carry credentials, URLs,
passwords, secrets or tokens retain their names but serialize only
`<redacted>`.

The default plan starts only the selected PostgreSQL/SQLite/Rustack services,
applies the declared migration command, and starts the API plus workers marked
as default-enabled. Seeds require `--seed`; non-default workers require
`--with-worker`; frontend startup requires both an application declaration and
`--frontend` or a manifest default. Schedules are always listed as omitted and
never started.

Commands are represented as a program plus argument vector and are spawned
directly without shell reconstruction. Long-running commands use isolated Unix
process groups. Ctrl-C, readiness failure or any unexpected child exit
terminates every group, waits for cleanup, then executes selected Compose stop
commands in reverse order. A failed stop is a command failure and emits
`failed`, never `stopped`.

HTTP readiness accepts only credential-free loopback `http` URLs without user
information, queries or fragments, and disables redirects. Process output and
finite lifecycle output share labelled stdout/stderr events. Values supplied
through sensitive runtime environment keys are removed before log events are
emitted.

Running `--json` emits a first `plan` object followed by newline-delimited event
objects. Human output uses stable process labels.

## Consequences

- One command owns local service, migration, process, readiness, logging and
  cleanup order.
- Plan derivation remains pure and testable independently of Docker and child
  processes.
- Application commands and optional frontend tooling remain framework-neutral
  manifest data.
- Local PostgreSQL and Rustack containers stop with the supervisor while named
  volumes preserve state; resetting data remains a separate explicit action.
- The older `scripts/dev` commands remain bounded diagnostic and conformance
  tools, not the primary developer workflow.

## Compatibility

The new `minco-dev` public types, `[development]` manifest section,
`cargo minco dev` CLI and serialized DevPlan/event shapes are a likely Minco
`0.4.0` compatibility boundary. Existing manifests without `[development]`
continue to parse, but `cargo minco dev` fails closed until the application
declares its profiles and API command.

## Safety

Dry-run and plan construction are local and read-only. Runtime secret values
are resolved only after dry-run at the CLI composition boundary and never enter
serialized Plan IR or DevPlan output. The supervisor does not run schedules,
reset storage, create cloud resources, resolve SSM parameters or call AWS.
