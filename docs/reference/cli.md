# CLI Reference

The published binary is `cargo-minco`; Cargo exposes it as `cargo minco`. The
workspace also defines a local `cargo minco` alias. Global options are
`--root PATH` and `--json`.

## Create an application

```text
cargo minco new <name> [--directory PATH] [--database postgres|sqlite] [--vcs jj|none]
```

The default creates a colocated JJ/Git repository and a layered workspace with
domain, application, adapter, API, and composition crates. `new` is the only
command that does not require an existing `minco.toml`.

## Repository and quality

```text
cargo minco doctor
cargo minco check [--with-cargo] [--with-optional]
cargo minco architecture
cargo minco inspect
cargo minco explain <operationId>
```

`cargo minco inspect --json` includes bounded registration provenance for the
manifest-selected, statically linked plugin graph. Service records contain only
Rust type and application/plugin owner; contribution records add deterministic
installation indices. Registered values, configuration values and provider
diagnostics are not emitted.

## Typed configuration

```text
cargo minco config check [--environment NAME] [--set KEY=VALUE]
cargo minco config explain <path> [--environment NAME] [--set KEY=VALUE]
cargo minco config diff --from NAME --to NAME
cargo minco config schema
```

`check` composes the application schema with the effective enabled-plugin
schema and reports a deterministic digest. `explain` includes override
provenance. `diff` compares effective values. Secret values and secret-reference
names are omitted from every command response. `schema` includes all statically
linked plugins for discovery, including disabled plugins.

See [`configuration.md`](configuration.md) for file shape, precedence,
environment classes, secret-reference syntax and migration.

## Contract

```text
cargo minco contract check
cargo minco contract sync [--check]
cargo minco contract diff --against <revision>
```

`contract diff` validates the current and VCS-stored contracts without checking
out the revision. Its deterministic report classifies bounded structural
changes as `breaking`, `non_breaking` or `uncertain`; a clean structural report
is not semantic, deployment or data-migration proof. See
[`compatibility.md`](compatibility.md) for rules and automation guidance.

## Generators and app-owned stubs

```text
cargo minco make module <name> [--dry-run]
cargo minco make operation <operationId> [--dry-run]
cargo minco make migration <name> [--dry-run]
cargo minco make seeder <name> [--dry-run]
cargo minco make worker <name> [--dry-run]
cargo minco make adapter <name> [--dry-run]
cargo minco make test <operationId> [--dry-run]
cargo minco make plugin <id> [--dry-run]
cargo minco stubs publish [--dry-run]
```

All commands print deterministic change plans; combine global `--json` with
`--dry-run` for non-mutating automation. Operation and test generation require an
existing valid OpenAPI operation and create intentionally failing specifications,
never placeholder success. Existing paths, symlinked path components, unknown
stub placeholders, and ambiguous migration/seed roots fail closed. See
[`../development/generators.md`](../development/generators.md) for generated
paths, safety defaults, and app-owned stub customization.

## Deployment, cost and performance

```text
cargo minco deploy plan [--config PATH] [--output PATH]
cargo minco deploy render-sam [--config PATH] [--output PATH]
cargo minco cost [--config PATH]
cargo minco perf [--config PATH]
```

`deploy plan` validates the selected configuration and emits canonical Plan IR.
Schema 1 retains the API-only topology. Schema 2 requires one explicit
`http_api` function/trigger and can add worker functions, queues, SQS mappings,
DLQs and reviewed schedules. `render-sam` assigns the artifact URI for every
function and emits only resources present in the plan.

`inspect --json` includes the full deployment projection. `explain
<operationId> --json` identifies the HTTP deployment function and trigger for
the operation.

`cost --json` reports database cost separately from runtime dimensions:
schedules and derivable monthly invocations, worker connection pressure, every
SQS mapping, fixed/request-based resources and missing regional rates. `perf
--json` reports every function artifact with its relative path, byte size and
SHA-256 when the file exists. Missing rates or artifacts stay explicit; the CLI
does not guess them.

See
[`plan-schema-v2-migration.md`](../deployment/plan-schema-v2-migration.md) for
schema compatibility and upgrade examples.

## Plugins

```text
cargo minco plugin list
cargo minco plugin enable <id>
cargo minco plugin disable <id>
cargo minco plugin new <id>
cargo minco plugin validate
```

## Tests

```text
cargo minco test unit
cargo minco test feature
cargo minco test e2e
cargo minco test all
```

## Roadmap and tasks

```text
cargo minco roadmap status
cargo minco roadmap render [--format mermaid|json] [--output PATH]
cargo minco task list
cargo minco task ready
cargo minco task next
cargo minco task show <id>
cargo minco task graph [--output PATH]
cargo minco task verify <id>
```

## Database and release

```text
cargo minco db plan [--set ID]
cargo minco db status [--set ID --database-url-env NAME]
cargo minco db verify [--set ID --database-url-env NAME]
cargo minco db migrate --set ID --database-url-env NAME
  --expected-plan-digest SHA256 --receipt PATH [--allow-destructive]
cargo minco db seed --profile CLASS [--environment ENV] [--set ID] --dry-run
cargo minco db seed --verify
cargo minco db seed --verify --profile CLASS [--environment ENV] --set ID
  --database-url-env NAME
cargo minco db seed --profile CLASS [--environment ENV] --set ID
  --database-url-env NAME --expected-plan-digest SHA256 --receipt PATH
  [--allow-destructive] [--authorize-bootstrap ENV]
cargo minco release create --artifact PATH [--plan PATH] [--output PATH]
cargo minco release verify <manifest>
```

`plan` and source-only `status`/`verify` do not connect. Target commands accept
the name of an environment variable holding the direct database URL; URL values
are not command-line arguments or JSON fields. `migrate` requires the exact
reviewed plan digest and reserves a new project-contained receipt before
mutation. See
[`../deployment/database-lifecycle.md`](../deployment/database-lifecycle.md).

Seed classes are `reference`, `demo`, `test` and `bootstrap`; the default
environment is `local`. Production rejects demo/test seeds throughout the
dependency closure. Source verification is target-free. Target verification
and execution require a selected set and a named URL environment variable;
execution also requires the reviewed plan digest and a new receipt. Bootstrap
execution requires `--authorize-bootstrap` with the exact selected environment.

## Update

```text
cargo minco update check
cargo minco update apply --yes
cargo minco upgrade report
```

Apply mode updates the pinned toolchain and dependencies and reruns checks. A
clean JJ working-copy change is required.

`upgrade report` is read-only and records Rust, CLI, Cargo feature,
configuration, plugin and serialized-schema boundaries. It remains available
when the manifest schema is unsupported so release notes and migration guides
can explain the blocking version boundary. See
[`compatibility.md`](compatibility.md).

## Jujutsu VCS

```text
cargo minco vcs init
cargo minco vcs status
cargo minco vcs task-start <id> [--destination PATH]
cargo minco vcs task-finish <id> --message TEXT [--push]
```

`task-start` creates the new task change on top of the current change. Run it
from the reviewed prerequisite workspace when tasks must remain ordered.
