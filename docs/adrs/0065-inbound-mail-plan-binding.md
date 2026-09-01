# ADR 0065: The inbound mail chain is an explicit plan binding

## Status

Accepted.

## Context

Stage D2's inbound path is proven live (ADR-0064) but the provider-side
chain — SES receiving rule, raw-MIME bucket, bucket notification, wake
queue, worker event source — existed only in prose. The continuation
prompt requires the Plan to render it explicitly with IAM, cost and wake
sources, with no real provider mutation in the PR.

## Decision

1. A dedicated sidecar, `minco_plan::inbound_mail`, mirrors the
   durable-work pattern: an explicit `InboundMailTopology` (one
   `InboundMailBinding` per mailbox: id, mailbox scope, bucket name, key
   prefix, retention days, worker function, queue, batching) synthesizes
   the wake queue and the worker's SQS trigger into the ordinary plan
   collections and records the bindings on `DeploymentPlan.inbound_mail`
   — the wake sources are first-class plan data.
2. `apply` is idempotent; `validate` fails closed with stable
   `MINCO-MAIL-*` codes: disabled topologies must declare nothing,
   binding ids and wake queues are unique, the worker must exist with the
   worker role and not be bound to another SQS trigger, and mailbox,
   bucket name, key prefix and retention are bounded.
3. SAM renders the full provider chain per binding: the raw bucket
   (public-access blocked, `ObjectCreated:*` notification to the wake
   queue with a key-prefix filter, lifecycle expiry at the retention
   bound), a bucket policy granting only `ses.amazonaws.com`
   `s3:PutObject`, an S3-to-SQS queue policy conditioned on the bucket
   ARN, and the SES receipt rule set/rule with content scanning disabled
   (the raw MIME is authoritative — ADR-0055). The worker function
   policy gains read-only `s3:GetObject` on the raw bucket — SES is the
   only writer.
4. Cost is stated as explicit per-binding assumptions
   (`estimate_inbound_mail_cost`: one S3 PUT and one SQS send per mail,
   one S3 GET and one receive set per wake attempt, storage volume per
   10k mails, retention bound) rather than invented prices, matching the
   database-profile rule of exposing assumptions.

## Consequences

- `cargo minco deploy plan` consumers can render and structurally review
  the exact chain that was proven live in ADR-0064; nothing here contacts
  a provider.
- `sam validate --lint` could not run in this workspace (the `sam` CLI is
  unavailable); the rendered template is written to a temp artifact for
  external validation and the gap is recorded, not converted to a pass.
- Wake queues deliberately have no DLQ by default: SQS redelivery is the
  retry authority (ADR-0060); an operator can add one via the base plan
  queues section when a workload needs it.

## Amendment (2026-09-01, M14-T74 stabilization reviews 5057195399,
5060065907, 5064401898 and 5072859042)

The original decision above is superseded in four places by the
stabilization reviews; the original text is retained for history.

1. **The bindings are NOT stored on `DeploymentPlan`.** Point 1's
   `DeploymentPlan.inbound_mail` field was a SemVer-major break against
   the published 1.x API (an exhaustively-constructible public struct
   gained a field) and was removed. `InboundMailTopology` remains an
   explicit sidecar in the durable-work sense: `apply` projects into
   the EXISTING queues/triggers collections, `validate`,
   `estimate_inbound_mail_cost` and `render_sam_with_inbound_mail`
   receive the topology explicitly, and a downstream witness test
   constructs `DeploymentPlan` with the full published v1.12 struct
   literal to keep it that way.
2. **Content scanning is ENABLED (`ScanEnabled: true`).** The rendered
   SES receipt rule enables spam/virus scanning; production consumers
   additionally configure a `ScanVerdictPolicy` (`require_clean`) so a
   missing or malformed `X-SES-Spam-Verdict`/`X-SES-Virus-Verdict` is
   quarantined rather than silently passing.
3. **Every wake queue carries a dead-letter queue.** The original "no
   DLQ by default" consequence is reversed: each synthesized wake queue
   redrives to its paired `<queue>-dlq` after a bounded max-receive
   count, with visibility derived from the bound worker's timeout
   (six-fold plus batching window), so exhausted notifications are
   inspectable rather than lost.
4. **`sam validate --lint` runs and passes.** The SAM CLI became
   available (`uv tool run --from aws-sam-cli sam`, 1.165.0); the gate
   is wired into `scripts/test/inbound_mail_template_parse.py` and it
   caught and closed a real E3004 circular dependency.

Additionally (review 5072859042) the sidecar owns its resources under an
exact-shape contract: a same-ID queue, DLQ or trigger is reused only
when semantically identical (FIFO, visibility, retention, DLQ,
max-receive, function, batch/window, partial-batch reporting and
concurrency all compared); a mismatch, a competing second consumer on
the wake queue, or binding ids collapsing to one CloudFormation logical
id are stable `MINCO-MAIL-014…018` diagnostics and the renderer refuses
the plan. The shared SES receipt rule set is named from the application,
environment and an order-independent digest of the binding set — never
from the first binding — so reordering bindings cannot replace the
provider rule set.

## Amendment (2026-09-02, M14-T74 stabilization review 5083559431)

The provider dependency graph and multi-binding contracts were
completed; the prior text stands with these refinements authoritative:

- **Clean-create ordering**: the S3-to-SQS queue policy builds its
  `aws:SourceArn` from the EXPLICIT configured bucket name (never
  `!GetAtt` the bucket resource), so the graph is
  Queue → QueuePolicy → Bucket(+Notification) → BucketPolicy →
  ReceiptRule; the bucket `DependsOn` the queue policy because S3
  validates the notification destination's permission at
  notification-apply time. The SES receipt rule references the rule
  set with `!Ref` (a real CloudFormation dependency; identical literal
  strings create none) and `DependsOn` the SES-write bucket policy.
  A rendered-graph regression proves acyclicity and the provider
  order.
- **Full-identity rule-set name**: the digest covers the FULL
  application, environment, region and sorted binding set (canonical
  length-framed), so visible-prefix truncation can never collide two
  deployments; the 64-character budget counts every separator.
- **Physical ingress ownership**: one normalized mailbox routes to
  exactly one binding and one physical bucket belongs to one binding
  (duplicate recipients would silently fan one mail into multiple
  buckets/wakes/projects; duplicate bucket names cannot deploy).
  Shared-mailbox fan-out requires an explicit future model.
