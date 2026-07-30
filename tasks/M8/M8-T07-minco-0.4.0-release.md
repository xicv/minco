---
id: M8-T07
title: Prepare the Minco 0.4.0 source and package boundary
milestone: M8
status: complete
priority: critical
area: release/crates-io
depends_on: [M9-T07, M10-T03]
operations: []
owned_paths:
  - .github/workflows/minco-manual.yml
  - Cargo.toml
  - Cargo.lock
  - CHANGELOG.md
  - README.md
  - CODEX_HANDOFF.md
  - PUBLISHING.md
  - REVIEW_STATUS.md
  - VERIFICATION.md
  - crates/minco-cli/src/main.rs
  - crates/minco-cli/tests/deploy_cli.rs
  - crates/minco-deploy-aws/**
  - crates/minco-dev/tests/supervisor.rs
  - crates/minco-plan/src/sam.rs
  - docs/**
  - extensions/minco-aws-adapters/README.md
  - extensions/minco-aws-lambda/Cargo.toml
  - extensions/minco-aws-lambda/src/lib.rs
  - infra/aws/generated/template.yaml
  - roadmap/**
  - scripts/validate_static.py
  - scripts/aws/build-lambda.sh
  - scripts/aws/build-worker-lambda.sh
  - scripts/aws/cleanup.sh
  - scripts/aws/lib/common.sh
  - scripts/aws/run-bounded-root-bootstrap.sh
  - scripts/aws/smoke.sh
  - scripts/generate_bootstrap_artifacts.py
  - scripts/quality.sh
  - scripts/release/publish.py
  - scripts/test/aws_shell_portability.sh
  - scripts/test/lambda_artifact_reproducibility.py
  - scripts/test/publish_validation.py
  - scripts/test/repository_truth.py
  - tasks/M8/M8-T07-minco-0.4.0-release.md
  - tasks/M10/**
  - verification/**
checks:
  - uv run --locked python scripts/validate_publish.py
  - cargo rustdoc -p cargo-minco --lib --all-features --locked
  - scripts/quality.sh
  - npm run --prefix plugins/minco-plugin-feedback test:browser
  - scripts/test/e2e.sh
  - scripts/dev/rustack-smoke.sh
  - scripts/aws/plan.sh
  - scripts/aws/validate.sh
  - scripts/aws/build-lambda.sh
  - scripts/aws/build-worker-lambda.sh
  - scripts/release/package-list.sh
  - scripts/release/publish.sh --skip-quality
---

## Goal

Prepare one exact `0.4.0` source and package candidate covering the accepted
framework work through M10-T03, with the four new crates, truthful zero-idle
doctrine, an upgrade guide and independently inspectable evidence.

## Release phases

1. Source reconciliation: version, package inventory, task graph, decisions,
   documentation and regression diagnostics.
2. Local qualification: compiler, tests, generated applications, browser,
   Rustack, Plan/SAM, Lambda and archive gates.
3. Hosted qualification: exact pull-request head, including the same material
   release gates.
4. Live AWS rehearsal: separately authorised account/Region mutation and
   runtime/rollback evidence.
5. Publication: separately authorised exact merged-main tag and crates.io
   upload, followed by registry, external-consumer and docs.rs verification.

Passing an earlier phase never implies a later phase. This task and its pull
request stop before merge, AWS mutation, tag creation and registry upload unless
those actions receive separate explicit approval.

## Release boundary

- Published baseline: `0.3.1`, 24 packages.
- Candidate: `0.4.0`, 28 packages.
- First-publish crates: `minco-config`, `minco-db`, `minco-dev` and
  `minco-deploy-aws`.
- Included program boundary: accepted work through M10-T03.
- Deferred: M10-T04, M10-T05, M10-T06, M10-T07, M11 and M12.
- MSRV: Rust `1.97.1`.

## Acceptance

- all 28 public crates use lock-step `0.4.0` dependencies and Cargo.lock is
  reviewed;
- repository truth, README, CLI reference, handoff, framework maturity,
  publishing guide and verification state agree;
- all four first-publish crates receive unpacked-archive tests;
- a `0.3.1` to `0.4.0` guide covers schemas, crates, commands, features,
  generators and deferred operational gates;
- exact source, package, generated-consumer, browser, Rustack and AWS-static
  gates are recorded as pass/fail/blocked/not run without converting missing
  tools or unauthorised live checks into passes;
- the manual hosted workflow builds both native ARM64 Lambda ZIPs and validates
  the generated Plan/SAM boundary without AWS credentials or provider contact;
- the manual hosted workflow pins JJ and verifies its version before running
  compatibility fixtures that require a real `@-` baseline;
- the manual hosted workflow pins ripgrep and verifies its version before
  generated-application checks use `rg`;
- the manual hosted workflow installs Zig `0.14.0` through an immutable action
  commit before Cargo Lambda performs native ARM64 cross-compilation;
- consecutive byte-identical Lambda builds produce the same normalized ZIP
  digest, permissions and entry inventory;
- no credentials, customer data, secret values or live provider mutations enter
  source or evidence.

## Non-goals

- merging the release pull request;
- creating or pushing `v0.4.0`;
- publishing to crates.io;
- creating, modifying or deleting AWS resources;
- implementing rollback/canary, static-site domains, preview lifecycle, a
  pricing engine, hosted Minco control plane, documentation platform or AI
  workbench.

## Evidence

Focused source, feature, new-crate, generated-application, browser, Orders E2E,
Rustack, Plan/SAM, Lambda and coordinated 28-archive dry-run gates pass locally.
The native Lambda helpers normalize volatile Cargo Lambda ZIP metadata and a
dedicated regression proves stable digests plus fail-closed entry validation.
The archive gate tests all five configured packages, compiles no-default,
default, full and four-new-crate consumers from unpacked archives, and installs
the unpacked `cargo-minco` binary. Exact commands, artifact hashes,
measurements, diagnostics and limitations are recorded in `VERIFICATION.md`.

The clean source passed the authoritative local suite. Corrected pull-request
head `46be92f0b68e6759a897ef5e99c010d77c2bf32b` passed manual hosted run
`30410242657`, including authoritative quality, browser, coordinated package
dry-run, Plan/SAM/native Lambda, Rustack and E2E stages. A later evidence
revision exposed a race in the packaged `minco-dev` shutdown fixture during
run `30411179583`; the fixture now waits for a complete numeric descendant PID
before requesting shutdown and passed 600 repeated full-suite runs locally.
Corrected exact head `b211b5083b43a0c9a0de9cd28ca4f748dfbbeb51`
then passed every stage of hosted run `30412849538`. The source/package task is
complete and ready for an exact-head guarded merge. Live AWS rehearsal, tag
and registry publication remain separately authorised and evidenced phases.

The first authorised live-AWS invocation on 2026-07-29 stopped before caller
discovery or resource creation because macOS Bash rejected the controller's
own hyphenated default SSM parameter name. The portable shared predicate and
`scripts/test/aws_shell_portability.sh` regression retain every fail-closed
normalization check while accepting the generated default. Exact-main
qualification and the live rehearsal must be repeated after this correction;
no tag or registry upload occurred.

That correction merged as
`d34c0e49d881a5ababdc1e9576c046c867f45ab3`, which passed the complete local
suite and exact-main hosted run
[`30422838559`](https://github.com/xicv/minco/actions/runs/30422838559).
The next authorised live rehearsal migrated and verified its disposable
private PostgreSQL database and built the native ARM64 Lambda, then failed
tagged Cognito user-pool creation because the bounded deployment role lacked
`cognito-idp:TagResource`. The run subsequently produced all-true application,
database/VPC/secret and bootstrap-IAM cleanup receipts. The candidate
least-privilege correction permits that action only for the current
Region/account user-pool namespace and the exact three run-ownership tags; the
local regression renders and compares the complete generated statement. AWS
IAM simulation returned `allowed` for the exact tag set and `implicitDeny` when
an additional tag key was supplied. No tag or registry upload occurred.

That correction passed PR-head hosted run
[`30425328469`](https://github.com/xicv/minco/actions/runs/30425328469),
merged as exact `main`
`cd5b0049cd55f3ba7093a202eff9b668c825ed0b`, and passed the full local suite,
AWS/SAM validation and exact-main hosted run
[`30426089277`](https://github.com/xicv/minco/actions/runs/30426089277).
Authorised replacement run `20260729t060221z-approved` migrated and verified
its disposable private PostgreSQL database and sealed the native ARM64
artifact, then stopped before application change-set creation because AWS CLI
shorthand parsed comma-delimited Lambda subnet IDs as a list where
CloudFormation requires a string `ParameterValue`. All application,
database/VPC/secret and bootstrap-IAM cleanup receipts are true. The candidate
fix serializes deployment and promotion parameters as one JSON list; its
focused regression and the AWS CLI `2.36.10` non-contacting output-skeleton
validator preserve comma-delimited values as strings. The live replacement,
tag and registry publication remain blocked pending merge and exact-main
requalification.

The JSON-parameter correction passed PR-head hosted run
[`30428780397`](https://github.com/xicv/minco/actions/runs/30428780397), merged
as exact `main` `100ffa276163a2c02149321b2b7ffcc542edb4c5`, and passed the
full local suite, AWS/SAM validation and exact-main hosted run
[`30429829246`](https://github.com/xicv/minco/actions/runs/30429829246).
Authorised replacement run `20260729t071107z-approved` migrated and verified
its disposable private PostgreSQL database, built the 5,038,349 byte native
ARM64 artifact and created the application change set, then stopped
fail-closed because the real `describe-change-set` response omitted
`ChangeSetType`. That field is guarded create input and is not a documented
`DescribeChangeSet` response element. Exact inspection also found an empty,
untagged `REVIEW_IN_PROGRESS` shell that the tag-only cleanup guard correctly
refused. The shell contained one unexecuted change set and zero resources; both
were deleted, the RDS-managed secret reached `ResourceNotFound`, and the exact
repository cleanup verifiers produced all-true application,
database/VPC/secret and bootstrap-IAM receipts.

The candidate parser requires the already-guarded expected type, uses it in the
redacted immutable review and rejects an optional contradictory provider
value. Cleanup may delete an untagged review shell only when the exact stack
was proven absent before the run, remains in `REVIEW_IN_PROGRESS` and has zero
resources. Focused red/green tests cover the real provider shape and every
cleanup refusal boundary. A replacement live rehearsal, tag and registry
publication remain blocked pending merge and exact-main requalification.

That correction passed PR-head hosted run
[`30433187335`](https://github.com/xicv/minco/actions/runs/30433187335), merged
as exact `main` `13be9b0a8d99281c98fec880b8d275a59c7499f9`, and passed the
full local suite, AWS/SAM validation and exact-main hosted run
[`30434365889`](https://github.com/xicv/minco/actions/runs/30434365889).
Authorised replacement run `20260729t082616z-approved` then migrated and
verified its disposable private PostgreSQL database, sealed and verified the
native ARM64 release, created and re-read the application change set through
the corrected provider parser, and entered the digest-approved apply. Both API
Gateway stages failed because the change set propagated only Minco release
tags while the bounded role and cleanup contract require the exact three
run-ownership tags. Rollback deleted every stack resource. The release-tagged
rollback shell and delayed RDS secret were subsequently removed after exact
ownership verification, and a cross-service sweep proved every application,
database, secret, network, storage, Cognito, Lambda/log and bootstrap-IAM
resource absent.

The candidate correction makes validated, deterministic target stack tags part
of the change-set JSON input and emits the bounded smoke run tags from one
tested helper. Reserved release tags and the `aws:` prefix cannot be
overridden, provider limits fail closed, and the API Gateway stage policy
admits only the run, release and SAM-owned tag keys through CloudFormation.
The authoritative local suite, AWS Plan/SAM validation, ShellCheck and AWS CLI
non-contacting shape gates pass. Hosted qualification, a replacement live
rehearsal, tag and registry publication remain blocked pending exact-head merge
and requalification.

That correction passed exact PR-head hosted run
[`30438686783`](https://github.com/xicv/minco/actions/runs/30438686783), merged
as `8dcc49e2cefec1b9a043da5ae50161ae1e2431d1`, and passed the full local
suite, AWS Plan/SAM validation and exact-main hosted run
[`30440072120`](https://github.com/xicv/minco/actions/runs/30440072120).
Authorised replacement run `20260729t094817z-approved` proved the run tags,
private PostgreSQL migration, native ARM64 artifact and exact-source sealed
release, then failed API Gateway stage tagging because CloudFormation's
automatic `aws:cloudformation:stack-name`, `aws:cloudformation:stack-id` and
`aws:cloudformation:logical-id` keys were absent from the bounded
`aws:TagKeys` allowlist. AWS IAM simulation reproduced `implicitDeny` with the
real key set and `allowed` after adding only those keys. The exact cleanup
verifiers subsequently produced all-true application,
database/VPC/secret and bootstrap-IAM receipts.

The candidate correction names only API Gateway V2's documented tagging IAM
action, `apigateway:POST`, and permits only those three provider-owned keys in
addition to the already reviewed run, release and SAM keys. The exact stage
collection ARN, CloudFormation caller chain and run-tag values remain
mandatory. A replacement live rehearsal, tag and registry publication remain
blocked pending merge and exact-main requalification.

That correction passed exact PR-head hosted run
[`30443671627`](https://github.com/xicv/minco/actions/runs/30443671627), merged
as `0f1271eec11bf2e4fd475f7093c04eddd8d47f6c`, and passed the full local
suite, AWS Plan/SAM validation and exact-main hosted run
[`30444766607`](https://github.com/xicv/minco/actions/runs/30444766607).
Authorised replacement run `20260729t105820z-approved` migrated and verified
its disposable private PostgreSQL database, built the 5,038,349-byte native
ARM64 ZIP with SHA-256
`ff9609127cedcf2aad6c563e1f524feda1258ec33f104f7973eccecaa80ea474`,
and sealed exact-source release `minco.44a1623ffb1ec9bd0b037813`. Both API
Gateway stage creates still failed the dependent `TagResource` authorization.
CloudTrail recorded `CreateStage` from CloudFormation and the complete expected
request tags. AWS documents the tagging API as `apigateway:POST` on `/tags/*`;
the current specialized statement instead names `/apis/*/stages`, while the
broader CloudFormation-only statement cannot satisfy a dependent evaluation
that omits `aws:CalledVia`. Application rollback completed, the delayed
RDS-managed secret reached `ResourceNotFound`, bootstrap user/role absence was
independently rechecked, and all application, database/VPC/secret and
bootstrap-IAM cleanup receipts are true.

The candidate correction preserves the CloudFormation-only statement for all
API Gateway mutations and grants the separate tagging authorization only on
the documented `/tags/*` namespace. It requires the three exact run-ownership
request-tag values and a closed allowlist containing only the reviewed run,
release, SAM and CloudFormation system keys. The focused test failed before
the policy change and passes afterward. IAM simulation returns `allowed` for
the exact request and `implicitDeny` for an extra key or wrong run ID. The
authoritative local quality suite and AWS Plan/SAM validation pass. A
replacement live rehearsal, tag and registry publication remain blocked
pending exact-head hosted qualification, merge and exact-main requalification.

That correction passed exact PR-head hosted run
[`30448531978`](https://github.com/xicv/minco/actions/runs/30448531978), merged
as `edabc701ee86b4adfee27b978f8d4d6187d19f2e`, and passed the full local
suite, AWS Plan/SAM validation and exact-main hosted run
[`30449710067`](https://github.com/xicv/minco/actions/runs/30449710067).
Authorised replacement run `20260729t121408z-approved` again migrated and
verified its disposable private PostgreSQL database, built the same
5,038,349-byte native ARM64 ZIP, and sealed exact-source release
`minco.6fba6aee8d28ce4d9bece03b`. Both stage creates still failed the
provider-reported `TagResource` dependency. CloudTrail records the actual
operation as tagged `CreateStage` against `/apis/${ApiId}/stages`; there is no
separate tagging event. This falsifies the `/tags/*` resource hypothesis.
Application cleanup is all true. The delayed managed secret subsequently
reached `ResourceNotFound`, the exact RDS cleanup verifier is all true, and the
deterministic bootstrap user and role are independently absent.

The replacement candidate preserves the CloudFormation-only statement for
general API Gateway mutations and grants the specialized
`apigateway:POST` authorization only on `/apis/*/stages`. It still requires the
three exact run-ownership request-tag values and closed reviewed tag-key
allowlist. The focused test failed before the policy change and passes
afterward; IAM simulation returns `allowed` for the exact observed request
without `aws:CalledVia`, and `implicitDeny` for a wrong run ID or extra tag key.
The authoritative local quality suite and AWS Plan/SAM validation pass.
Hosted qualification, a replacement live rehearsal, tag and registry
publication remain blocked.

That stage-collection correction passed exact PR-head hosted run
[`30453546940`](https://github.com/xicv/minco/actions/runs/30453546940), merged
as `8593b47eaf691cace2bf32d3d07e3408f036ca46`, and passed the full local
suite, AWS Plan/SAM validation and exact-main hosted run
[`30454760539`](https://github.com/xicv/minco/actions/runs/30454760539).
Authorised replacement run `20260729t132534z-approved` migrated and verified
its disposable PostgreSQL database over TLS `verify-full`, removed the local
`/32`, proved the database private, built the same 5,038,349-byte native ARM64
ZIP, and sealed exact-source release `minco.2b3857b9f12ff31ac32f183a`.
The run-owned S3 bucket was created, tagged, blocked from public access and
encrypted, but the cached build reached the controller within seconds and its
immediate `HeadBucket` returned 404 before change-set creation. Application
cleanup is all true. After the RDS-managed secret reached
`ResourceNotFound`, exact database/VPC/secret cleanup and bootstrap IAM/local
credential checks were consolidated in an all-true `final-cleanup.json`.

The replacement candidate adds a bounded bucket-visibility wait after creation
and hardening. It retries only `404`, `NoSuchBucket` and `Not Found`, fails
immediately on every other response, and stops after 15 attempts. The focused
test failed before the helper existed and now covers transient success,
non-404 fail-fast behavior and exhaustion of the retry bound. Exact-head hosted
qualification, merge, a replacement live rehearsal, tag and registry
publication remain blocked.

That correction passed exact PR-head hosted run
[`30458112104`](https://github.com/xicv/minco/actions/runs/30458112104), merged
as `dbe8a55f141c082a8329ec1871590c0199682eed`, and passed the full local
suite, AWS Plan/SAM validation and exact-main hosted run
[`30459913592`](https://github.com/xicv/minco/actions/runs/30459913592).
Authorised replacement run `20260729t143232z-approved` migrated and verified
its disposable PostgreSQL database over TLS `verify-full`, removed the local
`/32`, proved the database private, and passed the new S3 visibility guard on
its first attempt. It built the same 5,038,349-byte native ARM64 ZIP and sealed
exact-source release `minco.eefe49c4e87868c73164ecba`. Both API Gateway stage
creates then failed the provider-reported dependent `TagResource`
authorization. CloudTrail recorded the tagged `CreateStage` calls from exact
temporary role `MincoSmoke-d93173c82d99`, with the expected ten-key closed tag
set, and no separate `TagResource` event.

AWS's current API Gateway V2 operation mapping identifies both
`apigateway:POST` and `apigateway:PUT` as required for tagged `CreateStage`.
Together with the tagging IAM namespace, this proves the exact authorization
pair: `POST` on `/apis/*/stages` plus `PUT` on `/tags/*`. The replacement
candidate adds only the missing `PUT` statement, requiring the same three exact
run-tag values and closed ten-key allowlist. The focused regression failed with
`StopIteration` before the statement existed and passes afterward. IAM
custom-policy simulation permits only those two exact action/resource pairs;
crossed pairs, a wrong run ID and an extra tag key are all implicit deny.
Application cleanup is all true. After the delayed RDS-managed secret reached
`ResourceNotFound`, exact database/VPC, bootstrap IAM and local credential-file
cleanup checks were independently consolidated in an all-true
`final-cleanup.json`. Exact-head hosted qualification, merge, a replacement
live rehearsal, tag and registry publication remain blocked.

The first tagged-stage correction passed exact PR-head hosted run
[`30466012186`](https://github.com/xicv/minco/actions/runs/30466012186) at
`d7ffe82290ff2cfc215e737823e471226d661b56`, merged as
`4bf245cae924e2d3c89d008cf291da8bf862cba4`, passed the full local suite and
AWS Plan/SAM validation, and passed exact-main hosted run
[`30467769879`](https://github.com/xicv/minco/actions/runs/30467769879).
Authorised run `20260729t215737z-approved` migrated and verified its disposable
PostgreSQL database over TLS `verify-full`, removed the local `/32`, proved the
database private, passed S3 visibility on the first bounded attempt, and sealed
exact-source release `minco.683d7abad93046f3b4476621` with digest
`683d7abad93046f3b44766215f0ecea095bf9003e2fc4242b769db2f1deed30d`.
It created an exact release-bound change-set receipt with digest
`f32c48fb78964575188c2fe0035f053e0a4142d5e7030f08a19602284a209605`.

Both API Gateway stage creates then failed. AWS reported the dependent
`apigateway:TagResource` denial against the evaluated resource
`arn:aws:apigateway:ap-southeast-2::/apis/iaqgnlnghl/stages`. Custom-policy
simulation reproduced the mismatch: `POST` on the stage collection was
allowed, but `PUT` on the same collection was `implicitDeny` because the
candidate had placed it on the separate direct tagging API namespace
`/tags/*`.

The current correction puts both specialized methods on
`/apis/*/stages`, retaining the three exact run-ownership request tags and
closed ten-key allowlist. The focused regression failed before implementation
and passes afterward. Simulation permits exact-tag `POST` and `PUT` on the
stage collection; a wrong run ID, an extra tag key and direct `PUT` on
`/tags/*` are `implicitDeny`. Access Analyzer reports no findings for both
specialized statements. Application cleanup is all true; the second exact RDS
cleanup verification confirms the delayed managed secret, database instance,
stack, VPC, local secret files and synthetic data are absent. Bootstrap IAM
and temporary local credentials are absent. Exact-head hosted qualification,
merge, another live rehearsal, tag and registry publication remain blocked.

The stage-collection correction passed exact PR-head hosted run
[`30496875203`](https://github.com/xicv/minco/actions/runs/30496875203) at
`cffb60520a9311c72cf287f94c8dcbfa762bf1e0`, merged as
`36d09d5ce36242290ae99506afee64c1a2f0de91`, passed the full local suite and
AWS Plan/SAM validation, and passed exact-main hosted run
[`30498077062`](https://github.com/xicv/minco/actions/runs/30498077062).

Authorised replacement run `20260729t231646z-approved` stopped before
application, database or release work. Its fresh access key resolved on the
first identity attempt to exact run-owned user
`MincoSmokeBootstrap-ddf380d762c9`, but the immediately following first
`AssumeRole` returned `InvalidClientTokenId`. The bootstrap already retried
that propagation class during identity verification, but not during role
assumption.

The current correction adds that same fresh-key propagation class to the
existing role-assumption retry loop, still capped at 15 attempts two seconds
apart and still bound to the exact principal, role and one-hour session. It
also marks application invocation before the runner so cleanup can distinguish
a never-started application from a started run that must supply its existing
all-true receipt. The focused regression failed before implementation and
passes afterward. Independent exact-name checks confirm the application and
RDS stacks, bootstrap user and bootstrap role are absent; local temporary
credentials and profiles are absent. Exact-head hosted qualification, merge,
another live rehearsal, tag and registry publication remain blocked.

That fresh-key correction passed exact PR-head hosted run
[`30499941916`](https://github.com/xicv/minco/actions/runs/30499941916) at
`579e240328b3415dd8a839535c2efd8dbc6fcd40`, merged as exact `main`
`fbba94496e14fce0629efef78d5bee4f71aa132a`, passed the full local suite and
AWS Plan/SAM validation, and passed exact-main hosted run
[`30500931722`](https://github.com/xicv/minco/actions/runs/30500931722).
Authorised replacement run `20260730t001031z-approved` resolved its fresh
bootstrap key on the fifth bounded identity attempt and assumed the exact
temporary role on the first role attempt. It migrated and verified private
PostgreSQL, built the 5,038,349-byte ARM64 ZIP with SHA-256
`ff9609127cedcf2aad6c563e1f524feda1258ec33f104f7973eccecaa80ea474`,
sealed exact-source release `minco.d6168caadfd9d66f5d593c4d` with digest
`d6168caadfd9d66f5d593c4d2afb751f330dcff3b62162debe92d7df565546fd`,
and entered the digest-approved application apply from change-set receipt
`8ef973c492f41d89a934b8367278253d01edae50504568274c2dc41e7d02aeed`.

Both API Gateway V2 stages then failed. CloudFormation reported that exact
temporary role `MincoSmoke-2379eb7eebfa` lacked
`apigateway:TagResource` on
`arn:aws:apigateway:ap-southeast-2::/apis/sefukjj5f2/stages`. The candidate's
specialized statement had the correct stage-collection ARN and closed request
tag conditions but still named `apigateway:PUT`. IAM custom-policy simulation
returns `allowed` for the provider-evaluated `apigateway:TagResource` action on
that exact collection shape.

The current correction changes only the specialized action from
`apigateway:PUT` to `apigateway:TagResource`; the general
CloudFormation-called mutation statement, exact resource, three run-owned tag
values and ten-key allowlist remain unchanged. Access Analyzer currently calls
the literal action invalid even though the live provider requires it and IAM
custom-policy simulation returns `allowed`. The bootstrap accepts only that
single stale `INVALID_ACTION` finding at the exact structurally verified
statement index; fixtures reject an additional error, another location or a
broader tagging resource, and reject any additional action wildcard.
Application cleanup contains only true values. After the delayed RDS-managed
secret reached `ResourceNotFound`, the exact RDS cleanup verifier also contains
only true values. Independent exact-name checks confirm the application and
RDS stacks, artifact bucket, managed secret, bootstrap user and bootstrap role
are absent. Exact-head hosted qualification, merge, another live rehearsal,
tag and registry publication remain blocked.

Candidate `d9c2e541889aec007038bfe12cd60114ff863317` passed the
authoritative quality and Feedback browser stages of exact-head hosted run
[`30504351107`](https://github.com/xicv/minco/actions/runs/30504351107), then
the unpacked `minco-dev` archive test reported that a coordinated-shutdown
descendant survived. The fixture used `kill -0`, which reports a terminated
Linux zombie as present until the hosted runner reaps the orphan. Supervisor
cleanup already waits for the descendant-held log pipe to close, so the
assertion was measuring runner reaping rather than runnable process state. The
test-only correction uses portable Unix `ps` state, treats zombies as
terminated, and shares the helper with the lifecycle-descendant case. The full
nine-test supervisor suite and 100 repeated focused runs pass locally. No
production supervisor code changed. A replacement exact-head hosted run,
merge, exact-main qualification, live AWS, tag and registry publication remain
blocked.

That test correction passed exact-head hosted run
[`30505833178`](https://github.com/xicv/minco/actions/runs/30505833178) at
`bab0e8ca63ce4917251f7b5c75f0c17d37f4ccf2`, merged as exact `main`
`84598996a86067eb8b57015591a665445217af49`, and passed the full local suite,
AWS Plan/SAM validation and exact-main hosted run
[`30506695053`](https://github.com/xicv/minco/actions/runs/30506695053).

Authorised live run `20260730t020609z-approved` then proved the corrected
tagged-stage permissions: both API Gateway stages reached `CREATE_COMPLETE`.
The run stopped during hosted verification because API Gateway returned its
own `401 {"message":"Unauthorized"}` response for contract-public
`GET /health/live`. Exact AWS SAM translator `1.111.0` treats empty
operation-level `security: []` as absent when it applies
`Auth.DefaultAuthorizer`, replacing it with the default JWT authorizer.

The current renderer correction declares the JWT authorizer without a SAM
default, emits explicit `JwtAuthorizer` security on protected routes, and
retains `security: []` on public routes. The focused renderer regression covers
both route classes. Transforming the generated template with exact
`aws-sam-translator==1.111.0` preserves public security on both health routes
and JWT security on both Orders routes. All application, database, managed
secret, bootstrap-IAM and local credential resources from the failed run are
independently absent. Exact-head hosted qualification, merge, exact-main
qualification, another live rehearsal, tag and registry publication remain
blocked.

The named-stage correction passed exact PR-head hosted run
[`30532832860`](https://github.com/xicv/minco/actions/runs/30532832860) at
`d7e5a1c6e9ff5f5c43c754bc145bdefd63c7b60e`, merged as exact `main`
`73807d918bc860b60d592611f388bb63775d7c54`, passed the complete local
qualification and passed exact-main hosted run
[`30534601227`](https://github.com/xicv/minco/actions/runs/30534601227).
Both hosted boundaries passed authoritative quality, Feedback browser
evidence, the coordinated 28-package dry run, Plan/SAM/native ARM64 Lambda,
Rustack/SSM and explicit Orders E2E stages.

Authorised replacement run `20260730t104626z-release040` migrated and verified
private PostgreSQL, built the 5,039,398-byte native ARM64 artifact with SHA-256
`92dc989125a6032e378eaa660303939a9fadc0920bb3b2d0606bc2bcaaf86d11`,
sealed release `minco.789c2425846acb0fda2039f0` with digest
`789c2425846acb0fda2039f0eca3179978a48ce2be8af34ebb9b4ab42593c7b7`,
and applied the exact reviewed change set. Candidate liveness and readiness
passed, and the unauthenticated protected-order probe returned its expected
401. API Gateway supplied `apigw-requestid`, which the smoke verifier did not
recognize as a request ID, so verification failed before authenticated
mutation or promotion.

The bounded correction centralizes response request-ID extraction and accepts
Minco's `x-request-id`, Lambda/API Gateway's `x-amzn-requestid`, and API
Gateway's observed `apigw-requestid`. Executable fixtures cover all three
spellings and reject unrelated headers. Application cleanup is all true. An
exact bounded rerun after the RDS-managed secret deletion window proves the
temporary PostgreSQL stack, instance, managed secret and VPC absent, with
synthetic data and local secret files absent. Bootstrap user, role, profiles
and credential files are absent. Exact-head hosted qualification, merge,
exact-main qualification, another live rehearsal, tag and registry
publication remain blocked.

That correction passed exact PR-head hosted run
[`30539721321`](https://github.com/xicv/minco/actions/runs/30539721321) at
`8e97b38ef22608f849d531145f13dbf0e3e7243e`, merged as exact `main`
`30260209c49acb048f6549a31eb1e375fd1e923e`, passed the complete local
release matrix and passed exact-main hosted run
[`30542710147`](https://github.com/xicv/minco/actions/runs/30542710147).

Authorised live run `20260730t124426z-release040` migrated and verified its
disposable private PostgreSQL database, built the 5,039,398-byte native ARM64
artifact with SHA-256
`92dc989125a6032e378eaa660303939a9fadc0920bb3b2d0606bc2bcaaf86d11`,
sealed release `minco.761bb0f73b895275c78858ff`, and applied its reviewed
change set. Candidate liveness, readiness, unauthenticated 401, authenticated
place/get and idempotent replay all passed. Strict hosted-report construction
then rejected only the Authentication check because API Gateway's padded
request ID did not satisfy the narrower Rust character set. No promotion, tag
or registry upload occurred.

The bounded correction accepts one or two `=` characters only as trailing
request-ID padding. It retains the 128-byte limit and rejects empty IDs,
internal padding and all other unsupported characters. The focused regression
failed before the change and all 13 hosted-verification tests pass after it.
Exact cleanup reruns prove all application, database/VPC/managed-secret,
bootstrap-IAM and local credential resources absent. Exact-head hosted
qualification, merge, exact-main qualification, another live rehearsal, tag
and registry publication remain blocked.

The corrected source boundary passes `cargo test -p minco-plan --all-targets
--all-features --locked`, exact SAM translator `1.111.0` transformation, the
complete `scripts/quality.sh` matrix, AWS validation and deployment planning,
and the final source-manifest guard. This completes the source-preparation
task; exact-head and exact-main qualification remain release-controller gates
before another live rehearsal.

That correction passed exact-head hosted run
[`30509848637`](https://github.com/xicv/minco/actions/runs/30509848637) at
`b42909c17febb20109f1fa6cb66b419757130d23`, merged as exact `main`
`d760b0d9f833cc88d23a34b852c4f79ffd5f9e0c`, and passed exact-main hosted
run [`30511095728`](https://github.com/xicv/minco/actions/runs/30511095728).

Authorised live runs `20260730t034110z-release040` and
`20260730t040531z-diag` then reached the candidate integration but received
API Gateway's generic `500` before any Lambda log stream was created. The
diagnostic run captured the provider policy before cleanup: the unqualified
function held the only `lambda:InvokeFunction` statement, while
`get-policy --qualifier candidate` returned `ResourceNotFoundException`.
API Gateway invokes the qualified `candidate` ARN, so the Rust process was
never started.

The current correction gives `candidate` and `live` stable Lambda aliases and
separate API-scoped permissions. The initial `candidate`
`LiveFunctionVersion` sentinel points `live` at the generated version; after
promotion the preserved numeric parameter keeps ordinary updates from moving
live traffic. Promotion is now guarded to exactly one property modification on
`LiveFunctionAlias`, and its postcheck proves both aliases resolve to the
hosted-verified version and artifact. Exact SAM translator `1.111.0` resolves
the generated candidate alias/version references to concrete CloudFormation
resources. All deterministic application, database, secret, bootstrap-IAM and
local credential names from both failed runs are independently absent.
`./scripts/quality.sh`, `./scripts/aws/validate.sh`,
`./scripts/aws/plan.sh`, and the regenerated source-manifest check pass in the
isolated correction workspace. Exact-head hosted qualification, merge,
exact-main qualification, another live rehearsal, tag and registry
publication remain blocked.

That correction passed exact PR-head hosted run
[`30515135505`](https://github.com/xicv/minco/actions/runs/30515135505) at
`5b269157f456591fb5167c32277067ee88c15bae`, merged as exact `main`
`ccce06c180c29ba0f5c5471120b2d223a9baece9`, passed the complete local
qualification again and passed exact-main hosted run
[`30516228934`](https://github.com/xicv/minco/actions/runs/30516228934).

Authorised live run `20260730t053430z-release040` migrated and verified its
disposable private PostgreSQL database, built the deterministic 5,038,349-byte
native ARM64 artifact and sealed exact-source release
`minco.81b8b9d9bb94a9e711c28d3f`. The run-owned S3 bucket was created, blocked
from public access, encrypted and visible to the smoke runner. The deployment
controller's following `HeadBucket` nevertheless returned 404 before
packaging or change-set creation.

The current candidate adds the same bounded not-found policy at that second
CLI provider boundary. It retries only `404`, `NoSuchBucket` and `Not Found`,
fails every other error immediately and stops after 15 attempts. The focused
regression failed with a missing boundary before implementation and now covers
eventual success, non-404 fail-fast behavior and bounded exhaustion. The full
CLI and AWS shell portability suites pass. Later exact-name provider checks
confirm both stacks, the bucket, Cognito pool, SSM parameter, RDS instance,
managed secret and temporary IAM principals are absent. Exact-head hosted
qualification, merge, exact-main qualification, a replacement live rehearsal,
tag and registry publication remain blocked.

That correction passed exact-head hosted run
[`30519948680`](https://github.com/xicv/minco/actions/runs/30519948680) at
`612dbf16fd998538d941308079e2b9437d4be87e`, merged as exact `main`
`daae0595deffe945726df54c6f43ee82ff7bc7fd`, passed the complete local
qualification and passed exact-main hosted run
[`30521267303`](https://github.com/xicv/minco/actions/runs/30521267303),
including explicit Rustack and E2E stages.

Authorised live run `20260730t071445z-release040` migrated and verified private
PostgreSQL, reproduced the deterministic native ARM64 artifact, sealed and
verified exact-main release `minco.0b60a084c8c9029899e8fc27`, and applied its
reviewed change-set receipt. Candidate `GET /health/live` reached Lambda and
returned Minco's request ID, but Axum returned 404 because pinned
`lambda_http 1.3.0` prefixes named stages into the request URI unless
`AWS_LAMBDA_HTTP_IGNORE_STAGE_IN_PATH` is set. The replacement renderer sets
that switch for the Lambda environment and a focused regression covers it
alongside candidate/live alias isolation. The generated template is refreshed.
Direct exact-name provider checks prove all application, database/VPC,
managed-secret, storage, Cognito, SSM and bootstrap-IAM resources absent; three
resource-tag index entries are stale and their direct lookups return not found.
Exact-head hosted qualification, merge, exact-main qualification, another live
rehearsal, tag and registry publication remain blocked.

The stage-environment correction passed exact PR-head hosted run
[`30526281458`](https://github.com/xicv/minco/actions/runs/30526281458) at
`d5b4a76946a47bb4aeffb8be64b7460e1e61ce2d`, merged as exact `main`
`83d1583e9a385070306c95665a5219700cbc1c5e`, passed the complete local
qualification and passed exact-main hosted run
[`30527357088`](https://github.com/xicv/minco/actions/runs/30527357088).
Both hosted runs passed authoritative quality, the coordinated 28-package dry
run, Plan/SAM/native ARM64 Lambda, Rustack/SSM and explicit Orders E2E stages.

Authorised live run `20260730t085318z-release040` migrated and verified private
PostgreSQL, reproduced the 5,038,349-byte artifact with SHA-256
`ff9609127cedcf2aad6c563e1f524feda1258ec33f104f7973eccecaa80ea474`,
sealed exact-main release `minco.faf23ae016624d15d0b8f11f` with digest
`faf23ae016624d15d0b8f11f1e0d7cb20c13c3ea6d2ce247e416427e3b3e977e`,
and applied reviewed change-set receipt
`3d349a2be71b1aa04491f61f388780bb5c8d973e756aa4296c388103a8f27443`.
The application stack reached `CREATE_COMPLETE`, but candidate liveness still
returned Axum's empty 404 with Minco request ID
`1dcc9a69-cae5-4c68-ba8e-bac9fec24128`.

The live event and pinned `lambda_http 1.3.0` fixture refine the cause: API
Gateway v2 already supplies `/candidate/health/live` in `rawPath`. The
environment switch only suppresses additional stage insertion and therefore
cannot remove the existing prefix. The bounded release correction moves the
normalization to `minco-aws-lambda` before Axum route matching, strips only the
exact non-default stage from API Gateway v2 context, preserves URI authority
and query, and rejects prefix lookalikes. A realistic named-stage event now
matches the contract-owned `/health/live` route in-process. The ineffective
SAM environment setting is removed. This exact first-party adapter path is
added to the task boundary because no ready task owns the discovered release
blocker. The application cleanup receipt is all true. The initial database
cleanup check caught the RDS-managed secret during AWS's asynchronous deletion
window and failed closed; a bounded rerun then proves the exact temporary
PostgreSQL stack, instance, managed secret and VPC absent, with synthetic data
and local database secret files absent. The bootstrap user, role, profiles and
credential files are absent. Exact-head hosted qualification, merge, exact-main
qualification, another live rehearsal, tag and registry publication remain
blocked.

The padded-request-ID correction passed exact PR-head hosted run
[`30548150116`](https://github.com/xicv/minco/actions/runs/30548150116) at
`ade67d7f6d2866ed6bfde610742cf53660fe8ec9`, merged as exact `main`
`25ffdd4c38eba8e8a759cf7e83404fbfebd36e60`, passed the complete local release
matrix and passed exact-main hosted run
[`30550393414`](https://github.com/xicv/minco/actions/runs/30550393414).

Authorised live run `20260730t142515z-release040` migrated and verified its
private disposable PostgreSQL database, built the 5,039,398-byte native ARM64
artifact with SHA-256
`92dc989125a6032e378eaa660303939a9fadc0920bb3b2d0606bc2bcaaf86d11`,
sealed release `minco.31235789f783406088906750`, applied the exact reviewed
change set and passed every candidate hosted check. Promotion stopped before
creating or executing a change set because the bounded deployment role lacked
`cloudformation:DetectStackResourceDrift`; no live alias changed.

The current correction follows AWS's documented drift-detection dependency
set. It adds `DetectStackDrift` and `DetectStackResourceDrift` only to the two
exact run-owned stack ARN patterns, and places
`BatchDescribeTypeConfigurations` plus
`DescribeStackDriftDetectionStatus` in a separate discovery statement because
those actions do not support resource-level permissions. The focused rendered
policy regression failed before the change and passes after it, with no action
wildcard. Application cleanup is all true; an exact RDS recovery rerun and the
required `final-cleanup.json` prove the temporary database, managed secret,
VPC, bootstrap principals, profiles and local credential files absent.
Exact-head hosted qualification, merge, exact-main qualification, another live
rehearsal, tag and registry publication remain blocked.

The bounded drift-policy correction passed exact PR-head hosted run
[`30556566177`](https://github.com/xicv/minco/actions/runs/30556566177) at
`541e61e6fbb23a582011244539b2befddcd38fbf`, merged as exact `main`
`fbdcb002b5df7632e6233f3d08be97b13e571fb3`, passed the complete local
release matrix and passed exact-main hosted run
[`30558916893`](https://github.com/xicv/minco/actions/runs/30558916893).

Authorised live run `20260730t160831z-release040` migrated and verified its
private disposable PostgreSQL database, reproduced the 5,039,398-byte native
ARM64 artifact with SHA-256
`92dc989125a6032e378eaa660303939a9fadc0920bb3b2d0606bc2bcaaf86d11`,
sealed release `minco.2b93b493fa3a454d51a4cbcb`, applied the exact reviewed
change set and passed every candidate hosted check. CloudFormation drift
detection completed with the stack `IN_SYNC`, but promotion stopped before
creating or executing a change set because Minco deserialized the provider's
`DetectionStatus` as the nonexistent `StackDriftDetectionStatus`; no live
alias changed.

The current correction changes only the drift response-field binding. A
provider-shaped regression failed before the change with the exact live parse
error and passes after it. Failed, unknown and drifted responses retain their
existing fail-closed handling. Application cleanup is all true; an exact RDS
recovery rerun and the required all-true `final-cleanup.json` prove the
temporary database, managed secret, VPC, bootstrap principals, profiles and
local credential files absent. Exact-head hosted qualification, merge,
exact-main qualification, another live rehearsal, tag and registry
publication remain blocked.

The response-field correction passed exact PR-head hosted run
[`30563657881`](https://github.com/xicv/minco/actions/runs/30563657881) at
`f952af63d3848333c8a56782fe3b42e73dd457fd`, merged as exact `main`
`ff242141c98c4d555de3ed232dba4437ff59ee17`, passed the complete local release
matrix and passed exact-main hosted run
[`30565805289`](https://github.com/xicv/minco/actions/runs/30565805289).

Authorised live run `20260730t174217z-release040` migrated and verified its
private disposable PostgreSQL database, reproduced the 5,039,398-byte native
ARM64 artifact with SHA-256
`92dc989125a6032e378eaa660303939a9fadc0920bb3b2cbaaf86d11`,
sealed release `minco.b100be45a4972f08cb3a554f`, applied the exact reviewed
change set and passed every candidate hosted check. Promotion stopped before
creating or executing a change set because CloudFormation drift inspection
reported that the bounded role lacked
`lambda:GetProvisionedConcurrencyConfig` for the exact function versions and
aliases plus `logs:DescribeIndexPolicies`; no live alias changed.

The current least-privilege correction adds the Lambda read only to the exact
run-owned function ARN and qualified ARN pattern. It adds the CloudWatch Logs
List action to the existing wildcard-resource metadata-discovery statement,
matching AWS's service-authorization classification. The rendered-policy
regression failed before the change and passes after it while retaining exact
function boundaries. Application cleanup is all true; the exact RDS recovery
rerun and all-true `final-cleanup.json` prove the temporary database, managed
secret, VPC, bootstrap principals, profiles and local credential files absent.
Exact-head hosted qualification, merge, exact-main qualification and another
live rehearsal remain required before tag creation or registry publication.
