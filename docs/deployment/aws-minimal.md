# Minimal AWS Deployment

## Default topology

```text
Internet
  -> API Gateway HTTP API
      -> native ARM64 Rust Lambda ZIP
          -> external/serverless PostgreSQL
```

Supporting resources are the Lambda role, bounded CloudWatch log group, exact
CORS configuration, SSM parameter-name reference and optional JWT authorizer.
The default profile does not create a VPC, NAT Gateway, RDS Proxy, ECR,
CloudFront, Cognito, queue, scheduler or provisioned concurrency.

## Authentication

The generated HTTP API supports a generic JWT authorizer. API Gateway verifies
the token and `minco-aws-lambda` maps authorizer claims into the provider-neutral
`Principal`. Application use cases enforce permissions and business scope.
Public health routes explicitly opt out of the default authorizer.

## Secret flow

The template accepts the **name** of an existing SSM `SecureString`. The Lambda
role receives scoped `ssm:GetParameter` plus KMS decryption through SSM. At
startup, `minco-aws-lambda` loads the parameter with decryption and supplies the
runtime database URL. The template, plan and release manifest contain no secret
value.

## Explicit stages

```bash
# Generate/inspect before mutation
./scripts/aws/plan.sh examples/orders/config/minco.dev.toml

# Build and validate
./scripts/aws/build-lambda.sh
./scripts/aws/validate.sh

# Run database migration separately
DATABASE_KIND=postgres DATABASE_URL='postgresql://...' cargo minco db migrate

# Deploy exact built artifact
MINCO_STACK_NAME=minco-dev \
MINCO_DATABASE_URL_PARAMETER=/minco/dev/database-url \
AWS_REGION=ap-southeast-2 \
./scripts/aws/deploy.sh
```

`deploy.sh` is intentionally mutating and requires explicit environment values.
Production account/role approval, change-set review, backup/restore evidence and
hosted smoke verification remain release responsibilities.

## Database boundary

The SAM renderer accepts externally provisioned PostgreSQL-compatible profiles
(Neon, self-hosted, RDS and Aurora) because the runtime adapter is SQLx
PostgreSQL. DynamoDB and mutable SQLite are rejected by this renderer until an
appropriate runtime adapter/deployment plugin is selected. See
[`database-options.md`](database-options.md).
