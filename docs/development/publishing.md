# Publishing Minco to crates.io

Minco is released as a lock-step crate family. The `minco` facade is the normal
application dependency; the smaller crates remain independently usable for
applications that need a narrower dependency graph.

## Published baseline and release inventory

The published `0.4.0` release contains the complete lock-step 28-package
inventory and added first releases for `minco-config`, `minco-db`, `minco-dev`
and `minco-deploy-aws`. A workspace version or source tag is not registry
proof: release status must be verified independently against the exact
crates.io records. The package inventory is derived from
`[workspace.metadata.minco.release]` and checked against every publishable
workspace member by `scripts/validate_publish.py`.

The current workspace is an unpublished `0.5.0` candidate with the same
28-package inventory. Candidate qualification must use the exact source and
must not be described as registry, tag or deployment proof.

| Package | Role |
|---|---|
| `minco-config` | Typed environment graph, strict schema, secret references, provenance and deterministic digest. |
| `minco-db` | Migration and seed catalogs, digest-bound plans, verification and receipts. |
| `minco-core` | Provider-neutral plugins, typed services, capabilities, and application graph. |
| `minco-contract` | OpenAPI 3.1 validation, operation inventory, hashing, and deterministic bindings. |
| `minco-deploy-aws` | Fail-closed AWS target guards, CloudFormation change review and immutable receipts. |
| `minco-dev` | Deterministic local development planning and supervised process topology. |
| `minco-http` | Axum/Tower conventions, principals, request metadata, limits, and Problem Details. |
| `minco-plan` | Deployment Plan IR, database profiles, structural cost/performance policy, and SAM rendering. |
| `minco-release` | Immutable release manifests and artifact digest verification. |
| `minco-test` | In-process HTTP and command-evidence test helpers. |
| `minco-plugin-health` | Official health/readiness plugin. |
| `minco-plugin-observability` | Official structured tracing plugin. |
| `minco-plugin-idempotency` | Official idempotency primitives and port. |
| `minco-plugin-sessions` | Session contracts and bounded providers. |
| `minco-plugin-identity` | Identity contracts and explicit providers. |
| `minco-plugin-object-storage` | Object-storage contracts and provider boundaries. |
| `minco-plugin-events` | Transactional event/outbox contracts and dispatch primitives. |
| `minco-plugin-notifications` | Notification contracts and bounded delivery providers. |
| `minco-plugin-audit` | Durable audit contracts and adapters. |
| `minco-plugin-feedback` | Feedback capture, persistence, administration, and widget contract. |
| `minco-plugin-static-site` | Static-site runtime integration. |
| `minco-sqlx-postgres` | Bounded PostgreSQL pools and migrations. |
| `minco-sqlx-sqlite` | SQLite pools, WAL policy, and migrations. |
| `minco-aws-adapters` | Opt-in AWS provider adapters. |
| `minco-aws-lambda` | Native Lambda HTTP, API Gateway identity, and SSM integration. |
| `minco-aws-worker` | Opt-in SQS Lambda worker with partial-batch responses. |
| `minco` | Ergonomic facade with feature-gated re-exports and official defaults. |
| `cargo-minco` | Cargo subcommand installed as `cargo minco`. |

The reference Orders application is deliberately marked `publish = false`.

## Version policy

All Minco packages use the same version. During the pre-1.0 period, a minor
version may contain breaking public-API changes. Patch releases must remain
compatible within the same minor line.

The version is defined once in `[workspace.package]` in the root `Cargo.toml`.
Every publishable internal path dependency also carries the same explicit
version. Cargo removes local `path` keys while packaging and resolves those
version requirements from crates.io.

## Required release gates

Run from a clean JJ working copy at the release change:

```bash
uv sync --locked --only-dev
uv lock --check
uv run --locked python scripts/validate_static.py
uv run --locked python scripts/validate_publish.py --check-registry --require-registry
uv run --locked python scripts/deep_review.py

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
cargo rustdoc -p cargo-minco --lib --all-features --locked
cargo doc --workspace --all-features --no-deps --locked

scripts/release/publish.sh
```

`publish.sh` is a dry run unless `--execute` is supplied. It uses Cargo's
multi-package publishing support and selects only the crate family declared in
`[workspace.metadata.minco.release]`.

`scripts/test/generated_apps.sh` generates PostgreSQL and SQLite applications, patches them to the local crate family, and compiles/tests both complete workspaces.

The dry run performs package normalization, extracts each package, and compiles
what would be uploaded. Packages listed in
`workspace.metadata.minco.release.package_tests` are also tested from Cargo's
unpacked archive before any dry run or upload; this catches tests that depend on
workspace-only files. Inspect packaged files and sizes as an additional review:

```bash
scripts/release/package-list.sh
ls -lh target/package/*.crate
```

No release may use `--no-verify` or `--allow-dirty`.

The explicit `cargo rustdoc -p cargo-minco --lib` gate mirrors the target shape
used by docs.rs. The `cargo-minco` library target renders the CLI README as
crate-level documentation; executable behavior remains in the `cargo-minco`
binary target.

## Authenticated crates.io publication

Before each release, verify registry access and confirm the exact version is
not already published:

```bash
uv run --locked python scripts/validate_publish.py --check-registry --require-registry
```

For the first release of a new crate, or an explicitly approved recovery when
trusted publishing is unavailable:

1. Sign in to crates.io, verify the publisher account, and create a short-lived
   API token with the minimum required scope.
2. Authenticate without placing the token in the repository:

   ```bash
   cargo login
   ```

3. Run all required gates and the dry run.
4. Merge the release change, rerun the hosted qualification workflow against
   the exact resulting `main` SHA, then create the lightweight tag at that
   qualified SHA using JJ:

   ```bash
   jj tag set v<workspace-version> -r <qualified-main-sha>
   jj git export
   git push origin refs/tags/v<workspace-version>
   ```

5. Confirm the remote tag resolves to the qualified `main` SHA, then publish
   only the new crate or explicitly selected recovery set:

   ```bash
   scripts/release/publish.sh --execute --package <crate>
   ```

Cargo multi-package publication is ordered but not atomic. If crates.io accepts
some packages before a later upload fails, do not change or overwrite accepted
versions. Diagnose the failure, verify the registry state, and publish only the
remaining packages with explicit `--package` arguments.

The first version of a new crate additionally requires a manual authenticated
publish because trusted publishing can only be configured after ownership
exists. Every package in the published 28-package `0.4.0` family has crossed
that boundary.

## Trusted publishing after the first release

The first version of each new crate must be published manually. After ownership
exists on crates.io, configure a trusted publisher for that package:

- repository: `xicv/minco`
- workflow: `publish-crates.yml`
- environment: `crates-io`

The complete published family has crossed the first-publication ownership
boundary. Revalidate each package's current trusted-publisher configuration
before a later upload rather than inferring it from historical release state.

The checked-in workflow uses GitHub OIDC to obtain a short-lived crates.io token;
it does not require a long-lived crates.io secret. Keep the workflow manual-only
unless release policy is intentionally changed.

Minco currently has one maintainer and one crates.io owner. The `crates-io`
GitHub environment therefore has no required-reviewer rule by explicit
single-maintainer policy. Agent review and the following technical controls are
the release boundary:

- the publishing action is pinned to an exact commit;
- the authentication-only job has only `id-token: write`, performs no checkout
  or shell command, does not consume the token, and relies on the action's
  post-step revocation;
- the publication job has only `contents: read` and `id-token: write`;
- uploads remain manual, exact-tag-only, and require an explicit
  `publish=true` selection;
- the complete static, compiler, test, documentation, and package dry-run gates
  run before the upload step;
- publication and independent registry verification remain a separate release
  task.

To verify the OIDC configuration without uploading, manually dispatch
`publish-crates.yml` with `authenticate=true` and `publish=false`. That dispatch
runs only the authentication action. The normal dry-run path leaves both inputs
false and uses the exact workspace-version tag. A release dispatch uses that
same exact tag, leaves `authenticate=false`, and explicitly selects
`publish=true`.

The workflow refuses to publish unless:

- the ref is exactly `refs/tags/v<workspace-version>`;
- all static and Rust gates pass;
- the publish dry run passes;
- the operator explicitly selects `publish=true`.

After publication, require exact registry evidence for the complete workspace
version:

```bash
uv run --locked python scripts/validate_publish.py --expect-published
```

This mode fails on an absent or yanked exact version and treats registry
connectivity failure as an error.

## Ownership and recovery

The `xicv` account is the sole owner of the current crate family by explicit
single-maintainer policy. Keep crates.io publication notifications and GitHub
account recovery controls enabled. If a co-maintainer is added later, add an
environment reviewer or an equivalently scoped approval rule before granting
that maintainer release access.

A published version is permanent. A broken release may be yanked, but it cannot
be replaced. Correct the issue, increment the version, rerun every gate, and
publish a new version.

## Consumer installation

Most applications should use the facade:

```bash
cargo add minco
```

Minimal core only:

```bash
cargo add minco --no-default-features
```

AWS Lambda with PostgreSQL, planning, release, and test support:

```bash
cargo add minco --features sqlx-postgres,aws-lambda,plan,release,test
```

Install the development control plane independently and generate a layered
application:

```bash
cargo install cargo-minco --locked
cargo minco new example-api --database postgres
cd example-api
cargo minco doctor
```

`cargo minco new` emits ordinary Rust/TOML/YAML source for the domain,
application, adapter, API, composition, local runtime, Lambda runtime, OpenAPI,
migrations, tests, roadmap, tasks, and quality gates. JJ with colocated Git is
the default VCS profile.
