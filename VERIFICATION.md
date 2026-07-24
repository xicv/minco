# Minco crates.io readiness verification

Date: 2026-07-24  
Workspace version: `0.1.0`  
Purpose: record the compiler, generated-application, registry, and Cargo package
evidence for task `M8-T02` without representing the separate first upload as
complete.

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
python3 scripts/validate_publish.py --check-registry --require-registry
```

Result:

```text
status:               ok
errors:               0
warnings:             0
public packages:      14
private packages:     5
registry checks:      14
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

On 2026-07-24, exact crates.io API lookups returned `404` for all 14 proposed
names. This is evidence only; it is not a reservation and must be repeated
immediately before the first upload.

Evidence: `verification/crate-name-availability.json`.

### Generated application profiles

Command:

```bash
python3 scripts/test/scaffold_templates.py
scripts/test/generated_apps.sh
```

Passed for both generated profiles:

```text
postgres
sqlite
```

For each profile the static test renders and parses the layered workspace,
validates 11 TOML files, 2 YAML files, 8 Rust source files, 5 workspace
packages, migrations, and the two-operation OpenAPI contract. The compiler
test then generated fresh PostgreSQL and SQLite workspaces and successfully
ran both `cargo check --workspace --all-targets` and
`cargo test --workspace --all-targets`. The first compiler run found that
generated API DTOs used `chrono` and `uuid` without direct dependencies; the
scaffold manifests were repaired and both clean generations passed.

Evidence: `verification/scaffold-templates.json`.

### Deep static review

Command:

```bash
python3 scripts/deep_review.py
```

Result:

```text
status:   ok
errors:   0
warnings: 2
```

The two heuristic warnings count `expect` calls used after `writeln!` into
`String` in the contract and SAM renderers. Those writes are infallible by the
`fmt::Write for String` implementation, and strict Clippy plus renderer tests
pass. They are retained as visible review findings rather than suppressed.

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

### Rust compiler and feature gates

The dedicated JJ workspace used the repository-pinned toolchain:

```text
rustc 1.97.1 (8bab26f4f 2026-07-14)
cargo 1.97.1 (c980f4866 2026-06-30)
rustfmt 1.9.0-stable
clippy 0.1.97
jj 0.43.0
```

`Cargo.lock` was generated by Cargo, reviewed, and contains 326 external
packages from the crates.io index only. The following exact gates passed:

```bash
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
```

The compiler pass found and repaired source-assembly defects including a
missing direct `thiserror` dependency, feature-specific mutability, generated
Rustfmt drift, strict Clippy findings, and invalid Lambda error context
conversion. `./scripts/quality.sh` then passed end to end.

### Cargo package and publication dry run

From a clean JJ working-copy commit:

```bash
scripts/release/publish.sh
scripts/release/package-list.sh
cargo package --locked --package <all 14 release packages>
```

The dry-run driver re-ran the complete quality suite, completed 14 live
registry checks, normalized and extracted every package, compiled every
package against Cargo's temporary registry, and stopped each upload at Cargo's
dry-run boundary. No `--allow-dirty` or `--no-verify` option was used.

The retained `.crate` archives range from 8.8 KiB to 37.0 KiB compressed.
Their file counts, sizes, SHA-256 digests, and intended content review are
recorded in `verification/package-artifacts.txt`.

The driver originally failed closed because JJ 0.43 removed
`jj resolve --list`; its conflict guard now uses the repository-standard
`jj log -r 'conflicts()'` query.

## Not performed

No crate was uploaded. No crates.io token was used. No GitHub release, tag,
trusted publisher, or owner assignment was created. Those are task `M8-T03`
actions and remain outside this compiler/package task.

## First-upload boundary

This read-only preflight also passed on 2026-07-24:

```bash
python3 scripts/validate_publish.py --expect-unpublished --require-registry
```

All 14 exact names were absent at check time. This is not a reservation and
must be repeated immediately before the first upload. Then follow
`docs/development/publishing.md`. The first version of every new crate must be
published by an authenticated owner. Configure protected OIDC trusted
publishing only after each crate exists and ownership has been established.

## Current conclusion

Minco `0.1.0` is **compiler-verified and Cargo dry-run verified** across the
complete 14-crate family. The generated PostgreSQL and SQLite applications
also compile and test successfully.

Task `M8-T03` remains the separate irreversible registry-release task. Nothing
was published in this task.
