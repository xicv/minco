# Release and Promotion

A Minco release manifest includes:

- immutable source commit;
- Lambda ZIP path, size, and SHA-256;
- OpenAPI path, size, and SHA-256;
- deployment Plan IR path, size, and SHA-256;
- Cargo.lock path, size, and SHA-256;
- ordered migration files and migration-set digest;
- Rust and Minco toolchain versions.

Create and verify:

```bash
cargo minco release create \
  --artifact target/lambda/orders-lambda/bootstrap.zip \
  --deployment-plan infra/aws/generated/plan.json \
  --migrations examples/orders/migrations/postgres \
  --output target/minco/release.json

cargo minco release verify target/minco/release.json
```

Promotion selects a verified release manifest and deploys the same artifact. It never
recompiles source in staging or production.
