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

Plan IR schema 2 can add workers, queues, mappings, DLQs and schedules, but only
when each is explicitly declared. Queues use server-side encryption, mappings
use partial-batch responses and bounded concurrency, and schedules render as
explicit EventBridge Scheduler events. The default
`deny_scheduled_wakeups = true` policy still rejects an enabled schedule.

## Authentication

The generated HTTP API supports a generic JWT authorizer. API Gateway verifies
the token and `minco-aws-lambda` maps authorizer claims into the provider-neutral
`Principal`. Application use cases enforce permissions and business scope.
Public health routes explicitly opt out of the default authorizer.

The checked-in development config uses `.invalid` issuer and origin placeholders
and is safe for planning only. `deploy.sh` rejects the placeholder JWT issuer.
Render the release from environment-specific reviewed values before deployment;
replace the CORS origin before any browser-facing release.

## Secret flow

The template accepts the **name** of an existing SSM `SecureString`. The Lambda
role receives `ssm:GetParameter` for that exact parameter. The AWS-managed
`aws/ssm` key needs no wildcard identity-policy grant. When a customer-managed
key protects the parameter, deployment supplies its exact key ARN and the
generated policy restricts `kms:Decrypt` to that key, SSM in the selected
Region, and the parameter ARN encryption context. At startup,
`minco-aws-lambda` loads the value with decryption. The template, plan, release
manifest and evidence journal contain no secret value.

## Explicit stages

```bash
# Build once from a reviewed environment config, then lint and hash the inputs
./scripts/aws/build-release.sh path/to/reviewed.minco.toml

# Run database migration separately
DATABASE_KIND=postgres MIGRATION_DATABASE_URL='postgresql://...' cargo minco db migrate

# Promote the already verified release
MINCO_STACK_NAME=minco-dev \
MINCO_DATABASE_URL_PARAMETER=/minco/dev/database-url \
MINCO_AWS_ARTIFACT_BUCKET=minco-dev-artifacts-unique \
MINCO_RELEASE_MANIFEST=target/minco/release.json \
MINCO_AWS_RUN_ID=reviewed-run-id \
MINCO_AWS_EXECUTE_CHANGESET=yes \
AWS_REGION=ap-southeast-2 \
./scripts/aws/deploy.sh
```

`deploy.sh` never builds or replans. It verifies the release, refuses an
existing stack or artifact bucket, creates a blocked and encrypted temporary S3
bucket, asks SAM to create but not execute a change set, retains the change-set
description, requires an explicit execution acknowledgement, and then executes
that exact create-only change set.

For the disposable development proof, use:

```bash
MINCO_DATABASE_URL_PARAMETER=/minco/dev/database-url \
AWS_REGION=ap-southeast-2 \
./scripts/aws/run-bounded-smoke.sh
```

If only an approved root login exists, use the bounded bootstrap wrapper with
a Minco-specific PostgreSQL URL source. It creates a temporary bootstrap user
that can assume exactly one bounded non-root role, uses an isolated one-hour
role session and run-owned `SecureString`, then deletes the access key, user,
role, profiles and credential files:

```bash
MINCO_DATABASE_URL_FILE=/absolute/path/to/mode-0600-minco-database-url \
AWS_REGION=ap-southeast-2 \
./scripts/aws/run-bounded-root-bootstrap.sh
```

If no development database exists, set `MINCO_CREATE_TEMP_RDS=true` instead.
That bounded harness briefly creates a minimal encrypted RDS PostgreSQL
instance, migrates it over a single trusted `/32`, makes it private before
runtime verification, and deletes the database and isolated VPC afterward. It
does not change the default production topology or add a NAT Gateway. It never
creates a root access key, grants application permissions directly to the
bootstrap user, or writes temporary credentials to the default AWS CLI
configuration.

The bounded runner builds before creating resources, performs the explicit
migration, uses a temporary Cognito Lite user pool and ten-minute synthetic
identity to prove authenticated order operations, verifies the deployed Lambda
ZIP digest, and always invokes cleanup. Cognito is test harness infrastructure;
it is not part of the default application topology. See
[`real-aws-smoke.md`](real-aws-smoke.md) for the evidence and recovery contract.

## Database boundary

The SAM renderer accepts externally provisioned PostgreSQL-compatible profiles
(Neon, self-hosted, RDS and Aurora) because the runtime adapter is SQLx
PostgreSQL. DynamoDB and mutable SQLite are rejected by this renderer until an
appropriate runtime adapter/deployment plugin is selected. See
[`database-options.md`](database-options.md).

The schema 2 compatibility and migration procedure is documented in
[`plan-schema-v2-migration.md`](plan-schema-v2-migration.md).
