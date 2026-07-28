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
groups so shutdown includes descendants.
Selected Compose services are stopped in reverse order; volumes are not reset.
`MINCO_LOCAL_DATABASE_URL` may select another local PostgreSQL database, but
its URL must use a loopback host; remote database overrides fail before
startup.

Dry-run is a single JSON DevPlan. A running command with `--json` emits a plan
object followed by newline-delimited lifecycle/log events. Sensitive command
environment values and sensitive runtime values are redacted before
serialization or logging.

The scripts under `scripts/dev` remain available for isolated topology,
database and Rustack conformance work. They are lower-level diagnostics rather
than the normal start sequence.
