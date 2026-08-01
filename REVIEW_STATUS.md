# Review status

Minco `0.6.0` is published from exact source
`2c4605b7d4abcd865035196ffc0484c4a0e82f1e`. PR-head hosted release run
`30688694186`, merged-main run `30689722134` and trusted publication run
`30690519946` passed. All 28 exact crates are non-yanked and owned by `xicv`;
downloaded archive checksums, a fresh CLI installation, default/no-default/
full-feature consumers and all exact docs.rs routes passed independently.
PR #74 passed exact-head essential run `30691699436`, merged as exact commit
`651a1886476556805991d83cbc71f9054f7703fe`, and deployed through merged-main
Pages run `30691854137`. The four key public routes return HTTP 200 with the
expected canonical and latest-stable metadata, and 13/13 applicable live
desktop/mobile browser checks pass. M11-T08 is complete. Planned M11-T04
through M11-T06 remain deferred and are not represented as shipped behavior.

The Minco `0.5.0` release boundary is closed. Exact source
`485d67104a49f139820722eb73334415f69a653c` is tagged, published as a
[GitHub release](https://github.com/xicv/minco/releases/tag/v0.5.0) and
available across all 28 crates.io packages. Candidate hosted release run
`30618708535`, merged-main release run `30623278437` and publication run
`30632811070` passed. Independent registry metadata, owner, checksum, archive,
installation, external consumer and all 28 exact docs.rs checks also passed.

The published source contains the opt-in five-action resource API convention,
local-authoritative CI split and structured zero-idle cost evidence. No new AWS
deployment was performed for `0.5.0`; the independently recorded `0.4.0`
disposable AWS rehearsal remains the latest live runtime proof. This paragraph
is retained as historical `0.5.0` evidence.

This record separates:

- source review and local qualification;
- hosted checks on the exact pull-request head;
- pull-request merge and merged-main requalification;
- live AWS deployment/rollback evidence;
- exact tag creation;
- crates.io publication and independent registry/consumer/docs.rs proof.

The stage-environment correction passed exact-head hosted run `30526281458` at
`d5b4a76946a47bb4aeffb8be64b7460e1e61ce2d`, merged as exact `main`
`83d1583e9a385070306c95665a5219700cbc1c5e`, passed the complete local
qualification and passed exact-main hosted run `30527357088`. All
authoritative quality, browser, 28-package dry-run, Plan/SAM/native ARM64,
Rustack/SSM and explicit Orders E2E stages passed.

Authorised live run `20260730t085318z-release040` migrated and verified its
disposable private PostgreSQL database, reproduced the deterministic
5,038,349-byte native ARM64 artifact with SHA-256
`ff9609127cedcf2aad6c563e1f524feda1258ec33f104f7973eccecaa80ea474`,
sealed exact-main release `minco.faf23ae016624d15d0b8f11f`, applied reviewed
change-set receipt
`3d349a2be71b1aa04491f61f388780bb5c8d973e756aa4296c388103a8f27443`,
and reached `CREATE_COMPLETE`. Candidate `GET /health/live` still reached
Lambda and returned Minco request ID
`1dcc9a69-cae5-4c68-ba8e-bac9fec24128`, but Axum returned an empty 404.

The live event refines the root cause: API Gateway v2 already places
`/candidate` in `rawPath`, so
`AWS_LAMBDA_HTTP_IGNORE_STAGE_IN_PATH` leaves the prefixed path unchanged. The
current correction normalizes the exact non-default API Gateway context stage
in `minco-aws-lambda` before Axum route matching, preserves authority/query,
rejects prefix lookalikes and leaves `$default` unchanged. A realistic event
regression reaches the contract-owned `/health/live` route in-process. The
ineffective SAM environment setting is removed. No promotion, tag or registry
upload occurred. The application cleanup receipt is all true; a bounded
follow-up check after AWS's asynchronous deletion window also proves the exact
temporary PostgreSQL stack, instance, managed secret and VPC absent, with
synthetic data and local database secret files absent. The bootstrap user, role,
profiles and credential files are absent.

Replacement live AWS proof, exact tag creation and crates.io publication remain
separate pending gates. Current and historical command evidence is maintained
in `VERIFICATION.md`; Feedback-specific architecture evidence remains in
`FEEDBACK_REVIEW_STATUS.md`.

The named-stage correction passed PR-head hosted run `30532832860` at
`d7e5a1c6e9ff5f5c43c754bc145bdefd63c7b60e`, merged as exact `main`
`73807d918bc860b60d592611f388bb63775d7c54`, and passed both the complete
local qualification and exact-main hosted run `30534601227`.

Authorised live run `20260730t104626z-release040` then migrated and verified
private PostgreSQL, sealed exact release `minco.789c2425846acb0fda2039f0`,
and applied its reviewed change set. Candidate liveness and readiness passed;
the protected-order probe returned the expected 401 with API Gateway header
`apigw-requestid`. The verifier rejected that valid provider request ID because
it recognized only `x-request-id` and `x-amzn-requestid`, so no authenticated
mutation or promotion ran. Application cleanup is all true. A bounded exact
rerun after the asynchronous RDS secret window proves the temporary database,
managed secret and VPC absent; bootstrap principals, profiles and credentials
are also absent.

The current correction centralizes response request-ID extraction and adds
executable positive fixtures for all three supported provider/application
headers plus a negative unrelated-header fixture. Exact-head hosted
qualification, merge, exact-main qualification and another live rehearsal are
required before tag creation or crates.io publication.

That correction passed exact PR-head hosted run `30539721321` at
`8e97b38ef22608f849d531145f13dbf0e3e7243e`, merged as exact `main`
`30260209c49acb048f6549a31eb1e375fd1e923e`, passed the complete local release
matrix and passed exact-main hosted run `30542710147`.

Authorised live run `20260730t124426z-release040` then migrated and verified
private PostgreSQL, built the 5,039,398-byte native ARM64 artifact, sealed
release `minco.761bb0f73b895275c78858ff`, applied the exact reviewed change
set, and passed candidate liveness, readiness, the unauthenticated 401,
authenticated place/get and idempotent replay. Hosted-report construction
rejected only the Authentication check because the accepted API Gateway
request ID ended in Base64-style `=` padding, which the Rust verifier's
character set excluded. No promotion, tag or registry upload occurred.

The current correction admits one or two `=` characters only as trailing
request-ID padding and preserves the existing length and character bounds.
The focused regression failed before the change and all 13 hosted-verification
tests pass after it, including an internal-padding rejection. Exact cleanup
reruns prove all application, database/VPC/secret, bootstrap-IAM and local
credential resources absent. Exact-head hosted qualification, merge,
exact-main qualification and another live rehearsal remain required before
tag creation or crates.io publication.

That correction passed exact PR-head hosted run `30548150116` at
`ade67d7f6d2866ed6bfde610742cf53660fe8ec9`, merged as exact `main`
`25ffdd4c38eba8e8a759cf7e83404fbfebd36e60`, passed the complete local release
matrix and passed exact-main hosted run `30550393414`.

Authorised live run `20260730t142515z-release040` then migrated and verified
private PostgreSQL, built the 5,039,398-byte native ARM64 artifact, sealed
release `minco.31235789f783406088906750`, applied its reviewed change set and
passed every candidate hosted check. Promotion failed closed before any change
set or live alias mutation because the bounded role lacked
`cloudformation:DetectStackResourceDrift`.

The current least-privilege correction adds the two stack-scoped drift actions
to the exact owned-stack ARNs and isolates the provider-required type
configuration and drift-status reads in a wildcard-only discovery statement.
The focused rendered-policy regression failed before the change and passes
after it without introducing an action wildcard. Application cleanup is all
true; the exact RDS recovery rerun and `final-cleanup.json` prove the managed
secret, temporary database/VPC, bootstrap principals, profiles and local
credential files absent. No tag or registry upload occurred. Exact-head hosted
qualification, merge, exact-main qualification and another live rehearsal
remain required.

That least-privilege correction passed exact PR-head hosted run `30556566177`
at `541e61e6fbb23a582011244539b2befddcd38fbf`, merged as exact `main`
`fbdcb002b5df7632e6233f3d08be97b13e571fb3`, passed the complete local
release matrix and passed exact-main hosted run `30558916893`.

Authorised live run `20260730t160831z-release040` then migrated and verified
private PostgreSQL, reproduced the 5,039,398-byte native ARM64 artifact,
sealed release `minco.2b93b493fa3a454d51a4cbcb`, applied its reviewed change
set and passed every candidate hosted check. CloudFormation drift detection
also completed with the stack `IN_SYNC`, but promotion failed closed before
any change set or live alias mutation because Minco required the nonexistent
response key `StackDriftDetectionStatus` instead of the provider's
`DetectionStatus`.

The current correction changes only that response-field binding and adds a
provider-shaped regression. The test failed before the change with the exact
live parse error and passes after it; failed, unknown and drifted states retain
their existing fail-closed handling. Application cleanup is all true. The
exact RDS recovery rerun and all-true `final-cleanup.json` prove the managed
secret, temporary database/VPC, bootstrap principals, profiles and local
credential files absent. No tag or registry upload occurred. Exact-head
hosted qualification, merge, exact-main qualification and another live
rehearsal remain required.

That correction passed exact PR-head hosted run `30563657881` at
`f952af63d3848333c8a56782fe3b42e73dd457fd`, merged as exact `main`
`ff242141c98c4d555de3ed232dba4437ff59ee17`, passed the complete local release
matrix and passed exact-main hosted run `30565805289`.

Authorised live run `20260730t174217z-release040` reproduced the qualified
native artifact, sealed and applied reviewed release
`minco.b100be45a4972f08cb3a554f`, and passed all candidate hosted checks.
Promotion stopped before a change set or live alias mutation because
CloudFormation drift inspection additionally required
`lambda:GetProvisionedConcurrencyConfig` and
`logs:DescribeIndexPolicies`. No tag or registry upload occurred.

The current correction grants only those two provider reads. The Lambda action
is bound to the exact run-owned function and qualified ARN pattern; the Logs
List action uses the provider-required wildcard resource in the existing
metadata-discovery statement. The focused rendered-policy regression failed
before the change and passes after it. Application and RDS cleanup evidence is
all true, including the managed secret, VPC, bootstrap identities, profiles and
local credential files. Exact-head hosted qualification, merge, exact-main
qualification and another live rehearsal remain required.

That correction passed exact PR-head hosted run `30570766634` at
`367d04e0476e9225e64626966245313340d54a71`, merged as exact `main`
`982bc9bf2e58597b9d7df2b7fe3e39d5a89f83b9`, passed the complete local release
matrix and passed exact-main hosted run `30573067627`.

Authorised live run `20260730t191908z-release040` then migrated and verified
private PostgreSQL, reproduced the 5,039,398-byte native ARM64 artifact, sealed
release `minco.30360dc26d7e73b91c2657fe`, applied its reviewed change set and
passed every candidate hosted check. Promotion stopped before a change set or
live alias mutation because CloudFormation drift inspection additionally
required `lambda:GetRuntimeManagementConfig` for the published function
version. No tag or registry upload occurred.

The current least-privilege correction adds only that read to the existing
exact function and qualified-version ARN boundary. Its focused rendered-policy
regression failed before the source change with `owned function policy misses
the version runtime drift-read permission` and passes after it; ShellCheck
warning/error classes also pass. Application cleanup is all true. The exact
RDS recovery rerun and all-true `final-cleanup.json` prove the managed secret,
temporary database/VPC, bootstrap identities, profiles and local credential
files absent. Exact-head hosted qualification, merge, exact-main qualification
and another live rehearsal remain required before tag creation or crates.io
publication.
