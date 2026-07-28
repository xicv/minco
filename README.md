<p align="center">
  <img src="docs/assets/minco-icon.svg" width="180" alt="Minco mascot: a cute purple robot cloud" />
</p>

<h1 align="center">Minco</h1>

<p align="center"><strong>Minimal cost, maximum capability.</strong></p>

Minco is the contract-to-cloud framework for building, operating and evolving
low-idle-cost Rust web applications through one inspectable application graph.
Business logic stays in ordinary Rust; OpenAPI is canonical; plugins are
statically linked and explicitly selected; SQL remains visible; the graph
connects code, capabilities, resources, cost, deployment and evidence.

The accepted product definition, golden path and 1.0 completion boundary are in
[`docs/vision/minco-framework-definition.md`](docs/vision/minco-framework-definition.md).

The default AWS profile uses API Gateway HTTP API and one native ARM64 Lambda
ZIP. It contains no provisioned concurrency, NAT Gateway, scheduled poller or
always-on compute. Minco targets zero provisioned application compute at idle.
Storage, retained logs, DNS, secrets, database storage, schedules and other
fixed/request dimensions remain explicit and bounded.

> Published baseline: `0.3.1`
>
> Current workspace version: `0.4.0`
>
> Current publishable package count: `28`
>
> Minco is pre-1.0. Source is preparing the coordinated `0.4.0` public CLI,
> configuration, lifecycle and serialized-schema boundary. A source version is
> not registry publication or live deployment proof.


## Use Minco as a dependency

The normal application dependency is the feature-gated facade crate:

```bash
cargo add minco
```

The default features provide the provider-neutral core, OpenAPI contract tools,
Axum/Tower HTTP conventions, and the official health, observability, and
idempotency plugins. Add only the deployment and persistence capabilities the
application uses:

```bash
# PostgreSQL + native Lambda + planning/release support
cargo add minco --features sqlx-postgres,aws-lambda,plan,release,test

# Local or single-process SQLite API
cargo add minco --features sqlx-sqlite,test

# SQS Lambda worker without AWS SDK clients
cargo add minco --no-default-features --features aws-worker
```

Install the development control plane and generate a layered application:

```bash
cargo install cargo-minco --locked
cargo minco new example-api --database postgres
cd example-api
cp .env.example .env
cargo minco doctor
```

The generator creates domain, application, adapter, API, local/Lambda
composition, migration, OpenAPI, test, plugin, roadmap, task, and quality files.
It initializes a colocated JJ/Git repository by default; `--database sqlite` and
`--vcs none` are explicit alternatives.

The published `0.3.1` baseline contains 24 packages. The `0.4.0` source
candidate contains 28: the existing family plus first-publish crates
`minco-config`, `minco-db`, `minco-dev` and `minco-deploy-aws`. See the
[`0.3.1` to `0.4.0` upgrade guide](docs/adoption/0.3.1-to-0.4.0.md) and
[`docs/development/publishing.md`](docs/development/publishing.md) for the exact
first-publish, dry-run, ownership, and trusted-publishing process.

A complete application-consumer walkthrough is available in
[`docs/development/using-minco-crate.md`](docs/development/using-minco-crate.md).

## Core guarantees

- **Contract first:** committed OpenAPI 3.1 operations, schemas, examples,
  security and problem responses precede handler implementation.
- **AI native:** stable paths, operation IDs, checked-in deterministic
  generation, JSON inspection, diagnostics, roadmap/tasks and `AGENTS.md`.
- **AWS native:** standard Lambda/API Gateway events, AWS SDK configuration,
  SAM/CloudFormation rendering, SSM secret references and IAM intent.
- **Small core:** graph, contracts, HTTP conventions, plan/release model,
  testing and CLI; optional capabilities remain plugins/extensions.
- **Strong boundaries:** `delivery -> application -> domain`; adapters
  implement narrow application-owned ports.
- **Performance and cost aware:** body/time/artifact limits, concurrency ×
  connection budgets, wake-source detection and explicit database profiles.
- **JJ first:** colocated Jujutsu/Git repository and one workspace per task.
- **Dev to deploy:** the same OpenAPI inventory, router, plugin/resource graph,
  release artifact and plan cross local/dev/staging/production boundaries.

## Implemented source tree

```text
crates/
  minco/            ergonomic facade and feature-gated public entrypoint
  minco-config/     typed environments, secret references and provenance
  minco-db/         migration, seed-plan and database lifecycle contracts
  minco-dev/        deterministic local dependency/process supervision
  minco-core/       plugin API, typed services, capability/application graph
  minco-contract/   OpenAPI profile, digest, operation inventory, generation
  minco-deploy-aws/ guarded CloudFormation target, receipt and promotion model
  minco-http/       Axum/Tower middleware, principal, RFC 9457 errors
  minco-plan/       Plan IR, performance/cost rules, database profiles, SAM
  minco-release/    immutable release manifests and digest verification
  minco-test/       in-process HTTP/test evidence helpers
  minco-cli/        `cargo-minco` package: contracts, plugins, tests, JJ and plans
plugins/
  minco-plugin-health/          readiness registry and contributed checks
  minco-plugin-observability/   structured tracing
  minco-plugin-idempotency/     key and fingerprint primitives
  minco-plugin-sessions/        opaque sessions and CSRF primitives
  minco-plugin-identity/        verified claims, scopes and permissions
  minco-plugin-object-storage/  objects and direct-access signing ports
  minco-plugin-events/          publisher and explicit outbox ports
  minco-plugin-notifications/   email, webhook, in-app and developer notices
  minco-plugin-audit/           append-only business audit port
  minco-plugin-feedback/        widget, screenshots, voice, discussion and AI handoff
  minco-plugin-static-site/     static-site deployment intent
extensions/
  minco-sqlx-postgres/
  minco-sqlx-sqlite/
  minco-aws-adapters/
  minco-aws-lambda/
  minco-aws-worker/
examples/orders/
  domain/ application/ adapters/ api/ service/
  openapi/ migrations/ config/
infra/
  local/            PostgreSQL + Rustack Compose topology
  aws/generated/    checked-in Plan IR and SAM snapshot
roadmap/ tasks/      machine-readable roadmap and dependency-aware work items
scripts/             quality, unit/feature/e2e, JJ, local, AWS and packaging
```

The Orders reference slice implements:

```text
GET  /health/live
GET  /health/ready
POST /orders              (authorization + Idempotency-Key replay/conflict)
GET  /orders/{orderId}
```

It has pure domain/application tests, a memory adapter, transactional PostgreSQL
and SQLite adapters, in-process Axum tests, local TCP and Lambda entrypoints,
migrations and deployment-route generation.

## Settled decisions

| Concern | Decision |
|---|---|
| Contract | OpenAPI 3.1 is the external HTTP source of truth. |
| HTTP | Axum + Tower directly; Minco adds conventions, not a second runtime. |
| Architecture | Modular monolith, inward dependencies and feature-aligned paths. |
| Persistence | SQLx, explicit SQL and use-case ports; no ORM/generic repository. |
| Plugins | Static Rust crates, typed injection, descriptors and explicit selection. |
| VCS | Jujutsu default, colocated Git for GitHub transport/tool compatibility. |
| AWS | Native ARM64 Lambda ZIP + API Gateway HTTP API. |
| Databases | Explicit Neon, self-hosted PostgreSQL, RDS, Aurora, DynamoDB and SQLite profiles. |
| Local AWS | Standard endpoint override, Rustack preferred, real AWS authoritative. |
| Release | Build once, hash everything, migrate explicitly, promote exact artifact. |
| Idle cost | Zero provisioned application compute by default; retained, scheduled, request and fixed dimensions remain explicit. |
| Frontend | Not assumed; a future static-artifact plugin may be selected explicitly. |
| Update | Reviewed source/toolchain/dependency update; no unsigned self-replacement. |

See [`docs/DECISIONS.md`](docs/DECISIONS.md) and the ADRs.

## Prerequisites

Required for full framework development:

- Rust **1.97.1**, Rustfmt and Clippy, pinned in `rust-toolchain.toml`;
- Jujutsu (`jj`) and Git;
- Python 3 with PyYAML for bootstrap/static validation.

Additional paths require Docker, Cargo Lambda, SAM CLI and AWS CLI v2.

## Bootstrap and JJ

```bash
cp .env.example .env
./scripts/bootstrap.sh

# or explicitly
./scripts/jj/init.sh
cargo minco vcs status
cargo minco task ready
./scripts/jj/task-start.sh M1-T01
```

Jujutsu workspaces let separate tasks and long-running tests proceed in parallel.
Use `jj` for mutations and Git/GitHub for transport. See
[`docs/development/jj-workflow.md`](docs/development/jj-workflow.md).

## Local development

SQLite-only:

```bash
cargo run -p orders-service --bin orders-local --features sqlite
```

Inspect or run the graph-selected local environment:

```bash
cargo minco dev --dry-run --json
cargo minco dev
cargo minco dev --profile sqlite
```

The default reference profile starts PostgreSQL, only its declared Rustack
services, the migration and the API. Use `--rustack-port 4567` when the default
port is occupied. Ctrl-C stops all process groups and selected containers
together without resetting volumes. See
[`docs/development/local-development.md`](docs/development/local-development.md)
for profiles, workers, seeds, frontend commands, JSON events and safety
boundaries, and [`infra/local/README.md`](infra/local/README.md) for isolated
conformance commands.

Contract-first workflow:

```bash
cargo minco contract check
cargo minco contract sync --check
cargo minco explain placeOrder --json
cargo minco inspect --json
```

Inspection includes metadata-only singleton and contribution provenance so
duplicate or unexpected providers can be traced to the application or exact
plugin owner. Concrete service values, configuration, credentials and provider
diagnostics are never serialized.

## Plugins

Default plugins are `health`, `observability` and `idempotency`. The broader
official set adds sessions, identity, object storage, events/outbox,
notifications, audit, and the opt-in Feedback vertical slice. They remain
separate Cargo features and can be linked or omitted independently.

```bash
cargo minco plugin list
cargo minco plugin disable idempotency
cargo minco plugin enable idempotency
cargo minco plugin new audit-export
cargo minco plugin validate
```

Selection is stored in `minco.toml`; code remains explicitly linked and
constructed. Third-party plugins use the public `minco-core::Plugin`, descriptor,
`PluginContext` and typed `ServiceCollection` APIs. See
[`docs/architecture/plugin-authoring.md`](docs/architecture/plugin-authoring.md).


### Feedback plugin

The official stable Feedback plugin provides a configurable floating action
button, browser-authorized screenshots, file attachments, optional voice
recording/transcription, client/developer discussion, explicit clarification and
development states, PostgreSQL/SQLite persistence, notifications, audit/events,
and deterministic AI-ready context.

```html
<script
  src="/_minco/feedback/widget.js"
  data-endpoint="/_minco/feedback"
  data-position="bottom-right"
  defer
></script>
```

Developer workflow:

```bash
cargo minco feedback inbox
cargo minco feedback show <id>
cargo minco feedback reply <id> --body "Could you clarify the expected result?"
cargo minco feedback status <id> ready_for_development
cargo minco feedback pull <id> --output tasks/feedback/<id>.md
```

See [`plugins/minco-plugin-feedback/README.md`](plugins/minco-plugin-feedback/README.md),
[`docs/architecture/feedback-loop.md`](docs/architecture/feedback-loop.md), and the
[cross-project capability audit](docs/architecture/capability-audit.md).

## Testing and quality

```bash
./scripts/test/unit.sh
./scripts/test/feature.sh
./scripts/test/e2e.sh
./scripts/test/all.sh
./scripts/quality.sh
```

`quality.sh` requires the compiler and runs static validation, deep review,
Rustfmt, Clippy, all workspace targets and Rustdoc. The optional GitHub workflow
is manual (`workflow_dispatch`) and off by default. See
[`docs/development/testing.md`](docs/development/testing.md).

## Database profiles and cost

```bash
cargo minco cost --config examples/orders/config/minco.dev.toml
cargo minco cost --config examples/orders/config/minco.neon-launch.toml
cargo minco cost --config examples/orders/config/minco.self-hosted-postgres.toml
cargo minco cost --config examples/orders/config/minco.rds-postgres.toml
cargo minco cost --config examples/orders/config/minco.aurora-serverless-v2.toml
cargo minco cost --config examples/orders/config/minco.dynamodb.toml
cargo minco cost --config examples/orders/config/minco.local-sqlite.toml
```

Published Neon rates in `pricing/catalog.toml` are dated. AWS regional prices are
explicit inputs; missing rates make an estimate incomplete rather than silently
zero. DynamoDB is a real cost/planning profile but the Orders relational port is
not falsely mapped to DynamoDB; its adapter remains a tracked task. See
[`docs/deployment/database-options.md`](docs/deployment/database-options.md).
The broader default/opt-in/excluded service policy and dated evidence boundary
are in
[`docs/deployment/aws-service-doctrine.md`](docs/deployment/aws-service-doctrine.md).

## Plan, build and deploy

```bash
./scripts/aws/plan.sh examples/orders/config/minco.dev.toml
./scripts/aws/build-lambda.sh
./scripts/aws/validate.sh

# explicit mutating deployment; requires reviewed environment variables
MINCO_STACK_NAME=minco-dev \
MINCO_DATABASE_URL_PARAMETER=/minco/dev/database-url \
AWS_REGION=ap-southeast-2 \
./scripts/aws/deploy.sh
```

Migrate separately before deployment:

```bash
# MINCO_MIGRATION_DATABASE_URL is injected out of band.
cargo minco db plan --set orders-postgres --json \
  > target/minco/orders-postgres-plan.json
MINCO_REVIEWED_MIGRATION_DIGEST="$(
  jq -r '.digest' target/minco/orders-postgres-plan.json
)"
cargo minco db migrate \
  --set orders-postgres \
  --database-url-env MINCO_MIGRATION_DATABASE_URL \
  --expected-plan-digest "$MINCO_REVIEWED_MIGRATION_DIGEST" \
  --receipt target/minco/orders-postgres-receipt.json
```

The generated SAM template reads the named SSM `SecureString` at runtime through
least-privilege `ssm:GetParameter` and KMS-decrypt permission. Secret values are
not stored in the database plan/receipt, deployment plan, template or release
manifest. See
[`docs/deployment/database-lifecycle.md`](docs/deployment/database-lifecycle.md).

## Update

```bash
cargo minco update check --json
cargo minco update apply --yes
```

Apply mode requires a clean workspace and updates the pinned toolchain,
`Cargo.lock` and quality evidence. See [`docs/development/update.md`](docs/development/update.md).

## Roadmap and tasks

```bash
cargo minco roadmap status
cargo minco roadmap render --format mermaid --output roadmap/roadmap.mmd
cargo minco task list
cargo minco task ready
cargo minco task graph --output roadmap/tasks.mmd
cargo minco task verify M1-T01
```

The repository is the planning source of truth. `roadmap/roadmap.mmd` and
`roadmap/tasks.mmd` are generated visualisations.

Existing applications should begin with the reversible, contract-first steps in
[`docs/development/adopting-existing-application.md`](docs/development/adopting-existing-application.md),
then use the facade/stability matrix in
[`docs/adoption/incremental-adoption.md`](docs/adoption/incremental-adoption.md).


## Crates.io release preparation

Minco uses a lock-step release family. The published `0.3.1` release contains
24 packages, unchanged from `0.3.0`. The `0.4.0` source candidate contains 28
publishable packages, including first releases for `minco-config`, `minco-db`,
`minco-dev` and `minco-deploy-aws`. Source and package qualification are not
registry proof.
Static metadata and dependency validation can run without Cargo:

```bash
uv run --locked python scripts/validate_publish.py
```

The authoritative pre-upload gate requires the pinned Cargo toolchain and a
committed `Cargo.lock`:

```bash
cargo generate-lockfile
scripts/release/publish.sh                 # dry run; uploads nothing
scripts/release/package-list.sh            # inspect every packaged file
scripts/release/publish.sh --execute       # explicit irreversible upload
```

`publish.sh` selects only the packages declared under
`workspace.metadata.minco.release`, runs the complete quality suite, and refuses
dirty workspaces. The first release is manual; the checked-in manual GitHub
workflow supports crates.io OIDC trusted publishing after each crate has an
initial owner.

## Handoff and status

- [`CODEX_HANDOFF.md`](CODEX_HANDOFF.md): exact continuation path for Codex Desktop.
- [`VERIFICATION.md`](VERIFICATION.md): commands that actually ran and unavailable gates.
- [`CHANGELOG.md`](CHANGELOG.md): implemented foundation and known gaps.

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
