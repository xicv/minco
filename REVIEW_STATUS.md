# Review status

## Completed M14-T37 Minco 1.8.0 published release closure

Minco `1.8.0` is the current published baseline from exact release source
`fe1a20d4a6c76c7adef268727bb30b92b594e072`. PR #168 reviewed tree
`3def2f3b5852f418d92e9ed87e86395b67d9870f` with zero unresolved review
threads and a sealed security scan with zero findings, passed exact-head clean
Linux, and merged as the same tree. Immutable tag `v1.8.0`, publication run
`31775399279`, all 34 exact non-yanked crates.io records, a fresh public CLI
install and the GitHub release are independently verified.

This truth-only closure changes no Rust API, Plan IR, plugin capability,
runtime selection, provider topology or default cost profile. Stable Pages and
exact docs.rs routes are verified independently as of 2026-08-19: the registry
validator passed with zero errors for all 34 exact packages, all 34 versioned
docs.rs routes served HTTP 200, and the Pages site presented `1.8.0 ·
Stable`. Live AWS, production, hosted
performance and content-safety evidence remain unproven or `NOT RUN`.

## Completed M14-T36 Minco 1.8.0 object-transfer candidate

The candidate starts from exact merged `main`
`9e4e4c2b5b8e35457d4d45f94b4114236a775069` and published baseline
`v1.7.0`. Review must prove the authenticated JSON control plane stays bounded,
large bytes remain direct, authorization precedes cache decisions, multipart
and range validators are exact, updates use immutable revisions, untrusted
completion stays quarantined and cost claims remain structural.

The 34-package family and 19 official descriptors advance together with no new
package. Existing buffering/single-upload APIs remain compatible, while non-S3
providers need the additive transfer implementations before claiming resumable
HTTP support. Local, security, hosted, merge, tag, registry, docs.rs, Pages,
live-provider, deployment and production proof remain separate.

The exact sealed source passes the complete macOS quality and local-release
matrix from a clean JJ child. Pinned assurance proves 127 nextest tests plus one
doctest, 85.78% line and 81.97% function coverage, 43 caught viable mutants
with zero misses/timeouts, and additive compatibility for all 34 packages
against `v1.7.0`. Candidate load/recovery, package archives, Plan/SAM, native
Lambda builds, AppSync proof, owned PostgreSQL/Rustack runtime and Orders E2E
also pass locally. Exact-head immutable security review, hosted Linux, merge,
tag, registry, docs.rs and Pages remain independent gates; live provider,
deployment, production and hosted performance evidence remain absent or
`NOT RUN`.

## Completed M14-T33 Minco 1.7.0 published release closure

Minco `1.7.0` is the current published baseline from exact release source
`7773892792696ccf061ddbb49fa284e5ba7f6747`. PR #163 reviewed tree
`31d279aca70e747ea934258ec2ce1548c66fd90d` with zero unresolved review
threads, passed exact-head clean Linux and merged as the same tree. Immutable
tag `v1.7.0`, publication run `31713475849`, all 34 exact non-yanked crates.io
records, a fresh public CLI install and the GitHub release are independently
verified.

This truth-only closure changes no Rust API, Plan IR, plugin capability,
runtime selection, provider topology or Docker fallback. Stable Pages and exact
docs.rs routes remain independent gates. Live AWS, production, hosted
performance, model outcome and measured human-review evidence remain unproven
or `NOT RUN`.

## Completed M14-T32 Minco 1.7.0 candidate preparation

The exact published `1.6.0` baseline comes from release source
`9abae9128dddc9bc32d099732e1421a0332e4785`. The workspace is an unpublished
`1.7.0` candidate that coordinates the existing 34-package family, 19 official
descriptors, nine skills, frozen documentation and Apple-first fresh automatic
local-service selection.

Review must prove additive SemVer against immutable `v1.6.0`, exact receipt and
resource precedence, Apple selection, Docker fallback and the absence of
implicit data migration or deletion. Qualification, hosted Linux, merge, tag,
registry, docs.rs, Pages, provider, deployment and production remain separate.

The exact sealed source passes the complete macOS quality and local-release
matrix from a clean JJ child. Pinned assurance proves 127 nextest tests plus one
doctest, 85.80% line and 81.97% function coverage, 43 caught viable mutants
with zero misses/timeouts, and additive compatibility for all 34 packages
against `v1.6.0`. Candidate load/recovery, package archives, Plan/SAM, native
Lambda builds, AppSync proof, owned PostgreSQL/Rustack runtime and Orders E2E
also pass locally; hosted, registry, provider and production states remain
independent.

## Active M14-T30 Minco 1.6.0 published release closure

Minco `1.6.0` is the current published baseline from exact release source
`9abae9128dddc9bc32d099732e1421a0332e4785`. PR #160 reviewed tree
`8747a5bf12991bc54263b635c1202912f729609d` with zero unresolved review
threads, passed exact-head clean Linux, and merged as the same tree. Immutable
tag `v1.6.0`, publication run `31690283715`, all 34 exact non-yanked crates.io
records and the GitHub release are independently verified.

This truth-only closure changes no Rust API, Plan IR, plugin capability, audit
storage semantics, provider topology or runtime selection. Stable Pages and
exact docs.rs routes remain independent gates. Live AWS, production, hosted
performance, model outcome and measured human-review evidence remain unproven
or `NOT RUN`.

## Completed M14-T29 Minco 1.6.0 candidate preparation

The candidate starts from exact merged audit source
`4bba904f498289bf2bfe6a4fa09a165e84e9d2e2`. Its release boundary is the
additive schema-agnostic ledger, separate SQL storage/journal relay, atomic
DynamoDB audit table and permission-gated Orders history already merged by PR
#159. It advances the existing 34-package family, 19 descriptors, nine skills
and versioned manual together.

Review proved compatibility against immutable `v1.5.0`, retained-growth
and incomplete-cost truth, exact local qualification, exact-head hosted Linux,
zero unresolved threads and reviewed-tree merge identity. Tag, registry,
docs.rs, Pages, provider, deployment and production states remain separate.

## Previous M14-T24 Minco 1.5.0 published release closure

Minco `1.5.0` was the previous published baseline from exact release source
`c3706559357510d33d046fa461f8550fbbd4c04c`. PR #157 reviewed tree
`6d7bd41cb1af0d83eb2e16324906a67b17643e0b` with zero review threads,
passed exact-head clean Linux, and merged as the same tree. Immutable tag
`v1.5.0`, publication run `31593507996`, all 34 exact non-yanked crates.io
records and the GitHub release are independently verified.

This truth-only closure changes no Rust API, Plan IR, plugin capability,
provider topology or runtime selection. Stable Pages and exact docs.rs routes
remain independent gates. Live AWS/Waffo, production, hosted performance,
model outcome and measured human-review evidence remain unproven or `NOT RUN`.

## Completed M14-T23 Minco 1.5.0 candidate preparation

The release workspace starts from exact merged `main`
`ef7c3e30bebcae162d0c145ed4d9b6ba94cfc2f9`. An architecture audit found no
additional safe implementation that passed the deletion test: the sole
remaining P2 item requires an actual model-driven application run and measured
human review. The candidate therefore packages only already merged work and
retains those outcome lanes as `NOT RUN`.

The intended 1.5 boundary is additive: five official typed side-effect fakes,
the golden-topology cost regression and pinned measured local assurance. It
changes no production adapter selection, Plan IR, CLI name, provider topology,
package inventory, schedule, poller, fixed compute or control plane. All 34
packages, 19 official descriptors, nine AI skills and the frozen manual move
together. The exact source passes the complete macOS quality and local-release
gates, including pinned assurance, all 34 archive dry-runs, SAM/Lambda builds,
owned PostgreSQL/Rustack checks and Orders E2E. Tag, registry, docs.rs, Pages,
hosted Linux and live-provider states remain independent; no publication,
provider contact or deployment occurred during preparation.

## Completed M14-T22 typed side-effect fakes

The P2 task starts from exact merged `main`
`958e6ebf40db1f63614cf9a3da0e0af65188eafe` in the isolated
`/private/tmp/minco-task-m14-t22` JJ workspace. M14-T20 already completed the
SemVer, selected-crate coverage and bounded mutation pilots; this change owns
the next missing controlled pilot.

Five additive public fakes implement their owning ports: SQS message handling,
domain-event publication, object storage, feedback persistence and rich mail.
They retain typed ordered attempts, consume explicit failures once, exercise
the real partial-batch/fallback paths and keep private payloads out of `Debug`.
They add no generic mocking facade, provider dependency, production selection,
schedule, poller, fixed compute or control plane.

Each tracer test first failed on the absent public fake and then passed after
the minimum implementation. The combined all-feature focused package run
passes 95 unit, integration and adapter tests, including real SQS partial-batch
and mail fallback behavior. Targeted Clippy passes with warnings denied. The
complete `./scripts/quality.sh` matrix passes, including generated references,
53 repository-truth mutations, deterministic Codex/Claude workflow
qualification, both browser suites, workspace Clippy/tests, generated
PostgreSQL and SQLite applications, documentation, package policy, dependency
audits, gitleaks and final source-manifest verification.

Publication validation initially failed closed because five explicit
`package.include` lists omitted the new integration tests. Every affected
manifest now includes its test sources and all 34 publishable packages
validate. Exact-head clean-Linux review remains a separate pre-merge gate and
is not inferred from the passing local macOS matrix.

No AWS, mailbox, object provider, deployment, publication or production
evidence is inferred from these provider-free application fakes.

## M14-T21 golden-topology cost regression

The isolated P1 change starts from exact merged `main`
`b9e2cf3b0621cfe67487142e609a6c26cf7391ee`. It adds a deterministic,
provider-free regression gate over seven reviewed Orders configurations:
local SQLite, Neon Free, Neon Launch, Aurora Serverless v2, provisioned RDS,
self-hosted PostgreSQL and DynamoDB on-demand.

Each baseline record binds exact configuration bytes, a readable canonical
`cargo minco cost --json` projection and its SHA-256. Normal local and bounded
clean-Linux quality lanes check the baseline without regenerating it. Nine
focused tests cover semantic drift, duplicate records and paths, non-canonical
or non-finite JSON, missing files, critical zero-idle invariants, symlinks,
bounded CLI execution, secret-free failure output and exact reproduction.

The final local quality matrix and the bounded hosted-essential script pass on
the source recorded by this change. Exact-head clean-Linux execution remains a
separate pre-merge gate and is not inferred from the local macOS result.

The baseline does not fetch provider prices, define a production budget or
qualify AWS behavior. It changes no Plan IR schema, public Rust API, CLI output,
plugin compatibility, provider selection or deployment topology, and adds no
runtime resource, schedule, poller or control plane.

## Active M14-T20 measured framework assurance

The current isolated change is based on exact published `main`
`f48ead125b09699f1d7e8ab8bf02deeeb9dc6fb4`. It adds pinned, measured
nextest/coverage/mutation/SemVer assurance, a deterministic release-identity
projection, focused Plan/release invariants and one behavior-preserving private
CLI schema extraction. The CLI help output remains byte-identical with SHA-256
`ce7f5203366875eeb62daf3f1584eba5eb7f2b91b7930f8b59b1de0dfdf5d2f7`.

The exact base contained 122 executable tests plus one doctest for the selected
core packages. Four focused P0 regressions raise the final inventory to 126
plus one doctest. The base measured 84.91% line/80.98% function coverage;
the focused regressions raise current coverage to 85.65%/82.01%. There are 46 bounded mutants:
43 caught, zero missed/timeouts and three unviable. The four quality tools are
exactly pinned and the immutable `v1.4.0` SemVer baseline resolves to
`2b02bf956eed3ef2a17bae6d10970dff1408e231`.

The completed exact-head security scan covered 23/23 source-like rows with zero
deferred work. Its only finding was a Low/P3 frozen-receipt authentication gap.
The current remediation verifies every private evidence file by exact bytes and
SHA-256 through retained no-follow descriptors, rejects symlink or replacement
paths, and regenerates release assurance under ignored `target/minco` paths.
The canonical receipt remains checkable only in the workspace that retains its
matching private artifacts.

M14-T20 remains active because exact-tree hosted Linux performance and current
live-provider evidence are unavailable. Local macOS performance is diagnostic,
provider-free and never a production SLO. No public API, serialized Plan IR,
plugin compatibility, version, provider support or deployment topology changes.

The following retained section records the published `1.4.0` maintenance-minor
release at the time it was current.

Published baseline: `1.4.0`

Current workspace version: `1.4.0`

Workspace release state: `published`

The current review covers lock-step version and descriptor consistency, the
frozen 1.4.0 manual, homepage desktop/mobile presentation regressions, the
reproducible dependency/toolchain refresh, cumulative nine-skill release
coverage, package archives and exact evidence truth. Public Rust APIs,
serialized contracts, CLI behavior, provider selection and deployment topology
are unchanged. No new package, provider capability, hidden worker, poller,
schedule, AWS resource or always-on control plane is introduced.

Exact local release qualification, candidate and merged-main clean-Linux runs,
immutable tag, all 34 crates.io records and the GitHub release are independently
verified. All 34 exact docs.rs routes return HTTP 200. Post-publication PR #152
passed exact-head clean-Linux run `31482873533`, merged as
`9afd71cfa79362b98d9ff7497fc96e6235e1ce66`, and exact merged-main Pages
run `31483298491` deployed the stable site. Live-provider and production
evidence remain separately unproven.

The historical PR #125 source was reconstructed onto current `main`; the
task-specific workflow was removed and the exact three-file workflow allowlist
fails closed in both static and policy tests. Exact release source passed the
complete local gate and clean-Linux run `31451883403`. Immutable tag `v1.3.0`,
all 34 crates.io versions and the GitHub release are verified independently.
All 34 exact docs.rs routes return HTTP 200. Promotion PR #146 passed exact-head
clean-Linux run `31457619990`, merged as exact reviewed tree
`3de7375ec5fdc5ec16ea240a4a142c33ff0a6c17` in main commit
`f46304d4c59061a1d4c118681eac45de748aadd4`, and merged-main Pages run
`31457889688` passed. The stable root, 1.3.0 manual, versions, Waffo payments,
local-development, files/static-sites, events/notifications/mail, plugins and
AI-agent routes return HTTP 200 with expected content. Live Waffo, deployment
and production evidence remain separate; no provider or deployment claim is
inferred from publication.

## Previous published 1.4.0 baseline

Minco `1.4.0` was the previous published baseline from exact release source
`2b02bf956eed3ef2a17bae6d10970dff1408e231`, tree
`e9e5138eed39d48d0d58cb7440310f198695f47b` and source-tree digest
`21ff73906bdfa441dcb44d5c8e9700332757b348b7f7e310c4e2cbddf51255f2`.

Candidate run `31475310242` and merged-main run `31475705506` passed. OIDC
publication run `31476217865` accepted 23 packages before the missing Waffo
trusted-publisher entry failed closed; guarded recovery run `31479118464`
verified the exact complement and published only the 11 absent packages. The
GitHub release is published from immutable tag `v1.4.0`.

## Previous published 1.2.2 baseline

The release corrects diagram text overflow and the ordered-list cascade that
misaligned the four operating-model cards. Local, hosted, tag, registry,
docs.rs, Pages, provider, runtime and production evidence remain distinct.
Local and clean-Linux qualification, tag identity, registry publication, the
GitHub release, stable Pages and docs.rs are independently verified.

Minco `1.2.2` is the previous published baseline from exact release source
`0496e6294b213c839af551a82858e2c1c3f7f45d`, tree
`577caf88f99746b2ac62b50ad90f3e5ea1f66b4e` and source-tree digest
`c548cdb7c2aa967b2dcc1aa441d8a07861caecff46d33970b5b0bf80f73bf2a6`.
PR-head run `31395154514` and merged-main run `31395740260` passed. OIDC
publication run `31396167046` uploaded the exact `v1.2.2` tag, independent
registry validation found all 33 exact versions present and non-yanked, and the
GitHub release is published from the same tag. Promotion PR #144 passed
exact-head run `31399236714` and merged as exact tree
`92cad4c3e3cbd7912f0f711d44cfc375ddbc563e` in main commit
`62de61f7c8e510b93933e5337289a630e391b3e9`. Pages run `31399712561`
passed; the root, frozen 1.2.2 manual and versions page returned HTTP 200 with
current stable truth, and all 33 exact 1.2.2 docs.rs rustdoc routes returned
HTTP 200. No live AWS application evidence or production performance SLO is
claimed.

The workspace is the published `1.2.1` lock-step release for release-bound AI
skill freshness. Its local, hosted, tag and registry checks remain separate
from docs.rs, stable-site, provider, runtime and production evidence. The
stable-site and docs.rs lanes are now independently closed; provider, runtime
and production claims remain absent or historical.

Minco `1.2.1` is the previous published baseline from exact release source
`5f329ebbabef2840b01f10743f8dbb25a0b0dbe4` and source-tree digest
`4207fb168ee9c71eb7291efbf4dc03464a9009f7ae5889d34e09f030fca2caf3`.
The coordinated 33-package release adds browser/native HTTP metadata, verified
uploads, rich observable mail, owned local services and release-bound delivery
evidence to the existing agent-native, realtime, lifecycle,
ProjectView/MCP/workbench and DynamoDB boundaries. Current release truth is guarded by
`verification/repository-truth.toml`; the detailed records below are retained as
historical release and provider evidence rather than current-version claims.

Exact 1.2.1 candidate run `31378055301` and merged-main run `31378944090`
passed before tagging. OIDC publication run `31379324388` uploaded the exact
`v1.2.1` tag, and independent registry validation found all 33 exact versions
present and non-yanked. The GitHub release is published from the same tag.
Promotion PR #141 passed exact-head clean-Linux run `31383722610` at
`681fd11bf078fdd4c0f8eb7a26f0703ca3f7e4b4` and merged as exact tree
`2c0cb03598f879ae80cf5f60e8d106a7a910914f` in main commit
`140c7278c9c7f60cb7ce3be949583f17f0d71a17`. Exact merged-main Pages run
`31384082079` passed; the root, frozen 1.2.1 manual and versions page returned
HTTP 200 with current stable truth, and all 33 exact 1.2.1 docs.rs rustdoc
routes returned HTTP 200. No live AWS application evidence or production
performance SLO is claimed.

Exact local release qualification and hosted run `31360400586` passed before
tagging. OIDC publication run `31362919458` uploaded the exact tag after the
dependency-prefetch recovery in PR #137, and registry validation found all 33
exact versions present and non-yanked. Post-publication PR #138 merged as
`8f9ec1e566df1fa496909775c87b4ca23c07421e`; exact merged-main Pages run
`31367645402` passed, the live stable routes returned HTTP 200 and all 33 exact
docs.rs routes returned HTTP 200. The release does not claim current live AWS
application evidence or a production performance SLO; M14-T10 remains active
with those states recorded as `not_run` or stale.

## Historical release evidence

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
