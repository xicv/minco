# Database migration and seed lifecycle

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

## Classified seed metadata

Seed roots are configured separately:

```toml
[seeds]
roots = [
  "examples/orders/seeds/postgres",
  "examples/orders/seeds/sqlite",
]
```

Each root contains `.minco-seeds.toml`:

```toml
schema = 1
id = "orders-postgres-seeds"
owner = "application:orders"
backend = "postgres"

[[seed]]
id = "orders-postgres-demo"
version = 1
class = "demo"
source = "demo.sql"
verify = "demo.verify.sql"
depends_on = []
environments = ["local", "development"]
idempotency = "upsert"
mutable_state = "owned_rows"
risk = "replaces_owned_rows"
transaction = "required"
preservation = "preserve_unowned_rows"
```

Classes are `reference`, `demo`, `test` and `bootstrap`. Environments are
`local`, `development`, `test`, `staging` and `production`. Demo and test seeds
are forbidden anywhere in a production plan's dependency closure. Dependencies
must use the same backend and allow the selected environment.

`insert_once`, `upsert` and `reconcile` describe idempotency. Mutable state is
either `none` or `owned_rows`; preservation is `preserve_all_existing`,
`preserve_unowned_rows` or `replace_owned_rows`. Risk is `non_destructive`,
`replaces_owned_rows` or `destructive`. These declarations do not reinterpret
SQL: the backend-specific source must implement the reviewed claims. Ordered
data evolution and backfills remain migrations.

## Seed planning, execution and verification

Plan both configured demo sets without a connection, or select one executable
backend set:

```bash
cargo minco db seed --profile demo --environment local --dry-run --json

cargo minco db seed \
  --profile demo \
  --environment local \
  --set orders-postgres-seeds \
  --dry-run \
  --json > target/minco/orders-postgres-seed-plan.json
```

Execution requires the matching digest, a direct URL supplied through a named
environment variable and a new receipt:

```bash
MINCO_REVIEWED_SEED_DIGEST="$(
  jq -r '.digest' target/minco/orders-postgres-seed-plan.json
)"

cargo minco db seed \
  --profile demo \
  --environment local \
  --set orders-postgres-seeds \
  --database-url-env MINCO_SEED_DATABASE_URL \
  --expected-plan-digest "$MINCO_REVIEWED_SEED_DIGEST" \
  --receipt target/minco/orders-postgres-seed-receipt.json \
  --json
```

The command resolves and hashes all source and verification files before
mutation. Required-transaction plans commit as one unit. Autocommit plans make
partial-application risk explicit. Mixed transaction behavior is rejected.
Destructive plans additionally require `--allow-destructive`.

Bootstrap uses the same digest and receipt gates and requires an exact
environment acknowledgement. The gate applies whenever a plan's dependency
closure contains a bootstrap seed, not only when `--profile bootstrap` is the
root selection:

```bash
cargo minco db seed \
  --profile bootstrap \
  --environment production \
  --set application-postgres-seeds \
  --database-url-env MINCO_SEED_DATABASE_URL \
  --expected-plan-digest "$MINCO_REVIEWED_SEED_DIGEST" \
  --receipt target/minco/bootstrap-seed-receipt.json \
  --authorize-bootstrap production \
  --json
```

The acknowledgement is not an identity or permission system. Supply the
credential only through the operator's authorization boundary.

Source-only verification never connects:

```bash
cargo minco db seed --verify --json
```

Target verification requires the exact class/environment/set selection:

```bash
cargo minco db seed \
  --verify \
  --profile demo \
  --environment local \
  --set orders-postgres-seeds \
  --database-url-env MINCO_SEED_DATABASE_URL \
  --json
```

PostgreSQL uses a read-only transaction and SQLite uses a connection-local
read-only guard. Every verification file must return exactly one boolean row.
The receipt and verification JSON explicitly distinguish source-only evidence
from an inspected and verified target, and never contain the database URL.
