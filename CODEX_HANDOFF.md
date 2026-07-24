# Minco Codex desktop handoff

## Current objective

Finish M4-T02 by proving graph-selected local dependencies and the real AWS SSM
adapter against Rustack. The compiler-enabled M8-T02 package gate is complete;
no crate has been uploaded.

Minco remains contract-first, AI-native, AWS-native, performance-aware, deployment-oriented, JJ-first, statically extensible, and deliberately small at its core.

## Start here

Read in this order:

```text
AGENTS.md
README.md
VERIFICATION.md
PUBLISHING.md
docs/development/publishing.md
docs/development/using-minco-crate.md
docs/DECISIONS.md
docs/architecture/overview.md
docs/architecture/contract-first.md
docs/architecture/extensions.md
docs/development/jj-workflow.md
docs/deployment/database-options.md
roadmap/roadmap.yaml
tasks/M4/M4-T02-rustack-local.md
```

## Publication architecture

The workspace has 19 packages:

```text
14 public crates.io packages
5 private Orders example packages
```

The normal consumer dependency is:

```toml
[dependencies]
minco = "0.1"
```

The application-development CLI is installed independently:

```bash
cargo install cargo-minco --locked
cargo minco new example-api --database postgres
```

Facade feature profiles:

```text
no default features: provider-neutral plugin/application kernel
default:             contract + HTTP + official default plugins
all features:        planning + release + tests + SQLx PostgreSQL/SQLite + Lambda
```

The canonical publish order lives in:

```text
Cargo.toml -> workspace.metadata.minco.release.publish
```

Do not publish packages in a manually improvised order.

## Completed compiler and package gate

PR #1 was merged to `main` as
`1a6df4d773e54b868ba631f051a536ef5cd3c5ec`. Local
`cargo minco check --with-cargo` passed all 16 gates. GitHub Actions run
`30060878644` passed the exact merge commit, including Rust 1.97.1, strict
Clippy, workspace tests, docs and the publish dry run. E2E was intentionally not
requested in that run.

The lockfile and dry-run packages were reviewed in M8-T02. This is not a
crates.io publication claim.

## Current Rustack gate

Local dependency selection is derived from `[plugins].enabled`:

```text
sqlx-postgres -> PostgreSQL
aws-lambda    -> Rustack with SSM only
```

Run the behavior proof with Docker:

```bash
docker compose -f infra/local/compose.yaml config
./scripts/dev/up.sh
./scripts/dev/test-rustack.sh
```

The test uses the standard `AWS_ENDPOINT_URL` supported by the AWS SDK. It
starts a digest-pinned Rustack 0.9.1 image, asserts that only Rustack and SSM are
selected, then performs a SecureString put/load/delete through
`minco-aws-lambda::load_secure_parameter`. The same script is a default-on step
in the manually dispatched `.github/workflows/minco-manual.yml`.

## First crates.io release: M8-T03

Only after M8-T02 is green:

1. Recheck all names immediately before upload:

   ```bash
   python3 scripts/validate_publish.py --expect-unpublished --require-registry
   ```

2. Use a clean dedicated JJ release workspace.
3. Confirm `v0.1.0` points at the exact reviewed release change.
4. Authenticate with a short-lived crates.io credential outside the repository.
5. Execute:

   ```bash
   scripts/release/publish.sh --execute
   ```

6. Verify every crate and docs.rs build.
7. Add a co-maintainer or restricted GitHub team owner.
8. Configure the protected `crates-io` GitHub environment and OIDC trusted publisher for every crate.

Cargo multi-package publication is ordered but non-atomic. A partially accepted release must be recovered by publishing only missing packages at the same version; accepted versions cannot be replaced.

## Crate family

```text
minco-core                  provider-neutral plugin/capability/service graph
minco-contract              OpenAPI validation, inventory and deterministic generation
minco-http                  Axum/Tower policy and RFC 9457 errors
minco-plan                  deployment Plan IR, cost/performance checks and SAM rendering
minco-release               immutable artifact/release manifests
minco-test                  in-process HTTP and command evidence helpers
minco-plugin-health         official readiness plugin
minco-plugin-observability  official tracing plugin
minco-plugin-idempotency    official idempotency primitives and port
minco-sqlx-postgres         bounded PostgreSQL pools and migrations
minco-sqlx-sqlite           SQLite pools, WAL policy and migrations
minco-aws-lambda            Lambda HTTP, API Gateway identity and SSM integration
minco                       feature-gated facade
cargo-minco                 `cargo minco` control plane and project generator
```

Orders packages remain private and must never be included in a public package list.

## Generated-project verification

`cargo minco new` creates ordinary source, not an opaque runtime project:

```bash
cargo minco new example-api --database postgres --vcs jj
cargo minco new example-local --database sqlite --vcs none
```

The deterministic template gate is:

```bash
python3 scripts/test/scaffold_templates.py
```

After `cargo-minco` compiles, run `scripts/test/generated_apps.sh`; it generates, locks, compiles, and tests both PostgreSQL and SQLite workspaces against the local Minco crate family.

## JJ release workflow

For an extracted source archive:

```bash
./scripts/jj/init.sh
```

Create a dedicated release workspace rather than publishing from an active feature workspace:

```bash
jj workspace add ../minco-release -r main -m 'release: prepare Minco 0.1.0'
cd ../minco-release
```

Use JJ for mutations, rebases, conflict resolution and descriptions. Keep Git primarily as the GitHub transport in the colocated repository. Before any dry run or upload, the release script requires a clean JJ or Git working copy and no unresolved JJ conflicts.

## Development and framework verification

After publication work, continue the normal framework gates:

```bash
cargo minco doctor
cargo minco contract sync --check
cargo minco architecture
cargo minco plugin validate
cargo minco check --with-cargo
cargo minco test all
```

With Docker:

```bash
scripts/dev/up.sh
scripts/dev/test-rustack.sh
scripts/test/e2e.sh
scripts/dev/down.sh
```

For AWS artifacts:

```bash
scripts/aws/build-lambda.sh
scripts/aws/plan.sh
scripts/aws/validate.sh
```

No real AWS deployment should occur until compiler, database, Lambda, SAM, IAM, cost and hosted verification evidence is green.

## Database decision model

Deployment cost planning includes:

```text
Neon PostgreSQL
self-hosted PostgreSQL
Amazon RDS PostgreSQL
Aurora Serverless v2
DynamoDB on-demand
persistent-host SQLite
rejected mutable SQLite on Lambda
```

SQLx PostgreSQL is the relational adapter for Neon, self-hosted PostgreSQL, RDS and Aurora. DynamoDB requires a purpose-built application port and access-pattern adapter; do not pretend it is a relational drop-in. Keep regional rate cards dated and represent missing rates as incomplete estimates rather than zero cost.

## Known verification boundary

Compiler, feature-matrix, strict Clippy, workspace test, docs, `.crate` dry-run,
JJ and local Rustack SSM evidence now exist. Database-backed E2E, native Lambda
packaging, SAM CLI validation, real AWS smoke, crates.io upload, docs.rs
verification and trusted-publisher configuration remain separate and
unperformed.
