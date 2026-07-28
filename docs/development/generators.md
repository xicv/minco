# Contract-aware generators

Minco generators create reviewable structure, inventory updates, documentation,
and tests. They do not invent domain rules or return placeholder success.

## Plan before writing

Every generator accepts `--dry-run`; global `--json` returns the same
deterministically ordered change plan in machine-readable form:

```text
cargo minco --json make operation getPlatform --dry-run
cargo minco --json make migration add-widgets --dry-run
```

Each plan identifies the generator, requested name, selected contract operation
when applicable, and ordered create/update paths. File contents and configuration
values are never printed. Without `--dry-run`, Minco flushes the plan to stderr
with `applied: false` before writing, then prints the machine-readable result to
stdout with `applied: true` only after every edit succeeds.

Generation fails before writes when a target exists, an input changed after
planning, a path or ancestor is a symlink, or a name could escape the project.
Create installation uses an atomic no-clobber filesystem operation, so a target
created after preflight is preserved rather than replaced.
Existing TOML inventories are parsed and re-rendered; generators do not splice
unreviewed text into Rust or YAML. Multiple migration or seed roots fail closed
because the current command has no ambiguous implicit set selection.

## Generator family

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

Names other than `operationId` are lower-kebab-case ASCII. Operation IDs are
lowerCamelCase ASCII and must already exist in the valid OpenAPI document.
Minco deliberately has no contract-stub flag: add and review the complete
OpenAPI operation—including security, success, Problem, examples, and
idempotency semantics—before generating code.

`make operation` creates application and in-process HTTP specification files,
updates the operation trace in `minco.toml`, and adds a short implementation
guide. Both specifications fail with an explicit `TODO(operationId)` until the
business behavior and Axum contract assertions replace them. `make test`
generates only the two specifications and trace update.

`make migration` chooses the next integer version from the single configured
migration set, creates an empty SQL file, and records a conservative
`destructive`, non-reversible classification. Review both SQL and metadata before
planning or applying it.

`make seeder` creates empty mutation SQL and a `SELECT FALSE` verification query.
Its initial metadata is local/development `demo`, insert-once, no mutable state,
non-destructive, transactional, and preserve-all-existing. Change those fields
only after reviewing the intended ownership and preservation contract.

Workers are registered disabled by default and intentionally panic until their
runtime semantics exist. Module and adapter files have compiler-visible boundary
tests. Plugins are ordinary application-owned workspace crates added
deterministically to the Cargo workspace and plugin catalog; they are not
enabled automatically, and their completion test fails until typed services,
configuration, capabilities, health, resources, cost, and conformance are
defined.

## App-owned stubs

```text
cargo minco --json stubs publish --dry-run
cargo minco stubs publish
```

The command publishes the 20 framework defaults under `stubs/minco/`. Edit those
files in the application; later generators prefer the app-owned copy. Publishing
never overwrites an existing stub.

Supported placeholders are:

- `{{NAME}}`, `{{SNAKE_NAME}}`, and `{{PASCAL_NAME}}`;
- `{{OPERATION_ID}}`, `{{METHOD}}`, and `{{PATH}}`;
- `{{RUST_PATH_LITERAL}}` for a quoted, escaped contract path in Rust stubs;
- `{{VERSION}}` for migrations;
- `{{LAYER}}` for module compiler-boundary tests.

An unknown or unresolved placeholder fails generation. Safety-critical inventory
updates remain implemented by Minco rather than customizable text templates.
