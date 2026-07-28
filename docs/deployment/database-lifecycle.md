# Database migration lifecycle

Minco treats migration source, target state, execution and verification as
separate operations. Application and Lambda startup never run migrations.

## Migration-set metadata

Every root in `minco.toml` under `[migrations].roots` contains
`.minco-migrations.toml`. The sidecar owns lifecycle metadata without modifying
released SQL:

```toml
schema = 1
id = "orders-postgres"
owner = "application:orders"
backend = "postgres"
history_table = "_minco_orders_migrations"
depends_on = []
verify_tables = ["orders", "order_idempotency"]

[[migration]]
version = 1
risk = "additive"
reversible = false
```

IDs use lower-kebab-case. Owners are `application:<id>` or `plugin:<id>`.
History and verification table names are plain ASCII SQL identifiers. Every SQL
version needs exactly one risk entry. Supported risks are `additive`,
`data_rewrite` and `destructive`.

Within one backend, each set owns a unique history table. Dependencies must use
the same backend and may not cycle.

## Read-only planning and inspection

Source-only commands never resolve a credential or connect:

```bash
cargo minco db plan --set orders-postgres --json
cargo minco db status --json
cargo minco db verify --json
```

Source-only verification reports `target_inspected: false` and
`target_verified: null`.

For a target, inject the direct migration URL through the named environment
variable using the operator's secret broker or process supervisor. Pass only
the environment-variable name:

```bash
cargo minco db status \
  --set orders-postgres \
  --database-url-env MINCO_MIGRATION_DATABASE_URL \
  --json

cargo minco db verify \
  --set orders-postgres \
  --database-url-env MINCO_MIGRATION_DATABASE_URL \
  --json
```

Status fails on malformed lifecycle metadata or an inaccessible target and
reports each migration as pending, applied, drifted or missing from source.
Verify exits unsuccessfully when history is dirty, any migration is not exactly
applied, or a declared table is absent.

## Digest-bound execution

Save and review the plan, then pass its exact digest:

```bash
mkdir -p target/minco
cargo minco db plan --set orders-postgres --json \
  > target/minco/orders-postgres-plan.json

MINCO_REVIEWED_MIGRATION_DIGEST="$(
  jq -r '.digest' target/minco/orders-postgres-plan.json
)"

cargo minco db migrate \
  --set orders-postgres \
  --database-url-env MINCO_MIGRATION_DATABASE_URL \
  --expected-plan-digest "$MINCO_REVIEWED_MIGRATION_DIGEST" \
  --receipt target/minco/orders-postgres-receipt.json \
  --json
```

The command fails before mutation when the source plan changed, target history
is dirty or drifted, or the receipt already exists. Add
`--allow-destructive` only after reviewing a pending `data_rewrite` or
`destructive` migration.

PostgreSQL uses SQLx's advisory migration lock. SQLite migration requires a
file-backed target and holds an adjacent `.minco-migrate.lock` file; in-memory
SQLite is refused. The lock file may persist after the process exits, but the
operating-system lock is released when its file handle closes.

The receipt contains no database URL. It binds the source change, catalog and
plan digests, selected set, before/after states, versions newly observed as
applied, declared table verification and final outcome. Receipt files use
create-new semantics; use a new path for each attempt.
