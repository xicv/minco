# Local Development Quickstart

## 1. Install required tools

Install Rust through Rustup; `rust-toolchain.toml` pins 1.97.1 with Rustfmt and
Clippy. Install uv 0.11.32, Jujutsu, and Git. Docker is optional for SQLite-only
development and required for the local PostgreSQL/Rustack topology.

```bash
./scripts/bootstrap.sh
```

The bootstrap script reports missing tools and deliberately does not execute
unreviewed remote installers.

## 2. Initialise JJ and select a task

```bash
./scripts/jj/init.sh
cargo minco task ready
./scripts/jj/task-start.sh M1-T01
```

See [`jj-workflow.md`](jj-workflow.md) before using mutating Git commands.

## 3. Configure

```bash
cp .env.example .env
```

The local binary supports SQLite or PostgreSQL. Development identity headers are
accepted only when `ALLOW_DEVELOPMENT_HEADERS=true`; the Lambda entrypoint uses
API Gateway JWT authorizer claims.

## 4. Validate

```bash
uv sync --locked --only-dev
uv run --locked python scripts/validate_static.py
cargo minco doctor
cargo minco check --with-cargo
```

## 5. Run with SQLite

```bash
DATABASE_KIND=sqlite \
SQLITE_PATH=target/minco/orders.db \
ALLOW_DEVELOPMENT_HEADERS=true \
cargo run -p orders-service --bin orders-local --features sqlite
```

## 6. Run with PostgreSQL and Rustack

Inspect the graph-derived topology before starting it:

```bash
python3 scripts/dev/topology.py
./scripts/dev/up.sh --dry-run
./scripts/dev/up.sh
./scripts/dev/migrate.sh
./scripts/dev/run.sh
```

The reference graph selects PostgreSQL plus Rustack SSM/STS and configures the
application through standard AWS endpoint variables. If port 4566 is already
in use, pass the same override to both scripts:

```bash
MINCO_RUSTACK_PORT=4567 ./scripts/dev/up.sh
MINCO_RUSTACK_PORT=4567 ./scripts/dev/run.sh
```

See [`../../infra/local/README.md`](../../infra/local/README.md) for isolated
database and Rustack conformance commands.

## 7. Exercise the contract

```bash
curl -sS http://127.0.0.1:3000/health/live
curl -sS http://127.0.0.1:3000/health/ready

curl -sS -X POST http://127.0.0.1:3000/orders \
  -H 'content-type: application/json' \
  -H 'idempotency-key: local-order-1' \
  -H 'x-minco-subject: local-user' \
  -H 'x-minco-permissions: orders.create,orders.read' \
  -d '{"customerReference":"LOCAL-001","lines":[{"sku":"MINCO-001","quantity":2}]}'
```

Repeat the POST with the same body/key to receive an idempotent replay; reuse the
key with a different body to receive a conflict Problem response.
