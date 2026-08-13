# AWS plugin adapters

The `minco-aws-adapters` crate implements official provider-neutral ports
without placing an AWS SDK in Minco core, application, or domain crates.
Applications construct clients and adapters in their composition root.

## Explicit selection

Enable only the services used by the application:

```toml
minco-aws-adapters = { version = "0.6.0", features = ["s3", "sqs"] }
```

Register `AwsAdaptersPlugin` with the matching `AwsAdapterSelection` alongside
the official plugins and their AWS-backed services. This marker is mandatory
for AWS Plan/IAM derivation; memory/reference selections must not register it.

The marker declares:

- S3 object storage as storage-only;
- SQS, SES, and Cognito as provider-managed;
- static-site S3/CloudFront resources through the static-site plugin;
- no fixed compute, NAT Gateway, schedule, or provisioned concurrency.

## IAM

Call `runtime_iam_policy` with the validated application graph and exact
resource ARNs. It emits only actions required by selected AWS provider markers:

- object S3: get, put, delete under the configured prefix, plus prefix-bounded
  bucket listing so the adapter can distinguish a missing key from access denial;
- SQS: `SendMessage` on one queue;
- SES: `SendEmail` on one verified identity;
- Cognito: four administrative user operations on one user pool;
- static site: prefix-bounded list/put/delete and one distribution's
  `CreateInvalidation`.

Missing ARNs and unsafe prefixes are errors. Generated policies never substitute
`Resource: "*"`.

## Persistent stores

Run `migrate_plugin_storage` as an explicit release migration operation; do not
call it at Lambda startup. The migrations are embedded in each SQLx adapter and
use the dedicated `_minco_plugin_storage_migrations` version/checksum history
table, separate from application and Feedback migration histories.

- PostgreSQL supplies outbox, session, idempotency, and audit storage.
- Use `PostgresOutboxStore::enqueue_in` inside the same SQLx transaction as the
  domain write.
- PostgreSQL claims use `FOR UPDATE SKIP LOCKED`; dispatch and expired-lease
  recovery remain explicit application/operator actions.
- SQLite supplies session, idempotency, and audit storage for the local or
  single-writer profile. `BEGIN IMMEDIATE` plus a bounded busy timeout makes
  idempotency claims atomic under contention.

Only token hashes are stored. Idempotency completion and abort compare exact
lease IDs. Audit tables expose no update/delete API.

When the relational AWS profile enables the `audit.ledger` capability, the
generated SAM template requires both `DatabaseUrlParameterName` and
`AuditDatabaseUrlParameterName`. Each names an existing SSM `SecureString`;
the latter must contain a PostgreSQL URL for a physically separate audit
database. The Lambda loads both parameters at startup, and the generated role
grants `ssm:GetParameter` only for those exact names. Optional customer-managed
KMS key ARNs are likewise separate and constrained to SSM decrypts for their
matching parameter ARN. Secret values never enter Plan IR, SAM, manifests, or
environment configuration.

Do not point both parameters at the same database. SQL source mutations commit
their audit intent to the source-side journal, and the explicit bounded relay
delivers it to the separate append-only ledger. This avoids pretending that a
cross-database write can be atomic while keeping the user-facing mutation fast
and retryable.

## Local conformance

Run:

```bash
./scripts/dev/rustack-smoke.sh
```

The script creates a unique local Compose project, enables only
S3/SQS/SSM/STS, tests the real Minco SDK adapters, and cleans every emulator
resource on exit. Rustack does not emulate SES, Cognito, or CloudFront; those
remain bounded real-AWS checks.

No Rustack-specific types enter application code. The same SDK clients use
loopback HTTP endpoint overrides locally and AWS regional endpoints in
production. Custom non-loopback endpoints must use HTTPS; endpoint and queue
URLs reject userinfo and query components before the SDK sees them.

## Bounded real-AWS run

Use `scripts/aws/run-adapter-smoke.sh` only from an authenticated operator
session. The script:

1. creates a run-specific least-privilege role/profile and tagged resources;
2. appends every AWS or external HTTP touch to `cloud-touches.jsonl`;
3. exercises S3 POST enforcement, SQS, Cognito with invitation delivery
   suppressed, IAM validation, and CloudFormation validation;
4. uses SES's success simulator only when a verified sender already exists;
5. removes created users, pool, queue, objects, bucket, policies, role, and
   temporary credentials;
6. records final absence proofs and exits nonzero if cleanup is incomplete.

Never place database URLs, credentials, session tokens, presigned URLs, or SES
recipient data in the journal. The journal stores operation names, resource
identifiers, timestamps, result classes, and cleanup state only.

CloudFront distribution creation is deliberately separate from routine smoke:
it is slow, globally replicated, and can incur charges. The rendered private
OAC template is structurally tested locally and validated by CloudFormation;
an explicit release rehearsal owns any live distribution and its cleanup.
Managed custom-domain output includes both `A` and `AAAA` aliases when IPv6 is
enabled, and only the `A` alias when it is disabled.
Serialize publication per bucket/prefix: S3 synchronization is bounded to the
owned prefix but is not a distributed deployment lock.
