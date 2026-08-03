# Compare explicit PostgreSQL profiles

Minco keeps one PostgreSQL adapter boundary while exposing provider-specific
correctness, connection, wake, and cost assumptions. Neon, Aurora Serverless v2,
fixed RDS, and self-hosted PostgreSQL are choices—not transparent aliases.

## Features

Enable `sqlx-postgres`. Do not enable `sqlx-sqlite` unless the application
deliberately supports both adapters and their shared behavioral contract.

## Provider assumptions

Compilation and structural cost inspection are local. Real adapter integration
tests run only when `MINCO_ORDERS_TEST_POSTGRES_URL` identifies an explicitly
approved disposable database. The recipe runner removes that variable so its
default proof cannot contact a database accidentally.

## Cost and wake behavior

Neon and eligible Aurora profiles can expose `zero_compute` while retaining
`storage_only` dimensions. I/O and HTTP are `request_only`. Fixed RDS and a
self-hosted server retain `fixed_monthly` capacity. Current regional prices,
eligibility, quotas, backup policy, and wake latency remain external evidence.

```bash
cargo test --locked -p orders-adapters --no-default-features --features postgres
cargo minco cost --config examples/orders/config/minco.neon-launch.toml --json
cargo minco cost --config examples/orders/config/minco.aurora-serverless-v2.toml --json
cargo minco cost --config examples/orders/config/minco.rds-postgres.toml --json
cargo minco cost --config examples/orders/config/minco.dynamodb.toml --json
bash scripts/test/sqlx_feature_isolation.sh
```

The DynamoDB comparison is intentionally diagnostic: it shows request/storage
cost classes while warning that DynamoDB is not a relational PostgreSQL adapter.

## Verification

The runner executes `orders-postgres`, `cost-neon`, `cost-aurora`, `cost-rds`,
`cost-dynamodb`, and `sqlx-isolation`.

## Unsupported gates

No provider is contacted. A structural or example price result is not financial
approval, production database qualification, migration approval, or a claim
that the selected service is available in an account and Region.
