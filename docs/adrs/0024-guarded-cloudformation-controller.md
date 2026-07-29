# ADR 0024: Guarded CloudFormation change-set controller

## Status

Accepted

## Context

Plan IR, deterministic SAM rendering and release manifests already establish
what Minco intends to deploy. The remaining AWS scripts could still package,
create and execute a change set inside one shell flow. That made the provider
preview difficult to bind to an exact approval and left account, role, drift
and migration evidence outside the immutable deployment boundary.

CloudFormation change sets distinguish creation from execution and report
resource actions plus replacement behavior. SAM packaging uploads local
artifacts and emits a transformed template but does not apply it. Existing
stack drift detection is asynchronous and reports only explicitly modelled
properties, so a controller must wait for a completed `IN_SYNC` result without
claiming that it proves runtime behavior.

Current provider behavior was checked on 2026-07-28 against the AWS CLI
references for
[`create-change-set`](https://docs.aws.amazon.com/cli/latest/reference/cloudformation/create-change-set.html),
[`describe-change-set`](https://docs.aws.amazon.com/cli/latest/reference/cloudformation/describe-change-set.html),
[`detect-stack-drift`](https://docs.aws.amazon.com/cli/latest/reference/cloudformation/detect-stack-drift.html),
[`get-caller-identity`](https://docs.aws.amazon.com/cli/latest/reference/sts/get-caller-identity.html)
and
[`sam package`](https://docs.aws.amazon.com/serverless-application-model/latest/developerguide/sam-cli-command-reference-sam-package.html).
`CreateChangeSet` accepts `ChangeSetType`, but the documented
`DescribeChangeSet` response does not return it. The controller therefore
binds the provider response to the type established by its guarded stack-state
inspection and rejects any optional provider value that contradicts that
expected type.

CloudFormation also propagates three provider-owned tags to supported
resources: `aws:cloudformation:stack-name`, `aws:cloudformation:stack-id` and
`aws:cloudformation:logical-id`. A bounded `aws:TagKeys` policy must account
for those keys without allowing target configuration to set arbitrary
provider-reserved tags.

API Gateway V2 authorizes tagged stage creation as `apigateway:POST` on the
`/apis/${ApiId}/stages` collection. Its CloudFormation provider can report a
dependent `TagResource` denial even though CloudTrail records only the
`CreateStage` request, with its tags, against that stage collection. That
authorization does not carry the CloudFormation `aws:CalledVia` context used
by the generic mutation statement. The specialized stage-create statement is
therefore bounded by the exact run-ownership request tags and closed tag-key
allowlist rather than that absent caller-chain key. See the AWS
[`CreateStage` authorization mapping](https://docs.aws.amazon.com/service-authorization/latest/reference/list_apigatewayv2.html)
and
[`tagging IAM examples`](https://docs.aws.amazon.com/apigateway/latest/developerguide/apigateway-tagging-iam-policy.html).

## Decision

`minco-deploy-aws` owns strict deployment-target parsing, environment guards,
provider change classification and a digest-sealed change-set receipt.
`cargo minco deploy changeset`:

1. verifies the exact release and source revision;
2. requires a reviewed, enabled account, Region, environment, role, stack,
   pre-existing artifact bucket and SSM parameter **name**;
3. verifies the current STS caller and either proves the stack is new or waits
   for clean drift on a stable existing stack;
4. packages only the release-bound SAM template and artifacts;
5. creates, but does not execute, a deterministic CloudFormation change set
   with reserved release-identity tags plus validated, non-secret target stack
   tags;
6. binds the response to the already-guarded create/update type, then
   classifies additions, ordinary modifications, replacements, deletions,
   imports and indeterminate/provider-sync actions while discarding parameter
   and property values;
7. writes an immutable receipt binding the release, target configuration,
   packaged template, drift evidence and provider identifiers.

`cargo minco deploy apply` is a separate mutation. It requires the exact
change-set receipt digest as approval, a source-matching migration plan and a
successful verified migration receipt. It re-verifies every bound file,
current source, STS caller, target state, drift and the provider's still
available change set. Import, dynamic and provider-sync actions are rejected.
Before `execute-change-set`, it persists a deployment receipt in `started`
state, binding both database evidence and the change-set receipt. Execution or
wait failure makes that receipt terminal `failed`; successful infrastructure
apply deliberately leaves it `started` for hosted verification to complete in
M10-T03.

Both commands expose a local `--dry-run`. Dry-run never calls AWS, uploads an
artifact, creates a receipt or mutates infrastructure. There are no
dirty-source, drift, role, migration or approval bypass flags.

## Consequences

- Review and apply are separate commands with separate exact-digest approvals.
- Provider change details can contain secret values, but Minco deserializes
  only structural resource fields and never serializes those details.
- The artifact bucket is lifecycle infrastructure and must already exist; the
  controller does not silently provision it.
- Existing stacks must be stable and `IN_SYNC`; new stacks record drift as not
  applicable rather than fabricating a clean result.
- CloudFormation completion is infrastructure evidence only. Runtime,
  contract, authentication and artifact-identity proof remain M10-T03 work.
- The bounded AWS harness may approve a create-only receipt only after its
  explicit resource-type allowlist passes, then invokes the separate apply
  phase with that exact digest.

## Compatibility

The new `minco-deploy-aws` crate, target-catalog schema, change-set receipt and
deployment CLI are pre-1.0 serialized/API additions and part of the likely
Minco `0.4.0` compatibility boundary.

## Safety

Target configuration contains identifiers and secret names only. Process
errors are bounded, provider response values are discarded, all AWS calls use
the exact configured Region, and source identity is rechecked immediately
before mutation. No command claims that a change set, drift result or completed
stack proves application correctness. Target stack tags cannot replace Minco's
reserved release tags or use the provider-reserved `aws:` prefix. The bounded
AWS rehearsal keeps general API Gateway mutations behind the CloudFormation
caller chain. Its separate tagged-stage authorization permits `POST` only on
the stage collection when the three exact run-ownership tag values are present
and every requested key is in the reviewed run, release, SAM and
CloudFormation system-key allowlist.
