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
MINCO_REHEARSAL_AUTHORITY_FILE=/absolute/path/to/reviewed-authority.json \
MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST=REVIEWED_SHA256 \
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
MINCO_REHEARSAL_AUTHORITY_FILE=/absolute/path/to/reviewed-authority.json \
MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST=REVIEWED_SHA256 \
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
2. Create a local, uncommitted authority document using the exact schema below.
   Review it independently, then pass its SHA-256 as the separate approval.
   Missing, stale, tampered, extended or mismatched authority fails before a
   build or AWS command.
3. Confirm the target Region. For the direct runner, confirm the existing
   absolute SSM parameter name. For the bootstrap wrapper, select exactly one
   documented Minco database source or set `MINCO_CREATE_TEMP_RDS=true`.
4. Do not retrieve or print a parameter value during discovery. After authority
   approval, the direct runner may use `scripts/aws/inspect-account.sh` to
   verify only the exact account, role and parameter. It retains booleans and
   non-identifying parameter properties, not account IDs, ARNs or names, under
   ignored `target/minco/aws/<run-id>/`.
5. Check that no intended stack or temporary bucket already exists. The deploy
   script treats access errors and ambiguous responses as failures, not as
   proof of absence.

The authority file is deliberately local because it contains account, role,
profile and database-boundary identifiers. Never commit or copy it into run
evidence. The runner retains only a redacted receipt with the approval digest,
source revision, scope IDs, time/spend ceilings and approval window.

```json
{
  "schema_version": 1,
  "authority_kind": "minco.aws-controller-rehearsal.v1",
  "run_id": "reviewed-run-id",
  "source_revision": "EXACT_40_OR_64_HEX_TASK_HEAD",
  "expected_account_id": "REVIEWED_12_DIGIT_NONPROD_ACCOUNT",
  "expected_region": "ap-southeast-2",
  "expected_role_arn": "arn:aws:iam::REVIEWED_ACCOUNT:role/EXACT_ROLE",
  "aws_profile": "exact-reviewed-profile",
  "environment": "dev",
  "database_boundary": {
    "mode": "existing-ssm-secure-string",
    "parameter_name": "/minco/rehearsal/database-url",
    "parameter_owned": false,
    "instance_owned": false
  },
  "resource_allowlist": "bounded-direct-smoke-v1",
  "cleanup_blast_radius": "cleanup-bounded-direct-smoke-v1",
  "max_duration_minutes": 60,
  "max_spend_usd": 25,
  "approved_by": "release-owner",
  "approved_at": "2026-08-03T10:00:00Z",
  "expires_at": "2026-08-03T11:00:00Z"
}
```

For root bootstrap, `aws_profile` is the exact root bootstrap profile and
`expected_role_arn` is the deterministic `MincoSmoke-<run-hash>` role that the
wrapper will create. Use `run-owned-ssm-copy` with the exact selected source and
temporary parameter for an existing database, or this boundary for the
disposable database path:

```json
{
  "mode": "disposable-rds",
  "rds_stack_name": "minco-rds-RUN_HASH",
  "instance_id": "minco-RUN_HASH",
  "parameter_name": "/minco/smoke/RUN_HASH/database-url"
}
```

The parent prior → current → prior controller must not reuse the single-source
authority above. Its local approval uses the separate strict kind
`minco.aws-multi-release-controller-rehearsal.v1`, two distinct exact source
revisions and one fixed release sequence:

```json
{
  "schema_version": 1,
  "authority_kind": "minco.aws-multi-release-controller-rehearsal.v1",
  "run_id": "reviewed-multi-release-run-id",
  "source_revisions": {
    "current": "EXACT_CURRENT_40_OR_64_HEX_HEAD",
    "prior": "EXACT_PRIOR_40_OR_64_HEX_HEAD"
  },
  "release_sequence": ["prior", "current", "prior"],
  "expected_account_id": "REVIEWED_12_DIGIT_NONPROD_ACCOUNT",
  "expected_region": "ap-southeast-2",
  "expected_role_arn": "arn:aws:iam::REVIEWED_ACCOUNT:role/EXACT_ROLE",
  "aws_profile": "exact-reviewed-profile",
  "environment": "dev",
  "database_boundary": {
    "mode": "existing-ssm-secure-string",
    "parameter_name": "/minco/rehearsal/database-url",
    "parameter_owned": false,
    "instance_owned": false
  },
  "resource_allowlist": "bounded-multi-release-smoke-v1",
  "cleanup_blast_radius": "cleanup-bounded-multi-release-smoke-v1",
  "max_duration_minutes": 60,
  "max_spend_usd": 25,
  "approved_by": "release-owner",
  "approved_at": "2026-08-03T10:00:00Z",
  "expires_at": "2026-08-03T11:00:00Z"
}
```

The validator rejects identical revisions, any runtime/document revision
mismatch, any sequence other than `prior`, `current`, `prior`, unknown fields
and mismatched database, resource or cleanup profiles. The root-bootstrap
equivalents are
`bounded-root-multi-release-smoke-v1` with
`cleanup-bounded-root-multi-release-smoke-v1`, or
`bounded-root-temp-rds-multi-release-v1` with
`cleanup-bounded-root-temp-rds-multi-release-v1`. This contract qualifies no
provider call by itself; the unfinished parent controller must still verify both
canonical checkout roots and all exact release evidence before mutation.

The closed scope profiles mean:

| Authority profile | Provider/resource boundary | Cleanup boundary |
| --- | --- | --- |
| `bounded-direct-smoke-v1` | One run-tagged artifact bucket and Cognito harness; one create-only CloudFormation stack/change set containing the API, stages, Lambda function/version/aliases/permissions, execution role and log group; metadata/value access to the exact external SSM parameter; explicit migration and synthetic requests against the exact external PostgreSQL boundary. | Synthetic rows, stack resources, Cognito harness and artifact bucket only. The external parameter must byte-match its before metadata; schema migration history is retained. |
| `bounded-root-bootstrap-v1` | Direct scope plus one deterministic temporary IAM user/key/inline policy, one temporary assume-role session/profile and one run-owned SSM copy of the reviewed PostgreSQL source. | Direct cleanup plus the exact temporary parameter, IAM user/key/policies/role and isolated local credential/config files. |
| `bounded-root-temp-rds-v1` | Root-bootstrap scope plus one encrypted single-AZ RDS instance and managed secret, and one isolated VPC with its subnets, routes, internet gateway, security groups and exact SSM VPC endpoint; no NAT Gateway. | Root-bootstrap cleanup plus the run-owned database/secret, RDS stack, endpoint, network resources, CA/database files and synthetic data. |
| `bounded-multi-release-smoke-v1` | Direct scope retained across the fixed prior → current → prior release sequence; one shared run-owned stack, bucket and identity harness, with only release-bound versions, aliases and change sets added or updated. | Direct cleanup after every phase has finished or failed; no inner phase may independently remove the shared boundary. |
| `bounded-root-multi-release-smoke-v1` | Root-bootstrap scope plus the fixed multi-release sequence in the one shared stack. | Root-bootstrap and multi-release cleanup under one parent trap. |
| `bounded-root-temp-rds-multi-release-v1` | Temporary-RDS root scope plus the fixed multi-release sequence in the one shared stack and database. | Temporary-RDS, root-bootstrap and multi-release cleanup under one parent trap. |

Each resource profile has the corresponding `cleanup-<resource-profile>` ID
shown by the examples. The
validator accepts only these fixed pairings and exact database shapes. Duration
is limited to 60 minutes and checked before every journalled AWS, SAM or external
database touch; after expiry, only cleanup calls may continue. The spend value
is an operator-approved ceiling limited to USD 25, not a live billing alarm;
the closed resource profile, duration boundary and bounded call loops are the
enforced controls that keep this disposable rehearsal within that ceiling.

Validate locally, inspect the document, then approve its exact bytes:

```bash
authority=/absolute/path/to/reviewed-authority.json
approval_digest="$(shasum -a 256 "$authority" | awk '{print $1}')"
jq . "$authority"
MINCO_REHEARSAL_AUTHORITY_FILE="$authority" \
MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST="$approval_digest" \
scripts/aws/validate-rehearsal-authority.sh \
  "$authority" "$approval_digest" \
  reviewed-run-id EXACT_SOURCE_REVISION ap-southeast-2 \
  exact-reviewed-profile dev \
  '{"mode":"existing-ssm-secure-string","parameter_name":"/minco/rehearsal/database-url","parameter_owned":false,"instance_owned":false}' \
  bounded-direct-smoke-v1 cleanup-bounded-direct-smoke-v1
```

For a multi-release document, validate both ordered revisions explicitly:

```bash
scripts/aws/validate-multi-release-rehearsal-authority.sh \
  "$authority" "$approval_digest" reviewed-multi-release-run-id \
  EXACT_PRIOR_SOURCE_REVISION EXACT_CURRENT_SOURCE_REVISION \
  ap-southeast-2 exact-reviewed-profile dev \
  '{"mode":"existing-ssm-secure-string","parameter_name":"/minco/rehearsal/database-url","parameter_owned":false,"instance_owned":false}' \
  bounded-multi-release-smoke-v1 \
  cleanup-bounded-multi-release-smoke-v1
```

Before implementing or authorizing provider execution, render the closed local
phase plan from the two exact checkouts and the same reviewed authority:

```bash
MINCO_PRIOR_ROOT=/absolute/path/to/prior-clean-checkout \
MINCO_CURRENT_ROOT=/absolute/path/to/current-clean-checkout \
MINCO_MULTI_RELEASE_EVIDENCE_ROOT=/absolute/path/outside/checkouts/target/minco/aws/reviewed-multi-release-run-id \
MINCO_REHEARSAL_AUTHORITY_FILE="$authority" \
MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST="$approval_digest" \
MINCO_AWS_RUN_ID=reviewed-multi-release-run-id \
MINCO_REHEARSAL_PROFILE=exact-reviewed-profile \
AWS_REGION=ap-southeast-2 \
MINCO_REHEARSAL_DATABASE_BOUNDARY_JSON='{"mode":"existing-ssm-secure-string","parameter_name":"/minco/rehearsal/database-url","parameter_owned":false,"instance_owned":false}' \
MINCO_REHEARSAL_RESOURCE_ALLOWLIST=bounded-multi-release-smoke-v1 \
MINCO_REHEARSAL_CLEANUP_BLAST_RADIUS=cleanup-bounded-multi-release-smoke-v1 \
  scripts/aws/plan-multi-release-rehearsal.sh \
  >/absolute/path/outside/both/checkouts/multi-release-plan.json
```

Planning is local-only and reports `external_aws_contact: false`. It rejects a
relative, missing, symlinked, dirty or duplicated checkout, resolves both roots
canonically, rejects nested directories without their own Git or JJ metadata,
and requires their current revisions to match the authority. It also binds one
new canonical absolute whole-run evidence root outside both source checkouts;
the final path components must be `target/minco/aws/<run-id>`. Keeping evidence
outside the checkouts prevents phase 1 from making the exact sources dirty
before phase 2. The plan fixes one
shared stack lifecycle (`create`, `update`,
`update`, `delete`), one whole-run artifact bucket, three unique evidence
namespaces and exactly one parent-owned cleanup trap. The rollback phase reuses
the exact prior release from the initial phase only after an exact current-to-
prior compatibility assessment returns `compatible`. It fixes `build: false`,
`replan: false` and historical hosted-report reuse to false, but still requires
a fresh hosted verification and promotion. Every phase evidence namespace is
create-only.
Each phase also names one closed CloudFormation review policy. The initial
phase uses `bounded_create_v1`, preserving the existing eight-resource
create-only allowlist. The current and rollback phases use
`bounded_release_update_v1`: additions, removals and replacements are confined
to generated `ApiFunctionVersion*` resources, while ordinary modifications are
confined to the candidate `ApiFunction` and `ApiFunctionAliascandidate`. Imports,
indeterminate actions, provider metadata sync, IAM/API expansion and any
deployment-phase mutation of `LiveFunctionAlias` fail closed. Live routing
remains exclusively owned by the separately approved promotion operation.
Every admitted resource change must also retain the exact serialized action,
replacement, policy-action and scope shape; an incomplete or retention-bearing
receipt cannot pass merely because its logical ID is allowlisted.
The provider scripts distinguish the shared resource run ID from an optional
phase evidence ID. `MINCO_AWS_RUN_ID` remains the value in ownership tags,
resource names and cloud-journal records. `MINCO_AWS_EVIDENCE_ID` selects the
project-relative `target/minco/aws/<evidence-id>` chain used by release,
deployment, verification and promotion receipts. It defaults to the run ID for
standalone compatibility, must satisfy the same bounded-name policy, and does
not broaden resource ownership. A multi-release parent assigns three distinct
evidence IDs while retaining one resource run ID; in particular the two prior
phases cannot overwrite one another's receipts.
Write the output outside both checkouts so shell redirection does not make a
checkout dirty before validation. The output contains local absolute paths and
is an operator preflight, not a redacted publication artifact or authority to
run AWS commands.

Before handing one planned phase to a future parent controller, hash the exact
whole-run plan and project only its fixed phase:

```bash
plan=/absolute/path/outside/both/checkouts/multi-release-plan.json
plan_digest="$(shasum -a 256 "$plan" | awk '{print $1}')"
MINCO_MULTI_RELEASE_PLAN_FILE="$plan" \
MINCO_APPROVE_MULTI_RELEASE_PLAN_DIGEST="$plan_digest" \
MINCO_REHEARSAL_AUTHORITY_FILE="$authority" \
MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST="$approval_digest" \
MINCO_MULTI_RELEASE_PHASE_ID=03-prior-rollback \
  scripts/aws/plan-multi-release-phase.sh \
  >/absolute/path/outside/evidence/03-prior-rollback-phase.json
```

Phase projection is also local-only. It revalidates the schema-closed plan, its
exact digest, the original authority and digest, both clean source revisions,
parent cleanup ownership, and the create-only phase evidence destination. A
digest-matched plan with broader policy is still rejected. The projected
rollback handoff carries the compatible-only assessment and exact current and
target promotion phases, retains `build: false`, `replan: false`, exact phase-1
artifact reuse, rejects historical hosted-report reuse, and requires fresh
verification and promotion. Write the projection
outside its evidence namespace: shell redirection happens before validation.
This handoff does not execute, authorize, or contact AWS.

Initialize the parent controller only from the exact current checkout bound by
the plan:

```bash
cd /absolute/path/to/current-clean-checkout
MINCO_MULTI_RELEASE_PLAN_FILE="$plan" \
MINCO_APPROVE_MULTI_RELEASE_PLAN_DIGEST="$plan_digest" \
MINCO_REHEARSAL_AUTHORITY_FILE="$authority" \
MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST="$approval_digest" \
  scripts/aws/initialize-multi-release-rehearsal.sh
```

Initialization is still provider-free. It runs all three phase projections
again from the exact current controller checkout before creating evidence. It
then atomically publishes one mode-`0700` whole-run evidence root with
mode-`0600` control files:

- the byte-exact whole-run plan;
- a redacted authority receipt without the account, role, profile or database
  parameter name;
- one digest-bound projection for each fixed phase; and
- an immutable `initialized` controller receipt whose digest binds the exact
  plan, authority, source revisions, phase order and projections.

The receipt starts every phase at `pending`, names `01-prior-initial` as the
only next phase, records the shared stack and artifact bucket as `not_created`,
and keeps cleanup `pending` with `parent_controller`, `required: true` and
`trap_count: 1`. Initialization does not create any phase evidence namespace,
install a cleanup trap, build an artifact, contact AWS or authorize later
execution. A second initialization is rejected without changing the sealed
evidence. Running the initializer from a checkout other than the exact planned
current root is also rejected.

After reviewing the immutable initialized receipt, claim its exact first phase
from the same current controller checkout:

```bash
evidence_root=/absolute/path/outside/checkouts/target/minco/aws/reviewed-multi-release-run-id
controller_digest="$(
  jq -er '.receipt_digest' "$evidence_root/control/controller-receipt.json"
)"
MINCO_MULTI_RELEASE_EVIDENCE_ROOT="$evidence_root" \
MINCO_APPROVE_MULTI_RELEASE_CONTROLLER_RECEIPT_DIGEST="$controller_digest" \
MINCO_REHEARSAL_AUTHORITY_FILE="$authority" \
MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST="$approval_digest" \
MINCO_MULTI_RELEASE_PHASE_ID=01-prior-initial \
  scripts/aws/begin-multi-release-phase.sh
```

This transition is also provider-free. It revalidates the still-current
authority, exact clean source roots, initialized controller receipt and every
sealed control file. Only the initialized `next_phase` is accepted. The
command rejects broadened permissions, unsealed directory entries, projection
tampering, code outside the planned current checkout and a pre-existing phase
boundary. It atomically publishes the whole `phases/01-prior-initial` tree with
mode-`0700` directories and mode-`0600` files, containing the exact projection
and a schema-closed `started` receipt. A failed publish or repeated invocation
leaves prior evidence unchanged. The receipt binds the controller, plan,
authority, source revision, create review policy and parent cleanup ownership,
but records `external_aws_contact: false`; it neither installs the cleanup trap
nor authorizes or performs a provider call.

Before adding provider work to the parent process, exercise its exact
validation-only lifecycle from the same current controller checkout:

```bash
phase_start_digest="$(
  jq -er '.receipt_digest' \
    "$evidence_root/phases/01-prior-initial/phase-start-receipt.json"
)"
MINCO_MULTI_RELEASE_EVIDENCE_ROOT="$evidence_root" \
MINCO_APPROVE_MULTI_RELEASE_CONTROLLER_RECEIPT_DIGEST="$controller_digest" \
MINCO_APPROVE_MULTI_RELEASE_PHASE_START_RECEIPT_DIGEST="$phase_start_digest" \
MINCO_REHEARSAL_AUTHORITY_FILE="$authority" \
MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST="$approval_digest" \
MINCO_MULTI_RELEASE_PHASE_ID=01-prior-initial \
  scripts/aws/run-multi-release-parent-session.sh
```

This command is deliberately not a deployment runner. It revalidates the
private, schema-closed controller, every sealed projection, the byte-exact
phase handoff, the current authority window and both clean source revisions.
It then installs the parent lifecycle trap and publishes immutable mode-`0600`
`parent-session-start-receipt.json` and
`parent-session-completion-receipt.json` files. The terminal receipt binds the
exact start-receipt digest. Both receipts fix execution to
`validation_only`, record `provider_state: not_entered` and
`external_aws_contact: false`; the terminal cleanup action is therefore
`none_provider_boundary_not_entered`, not provider cleanup proof. A wrong
approval, altered or broadly accessible evidence, code outside the exact
current checkout, injected state or repeated session fails before creating a
receipt. Interruption after the start receipt never fabricates a terminal
validated receipt.

The first provider boundary is a separate deterministic plan. Render it from
the same exact current checkout before authorizing any AWS contact:

```bash
provider_entry_plan=/absolute/path/outside/checkouts/provider-entry-plan.json
MINCO_MULTI_RELEASE_EVIDENCE_ROOT="$evidence_root" \
MINCO_APPROVE_MULTI_RELEASE_CONTROLLER_RECEIPT_DIGEST="$controller_digest" \
MINCO_APPROVE_MULTI_RELEASE_PHASE_START_RECEIPT_DIGEST="$phase_start_digest" \
MINCO_REHEARSAL_AUTHORITY_FILE="$authority" \
MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST="$approval_digest" \
MINCO_MULTI_RELEASE_PHASE_ID=01-prior-initial \
MINCO_MULTI_RELEASE_EXECUTION_MODE=provider_identity_preflight \
MINCO_MULTI_RELEASE_PROVIDER_ACTION=plan \
  scripts/aws/run-multi-release-parent-session.sh >"$provider_entry_plan"
provider_entry_digest="$(
  shasum -a 256 "$provider_entry_plan" | awk '{print $1}'
)"
```

Planning repeats the complete local controller validation, contacts no
provider, requests no secret, creates no lifecycle receipt and fixes the only
permitted provider action to read-only `sts get-caller-identity` in the
authority's exact Region. Review and approve the byte-exact plan digest
separately, then execute only that identity preflight:

```bash
MINCO_MULTI_RELEASE_EVIDENCE_ROOT="$evidence_root" \
MINCO_APPROVE_MULTI_RELEASE_CONTROLLER_RECEIPT_DIGEST="$controller_digest" \
MINCO_APPROVE_MULTI_RELEASE_PHASE_START_RECEIPT_DIGEST="$phase_start_digest" \
MINCO_REHEARSAL_AUTHORITY_FILE="$authority" \
MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST="$approval_digest" \
MINCO_MULTI_RELEASE_PHASE_ID=01-prior-initial \
MINCO_MULTI_RELEASE_EXECUTION_MODE=provider_identity_preflight \
MINCO_MULTI_RELEASE_PROVIDER_ACTION=execute \
MINCO_APPROVE_MULTI_RELEASE_PROVIDER_ENTRY_DIGEST="$provider_entry_digest" \
  scripts/aws/run-multi-release-parent-session.sh
```

Execution re-renders and verifies the approved plan before publishing the
start receipt. It captures the identity fields before authority validation,
byte-checks the redacted receipt, then rehashes the authority document before
claiming lifecycle evidence; a concurrent authority replacement therefore
fails before AWS contact, and the mutable document is not read again at the
provider boundary. It makes one fixed STS identity call with the authority's
exact profile and Region, normalizes an assumed-role caller and compares
account and role to the validated values without serializing the identity.
Success publishes `provider_identity_verified`; a provider response or identity failure
publishes a conservative `failed` terminal receipt with
`external_aws_contact: true`. Both terminal states say
`none_read_only_identity_preflight`: no resource was created, no mutation or
secret was requested, and therefore no cloud cleanup was performed or proved.
A missing or wrong provider-entry digest fails before receipts and AWS.

For the `bounded-root-temp-rds-multi-release-v1` profile, the next read-only
boundary proves every deterministic shared resource name is absent before
creation. Render its separate plan from a fresh initialized first-phase
boundary; a parent session remains create-only, so it cannot reuse the identity
preflight receipts above:

```bash
resource_preflight_plan=/absolute/path/outside/checkouts/resource-preflight-plan.json
MINCO_MULTI_RELEASE_EVIDENCE_ROOT="$evidence_root" \
MINCO_APPROVE_MULTI_RELEASE_CONTROLLER_RECEIPT_DIGEST="$controller_digest" \
MINCO_APPROVE_MULTI_RELEASE_PHASE_START_RECEIPT_DIGEST="$phase_start_digest" \
MINCO_REHEARSAL_AUTHORITY_FILE="$authority" \
MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST="$approval_digest" \
MINCO_MULTI_RELEASE_PHASE_ID=01-prior-initial \
MINCO_MULTI_RELEASE_EXECUTION_MODE=provider_resource_preflight \
MINCO_MULTI_RELEASE_PROVIDER_ACTION=plan \
  scripts/aws/run-multi-release-parent-session.sh >"$resource_preflight_plan"
resource_preflight_digest="$(
  shasum -a 256 "$resource_preflight_plan" | awk '{print $1}'
)"
```

The schema-closed plan contains no account, ARN, stack, bucket, database or
parameter identifier. It permits only STS identity verification followed by
read-only absence checks for the deterministic application stack, artifact
bucket, temporary database stack and database instance. Review and approve its
exact bytes, then execute:

```bash
MINCO_MULTI_RELEASE_EVIDENCE_ROOT="$evidence_root" \
MINCO_APPROVE_MULTI_RELEASE_CONTROLLER_RECEIPT_DIGEST="$controller_digest" \
MINCO_APPROVE_MULTI_RELEASE_PHASE_START_RECEIPT_DIGEST="$phase_start_digest" \
MINCO_REHEARSAL_AUTHORITY_FILE="$authority" \
MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST="$approval_digest" \
MINCO_MULTI_RELEASE_PHASE_ID=01-prior-initial \
MINCO_MULTI_RELEASE_EXECUTION_MODE=provider_resource_preflight \
MINCO_MULTI_RELEASE_PROVIDER_ACTION=execute \
MINCO_APPROVE_MULTI_RELEASE_RESOURCE_PREFLIGHT_DIGEST="$resource_preflight_digest" \
  scripts/aws/run-multi-release-parent-session.sh
```

A wrong plan digest fails before receipts or contact. Success requires exactly
five read-only calls and publishes `provider_resources_absent`; an existing
resource, unexpected provider response or identity mismatch publishes a
conservative `failed` receipt. Neither result authorizes creation, requests a
secret or claims cleanup. This resource preflight does not authorize phase
deployment. The next provider-capable slice must continue in the same parent
process, retain the shared stack, bucket and identity harness across all three
phases, write terminal phase evidence on every exit, and make the one parent
trap perform and verify cleanup.

The absence reads require AWS CLI structured JSON errors and accept only exit
code `254` with the documented service code: CloudFormation `ValidationError`,
S3 `404`, or RDS `DBInstanceNotFound`. English message text is never treated as
proof. An older CLI without `--cli-error-format json`, an access denial,
credential/configuration failure, throttling response or any changed error
shape therefore fails closed before creation.

The standalone application-stack, artifact-bucket and temporary-RDS-stack
preflight paths use the same shared verifier. Keep this common boundary in
place when adding parent execution; do not reintroduce service-message regular
expressions in a phase runner or cleanup proof.

Teardown and absence verification use that verifier too. CloudFormation, S3,
Cognito, Lambda, CloudWatch Logs, IAM, API Gateway, SSM, RDS, Secrets Manager
and EC2 not-found outcomes are accepted only as CLI service exit `254` with the
exact expected structured code. Bounded IAM and STS propagation retries also
key on a closed code set. Error message text is neither an ownership proof nor
an absence proof, and provider messages are not copied to public evidence.

## Multi-release rollback evidence

Keep the current and prior releases in separate clean exact-source checkouts.
Do not copy their manifest trees into one checkout, rewrite bound paths, or
reuse the prior hosted report. The local assessment accepts two explicit roots
so each release keeps its original repository-relative evidence chain:

```bash
cargo minco rollback \
  --current-root /absolute/path/to/current-clean-checkout \
  --target-root /absolute/path/to/prior-clean-checkout \
  --current-promotion target/minco/current/promotion-receipt.json \
  --target-promotion target/minco/prior/promotion-receipt.json \
  --data-compatibility-evidence target/minco/rollback-data-compatibility.json \
  --json
```

Both roots must be absolute, existing non-symlink directories and are resolved
to canonical paths. A complete assessment verifies both promotion, deployment
and release chains, and requires each checkout's current Git or JJ revision to
equal its sealed release source. The data decision is read from the command root
and must bind the two release IDs. The result is still non-mutating
qualification: the prior checkout must redeploy its exact artifact as a new
candidate, create a fresh deployment receipt and hosted report, and pass
ordinary `promote` again.

The current bounded runner remains single-release. Its create review uses the
same centralized policy evaluator carried by the phase plan, so future update
execution cannot substitute a broader ad hoc review. The local plan,
per-phase projections, atomic controller initialization and first-phase claim
now close the parent ownership, provenance and initial state-transition
contracts. The parent can cross only the separately digest-approved read-only
identity and disposable-resource absence boundaries; it cannot yet build,
deploy, verify, promote or clean shared multi-release resources. Do not disable
the single-release runner's immediate cleanup or create-only review gate
independently; that would leave a partially owned provider boundary rather than
a recoverable rehearsal.

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
8. wait with a bounded 404-only retry for that newly created bucket to become
   visible, then upload the verified release and retain an unexecuted
   CloudFormation change set;
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
- A fresh access key can resolve correctly through `GetCallerIdentity` and
  still return `InvalidClientTokenId` on the immediately following
  `AssumeRole`. Both identity verification and exact-role assumption retry only
  the reviewed fresh-key/authorization propagation errors, at most 15 times
  two seconds apart. Cleanup records whether the application runner was
  invoked, so a pre-application bootstrap failure is clean without weakening
  the all-true receipt requirement after invocation.
- A PostgreSQL URI in `PGDATABASE` is treated as a database name by the local
  `psql`. The common helper converts a URL read through stdin into quoted libpq
  conninfo held only in process memory, keeping passwords out of argv.
- The generic RDS `available` waiter can return before a public-access change is
  applied. The database gate polls until status is `available`,
  `PubliclyAccessible` is false and no public-access change is pending.
- API Gateway V2's service-authorization reference describes HTTP-method
  aliases for tagged stage creation, but CloudFormation's live IAM denial
  identifies the dependent action as `apigateway:TagResource` on
  `/apis/${ApiId}/stages`. IAM custom-policy simulation also accepts that
  exact semantic action/resource pair. Access Analyzer currently reports the
  literal `apigateway:TagResource` action as one `INVALID_ACTION`; the
  bootstrap tolerates only that exact finding at the exact statement index
  after structurally matching the statement's action, stage-collection
  resource, three ownership values and closed tag-key allowlist, confirming
  the action appears once and rejecting every action wildcard. Any other
  Analyzer error remains fatal. The role therefore keeps general mutation
  behind the CloudFormation caller chain and separately permits only
  `apigateway:POST` and `apigateway:TagResource` on the stage collection when
  the run ID, managed and purpose request tags are present and every requested
  key is in the closed reviewed allowlist. Direct `/tags/*` mutation remains
  denied.
- AWS SAM translator `1.111.0` treats an operation-level empty
  `security: []` as missing while applying `Auth.DefaultAuthorizer`, so a
  contract-public liveness route was transformed into a JWT-protected route.
  The renderer now declares the JWT authorizer without a SAM default and emits
  explicit `JwtAuthorizer` security on every protected operation while
  retaining `security: []` on public operations. This also keeps the stable
  alias Lambda integrations required for exact-artifact promotion;
  the event-level `Authorizer: NONE` override documented by AWS SAM is not
  applicable because these routes are defined inline rather than as function
  events.
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
- A successful run-owned bucket create can briefly precede visibility to a
  following `HeadBucket` when the cached release build reaches the deployment
  controller within seconds. The bounded smoke runner retries only `404`,
  `NoSuchBucket` and `Not Found` after public-access blocking and encryption,
  fails immediately for every other response, and stops after 15 attempts.
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
