# Graph-driven local development

`cargo minco dev` is the primary local workflow. It compiles the typed runtime
environment and selected deployment Plan IR, then combines them with the
application's `[development]` declarations.

Inspect the complete plan before starting anything:

```bash
cargo minco dev --dry-run --json
```

Start the default profile and stop it with Ctrl-C:

```bash
cargo minco dev
```

The reference application defaults to the `postgres` profile: PostgreSQL,
Rustack's declared SSM/STS seams, the migration command and the API. SQLite is
an explicit profile:

```bash
cargo minco dev --profile sqlite
```

## Local service runtime

Minco keeps the Sail-like customization boundary in the application. The
project-owned `infra/local/compose.yaml` remains the Docker Compose source of
truth, while `cargo minco dev` supplies the graph-selected services, ports and
Rustack capabilities and supervises the application processes.

On Apple silicon macOS 26 or newer, the default `auto` runtime prefers a ready
Apple Container 1.2.x installation and falls back to ready Docker Compose.
Other supported hosts use Docker Compose:

```bash
MINCO_CONTAINER_RUNTIME=auto cargo minco dev
MINCO_CONTAINER_RUNTIME=docker cargo minco dev
MINCO_CONTAINER_RUNTIME=apple cargo minco dev
```

Apple Container does not implement Compose. Minco therefore maps only its
first-class PostgreSQL and Rustack service plans to native OCI commands. Keep
custom Compose-only infrastructure on Docker until it is represented in the
Minco development graph. Start Apple's runtime once with `container system
start`; Minco does not mutate the global runtime lifecycle.

An exact lifecycle receipt or existing owned resource takes precedence over
the fresh default, so an upgrade never silently moves PostgreSQL data between
runtimes. Select `apple` explicitly during a deliberate migration, validate the
database, and remove the old Docker volume only through a separate data-loss
decision.

Container and Compose project names include the normalized application name
and a fingerprint of the canonical Compose path. Separate checkouts and JJ
workspaces do not silently share service instances, while relative and symlink
aliases of one Compose file resolve to one identity. Required ownership labels,
an application-scoped lock and a secret-free atomic receipt under
`target/minco/dev` bind shutdown to the exact runtime selected by startup. A
same-named resource with missing or mismatched ownership is left untouched.

PostgreSQL data remains in a labeled named volume; ordinary stop removes no
data. This slice deliberately has no reset command. Remove a volume only after
inspecting its complete ownership labels and making a separate explicit
data-loss decision. A legacy implicit Compose project may have a
`local_minco-postgres` volume; Minco never adopts or deletes that volume
automatically.

Both runtimes bind database and emulator ports to `127.0.0.1`. PostgreSQL
readiness authenticates with SQLx, verifies the expected user and database, and
executes `SELECT 1`. Rustack readiness parses the health JSON and requires every
requested service to report `running`; an STS selection also executes an actual
Rust SDK `GetCallerIdentity` against the loopback endpoint with static local
credentials. Failed starts collect bounded logs with configured secret values
redacted and remove only an attempt-created container. Persistent volumes are
preserved. The following local-only overrides are available when an application
needs them. Image overrides must remain fully qualified `@sha256:` references;
mutable tags are rejected before runtime inspection or mutation:

```text
MINCO_POSTGRES_IMAGE
MINCO_POSTGRES_DB
MINCO_POSTGRES_USER
MINCO_POSTGRES_PASSWORD
MINCO_RUSTACK_IMAGE
MINCO_RUSTACK_LOG_LEVEL
AWS_REGION
AWS_DEFAULT_REGION
```

Rustack starts only the 18 identifiers supported by its exact 0.9.1 contract:
`apigatewayv2`, `cloudfront`, `cloudwatch`, `dynamodb`,
`dynamodbstreams`, `events`, `iam`, `kinesis`, `kms`, `lambda`, `logs`,
`s3`, `secretsmanager`, `ses`, `sns`, `sqs`, `ssm`, and `sts`. The default
image is the immutable multi-platform index
`ghcr.io/tyrchen/rustack:0.9.1@sha256:18cd91395e17453e2c34b299e45f4679dc2427473dc1db6541bbe212fd70a104`.
It is built from upstream commit
`ab8bc61a3e45058c7d42de8443f9d215cc110b18`, includes native arm64, and has
BuildKit provenance attestations; the tag is unsigned and no OCI signature was
established. The verified upstream namespace remains authoritative while the
identical xicv fork does not publish a separate image.

The PostgreSQL default is likewise immutable:
`docker.io/library/postgres:18.4-alpine3.24@sha256:9a8afca54e7861fd90fab5fdf4c42477a6b1cb7d293595148e674e0a3181de15`.
Local application processes receive a loopback `AWS_ENDPOINT_URL`, static local
credentials, explicit region, disabled EC2 metadata lookup, and S3 path-style
selection. No provider-chain endpoint fallback is used, and the development
plan continues to report `external_aws_contact = false`.

Service lifecycle dispatch is a hidden subcommand of the exact running
`cargo-minco` executable. There is no separately installed helper and no `PATH`
lookup or cross-version helper coupling. This is also the published-package
contract.

Useful explicit controls are:

```text
--environment <name>
--profile <id>
--no-migrate
--seed <reference|demo|test|bootstrap>
--with-worker <id>
--without-worker <id>
--frontend
--no-frontend
--port <1..65535>
--rustack-port <1..65535>
```

Worker IDs must exist in both the selected deployment plan and the
application's development declarations. Conflicting worker flags, unknown
workers, unknown seeds and an undeclared requested frontend fail before
startup. Staging and production environments reject development seeding.

The manifest owns commands without choosing a frontend framework:

```toml
[development]
default_environment = "local"
default_profile = "postgres"
compose_file = "infra/local/compose.yaml"

[development.profiles.postgres]
deployment_config = "environments/dev.toml"
migration = { program = "cargo", arguments = ["run", "-p", "app-service", "--bin", "app-migrate"] }

[development.api]
id = "api"
default_enabled = true
command = { program = "cargo", arguments = ["run", "-p", "app-service", "--bin", "app-local"] }
readiness = { kind = "http", url = "http://127.0.0.1:3000/health/ready" }
```

Commands are executed directly as a program and argument vector. Readiness HTTP
URLs must be loopback `http` endpoints without user information, queries or
fragments, and cannot redirect. Long-running processes use separate process
groups so shutdown includes descendants. Selected local services are stopped
in reverse order; volumes are not reset. `MINCO_LOCAL_DATABASE_URL` may select
another local PostgreSQL database, but its URL must use a loopback host; remote
database overrides fail before startup.

Dry-run is a single JSON DevPlan. A running command with `--json` emits a plan
object followed by newline-delimited lifecycle/log events. Sensitive command
environment values and sensitive runtime values are redacted before
serialization or logging.

The scripts under `scripts/dev` remain available for isolated topology,
database and Rustack conformance work. They are lower-level diagnostics rather
than the normal start sequence.
