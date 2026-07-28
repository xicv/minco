# Minimal AWS Deployment

## Default topology

```text
Internet
  -> API Gateway HTTP API
      -> candidate stage -> candidate Lambda alias
      -> live $default stage -> approved published Lambda version
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

# Plan and run database migration separately. The direct URL value is injected
# out of band into MINCO_MIGRATION_DATABASE_URL.
cargo minco db plan --set orders-postgres --json \
  > target/minco/orders-postgres-plan.json
MINCO_REVIEWED_MIGRATION_DIGEST="$(
  jq -r '.digest' target/minco/orders-postgres-plan.json
)"
cargo minco db migrate \
  --set orders-postgres \
  --database-url-env MINCO_MIGRATION_DATABASE_URL \
  --expected-plan-digest "$MINCO_REVIEWED_MIGRATION_DIGEST" \
  --receipt target/minco/orders-postgres-receipt.json

# Create an unexecuted change set using a reviewed, enabled target catalog.
# Its artifact bucket must already exist.
MINCO_DEPLOY_PHASE=changeset \
MINCO_DEPLOY_TARGET_CONFIG=path/to/reviewed-deployment-targets.toml \
MINCO_RELEASE_MANIFEST=target/minco/release.json \
MINCO_AWS_RUN_ID=reviewed-run-id \
MINCO_APPROVE_RELEASE_DIGEST="$(
  jq -er '.release_digest' target/minco/release.json
)" \
./scripts/aws/deploy.sh

# Inspect the change-set receipt, then apply that exact review only after the
# migration above succeeded.
MINCO_DEPLOY_PHASE=apply \
MINCO_DEPLOY_TARGET_CONFIG=path/to/reviewed-deployment-targets.toml \
MINCO_RELEASE_MANIFEST=target/minco/release.json \
MINCO_AWS_RUN_ID=reviewed-run-id \
MINCO_CHANGESET_RECEIPT=target/minco/aws/reviewed-run-id/change-set-receipt.json \
MINCO_MIGRATION_PLAN=target/minco/orders-postgres-plan.json \
MINCO_MIGRATION_RECEIPT=target/minco/orders-postgres-receipt.json \
MINCO_APPROVE_CHANGESET_DIGEST="$(
  jq -er '.receipt_digest' \
    target/minco/aws/reviewed-run-id/change-set-receipt.json
)" \
./scripts/aws/deploy.sh
```

See [`database-lifecycle.md`](database-lifecycle.md) for target status,
destructive-risk gates, locking, verification and receipt semantics.

`deploy.sh` never builds or replans and no longer provisions lifecycle
infrastructure. Its `changeset` phase delegates to the guarded controller and
stops after writing an immutable, redacted review receipt. Its separate `apply`
phase requires that receipt's exact digest plus the exact migration plan and
successful receipt. The controller rechecks caller identity, stack state,
drift, source and the provider change set before execution. A completed stack
is not hosted runtime proof; the deployment receipt remains pending until the
hosted verification phase.

After apply, run the configured hosted verification against the current
candidate stack output and then explicitly approve that report for routing:

```bash
cargo minco deploy verify \
  --manifest target/minco/release.json \
  --receipt target/minco/deployment-receipt.json \
  --output target/minco/hosted-verification.json

verification_digest="$(
  shasum -a 256 target/minco/hosted-verification.json | awk '{print $1}'
)"
cargo minco promote \
  --manifest target/minco/release.json \
  --receipt target/minco/deployment-receipt.json \
  --verification target/minco/hosted-verification.json \
  --approve-verification-digest "$verification_digest"
```

Use `deploy verify --dry-run` and `promote --dry-run` to inspect local blockers.
Both dry runs avoid AWS, HTTP calls, receipt transitions, rebuilds, and
replanning. The live command uses the original packaged template and refuses
any provider change beyond the exact live API Gateway stage property update.

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
identity to prove authenticated candidate operations, verifies the deployed
Lambda ZIP digest, promotes only that report-approved version through the
routing-only guard, and always invokes cleanup. Cognito is test harness
infrastructure; it is not part of the default application topology. See
[`real-aws-smoke.md`](real-aws-smoke.md) for the evidence and recovery contract.

## Database boundary

The SAM renderer accepts externally provisioned PostgreSQL-compatible profiles
(Neon, self-hosted, RDS and Aurora) because the runtime adapter is SQLx
PostgreSQL. DynamoDB and mutable SQLite are rejected by this renderer until an
appropriate runtime adapter/deployment plugin is selected. See
[`database-options.md`](database-options.md).

The schema 2 compatibility and migration procedure is documented in
[`plan-schema-v2-migration.md`](plan-schema-v2-migration.md).
