# Preview Verified Review Loop

Minco previews are short-lived AWS deployments for getting a working release in
front of a reviewer early. They use the same immutable release, guarded
CloudFormation controller, and hosted verification boundary as other
environments. Preview lifecycle metadata adds ownership, a bounded TTL, an
exact resource/retention inventory, cost classes, Feedback references, and
guarded cleanup. It does not add a Minco-hosted control plane.

```text
Plan IR
  -> exact release and change set
  -> apply and hosted verification
  -> immutable review manifest
  -> reviewer Feedback references
  -> local cleanup dry-run
  -> exact review-digest approval
  -> standard CloudFormation deletion
  -> absence-verified cleanup receipt
```

## Configure a preview target

A target must be named `preview` or use the `preview-` prefix and declare its
lifecycle explicitly. Checked-in targets should stay disabled until account,
Region, role, bucket, parameter name, and stack identity have been reviewed.

```toml
[environments.preview]
enabled = false
lifecycle = "preview"
expected_account_id = "000000000000"
expected_region = "ap-southeast-2"
expected_role_arn = "arn:aws:iam::000000000000:role/minco-preview"
stack_name = "minco-orders-preview"
artifact_bucket = "minco-preview-artifacts-placeholder"
database_url_parameter_name = "/minco/preview/database-url"

[environments.preview.preview]
owner = "orders-team"
ttl_seconds = 86400
pricing_complete = false

[[environments.preview.preview.resources]]
logical_id = "ApiFunction"
resource_type = "AWS::Lambda::Function"
retention = "delete"
idle_cost_class = "request_only"

[[environments.preview.preview.resources]]
logical_id = "ReviewUploads"
resource_type = "AWS::S3::Bucket"
retention = "retain"
idle_cost_class = "storage_only"
```

The checked-in resource list is a reviewed baseline. After deployment,
`deploy review` reads the processed CloudFormation template and provider stack
inventory and records every generated resource as well. A provider resource
missing from the template, a type mismatch, an unstable resource, or an
unsupported retention policy fails closed. Unknown resource types use the
conservative `fixed_monthly` cost class and keep pricing incomplete.

Inspect the repository-native plan without contacting AWS:

```bash
cargo minco --json deploy plan --environment preview --stdout
```

The preview section declares the owner, TTL, target, resource retention, cost
classes, pricing confidence, and optional cleanup schedule. With the default
policy it contains no schedule and `scheduled_wakeups` remains empty.

## Create an exact review manifest

First use the ordinary deployment workflow to package once, create and approve
the exact change set, apply migrations separately, apply the stack, and finish
hosted verification. Then inspect the review operation locally:

```bash
cargo minco --json deploy review \
  --environment preview \
  --manifest target/minco/release.json \
  --deployment-receipt target/minco/deployment-receipt.json \
  --dry-run
```

The dry-run performs no AWS call and writes nothing. It reports disabled or
missing-evidence blockers. With an enabled, reviewed target and complete
evidence, create the manifest:

```bash
cargo minco --json deploy review \
  --environment preview \
  --manifest target/minco/release.json \
  --deployment-receipt target/minco/deployment-receipt.json \
  --feedback 'feedback-019fa123=0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef' \
  --output target/minco/review.json
```

The live command contacts AWS read-only to recheck caller account/role, stack
identity and stability, termination-protection state, processed template, and
the exact provider resource inventory. It then writes one immutable manifest.
It does not accept Feedback content as instructions: only stable IDs and
lowercase SHA-256 digests are bound.

The digest-derived `review_id` binds:

- source revision, release manifest, release digest, and every artifact;
- application/environment/Region plus exact account, role, stack, and target
  configuration digest;
- one successful deployment receipt and one matching change-set receipt;
- owner, creation time, expiry, resource cost/retention, and pricing confidence;
- verification and delivery-trace file digests; and
- untrusted Feedback IDs and content digests.

Expiry is visibility, not deletion authority. Reaching the timestamp neither
contacts AWS nor authorizes cleanup.

## Review cleanup before applying it

Always start with the local dry-run:

```bash
cargo minco --json destroy \
  --environment preview \
  --review target/minco/review.json \
  --dry-run
```

It contacts no provider, writes no receipt, and lists the exact resources that
CloudFormation will delete or retain. It also reports blockers such as a
persistent target, disabled target, invalid review, mismatched review target,
missing approval, or an existing receipt.

Approval is the exact manifest digest, supplied separately after inspecting
the dry-run:

```bash
review_digest="$(jq -er '.manifest_digest' target/minco/review.json)"
cargo minco --json destroy \
  --environment preview \
  --review target/minco/review.json \
  --receipt target/minco/cleanup-receipt.json \
  --approve-review-digest "$review_digest"
```

Before mutation, Minco re-verifies the complete evidence chain, current source,
target configuration digest, AWS caller account/role, Region, exact stack,
stable status, provider inventory, and processed retention policies.
Termination protection must be explicitly proved disabled; Minco never disables
it. Cleanup uses CloudFormation `STANDARD` deletion only, never force deletion.
It persists a started receipt before calling AWS, waits for deletion, and marks
success only after `DescribeStacks` proves the stack absent. A failed start,
wait, or absence check produces a terminal failed receipt. Concurrent terminal
receipt writers fail closed.

Persistent targets, including production and ordinary staging targets, can be
shown by dry-run but can never pass the `target_not_preview` guard. Tags are
metadata only and are never accepted as deletion authority.

Retained resources survive stack deletion and can continue to incur storage,
request, or fixed charges. The cleanup receipt lists them explicitly; their
later removal requires a separate application-owned procedure.

## Optional one-time scheduling

Automatic cleanup is opt-in and visible in Plan IR:

```toml
[environments.preview.preview.cleanup_schedule]
expression = "at(2026-08-04T00:00:00)"
action_after_completion = "delete"
residual_resources = ["ReviewUploads"]
manual_fallback = "cargo minco destroy --environment preview --dry-run"
```

The application must explicitly allow scheduled wakeups. Only a one-time
`at(...)` expression is valid, residual resources must exactly match retained
resources, and a manual fallback is required. Minco records the wakeup and its
cost implications but does not synthesize or host a deletion controller.

For an application-owned EventBridge Scheduler integration,
`ActionAfterCompletion=DELETE` removes the schedule after its final target
invocation. It does **not** delete the target preview stack. The target still
has to execute the same exact-review and cleanup guard contract, and operators
must retain the manual fallback.

## Evidence boundaries

| Evidence | Proves | Does not prove |
|---|---|---|
| Preview Plan IR | declared lifetime, cost/retention intent, optional wakeup | a deployed environment |
| Deployment and hosted verification receipts | exact release reached and passed the candidate checks | reviewer acceptance |
| Review manifest | exact review identity and provider inventory at creation | future provider state or deletion authority |
| Cleanup dry-run | local blast radius and current evidence blockers | AWS state or approval |
| Cleanup receipt | attempted outcome and, on success, observed stack absence | deletion of explicitly retained resources |
