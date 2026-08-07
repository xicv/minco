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

The default `auto` runtime prefers a ready Docker Compose installation and then
falls back to Apple Container on Apple silicon macOS 26 or newer:

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

Container and Compose project names include the normalized application name
and a fingerprint of the Compose path. Separate checkouts and JJ workspaces do
not silently share service instances. PostgreSQL data remains in a named
volume; stopping `cargo minco dev` does not reset it.

Both runtimes bind database and emulator ports to `127.0.0.1`. Minco performs
its own bounded PostgreSQL protocol and Rustack health probes, prints recent
container logs on failure, and cleans up a service that failed to start. The
following local-only overrides are available when an application needs them:

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

Rustack starts only the AWS service identifiers derived from the selected
application graph. The default image is the pinned multi-platform Rustack
0.9.1 release. Local emulation never grants permission to contact AWS, and the
development plan continues to report `external_aws_contact = false`.

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
