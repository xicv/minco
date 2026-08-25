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
