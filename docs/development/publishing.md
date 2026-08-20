# Publishing Minco to crates.io

Minco is released as a lock-step crate family. The `minco` facade is the normal
application dependency; the smaller crates remain independently usable for
applications that need a narrower dependency graph.

## Published baseline and release inventory

The published `1.9.0` release contains the complete lock-step 34-package
inventory. A workspace version or source tag is not registry proof: release
status is verified independently against the exact crates.io records. The
package inventory is derived from `[workspace.metadata.minco.release]` and
checked against every publishable workspace member by
`scripts/validate_publish.py`.

The workspace is an unpublished `1.10.0` candidate with 36 publishable
packages. `minco-interaction` and `minco-plugin-ticketing` are explicitly
recorded in `new_publishable_packages`; neither name has crossed first manual
publication or trusted-publisher configuration. Source qualification, hosted
compatibility, tag, OIDC, registry, docs.rs and Pages evidence remain separate.

The 1.0 release added `minco-plugin-realtime`, `minco-project-view`,
`minco-mcp`, `minco-workbench` and `minco-aws-dynamodb`; the 1.1 release added
agent-native behavior, while 1.2 adds browser/native HTTP metadata, verified
uploads, rich mail, owned local services and delivery evidence within the same
family. The 1.3 release adds the opt-in Waffo payment boundary; the 1.6 release
adds durable action auditing without changing package ownership; and the 1.7
release changes only fresh automatic local dependency-runtime selection. The
1.8 release adds object-transfer contracts without changing ownership. All 34
packages in the published 1.9.0 baseline have crates.io ownership; the two
1.10.0 additions do not. Source qualification or merge still must not be
described as registry publication.

The exact published source is immutable tag `v1.8.0` at
`fe1a20d4a6c76c7adef268727bb30b92b594e072`. PR-head clean-Linux run
`31774750512`, exact-main run `31775061737` and authentication-only OIDC run
`31775371863` passed before guarded publication. Run `31775399279` passed its
archive and external-consumer checks and uploaded the dependency-ordered family.
Independent registry validation found every exact 1.8.0 version present and
non-yanked. Later candidate qualification
must use its own exact source and must not be described as registry, tag or
deployment proof.

The 1.3.0 first publication crossed the Waffo crate's ownership boundary. The
1.4.0 recovery configured its exact trusted publisher without changing crate
ownership; 1.5.0 through 1.8.0 reused the exact publisher family and still
re-proved OIDC.
Exact local and clean-Linux qualification, tag, authenticated upload, registry
verification, docs.rs and Pages deployment remain separate states.

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
| `minco-project-view` | Bounded repository-native project graph and independent evidence lanes. |
| `minco-mcp` | Local read-only MCP projection over ProjectView. |
| `minco-workbench` | Accessible loopback and static ProjectView presentation. |
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
| `minco-plugin-realtime` | Provider-neutral ephemeral publication and subscriber-only browser delivery contracts. |
| `minco-plugin-payments-waffo` | Opt-in signed Waffo hosted checkout, read-only queries and verified webhook mechanics. |
| `minco-sqlx-postgres` | Bounded PostgreSQL pools and migrations. |
| `minco-sqlx-sqlite` | SQLite pools, WAL policy, and migrations. |
| `minco-aws-adapters` | Opt-in AWS provider adapters. |
| `minco-aws-dynamodb` | Validated DynamoDB SDK client, table intent, readiness, and redacted provider errors. |
| `minco-aws-lambda` | Native Lambda HTTP, API Gateway identity, and SSM integration. |
| `minco-aws-worker` | Opt-in SQS Lambda worker with partial-batch responses. |
| `minco` | Ergonomic facade with feature-gated re-exports and official defaults. |
| `cargo-minco` | Cargo subcommand installed as `cargo minco`. |

The reference Orders application is deliberately marked `publish = false`.

## Version policy

All Minco packages use the same version. Published 1.x packages follow the
project's Rust, Cargo, CLI, schema, diagnostic and behavioral compatibility
policy. Additive minor releases and compatible patch releases must keep the
complete family coordinated.

The version is defined once in `[workspace.package]` in the root `Cargo.toml`.
Every publishable internal path dependency also carries the same explicit
version. Cargo removes local `path` keys while packaging and resolves those
version requirements from crates.io.

## Required release gates

The complete candidate procedure, evidence statuses and bounded load/recovery
contract are in [1.0 candidate qualification](release-qualification.md).

Every release must also update the cumulative agent feature coverage in
`crates/minco-cli/assets/agent/bundle.json`. Each top-level changelog bullet
must map to a stable feature, current versioned documentation and every skill
that teaches it. The ordinary quality gates reject stale section digests,
missing markers, documentation escapes, incomplete skill coverage and a stale
deterministic projection receipt:

```bash
cargo test -p cargo-minco --test agent_skills --locked
uv run --locked python scripts/test/agent_workflows.py \
  --check-output verification/agent-workflows.json
```

Refresh the receipt only after reviewing the exact bundle and scenario diff:

```bash
uv run --locked python scripts/test/agent_workflows.py \
  --output verification/agent-workflows.json
```

These checks do not invoke a model or contact a provider. They prove release
content and deterministic client projection, not model quality or deployment.

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

5. Confirm the remote tag resolves to the qualified `main` SHA. When the
   release family contains any first-publication crate, use the short-lived
   manual token for the complete dependency-ordered family so existing and new
   packages cross the version boundary together:

   ```bash
   scripts/release/publish.sh --execute
   ```

   Use explicit repeated `--package <crate>` selections only to resume after a
   verified partial upload or for an independently reviewed recovery.

Cargo multi-package publication is ordered but not atomic. If crates.io accepts
some packages before a later upload fails, do not change or overwrite accepted
versions. Diagnose the failure, verify the registry state, and publish only the
remaining packages with explicit `--package` arguments.

The first version of a new crate additionally requires a manual authenticated
publish because trusted publishing can only be configured after ownership
exists. The complete 34-package family has crossed that ownership boundary.
Before 1.1.0 publication, trusted-publisher configuration was independently
reconciled for all packages. The 1.2.0 upload used a short-lived OIDC token only
after exact-tag verification and locked dependency prefetch; every later OIDC
upload must verify the current configuration again. Configure and verify the
new Waffo crate's trusted publisher before relying on OIDC for a later family.

## Trusted publishing after the first release

The first version of each new crate must be published manually. After ownership
exists on crates.io, configure a trusted publisher for that package:

- repository: `xicv/minco`
- workflow: `publish-crates.yml`
- environment: `crates-io`

Only a family with an empty checked `new_publishable_packages` list in
`verification/repository-truth.toml` has crossed the
first-publication ownership boundary. Revalidate each package's current
trusted-publisher configuration before a later upload rather than inferring it
from historical release state.

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

The workflow refuses `publish=true` while repository truth still lists any
first-publication package. This preflight runs before the OIDC token step and
prevents an ordered upload from publishing existing crates and then failing at
the first crate that lacks trusted-publisher ownership.

Recovery input `resume_packages` is reserved for a verified partial upload. It
must equal the exact registry-absent complement, and every unselected package
must already be present and non-yanked at the tagged version. A failed ordered
upload is never retried blindly.

The workflow refuses to publish unless:

- the ref is exactly `refs/tags/v<workspace-version>`;
- all static and Rust gates pass;
- the publish dry run passes;
- the operator explicitly selects `publish=true`;
- repository truth lists no first-publication package.

After publication, require exact registry evidence for the complete workspace
version:

```bash
uv run --locked python scripts/validate_publish.py \
  --expect-published --check-registry --require-registry
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
