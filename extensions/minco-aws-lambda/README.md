# minco-aws-lambda

Native AWS Lambda integration for Minco Axum applications.

The crate adapts an Axum router to `lambda_http`, maps API Gateway v2 JWT claims
into Minco's provider-neutral `Principal`, and loads named SSM parameters with
decryption. IAM policy generation remains in the deployment-plan layer.

```rust,no_run
use axum::Router;

# async fn run(router: Router) -> anyhow::Result<()> {
minco_aws_lambda::run_router(router).await
# }
```
