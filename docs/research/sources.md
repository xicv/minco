# Research Sources and Snapshot Policy

This scaffold was reconciled against primary project/provider documentation on
2026-07-23. Version and pricing data are intentionally dated; rerun
`minco update check` and review provider pricing before a production estimate.

Key primary sources:

- Rust 1.97.1 release notes: https://doc.rust-lang.org/stable/releases.html
- Axum repository and documentation: https://github.com/tokio-rs/axum
- SQLx documentation: https://docs.rs/sqlx/
- AWS Lambda Rust runtime: https://github.com/aws/aws-lambda-rust-runtime
- AWS Lambda Rust packaging: https://docs.aws.amazon.com/lambda/latest/dg/rust-package.html
- AWS API Gateway pricing: https://aws.amazon.com/api-gateway/pricing/
- AWS Lambda pricing: https://aws.amazon.com/lambda/pricing/
- AWS RDS pricing: https://aws.amazon.com/rds/pricing/
- AWS DynamoDB pricing: https://aws.amazon.com/dynamodb/pricing/
- Aurora Serverless v2 auto-pause: https://docs.aws.amazon.com/AmazonRDS/latest/AuroraUserGuide/aurora-serverless-v2-auto-pause.html
- Neon pricing and connection pooling: https://neon.com/pricing and https://neon.com/docs/connect/connection-pooling
- Jujutsu working copies/workspaces: https://github.com/jj-vcs/jj/blob/main/docs/working-copy.md
- Jujutsu Git compatibility: https://github.com/jj-vcs/jj/blob/main/docs/git-compatibility.md
- Rustack: https://github.com/tyrchen/rustack
- Laravel, Echo, Encore, Easegress, Loco and Pavex repositories cited in
  `open-source-influences.md`.

Minco does not scrape mutable prices during normal builds. Approved snapshots
belong in `pricing/catalog.toml`, include source dates, and are treated as inputs
to an estimate rather than timeless constants.
