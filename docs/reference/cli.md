# CLI Reference

The [generated CLI command tree and exact Clap help](generated/cli.md) is the
authority for the current checkout. This page explains workflow, evidence, and
mutation boundaries without maintaining a second exhaustive command inventory.

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

## Local project view and workbench

```text
cargo minco mcp --check --json
cargo minco workbench --check --json
cargo minco workbench export --format json|mermaid|static --output PATH
cargo minco --root /canonical/project/root workbench serve [--port 0]
```

The MCP and workbench consume the same schema-versioned bounded `ProjectView`.
`workbench --check` opens no listener and writes nothing. Export is the only
workbench write operation: it creates one new project-relative directory and
never replaces an existing destination. Serve requires an explicit canonical
root, binds IPv4 loopback directly, and prints the exact origin before
blocking. See [`../how-to/local-workbench.md`](../how-to/local-workbench.md) for
the output safety contract, evidence interpretation, accessibility behavior,
and browser verification.

## Coding-agent projections

```text
cargo minco agent plan --target codex|claude|all --json
cargo minco agent sync --target codex|claude|all --expect-plan-digest SHA256 --json
cargo minco agent doctor --target codex|claude|all --json
```

`agent plan` is deterministic and read-only. It inventories only Minco's fixed
project skill destinations, classifies creates, owned updates, unchanged files
and conflicts, and emits the digest required by `agent sync`. Sync recomputes
that plan, refuses stale digests or ownership ambiguity, stages files privately
under retained no-follow directory descriptors, and publishes with no-clobber
or exact-owned replacement semantics. It does not delete neighboring files.

The ownership receipt is `.minco/agent-manifest.json`. Existing unowned files,
edited managed files, symlinks, non-regular entries and changed path identities
fail closed. `agent doctor` performs no writes. Client MCP configuration remains
user-owned, so this initial doctor reports it as `unknown` with a manual action
instead of parsing or reserializing client JSON or TOML.

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
cargo minco make resource <name> [--dry-run]
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

`make resource` accepts a lower-kebab-case resource name and only selects a
complete `x-minco-resource` family already present in the valid contract. Its
JSON plan includes the contract digest and ordered create/list/read/update/delete
operation selections. It generates failing specifications and traces, not
business or persistence behavior.

## Deployment, cost and performance

```text
cargo minco deploy plan [--config PATH] [--output PATH]
cargo minco deploy render-sam [--config PATH] [--output PATH]
cargo minco deploy changeset [--target-config PATH] [--environment ENV]
  [--manifest target/PATH] [--output target/PATH]
  --approve-release-digest SHA256
cargo minco deploy apply [--changeset target/PATH]
  [--migration-plan target/PATH] [--migration-receipt target/PATH]
  [--receipt target/PATH] --approve-changeset-digest SHA256
cargo minco deploy verify [--manifest target/PATH]
  [--receipt target/PATH] [--output target/PATH] [--dry-run]
cargo minco promote [--manifest target/PATH] [--receipt target/PATH]
  [--verification target/PATH] [--output target/PATH]
  --approve-verification-digest SHA256 [--dry-run]
cargo minco cost [--config PATH]
cargo minco perf [--config PATH]
```

`deploy plan` validates the selected configuration and emits canonical Plan IR.
Schema 1 retains the API-only topology. Schema 2 requires one explicit
`http_api` function/trigger and can add worker functions, queues, SQS mappings,
DLQs and reviewed schedules. `render-sam` assigns the artifact URI for every
function and emits only resources present in the plan.

`changeset` verifies the exact release/source and the reviewed account, Region,
environment, role, stack state and drift before packaging artifacts into a
pre-existing bucket. It creates an unexecuted provider change set and writes a
digest-sealed, value-redacted receipt classifying additions, modifications,
replacements and deletions. `apply` is deliberately separate: it requires that
receipt's exact digest, a matching migration plan and a successful migration
receipt, rechecks the live guards, and writes `started` deployment evidence
before executing the exact change-set ARN. Infrastructure completion remains
pending until hosted verification advances the deployment receipt. Both
commands support `--dry-run`; dry-run is local and non-contacting.

`deploy verify` binds the exact release, started deployment receipt, executed
artifact/version, HTTPS endpoint and required contract, readiness,
authentication, smoke and artifact-identity checks. It advances the receipt
only after the application-owned hosted command returns a strict successful
observation. `promote` requires that immutable verification report and its
exact digest, rechecks the release/deployment binding, and modifies only the
guarded live Lambda alias routing boundary. It never rebuilds or replans.
Its dry-run reports missing evidence and performs no AWS or HTTP contact.

An enabled static-site plan adds two explicit stages. `deploy static-site plan`
reports release, receipt and destination blockers without provider contact.
`deploy static-site apply --approve-release-digest <sha256>` publishes only the
release-bound asset manifest, verifies S3 checksums before stale deletion,
waits for CloudFront invalidation and writes an immutable publication receipt.
`deploy verify --static-site` then combines the normal hosted API checks with
current S3/CloudFront byte hashes, OAC, certificate, DNS, pricing and
invalidation evidence before succeeding the generic deployment receipt. See
[`../deployment/static-site.md`](../deployment/static-site.md).

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
schema compatibility and upgrade examples, and
[`../adoption/0.3.1-to-0.4.0.md`](../adoption/0.3.1-to-0.4.0.md) for the
coordinated release boundary.

## Plugins

```text
cargo minco plugin list
cargo minco plugin add <id-or-crate> [--dry-run]
cargo minco plugin init <local-package-path> [--dry-run]
cargo minco plugin explain <id-or-crate>
cargo minco plugin new <id> [--dry-run]
cargo minco plugin enable <id> [--dry-run]
cargo minco plugin disable <id> [--dry-run]
cargo minco plugin validate
cargo minco plugin test <id-or-crate>
cargo minco plugin test --all
cargo minco plugin remove <id-or-crate> [--dry-run]
cargo minco plugin doctor
```

`plugin list` returns catalog coordinates plus archive-visible distribution
metadata without constructing plugin code. `plugin validate` also checks the
published-file include, schema safety, catalog drift and overlapping fields in
official linked runtime descriptors.

`plugin add` accepts an ID or crate name, resolves the application's exact
`minco` Cargo version, verifies the known static composition root, and plans the
facade feature plus manifest selection. It supports reviewed Minco facade
features; it deliberately refuses to invent a constructor for an app-owned or
third-party plugin. `plugin enable` and `plugin disable` are selection-only
workflows; an app-owned enable plan reports its explicit application
registration as unverified instead of guessing it. `plugin remove` removes the
facade feature and selection only when no traced application
operation, enabled dependent, migration, seed, declared data class, or resource
ownership blocks safe removal. A blocked dry-run succeeds so tools can inspect its ordered
`blockers`; a real removal fails before writing.

`plugin new` is the compatibility alias for `make plugin` and scaffolds the
Cargo metadata pointer, `minco-plugin.json`, Rust crate, conformance test, and
catalog entry. `plugin init` adopts an existing local package's reviewed
distribution record into the catalog; it does not register or execute the
package. All mutating plugin commands accept `--dry-run`, and global `--json`
returns paths and actions without file contents or secret values.

For a locally inspectable distribution record, `plugin explain` returns
capabilities, plugin dependencies, operations,
migrations, seeds, data classes, resources, wake sources, idle-cost classes,
configuration metadata, and inert conformance evidence. `plugin doctor` checks
catalog validity, distribution compatibility, known and non-contradictory
selection IDs, an exact version match between the application and CLI, active
Cargo features, and
verified linked static facade registration. `plugin test <id>` or `--all` runs
local packages through the public, offline `minco-test`
conformance boundary. Neither command executes manifest evidence strings or
contacts providers. Registry-backed packages must run the kit from their own
package workspace. See [`../how-to/manage-plugins.md`](../how-to/manage-plugins.md),
[`plugin-distribution.md`](plugin-distribution.md) for schema authority and
[`plugin-conformance.md`](plugin-conformance.md) for assurance boundaries and
stable diagnostics.

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
cargo minco package [--config PATH] [--environment ENV]
  [--plan target/PATH] [--template target/PATH]
  [--output target/PATH] [--attestation PATH]...
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
