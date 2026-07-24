# Release and Promotion

A Minco release manifest includes:

- immutable source commit;
- Lambda ZIP path, size, and SHA-256;
- OpenAPI path, size, and SHA-256;
- deployment Plan IR path, size, and SHA-256;
- rendered deployment template path, size, and SHA-256;
- Cargo.lock path, size, and SHA-256;
- ordered migration files and migration-set digest;
- Rust and Minco toolchain versions.

Create and verify:

```bash
cargo minco release create \
  --artifact target/lambda/orders-lambda/bootstrap.zip \
  --plan infra/aws/generated/plan.json \
  --template infra/aws/generated/template.yaml \
  --output target/minco/release.json

cargo minco release verify target/minco/release.json
```

The manifest stores repository-relative paths, so a clean checkout can verify
the same release without retaining the builder's absolute filesystem paths.
Promotion selects a verified manifest and deploys its exact ZIP and rendered
template. It never replans or recompiles source in staging or production.
