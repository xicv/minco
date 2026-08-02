# Manage plugins safely

Minco plugin workflows separate metadata inspection, Cargo selection, static
Rust registration, offline conformance, provider evidence, and data removal.
Use `--dry-run --json` before every mutation in automation.

## Add an official facade plugin

List the reviewed application catalog, then preview the selection change. A
fresh generated application includes the default `health`, `observability`, and
`idempotency` facade plugins:

```bash
cargo minco plugin list --json
cargo minco plugin add minco-plugin-health --dry-run --json
```

The add plan resolves the exact `workspace.dependencies.minco` version before
editing, verifies the supported composition root, and lists only changed paths.
Apply the same command without `--dry-run`, then verify:

```bash
cargo minco plugin add minco-plugin-health --json
cargo minco plugin doctor --json
cargo minco plugin test health --json
```

Official facade registration remains static Rust code. The command enables its
reviewed Cargo feature and adds the plugin ID to `[plugins].enabled`; it does not
download or dynamically load anything.

`plugin explain` requires archive-visible distribution metadata that is already
locally inspectable. It works for path-backed entries and in the Minco source
catalog, for example `cargo minco plugin explain feedback --json`. A
registry-only catalog entry without a local record fails closed instead of
inventing incomplete capability, cost, migration, or conformance claims.

## Create an app-owned plugin

Preview and create the package:

```bash
cargo minco plugin new audit-export --dry-run --json
cargo minco plugin new audit-export --json
cargo minco plugin validate --json
cargo minco plugin test audit-export --json
```

`plugin new` creates the crate, package metadata pointer, strict
`minco-plugin.json`, public conformance test, workspace entries, and catalog
entry. It intentionally leaves a failing or minimal application-owned behavior
surface for TDD.

Add the crate dependency and its typed constructor to the application
composition root yourself. Constructor arguments can carry reviewed adapters,
clients, clocks, or typed configuration and therefore cannot be inferred from
catalog metadata:

```rust
let mut manager = minco::default_plugin_manager()?;
manager.register(AuditExportPlugin::new(audit_sink))?;
let plugins = manager.compose(&minco::core::PluginSelection::default())?;
```

`plugin add audit-export` fails until the plugin is a known facade feature. This
is intentional: no source scanning or constructor guessing occurs.

After the explicit Rust registration and its application test are in place,
select the plugin without asking the CLI to modify code:

```bash
cargo minco plugin enable audit-export --dry-run --json
cargo minco plugin enable audit-export --json
```

The plan labels application registration as `verified: false`; offline metadata
cannot prove a product-specific constructor. Keep the application composition
test as separate evidence.

## Adopt an existing local package

The package must already include a valid Cargo metadata pointer and distribution
record. Adopt only its metadata:

```bash
cargo minco plugin init plugins/minco-plugin-audit-export --dry-run --json
cargo minco plugin init plugins/minco-plugin-audit-export --json
```

The path must be normalized, project-relative, and remain inside the project.
The package version must be exact (or inherit an exact workspace version), and
the application's exact Minco version must match `cargo-minco`. `init` verifies
the record and updates only the configured catalog. Dependency and constructor
registration remain explicit follow-up work.

## Diagnose drift

```bash
cargo minco plugin doctor --json
```

The report has stable checks for catalog validity, distribution compatibility,
known and non-contradictory selections, an exact application/CLI version match,
active Cargo features, and linked static registration. A selected app-owned
plugin without a verified
facade registration fails closed. Doctor does not execute conformance evidence
strings or contact a provider.

## Disable versus remove

Disable changes runtime selection only:

```bash
cargo minco plugin disable health --dry-run --json
cargo minco plugin disable health --json
```

Remove also plans removal of the Cargo feature and refuses when ownership
evidence makes that unsafe:

```bash
cargo minco plugin remove feedback --dry-run --json
```

Review every ordered blocker. Typical blockers are enabled dependent plugins,
traced application operations, migration sets, seeds, declared data classes,
declared infrastructure resources, or unavailable archive metadata. Clean up
the application contract and code, run explicit infrastructure teardown or
retention and data retention/export/deletion procedures, and preserve that
evidence before retrying. The CLI never treats source deletion as proof that
customer data or cloud resources are safe to discard.

## Evidence boundaries

`plugin validate`, `plugin explain`, `plugin doctor`, and `plugin test` are
local/offline evidence. Database integration, emulator, provider sandbox,
bounded live AWS verification, deployment, and production readiness remain
separate states and commands.
