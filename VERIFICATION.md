# Minco crates.io readiness verification

Date: 2026-07-24  
Workspace version: `0.1.0`  
Purpose: prepare Minco as a crates.io crate family and Cargo subcommand without representing unperformed compiler or registry actions as complete.

## Publication shape

The workspace contains 19 Cargo packages:

- 14 public packages restricted to `crates-io`;
- 5 private Orders reference-application packages with `publish = false`.

The public family is published in this dependency order:

```text
minco-core
minco-contract
minco-http
minco-release
minco-test
minco-sqlx-postgres
minco-sqlx-sqlite
minco-plan
minco-plugin-health
minco-plugin-observability
minco-plugin-idempotency
minco-aws-lambda
minco
cargo-minco
```

The normal application dependency is the `minco` facade. The development control plane is the `cargo-minco` binary, exposed by Cargo as `cargo minco`.

## Performed and passed

### Static repository validation

Command:

```bash
python3 scripts/validate_static.py
```

Result:

```text
status:                 ok
errors:                 0
warnings:               0
workspace packages:     19
Rust source files:      47
OpenAPI operations:     4
OpenAPI schemas:        10
plugin catalog entries: 6
roadmap milestones:     9
task records:           18
```

The validator checks repository structure, TOML/YAML/JSON parsing, workspace member targets, the pinned toolchain declaration, OpenAPI profile rules, generated-contract drift, operation inventory, architecture boundaries, plugin selection and manifests, roadmap/task graphs, deployment-plan drift, structural cost/performance controls, SAM route coverage, placeholder detection, credential patterns, Python syntax, and shell syntax.

Evidence: `verification/static-validation.json`.

### crates.io publication-structure validation

Command:

```bash
python3 scripts/validate_publish.py
```

Result:

```text
status:               ok
errors:               0
warnings:             1
public packages:      14
private packages:     5
```

The one warning is intentional and unresolved:

```text
PUBLISH-067: Cargo.lock is absent.
```

The validator confirms:

- complete crates.io metadata;
- dual-license files and explicit package-content allowlists;
- `publish = ["crates-io"]` for every public package;
- `publish = false` for private examples;
- lock-step version `0.1.0`;
- explicit version plus local path for every public internal dependency;
- a dependency-valid multi-package release order;
- the `minco` facade and feature matrix;
- the `cargo-minco` executable name and Cargo-argument normalization;
- local README and package-file presence.

Evidence: `verification/publish-validation.json`.

### Crate-name availability check

On 2026-07-24, exact index-path lookups found no crates.io index entries for the 14 proposed names. This is evidence only; it is not a reservation and must be repeated immediately before the first upload.

Evidence: `verification/crate-name-availability.json`.

### Generated application profiles

Command:

```bash
python3 scripts/test/scaffold_templates.py
```

Passed for both generated profiles:

```text
postgres
sqlite
```

For each profile the test renders and parses the layered workspace, validates 11 TOML files, 2 YAML files, 8 Rust source files, 5 workspace packages, migrations, and the two-operation OpenAPI contract. It also rejects unresolved placeholders and Orders-specific coupling.

Evidence: `verification/scaffold-templates.json`.

### Deep static review

Command:

```bash
python3 scripts/deep_review.py
```

Result:

```text
status:   ok
findings: 0
```

Evidence: `verification/deep-review.json`.

### SQLite schema behavior

Command:

```bash
python3 scripts/test/sqlite_schema.py
```

The real SQLite engine executed the reference migration and verified foreign keys, JSON constraints, persistence behavior, and idempotency-key uniqueness.

Evidence: `verification/sqlite-schema.txt`.

### Deterministic non-Rust checks

Performed:

```text
Python py_compile over repository scripts
bash -n over every shell script
deterministic generation of Plan IR, SAM, roadmap and task graphs
source SHA-256 manifest generation
archive integrity and external checksum verification
```

Evidence is retained under `verification/`.

## Not performed

The assembly runtime does not contain `rustc`, Cargo, Rustfmt, Clippy, JJ, Docker, Cargo Lambda, SAM CLI, or AWS CLI. Network restrictions prevented installing the Rust toolchain. Therefore none of the following is represented as passed:

```bash
cargo generate-lockfile
cargo fmt --all -- --check
cargo check -p minco --no-default-features --locked
cargo check -p minco --locked
cargo check -p minco --all-features --locked
cargo check -p cargo-minco --locked
cargo test -p minco --no-default-features --locked
cargo test -p minco --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-targets --all-features --locked
scripts/test/generated_apps.sh
cargo doc --workspace --all-features --no-deps --locked
cargo publish --dry-run ...
cargo package --list ...
```

No `.crate` files were created. No crate was uploaded. No crates.io token was used. No GitHub release or tag was created.

## Required compiler-enabled release gate

Run from a clean, dedicated JJ release workspace:

```bash
rustup toolchain install 1.97.1 \
  --profile minimal \
  --component rustfmt \
  --component clippy

cargo generate-lockfile
# Review and commit Cargo.lock before continuing.

cargo fmt --all -- --check
cargo check -p minco --no-default-features --locked
cargo check -p minco --locked
cargo check -p minco --all-features --locked
cargo check -p cargo-minco --locked
cargo test -p minco --no-default-features --locked
cargo test -p minco --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-targets --all-features --locked
scripts/test/generated_apps.sh
cargo doc --workspace --all-features --no-deps --locked

scripts/release/publish.sh
scripts/release/package-list.sh
```

`publish.sh` is dry-run-only unless `--execute` is supplied. Do not use `--allow-dirty`, `--no-verify`, or a manually fabricated lockfile.

## First-upload boundary

Before the irreversible first upload:

```bash
python3 scripts/validate_publish.py --expect-unpublished --require-registry
```

Then follow `docs/development/publishing.md`. The first version of every new crate must be published by an authenticated owner. Configure protected OIDC trusted publishing only after each crate exists and ownership has been established.

## Current conclusion

Minco is **structurally prepared for Cargo packaging and crates.io publication**, with a usable facade design, feature matrix, Cargo subcommand, layered application generator, versioned internal dependencies, release order, publication validation, and guarded workflows.

It is **not yet compiler-verified or Cargo dry-run verified**. Task `M8-T02` remains the mandatory next gate; task `M8-T03` is the separate irreversible registry-release task.
