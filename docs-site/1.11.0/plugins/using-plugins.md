---
title: Install and Compose Plugins
description: Inspect, add, register, select, test, disable, and remove statically linked plugins safely.
---

# Install and Compose Plugins

Minco separates five questions that dynamic package systems often collapse:

1. Is the package and its metadata locally inspectable?
2. Is its Cargo feature compiled?
3. Is a typed constructor registered in the composition root?
4. Is the compiled plugin selected for this environment?
5. Which offline, adapter, emulator, or provider evidence has passed?

## Inspect before changing

```bash
cargo minco plugin list --json
cargo minco plugin explain feedback --json
cargo minco plugin validate --json
```

`list` and `explain` read Cargo/package metadata and strict distribution JSON.
They do not construct plugin code or contact a registry/provider.

## Add an official facade plugin

Preview the deterministic edit:

```bash
cargo minco plugin add minco-plugin-health --dry-run --json
```

Apply and verify:

```bash
cargo minco plugin add minco-plugin-health --json
cargo minco plugin doctor --json
cargo minco plugin test health --json
```

The command enables the reviewed facade feature and updates
`[plugins].enabled`. Runtime registration remains static Rust code.

## Create an application-owned plugin

```bash
cargo minco plugin new audit-export --dry-run --json
cargo minco plugin new audit-export --json
cargo minco plugin validate --json
cargo minco plugin test audit-export --json
```

The generator creates the crate, package metadata pointer, strict distribution
record, public conformance test, workspace entries, and catalog entry. It does
not invent the constructor dependencies or business implementation.

Register the typed constructor explicitly:

```rust
let mut manager = minco::default_plugin_manager()?;
manager.register(AuditExportPlugin::new(audit_sink))?;
let plugins = manager.compose(&minco::core::PluginSelection::default())?;
```

An application test must prove that the selected constructor receives the
intended adapter and capabilities.

## Adopt a local package

```bash
cargo minco plugin init plugins/minco-plugin-audit-export --dry-run --json
cargo minco plugin init plugins/minco-plugin-audit-export --json
```

The package must already have exact Cargo coordinates, a valid metadata pointer,
and a strict package-root distribution record. Paths are normalized and cannot
escape the project root. Dependency and constructor registration remain
explicit follow-up work.

## Diagnose selection drift

```bash
cargo minco plugin doctor --json
```

Doctor checks catalog validity, core compatibility, contradictory selections,
application/CLI version agreement, active Cargo features, and linked static
registration. It fails closed when metadata cannot prove a required boundary.

## Disable or remove

Disable changes environment selection only:

```bash
cargo minco plugin disable health --dry-run --json
```

Remove also considers Cargo features, dependent plugins, operation traces,
migrations, seed sets, data classes, and declared infrastructure:

```bash
cargo minco plugin remove feedback --dry-run --json
```

Source removal is never evidence that persisted data or cloud resources are
safe to discard. Complete the explicit retention/export/deletion and
infrastructure lifecycle first, retain its receipts, then retry the reviewed
source change.

## Evidence ladder

| Gate | What it proves |
|---|---|
| `plugin validate` | metadata and linked-descriptor agreement for the checked source |
| `plugin test` | public offline conformance contract |
| adapter test | concrete storage/provider behavior at that adapter boundary |
| emulator/Rustack | selected local AWS-compatible seam |
| bounded provider smoke | named account/Region/resources during that run |
| deployment observation | exact release in the selected environment |

Do not promote a lower row into a claim about a higher one.
