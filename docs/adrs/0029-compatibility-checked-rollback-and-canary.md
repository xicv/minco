# ADR-0029: Compatibility-checked rollback and alarm-guarded API canaries

- Status: Accepted
- Date: 2026-08-03

## Context

An older artifact is not automatically safe merely because its bytes still
exist. Current clients, configuration, infrastructure, migrations, persisted
data, and workers may have moved forward. Conversely, routing every release
immediately from zero to all live traffic discards a useful AWS-native safety
boundary.

Minco already seals releases, verifies the candidate Lambda alias, and promotes
only an exact published version through a routing-only CloudFormation change
set. Rollback and canary behavior must preserve those guarantees without adding
fixed compute, hidden schedules, provisioned concurrency, invented reverse SQL,
or a second deployment model.

## Decision

### Rollback is an assessment before an exact promotion

`cargo minco rollback` is a local, non-mutating compatibility assessment. It
loads successful current and target promotion receipts and follows their
digest-bound deployment receipts to the two sealed release manifests. The
current external contract is the baseline and the older target contract is the
candidate.

The assessment emits one of:

- `compatible` when every assessed boundary has exact compatible evidence;
- `operator_decision_required` when structural evidence cannot prove runtime or
  semantic safety;
- `incompatible` when a boundary is known to break.

It reports separate, ordered reasons for environment, contract, configuration,
deployment resources, migration/seed catalogs, exact applied database-plan
bindings, persisted data, API routing, and worker artifacts. A persisted-data
decision is a strict JSON file bound to both release IDs, a reviewer, a reason,
and an explicit `compatible` or `incompatible` decision. An arbitrary file
digest cannot authorize rollback.

The command never rebuilds, replans, reverses SQL, or repairs data. A historical
hosted report cannot be reused as proof of the current `candidate` alias. Once
the result is `compatible`, the exact older release is deployed without a
rebuild or replan so its artifact becomes a new candidate version, hosted
verification is repeated against that candidate, and the existing `cargo minco
promote` boundary routes it. Promotion still accepts only one property update
to `LiveFunctionAlias`.

Workers are explicit: their SQS event sources remain attached to the current
unqualified worker functions. Different worker artifacts therefore require an
operator decision; API rollback never silently rewires or replays a worker.

### Canary is opt-in and API-only

A persistent deployment target may declare a `canary` policy with:

- an initial traffic percentage from 1 through 50;
- a monitoring window from 1 through 180 minutes;
- one through five sorted CloudWatch metric-alarm ARNs in the exact target
  account and Region;
- fixed `weighted_live_alias` API routing;
- fixed `preserve_current_event_sources` worker behavior;
- no provisioned concurrency.

`cargo minco promote --dry-run --canary` is non-contacting qualification. Live
execution reuses the exact hosted candidate report, function version, artifact
digest, reviewed target, caller, and clean-drift checks used by immediate
promotion.

The controller derives a temporary routing template from the exact packaged
release template by adding one concrete `AdditionalVersionWeights` entry to
`LiveFunctionAlias`. It creates an update change set with the exact CloudWatch
rollback triggers and monitoring window. Provider review must still report one
ordinary property modification to the live alias and nothing else. The
controller writes an immutable `started` canary receipt before execution.

Minco first requires every declared metric alarm to exist exactly and be `OK`, and
requires both Lambda versions to have the same execution role and dead-letter
configuration.
CloudFormation then monitors those alarms while the candidate receives live
traffic. After a successful window, Minco verifies the exact base version,
candidate version, and weight on the alias, then re-reads every exact metric
alarm and requires `OK`. Missing or `INSUFFICIENT_DATA` post-traffic evidence
reverses. It then applies a second
routing-only change set from the original packaged template to restore the
previous unweighted alias, verifies that restoration, and records the canary
receipt `succeeded`. The ordinary exact-artifact promotion then moves all live
traffic. If CloudFormation rolls back, Minco records `reversed` only after it
observes the previous unweighted alias. Missing or insufficient alarm evidence
fails closed.

This adds no Minco-managed resource and no fixed or idle compute. Existing
CloudWatch alarms are externally managed and may have account-specific charges,
so canary pricing remains explicitly incomplete.

Composite alarms are excluded from this first contract. CloudFormation requires
the distinct `AWS::CloudWatch::CompositeAlarm` rollback-trigger type, while a
CloudWatch alarm ARN does not encode that type. A future typed alarm descriptor
can add composite support without weakening the sealed plan or receipt.

## Consequences

- Rollback has an honest semantic/data decision point rather than a misleading
  “old deployment exists” pass.
- Migration safety remains forward-operational; no arbitrary down migration is
  synthesized.
- Canary traffic is visible in CloudFormation, guarded by reviewed alarms, and
  removed before final promotion.
- API and worker versions can intentionally differ during rollback/canary
  review; the report makes this visible.
- A first canary requires an already anchored numeric live version. The initial
  `candidate` sentinel must first complete an ordinary promotion.
- CloudWatch alarm selection and cost stay with the application operator rather
  than becoming default framework infrastructure.

## AWS references

- [Lambda alias routing configuration](https://docs.aws.amazon.com/lambda/latest/dg/configuring-alias-routing.html)
- [CloudFormation Lambda alias](https://docs.aws.amazon.com/AWSCloudFormation/latest/TemplateReference/aws-resource-lambda-alias.html)
- [CloudFormation rollback configuration](https://docs.aws.amazon.com/AWSCloudFormation/latest/APIReference/API_RollbackConfiguration.html)
