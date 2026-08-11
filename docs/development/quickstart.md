# Local Development Quickstart

## 1. Install required tools

Install Rust through Rustup; `rust-toolchain.toml` pins 1.97.1 with Rustfmt and
Clippy. Install uv 0.12.3, Jujutsu, and Git. Docker is optional for SQLite-only
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

## 6. Run the graph-selected environment

Inspect the complete service/lifecycle/process plan, then start it:

```bash
cargo minco dev --dry-run --json
cargo minco dev
```

The reference default selects PostgreSQL plus Rustack SSM/STS, applies the
declared migration and waits for API readiness. SQLite and port overrides are
explicit:

```bash
cargo minco dev --profile sqlite
cargo minco dev --port 31000 --rustack-port 4567
```

Ctrl-C stops process groups and selected containers together without resetting
volumes. See
[`local-development.md`](local-development.md) for workers, explicit seed and
frontend options, and [`../../infra/local/README.md`](../../infra/local/README.md)
for isolated database and Rustack conformance commands.

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
