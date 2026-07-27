# Minco verification and release evidence

Date: 2026-07-27
Current workspace version: `0.3.1`
Published baseline: `0.3.1`
Purpose: preserve published `M8` evidence and the independently qualified
`0.3.1` patch-release boundary without rewriting release history.

## `0.3.1` publication evidence

The patch release contains the text-only Feedback boundary merged in PR #15
and exact SQLx backend feature isolation merged in PR #16. It changes no public
Rust API or serialized contract shape and retains the same 24-package release
inventory as `0.3.0`. The larger multi-runtime Plan IR redesign remains outside
this release and is tracked separately as M6-T10.

The source-fix merge commit is
`cd679c74d44e04abe1655b71c8ca9b9381aa6f6b`. Hosted run
`30247725599` passed authoritative quality, the Chromium/Firefox Feedback
matrix, all-package publication dry run, Rustack/SSM conformance, and Orders
E2E on that exact merged `main` source before this release change began.

Release PR #17 exact head
`36b52a18893aded72284601503272fa0b444a403` passed hosted run
`30249418058`. Merge commit
`33719376b634e995c0bfdbe6c215f1c304cd6b5d` passed merged-main hosted run
`30249977158`. Both runs passed authoritative quality, the Chromium/Firefox
Feedback matrix, the 24-package publish dry run, Rustack/SSM conformance, and
Orders E2E. Remote tag `v0.3.1` resolves exactly to that merge commit.

Trusted-publisher run `30250487113` passed every source and packaging gate but
stopped before upload because crates.io had no trusted-publisher configuration
for `xicv/minco`. The documented authenticated fallback then published all 24
packages from a clean detached worktree at the exact tag without a partial
failure.

Independent post-publication verification downloaded every exact `.crate`
archive, matched all 24 crates.io SHA-256 checksums, confirmed every record is
not yanked, and confirmed owner `xicv`. A fresh locked
`cargo-minco 0.3.1` installation reports `minco 0.3.1`; a fresh external
consumer resolves and checks `minco = "=0.3.1"` with the declared Rust 1.97.1
toolchain.

All 24 exact docs.rs library routes return HTTP 200 directly. The final
`minco` facade build reports that all builds succeeded.

## `0.3.0` release boundary

The `0.3.0` release adds bounded registration provenance to the strengthened
plugin kernel published in `0.2.0`. It is a pre-1.0 minor release because it
changes public registrar return types and the `ServiceError::Duplicate`
payload. Publication is proven separately by the exact remote tag and
independent crates.io records; source metadata alone is not publication proof.

The release verification covers:

- Rust format/check/Clippy/test/Rustdoc gates across all targets and features;
- generated PostgreSQL and SQLite applications;
- real SQLite/PostgreSQL Feedback persistence;
- Chromium/Firefox widget E2E, cargo-deny, gitleaks and npm audit;
- native ARM64 Lambda ZIP packaging and all-package publication dry runs;
- deterministic Plan IR and SAM generation;
- graph-derived PostgreSQL/Rustack startup and isolated real Rustack
  S3/SQS/SSM/STS conformance through standard AWS endpoint variables,
  including `minco-aws-lambda` SecureString loading through the Rust SDK;
- SAM CLI linting plus read-only CloudFormation and IAM Access Analyzer
  validation.

The current adoption-readiness task creates no AWS resources. Earlier M5/M6
tasks contain bounded real-AWS adapter evidence and verified cleanup; this task
does not refresh or broaden that evidence. The local Docker API did not answer
read-only status calls during M6-T06, so its PostgreSQL and Rustack reruns are
environment-blocked rather than passed; earlier evidence remains historical.
Rustack proof is emulator proof even when executable.
The repository-wide Codex Security Deep Scan did not produce a canonical
completed report for the Feedback release; M6-T05 records the release-scoped
waiver and compensating checks. That waiver is not a scan pass and does not
automatically apply to a later release.

## M6-T07 plugin-registration provenance evidence

Base Git SHA:
`c5b7749cec295fddd795827733e2889d6f1f896b`.

The candidate now retains authoritative application/plugin ownership for
typed singleton services and ordered contributions. Plugin owners are opaque
and created only by `PluginManager`; direct application collections retain a
distinct application owner. Duplicate singleton diagnostics include the Rust
type, first owner and attempted owner. Frozen contribution summaries retain
global deterministic installation indices.

`ComposedApplication::registration_provenance()` and `cargo minco inspect
--json` serialize metadata only. Focused tests use service values with
deliberately sensitive `Debug` output and prove that neither values nor debug
content enter JSON. A compile-fail public API example plus runtime ownership
tests prove a plugin cannot supply another plugin's identity.

Passed:

```text
cargo fmt --all -- --check
cargo check -p minco-core -p cargo-minco --all-targets --all-features --locked
cargo clippy -p minco-core -p cargo-minco --all-targets --all-features --locked -- -D warnings
cargo test -p minco-core -p cargo-minco --all-features --locked
cargo check --workspace --all-targets --all-features --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-targets --all-features --locked
cargo doc --workspace --all-features --no-deps --locked
cargo minco inspect --json
scripts/aws/build-lambda.sh
cargo lambda build --release --arm64 --output-format zip -p minco-aws-worker --example sqs_worker --locked
```

The first focused strict-Clippy run failed because the manual `Debug`
implementations for the two mutable registries omitted newly added metadata
fields. They now report only counts and the next installation index; the exact
focused and workspace Clippy commands pass. No concrete registration values
were added to `Debug`.

The refreshed Orders ARM64 ZIP is 5,028,504 compressed / 11,043,648
uncompressed bytes. That is 15,502 bytes (0.3092%) above the immutable M6-T06
baseline and remains below the 10 MiB policy. The SQS worker remains 573,418 /
1,203,520 bytes. Cold local observations were 10.15 seconds for default facade
compilation, another 40.72 seconds for the all-feature increment, 110.28 seconds
for Orders Lambda and 12.78 seconds for the worker. These are single local
samples, not CI budgets. Both Cargo Lambda builds emitted the existing macOS
linker warning that deprecated optimization setting `1` was ignored; packaging
still succeeded.

Real-AWS, Rustack and PostgreSQL tests requiring explicitly configured external
environments remained ignored in the ordinary workspace test command. This
task does not refresh those provider proofs and does not create remote
resources.

The authoritative `./scripts/quality.sh` command passes, including generated
PostgreSQL and SQLite consumers, Rustdoc/docs, cargo-deny, RustSec audit,
Feedback npm audit and Gitleaks. The separate bounded inspection assertion,
official-plugin validation, package inventory, reverse-apply whitespace check,
source-manifest check and JJ conflict query pass. The 24-package publication
driver passes without `--execute`; Cargo verified every package tarball and
aborted every upload because of `--dry-run`.

The first publication dry run packaged all 24 crates and then failed during
packaged `minco-http` verification with `No space left on device`. Only this
isolated workspace's generated Cargo target was cleared; the unchanged
clean-source retry passed. No upload, tag, deployment, database or product
repository mutation occurred.

Exact commands, results and current limitations are recorded in
`FEEDBACK_REVIEW_STATUS.md` and `CODEX_HANDOFF.md`. The release history below
preserves the `0.1.x` evidence and records the current `0.2.0` boundary.

## Adoption footprint measurements

The durable machine-readable comparison is
`verification/adoption-measurements.json`. Dependency trees and native ARM64
artifacts were measured on the same pinned Rust/Cargo toolchain from isolated
cold targets.

| Facade selection | Baseline packages / feature lines | Candidate packages / feature lines |
|---|---:|---:|
| no default features | 16 / 81 | 16 / 81 |
| default features | 105 / 820 | 105 / 820 |
| `official-plugins` | 118 / 1040 | 118 / 1040 |
| all features | 290 / 3351 | 298 / 3424 |

The no-default, default and official-plugin surfaces do not grow. The
all-feature graph adds eight packages for the opt-in SQS Lambda runtime. Cold
baseline default and all-feature-increment builds measured 10.23 and 48.87
seconds. The current candidate report does not record corresponding general
build timings. Its isolated native ARM64 artifact builds recorded 21.15 seconds
for the Orders Lambda and 5.88 seconds for the SQS worker. These single local
wall-clock samples are observational and are not CI budgets.

The baseline Orders ARM64 Lambda ZIP was 5,013,002 compressed bytes and
11,000,744 uncompressed bytes. The candidate ZIP measured 5,030,945 compressed
bytes and 11,047,008 uncompressed bytes, a 17,943-byte (0.3579%) compressed
increase. The new opt-in SQS worker ZIP measured 573,415 compressed and
1,203,520 uncompressed bytes. The candidate report records exact SHA-256
digests for both ZIPs in addition to their compressed/uncompressed sizes.
`cargo-bloat` and `cargo-llvm-lines` were unavailable.

The committed baseline snapshot is bound to Git SHA
`6fe9121ea9284e2fa4e2dbfd76f21bd8a13e263a`; the candidate measurement is bound
to the immutable `source-tree-sha256` recorded in both the adoption report and
`verification/source-manifest.json`. The manifest excludes itself and the
adoption report to avoid self-reference, and `scripts/source_manifest.py
--check` recomputes every other distributable file without writing. The report
is regenerated by `scripts/measure_adoption.py`, which accepts both revisions,
timings and artifact paths and computes compressed/uncompressed sizes and
deltas rather than relying on a hand-edited comparison.

## M6-T06 exact-source local evidence

The authoritative `./scripts/quality.sh` entry point passed after the complete
change. It ran current static/truth/publish/deep-review fixtures; SQLite schema,
scaffold and dependency hygiene; no-default/default/official/worker/all-feature
facade checks; workspace all-target/all-feature check, strict Clippy and tests;
fresh generated PostgreSQL and SQLite application check/tests; Rustdoc/docs;
`cargo deny`, `cargo audit`, Feedback `npm audit`; and redacted full-source
Gitleaks. The generated-consumer target was changed to share the repository
Cargo cache and disable debug/incremental artifacts in the quality runner; an
earlier exact command failed with `No space left on device` and was not treated
as a pass.

Additional passed checks:

```text
cargo minco contract sync
cargo minco contract sync --check
scripts/test/e2e.sh
npm run --prefix plugins/minco-plugin-feedback test:browser
scripts/aws/plan.sh
scripts/aws/validate.sh
scripts/aws/build-lambda.sh
cargo lambda build --release --arm64 --output-format zip -p minco-aws-worker --example sqs_worker --locked
sam validate --lint --template-file infra/aws/generated/template.yaml
jj diff --git | git apply --reverse --check --whitespace=error-all
jj log -r 'conflicts()'
```

The browser matrix passed 38 Chromium/Firefox tests. The local Orders HTTP E2E
passed. The shared Docker daemon did not answer read-only status calls, so the
Docker-backed PostgreSQL and Rustack reruns are explicitly environment-blocked.
No Docker restart was attempted because it could disrupt unrelated user
containers. No AWS mutation, deployment, crate upload or tag occurred.

Context7 was invoked for the current Lambda runtime/events and Cargo Lambda
documentation but returned `Monthly quota exceeded`; exact locally resolved
crate sources and installed CLI help were used as the documented fallback.

## Release history and current boundary

### 0.2.0 publication boundary

Remote tag `v0.2.0` resolves exactly to
`c5b7749cec295fddd795827733e2889d6f1f896b`. A review-time
`scripts/validate_publish.py --require-registry` lookup succeeded for all 24
package names and reported each exact `0.2.0` version as already present on
crates.io. This proves the version is immutable and cannot contain M6-T07.

That lookup did not refresh downloaded archive checksums, ownership, docs.rs,
installation, or a GitHub release object. Those remain separate evidence. The
M6-T07 workspace is therefore `0.3.0`; no tag, upload, release, or deployment
is performed by this change.

### 0.1.x release history

All 14 public packages were accepted by crates.io at version `0.1.0` on
2026-07-24 and are owned by `xicv`. The published CLI compiles, installs, and
runs, but its binary-only archive cannot satisfy docs.rs `cargo rustdoc --lib`.

Version `0.1.1` was the lock-step patch release containing the `M8-T04`
library documentation target and the local/hosted Rustdoc regression gate.

The sections below retain the original `M8-T02` pre-publication evidence. They
are historical evidence, not claims about the current registry state.

## M8-T05 publication evidence

Minco `0.1.1` was published from remote tag `v0.1.1`, which resolves exactly
to merge commit `3da298c094ef515a68dcc18ee6a2b867dcd4889e`.

Release gates:

- PR `#5` exact head `23afb15d8b2ec71baa5da203467fca9d7969be01`
  passed hosted run `30069887615`.
- The exact merged-main commit passed hosted run `30070145165`.
- The complete local quality suite, generated PostgreSQL and SQLite consumer
  compilation/tests, docs.rs-shaped Rustdoc command, and 14-package Cargo
  publish dry run passed before tagging.
- Cargo accepted all 14 uploads in dependency order without a partial failure.

Post-publication verification:

- all 14 exact `0.1.1` registry records exist and are not yanked;
- every downloaded `.crate` archive matches its registry SHA-256 checksum;
- `cargo owner --list` reports `xicv` for every package;
- `cargo install cargo-minco --version 0.1.1 --locked` succeeds from crates.io,
  and the executable reports `minco 0.1.1`;
- all 14 exact library documentation routes return HTTP 200 without redirect;
- the `cargo_minco 0.1.1` Rustdoc page renders the README-backed CLI usage from
  the new library target.

Task `M8-T03` remains active only for adding a trusted co-maintainer and
configuring the protected GitHub OIDC trusted publisher.

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

## Not performed by M8-T02

No crate was uploaded. No crates.io token was used. No GitHub release, tag,
trusted publisher, or owner assignment was created. Those are task `M8-T03`
actions and remain outside this compiler/package task.

## Historical first-upload boundary

This read-only preflight also passed on 2026-07-24:

```bash
python3 scripts/validate_publish.py --expect-unpublished --require-registry
```

All 14 exact names were absent at check time. This is not a reservation and
must be repeated immediately before the first upload. Then follow
`docs/development/publishing.md`. The first version of every new crate must be
published by an authenticated owner. Configure protected OIDC trusted
publishing only after each crate exists and ownership has been established.

## M8-T02 conclusion

Minco `0.1.0` is **compiler-verified and Cargo dry-run verified** across the
complete 14-crate family. The generated PostgreSQL and SQLite applications
also compile and test successfully.

Task `M8-T03` remains the separate irreversible registry-release task. Nothing
was published in this task.
