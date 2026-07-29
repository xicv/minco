# Bounded real-AWS smoke and cleanup

This run is the fidelity gate after unit, PostgreSQL, browser, SAM and Rustack
checks. It is intentionally disposable and must use an approved non-root
development deploy role/profile. The direct runner uses an existing development
`SecureString`; the root bootstrap wrapper creates a run-owned one from an
explicit Minco database source or creates a disposable database boundary. This
is not a production deployment recipe. Do not reuse a profile scoped or named
for another application.

When the approved account only has a root login, the root bootstrap wrapper
creates a run-scoped bootstrap user that can assume exactly one temporary IAM
role. It validates the role and assumption policies with IAM Access Analyzer,
uses an isolated one-hour role session for every application resource, and
deletes the access key, user, role, profiles and credential files after cleanup:

```bash
MINCO_DATABASE_URL_FILE=/absolute/path/to/mode-0600-minco-database-url \
AWS_REGION=ap-southeast-2 \
./scripts/aws/run-bounded-root-bootstrap.sh
```

The database source may instead be an explicitly selected Minco
`MINCO_DATABASE_URL_SOURCE_PARAMETER` or the process-only
`MINCO_DATABASE_URL` environment variable. The wrapper checks PostgreSQL
connectivity, copies the value into a new run-owned SSM `SecureString` without
placing it on the command line, and removes that parameter during the same
run. One isolated source profile reads the bootstrap user's credentials from a
mode-`0600` temporary process file, and the deploy profile reads the assumed
role session from a second such file. Neither modifies the global AWS CLI
configuration or role cache. Root access keys are never created. Root is used
only for caller verification, temporary user/role lifecycle, an explicitly
selected source-parameter read, and bootstrap teardown. The user access key is
long-lived by AWS credential type but short-lived operationally and can only
call `sts:AssumeRole` on the exact run-owned role.

When no PostgreSQL URL exists, the same wrapper can create a disposable
single-AZ RDS PostgreSQL test boundary:

```bash
MINCO_CREATE_TEMP_RDS=true \
AWS_REGION=ap-southeast-2 \
./scripts/aws/run-bounded-root-bootstrap.sh
```

This path creates an encrypted 20 GiB `db.t4g.micro` instance with no backups,
snapshots, Performance Insights, Multi-AZ or deletion protection. AWS manages
the random master password. The database is publicly reachable only for the
explicit migration, only from the current operator IPv4 `/32`, and only with
TLS `verify-full`; that ingress and the public address are removed before the
Lambda deployment. The runtime uses a security-group-to-security-group
PostgreSQL rule and an exact-resource SSM interface endpoint in an isolated VPC,
with no NAT Gateway. The regional RDS CA bundle is hashed into the exact Lambda
ZIP. Cleanup deletes the database, managed secret, endpoint and VPC; deleting
the owned database is the proof that all synthetic rows are gone.

## Before the first account call

1. Run the scoped Rust tests, ShellCheck, static validation, SAM lint and
   `scripts/dev/rustack-smoke.sh`.
2. Confirm the target Region. For the direct runner, confirm the existing
   absolute SSM parameter name. For the bootstrap wrapper, select exactly one
   documented Minco database source or set `MINCO_CREATE_TEMP_RDS=true`.
3. Do not retrieve or print a parameter value during discovery. For the direct
   runner, use `scripts/aws/inspect-account.sh` to retain caller identity and
   SecureString metadata under ignored `target/minco/aws/<run-id>/`.
4. Check that no intended stack or temporary bucket already exists. The deploy
   script treats access errors and ambiguous responses as failures, not as
   proof of absence.

## Bounded execution

`scripts/aws/run-bounded-smoke.sh` performs these ordered stages:

1. build the native ARM64 ZIP locally;
2. record STS identity and parameter metadata without its value;
3. resolve an exact customer-managed KMS key ARN only when the parameter does
   not use `aws/ssm`;
4. retrieve the database URL into process memory and run the explicit migration
   without logging the value;
5. create a temporary Cognito Lite pool, immutable permission attribute,
   non-secret client and synthetic user;
6. render and hash a release with that temporary issuer/audience;
7. create a private SSE-S3 artifact bucket;
8. upload the verified release and retain an unexecuted CloudFormation change
   set;
9. require the create-only review gate, allow only the eight expected
   SAM-transformed resource types and execute it;
10. verify candidate liveness, database readiness, unauthenticated rejection,
    authenticated place/get, idempotent replay, native ARM64 runtime and exact
    Lambda `CodeSha256`;
11. seal those redacted results into the hosted report and terminal successful
    deployment receipt;
12. approve the exact report digest, require a one-stage-only CloudFormation
    update, route live traffic to the verified numeric version, and retain the
    terminal promotion receipt.

Each top-level AWS CLI, SAM, API Gateway HTTP and external PostgreSQL action is
appended to `cloud-touches.jsonl`. Arguments described as redacted are never
written to that journal. Short-lived passwords and JWTs are held only in
mode-`0600` temporary files or process memory and are removed before completion.
The disposable stack, bucket and Cognito pool are tagged with the run ID. The
bucket also expires run-prefixed objects and incomplete uploads after one day
as a fallback; normal cleanup still deletes it immediately.

## Failures exercised and permanent fixes

The first production-shaped run intentionally remained fail-closed and exposed
several boundaries that local emulation could not prove:

- AWS root identities cannot call `AssumeRole`. The bootstrap now creates a
  minimal temporary user that can assume only the exact run-owned role; root
  access keys are never created.
- IAM user inline policies have a 2,048-character aggregate quota. Application
  permissions live on the temporary role; the user holds only the exact
  `sts:AssumeRole` grant.
- New IAM users, keys, policies and trust principals are eventually consistent.
  Bootstrap retries only the reviewed propagation errors, with bounded attempts
  and one journal entry per call.
- A PostgreSQL URI in `PGDATABASE` is treated as a database name by the local
  `psql`. The common helper converts a URL read through stdin into quoted libpq
  conninfo held only in process memory, keeping passwords out of argv.
- The generic RDS `available` waiter can return before a public-access change is
  applied. The database gate polls until status is `available`,
  `PubliclyAccessible` is false and no public-access change is pending.
- API Gateway stage tagging reports authorization as
  `apigateway:TagResource`, while IAM Access Analyzer rejects that explicit
  action. CloudTrail records the actual operation as tagged `CreateStage`, and
  API Gateway V2 authorizes it as `apigateway:POST` on
  `/apis/${ApiId}/stages`. The role keeps general stage mutation behind the
  CloudFormation caller chain and separately permits only that tagged create
  when the run ID, managed and purpose request tags are present and every
  requested key is in the closed reviewed allowlist.
- CloudFormation can delete an `--on-failure DELETE` stack before a later
  diagnostic pass. A failed create waiter now captures stack events before
  cleanup so the initial failure remains attributable without another cloud
  query.
- A successful create response can be lost before its resource identifier is
  saved. Cleanup rediscovers only deterministic IAM names and Cognito pool
  names whose managed, purpose and run-ID tags all match, then deletes every
  key on the exact temporary user before deleting that user.
- Recovery never deletes a merely name-matching stack, pool or SSM parameter;
  all three ownership tags must match. Current S3 general-purpose bucket
  creation accepts tags atomically, so `CreateBucket` supplies all three tags,
  IAM requires those request tags and cleanup has no untagged-bucket exception.
- The run role keeps regional discovery read-only. API Gateway and general VPC
  mutations require an AWS CloudFormation forward-access session; direct
  Cognito and security-group mutations require the exact run ownership tags.
- RDS-managed secret creation and tagging require an RDS forward-access
  session. Direct retrieval, description and orphan cleanup additionally
  require the RDS owning-service tag and an
  `aws:rds:primaryDBInstanceArn` tag equal to the exact disposable database,
  preventing the temporary role from reading another RDS credential.
- Cleanup ignores a prior RDS marker only when its complete absence proof is
  already all true, preventing a pre-IAM failure from invoking a nonexistent
  profile.
- A VPC Lambda can recreate its log group after CloudFormation deletes the
  explicit resource. Cleanup first proves the function absent, then deletes
  only the exact log-group name when needed and polls for absence.

The ignored cloud journal and the owning task retain the failed attempts,
per-attempt cleanup and final successful proof. Do not erase failed-run
evidence when recovering a bounded run.

## Cleanup and proof

Cleanup runs on success, interruption and failure:

1. fetch the database URL without printing it;
2. delete only the synthetic idempotency row and order ID, then prove the order
   count is zero;
3. delete and wait for the CloudFormation stack;
4. delete the temporary Cognito pool, synthetic user and client;
5. empty and delete the temporary S3 artifact bucket;
6. prove the stack, HTTP API, Lambda function and execution role are absent,
   delete only the exact Lambda log group if the VPC teardown recreated it,
   then prove the log group, Cognito pool and bucket are absent;
7. delete and prove absence of a run-owned SSM parameter, or capture external
   parameter metadata again and byte-compare it with the pre-run metadata;
8. when the root bootstrap wrapper was used, delete and prove absence of its
   temporary IAM access key, bootstrap user, role, isolated profiles and both
   credential files;
9. when the temporary RDS path was used, delete and prove absence of its
   database instance, managed secret, endpoint, VPC and local secret files.

The final `cleanup.json` and, when applicable, `bootstrap-cleanup.json` must
contain only `true` values. If a fail-closed cleanup requires recovery, preserve
the original documents and require a separate `final-cleanup.json` containing
only `true` values. Release, change-set, hosted verification, promotion,
runtime, HTTP and cleanup details remain
in the ignored run directory for local audit. Never commit account IDs, ARNs,
parameter names, URLs, tokens, passwords or database values.

The explicit schema migration is release state and is intentionally retained;
cleanup does not drop shared tables or migration history.

If automatic cleanup reports any `false` value, rerun `scripts/aws/cleanup.sh`
with the original `MINCO_AWS_RUN_ID`, stack name, bucket name, parameter name
and Region. Do not start another smoke run until the original cleanup passes.
An owned parameter is deliberately retained when synthetic database-row
cleanup cannot be proven, so the exact run can be recovered; it is deleted only
after that database boundary is clean.
