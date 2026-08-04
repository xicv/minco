---
id: M10-T08
title: Run a bounded real-AWS controller promotion and rollback rehearsal
milestone: M10
status: in_progress
priority: critical
area: deployment/aws/recovery
depends_on: [M10-T04, M10-T05, M10-T06, M10-T07]
operations: [getLive, getReady, placeOrder, getOrder]
owned_paths:
  - crates/minco-deploy-aws/**
  - crates/minco-release/**
  - crates/minco-cli/**
  - crates/minco-plan/**
  - docs/deployment/**
  - docs/reference/generated/cli.md
  - docs/reference/generated/diagnostics.md
  - infra/aws/**
  - scripts/aws/**
  - tasks/M10/M10-T08-real-aws-controller-rehearsal.md
  - verification/aws-rehearsal/**
  - verification/adoption-measurements.json
  - verification/deep-review.json
  - verification/source-manifest.json
  - verification/static-validation.json
checks:
  - ./scripts/quality.sh
  - scripts/aws/validate.sh
  - scripts/dev/rustack-smoke.sh
  - scripts/aws/run-bounded-root-bootstrap.sh
---

## Goal

Prove the M10 controller path in one disposable, approved non-production AWS
boundary: exact release and change-set review, apply, hosted verification,
promotion, compatibility-checked rollback to an exact prior artifact, fresh
verification, traffic restoration and complete teardown.

## Authority gate

`ready` records only that source dependencies are complete. Before the first
AWS API call, the operator must explicitly approve the exact non-production
account, Region, role/profile, environment, database boundary, resource
allowlist, maximum duration/spend and whole-run cleanup blast radius. An old
login or approval for another task is not authority for this rehearsal.

## Acceptance

- the exact task head passes complete local quality and hosted qualification
  before provider mutation;
- account, Region, role, environment, source, release, migration, change-set,
  destructive-action and operator-approval guards fail closed;
- one exact candidate is applied, passes all required hosted checks and is
  promoted without rebuild or replan;
- rollback assessment binds the exact current and prior releases and never
  promises SQL reversal or automatic data repair;
- the exact prior artifact is redeployed as candidate, receives a new hosted
  verification report and is promoted through the same guarded boundary;
- runtime identity, request IDs, artifact digests, receipts and provider touch
  classes are retained in redacted form without account IDs, ARNs, endpoints,
  parameter names, credentials, tokens or customer data;
- cleanup proves every run-owned compute, API, identity, storage, database,
  network, log and local credential boundary absent before M10 can close;
- any source defect found by the rehearsal is fixed with a red regression and
  the exact candidate is requalified before another provider run.

## Non-goals

- production or persistent-staging deployment;
- automatic promotion, rollback, SQL reversal or data repair;
- changing an application-owned DNS name, certificate or shared database;
- publishing crates, tags, releases or the documentation site;
- claiming canary or static-site provider proof unless separately included in
  the approved resource and cost boundary.

## Evidence

Provider execution has not run and remains blocked on the authority gate above.

Local preflight on 2026-08-03 found that the bounded runners previously relied
on an out-of-band review statement and could reach STS without a digest-bound
account, role/profile, source, database, resource, duration/spend and cleanup
approval. A red-first shell regression now proves that the direct runner, root
bootstrap and account inspection fail before build or AWS contact when that
authority is absent. The exact local document is schema-closed, expires within
24 hours, accepts only three fixed resource/cleanup profiles, limits new work to
60 minutes, preserves cleanup authority after expiry and writes only a redacted
receipt. Caller account and role are rechecked after STS without retaining them
in the authority receipt.

Local non-provider evidence currently passes:

- `scripts/aws/validate.sh`, including the authority regression, static
  validation and real SAM lint;
- `scripts/dev/rustack-smoke.sh` for S3, SQS, SSM, STS and the Minco adapters;
- `cargo minco deploy plan --environment dev --json --stdout`, retaining the
  no-NAT, no-fixed-compute, no-provisioned-concurrency and no-schedule plan;
- `cargo minco rollback --dry-run --json`, which made no AWS contact and failed
  closed on the intentionally absent current and target promotion receipts;
- Bash syntax and ShellCheck for every AWS script.

The remaining source-design gate is the multi-release rehearsal boundary. The
current bounded runner creates, verifies and promotes one release, then cleans
the stack immediately. It cannot yet establish a prior live release, promote
the current release, assess their exact evidence chains, redeploy the prior
artifact from its clean source checkout, reverify it and restore traffic in the
same stack before teardown. Do not weaken source provenance or reuse a
historical hosted report to bypass that gate. Complete local and hosted quality,
the closed multi-release design, exact provider authority and the live evidence
remain required before this task can complete.

The first post-merge multi-release slice now makes rollback assessment
explicitly multi-root. Current and prior promotion chains stay in separate
absolute, existing, non-symlink clean checkouts that resolve to canonical paths;
a complete assessment verifies each checkout is at the exact source revision
sealed by its release. Dry-run is still local-only and names both roots while
explicitly prohibiting historical hosted-report reuse. Red-green CLI coverage
proved the new arguments, canonical root reporting and rejection of relative
roots. The remaining controller work must parent the shared provider resources,
phase-specific immutable evidence and one cleanup trap before the single-release
runner's immediate cleanup can be relaxed.

The second controller slice now adds a distinct, schema-closed multi-release
authority kind. It binds two different exact source revisions, the only accepted
`prior`, `current`, `prior` phase order, and closed direct, root-bootstrap or
temporary-RDS resource/cleanup profile pairs. Red-first shell coverage proves
swapped or identical revisions, incomplete order and database/scope mismatches
fail locally. Its retained receipt includes exact source/order and approval
bounds but redacts account, role and database identifiers. Both validators use
one shared read-only policy for account, time, spend and database semantics; no
provider-capable command or sensitive temporary authority copy is introduced by
this slice.

The third controller slice adds a provider-free, schema-stable phase plan. It
accepts only two distinct absolute, existing, non-symlink clean Git or JJ
checkouts whose current revisions exactly match the multi-release authority.
The emitted lifecycle fixes one shared stack and artifact-bucket boundary,
unique `01-prior-initial`, `02-current` and `03-prior-rollback` evidence
namespaces, and exactly one parent-owned cleanup trap. The rollback phase must
first bind the initial prior and current promotion receipts in a compatibility
assessment, accept only `compatible`, then reuse the initial prior release
without build, replan or historical hosted-report reuse while still producing
fresh hosted verification and promotion evidence. Every phase namespace is
create-only. Red-first shell coverage
proves relative, duplicate, symlinked, nested, dirty and revision-mismatched roots fail,
and fakes provider/build commands to prove planning makes no external contact.
The bounded runner remains single-release: a later slice must make its deploy,
verify and promote work phase-capable under the parent without weakening its
standalone cleanup or create-only guards.

The fourth controller slice seals a provider-free per-phase handoff. It binds
the exact whole-run plan digest to the original authority digest, revalidates
both clean source revisions at every handoff, accepts only the three fixed phase
IDs and rejects even digest-matched plans outside the closed lifecycle policy.
The whole-run evidence root is now a new canonical absolute path outside both
checkouts, so prior-phase evidence cannot dirty the source required by the next
phase. Each projected namespace remains create-only and rollback retains exact
phase-1 artifact reuse without rebuild or replan, the compatible-only
assessment with exact promotion phases, and fresh verification and promotion.
Red-first coverage proves unsafe or pre-existing evidence roots,
pre-existing phase namespaces, authority mismatch, policy broadening and
post-plan source drift fail closed while fake provider/build commands remain
untouched. The remaining implementation gate is still provider-capable parent
orchestration with one shared resource boundary and one cleanup owner; this
slice does not authorize or contact AWS.

The fifth controller slice closes the previously ambiguous provider review
policy for those handoffs. The whole-run plan and every projected phase now
carry one of two fixed policies: the existing eight-resource create allowlist
for `01-prior-initial`, or a release-update allowlist for `02-current` and
`03-prior-rollback`. Update reviews admit only generated Lambda versions and
candidate Function/Alias changes; IAM/API expansion, imports, dynamic or
provider-sync actions and live-alias mutation fail closed. The standalone
bounded runner now delegates its create receipt to the same evaluator instead
of retaining a second inline policy. Red-first shell coverage proved the plan
omitted this boundary before implementation, and then proved exact create and
update examples plus rejection of broadened IAM, live routing and arbitrary
operator-defined policies. Security review added a second red regression for
an incomplete resource-change object; the evaluator now also closes action,
replacement, retention policy and property-scope semantics instead of trusting
an allowlisted logical ID alone. This slice remains provider-free; shared
resource setup, phase execution, compatibility assessment and one parent-owned
cleanup trap are still required before live rehearsal authority can be used.

The sixth controller slice atomically initializes the provider-free parent
evidence boundary. Only the exact current checkout named by the plan may run
the initializer. It reprojects all three handoffs before creating a private
whole-run directory, then seals the exact plan, a redacted authority receipt,
three projection digests and an immutable `initialized` controller receipt.
Every phase remains pending, no phase evidence namespace is consumed, shared
resources remain `not_created`, and cleanup remains required and owned by one
future parent trap. Red-first coverage proved the command was absent, then
proved exact receipt/digest shape, mode-`0700`/`0600` evidence, zero provider or
build contact, source-bound controller execution, authority redaction and
non-destructive rejection of repeated initialization. Security review added
the exact controller-root check, a schema-closed receipt validator and
authority/receipt digest rechecks. Provider-capable state transitions, shared
resource phase execution, compatibility handoff, the actual single cleanup
trap and the separately approved live rehearsal remain.

The seventh controller slice adds the first durable phase transition without
crossing the provider boundary. From the exact current checkout, the command
accepts only the initialized controller's `01-prior-initial` phase and exact
controller and authority approvals. It revalidates the sealed plan, all three
projections, both clean source revisions, fixed create review policy, private
access modes and the absence of unsealed state. It then publishes the complete
first-phase namespace with one atomic rename, preserving the immutable parent
receipt and later create-only namespaces. Red-first coverage proved the
command was absent, then exposed and fixed broadly accessible evidence,
adoption of a pre-existing `phases` directory, partial state after a failed
publish and injected unsealed root entries. The resulting schema-closed
`started` receipt is redacted and explicitly records no AWS contact. Provider
execution, successful/failed phase completion transitions, shared stack and
bucket ownership, compatibility handoff, the one parent cleanup trap and the
separately approved live rehearsal remain required.

The eighth controller slice now exercises the parent process lifecycle without
misstating provider proof. From the exact current checkout it accepts only the
sealed first-phase start digest, revalidates the complete controller, authority,
projection and clean-source chain, then installs one parent lifecycle trap and
writes immutable private start and terminal validation receipts. The terminal
receipt is digest-bound to the exact start receipt; both fix execution to
`validation_only`, record that the provider boundary was never entered and
therefore disarm without claiming cloud cleanup. Red-first coverage proved the
command was absent, then proved exact receipt shape, digest chaining, private
permissions, authority redaction, wrong-approval and wrong-checkout rejection,
mode enforcement, create-only repetition and zero provider/build contact.
Security review added explicit regular-file checks for every sealed projection
and ensured interruption after a durable start cannot fabricate a validated
completion. Provider execution, terminal provider-phase receipts, shared stack,
bucket and identity ownership, compatibility handoff, verified parent cleanup
and the separately approved live rehearsal remain required.

The ninth controller slice adds the first provider-entry tracer without
granting deployment authority. The same exact current parent process now emits
a schema-closed deterministic plan that binds the controller, authority,
first-phase handoff, exact Region, fixed read-only STS identity action, zero
mutation, zero secret request and parent cleanup ownership. Execution requires
a separate exact SHA-256 approval of that plan, revalidates the complete chain
before publishing its durable start receipt, then makes only the fixed
`get-caller-identity` call and compares the normalized account and role to the
authority without retaining either value. Success records
`provider_identity_verified`; an identity mismatch records a conservative
`failed` receipt with provider contact true. Both explicitly record that no
resource existed to clean. Red-first fake-provider coverage proves wrong
digests fail before evidence or contact, the exact STS-only command shape,
successful and mismatched identities, lifecycle digest chaining and redaction.
No live AWS call was made by this source slice, so provider deployment,
terminal resource-phase receipts, shared stack, bucket and identity ownership,
compatibility handoff, verified parent cleanup and the separately approved live
rehearsal remain required.

Security re-review on 2026-08-04 found that changing the authority document
after its redacted receipt comparison was rejected only after the parent had
claimed lifecycle evidence and contacted STS. The parent now captures all
identity fields before validation, rehashes the exact authority after the
comparison and never rereads the mutable path at provider entry. A deterministic
fake-command regression changes the file at that exact boundary and proves the
run fails before lifecycle receipts or AWS contact. The multi-release plan test,
ShellCheck and Bash syntax checks pass for this correction. Provider execution
itself remains unrun and authority-gated.

The tenth controller slice adds the separately approved disposable-resource
preflight required before the parent may create anything. Plan mode revalidates
the complete controller and emits one schema-closed, redacted contract allowing
only STS identity plus application-stack, artifact-bucket, temporary-RDS-stack
and database-instance absence reads. Execute mode requires the exact plan
SHA-256 before lifecycle evidence or contact, verifies the expected role, then
accepts only the provider's precise not-found responses. Success records
`provider_resources_absent`; any identity, pre-existing-resource or unexpected
response records a conservative failed terminal state without claiming cleanup.
Red-first fake-AWS coverage proves the exact five-call shape, wrong-digest
fail-before-contact behavior, create-only receipts, redaction, zero mutation
and rejection of a misleading not-found message whose structured service code
is not an absence code.

Post-implementation review on 2026-08-04 used AWS CLI 2.36.14 for four
read-only calls outside the controller: one caller-identity discovery and one
deliberately nonexistent-name probe each for CloudFormation, S3 and RDS. The
probes confirmed exit `254` with structured codes `ValidationError`, `404` and
`DBInstanceNotFound`. They created, changed and deleted no resource. Account,
ARN, message, endpoint and resource identifier values were intentionally not
retained in repository evidence. These documentation probes are not the live
rehearsal and prove no deployment or cleanup. Shared resource creation, phase
execution, compatibility handoff, one parent-owned cleanup trap and the
separately approved live rehearsal remain required.

The eleventh safety slice extracts the installed AWS CLI 2.36 structured-error
contract into one shared helper and applies it to all pre-creation checks used
by the standalone application runner and disposable-RDS creator as well as the
multi-release resource preflight. Red-first coverage rejects wrong exit codes,
wrong service codes, expanded response objects and legacy English message text;
it also fixes the global `--cli-error-format json` command shape. Application
stack absence now requires `ValidationError`, artifact-bucket absence requires
`404`, and temporary-RDS-stack absence requires `ValidationError`, always with
service exit `254`. This slice remains provider-free. It closes a prerequisite
for live creation but does not create, deploy or clean any AWS resource.

The twelfth safety slice extends that fail-closed contract through the terminal
cleanup boundary. Application, identity and temporary-database teardown now
accept provider absence only as service exit `254` plus the exact structured
CloudFormation, S3, Cognito, Lambda, Logs, IAM, API Gateway, SSM, RDS, Secrets
Manager or EC2 code. Bounded IAM/STS propagation retries accept only a closed
structured code set, and S3 visibility polling no longer reads provider message
text. Red-first source coverage rejects any reintroduced extended-regex parsing
in these cleanup and retry paths. This slice remains provider-free: it proves
source behavior but creates, changes and deletes no AWS resource.

The thirteenth provider-free slice separates shared resource ownership from
phase receipt storage. `MINCO_AWS_RUN_ID` continues to bind resource names,
tags and journal entries, while a separately validated
`MINCO_AWS_EVIDENCE_ID` selects the project-relative receipt chain consumed by
release, migration, deploy, hosted verification and promotion commands. The
new ID defaults to the run ID, so standalone behavior is unchanged, and path
syntax is fail-closed even when initialization runs in a shell conditional.
Red-first portability coverage proves one shared run can select a distinct
phase directory without changing its journaled ownership and rejects traversal.
This closes the phase-1 versus rollback overwrite hazard; no provider command
was executed by the slice.

Exact hosted qualification of the lower stacked change then exposed a Linux
portability defect after the same gate passed on macOS: GNU `stat -f` emitted
partial filesystem output before returning failure, so the BSD-first fallback
polluted captured modes in the test and the controller runtime gates. One
shared helper now probes GNU `stat -c` with all detection output suppressed,
emits only the selected implementation, and retains the BSD fallback. Focused
coverage simulates both implementations, including a failed probe that emits
partial output, while the descendant hosted qualification remains the exact
cross-platform verification gate.

The fourteenth provider-free slice closes all three create-only phase
transitions. A schema-closed provider-result envelope retains only exact
release, migration, change-set, deployment, fresh hosted-verification,
promotion and rollback-assessment digests. Completion copies the approved
result before further reads, revalidates both exact clean source revisions and
publishes an immutable success receipt whose next phase is fixed. Later phase
starts require the exact predecessor completion digest; rollback also verifies
the complete phase-1-to-phase-2 chain and must reuse phase one's exact release
digest with a new assessment and hosted report. Red-first tests prove the
missing completion command, then reject wrong approvals, repeated completion,
source drift and a substituted rollback artifact while fake provider/build
commands receive no additional call. Provider result construction, shared
resource execution and the sole parent cleanup trap remain to be wired before
the separately approved live rehearsal.
