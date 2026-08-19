# Minco 1.9.0 traffic and compression candidate handoff

Date: 2026-08-19
Published baseline: `1.8.0`
Current workspace version: `1.9.0`
Workspace release state: `candidate`
Published `1.8.0` source: `fe1a20d4a6c76c7adef268727bb30b92b594e072`
Published source-tree digest: `99ee942d928c4e1b7626ce89a7f566d8a418a6c591b0d56743b8626837fdd00f`
Published release task: `M14-T37`
Latest release task: `M14-T39` (`active`)
Active evidence tasks: `M14-T10`, `M14-T20`

## Active 1.9.0 traffic and compression candidate

M14-T39 prepares the additive `1.9.0` family from merged `main`
`dc9ed98b05725589e5416411a9ac6b030ea70ee2`: the PR #171 traffic and
compression controls plus the PR #173 publish-workflow repair. The candidate
keeps all 34 packages, updates the cumulative agent feature coverage, freezes
the `docs-site/1.9.0/` manual and refreshes the source-bound evidence
receipts. Tag, GitHub release, OIDC publication, registry, docs.rs and Pages
remain separate later gates; no live AWS or provider evidence is claimed.

## Completed 1.8.0 release closure

PR #168 reviewed exact source `b589612b17c2288a92e176cb08543eb6eacb826b`
and tree `3def2f3b5852f418d92e9ed87e86395b67d9870f` with no unresolved
threads, passing exact-head clean-Linux run `31774750512` and a sealed security
review with zero findings. The guarded squash merge produced exact tree-equal
main commit `fe1a20d4a6c76c7adef268727bb30b92b594e072`. Merged-main clean-Linux
run `31775061737`, authentication-only OIDC run `31775371863` and publication
run `31775399279` passed. Immutable tag `v1.8.0`, all 34 exact non-yanked
registry versions, a fresh public CLI install and the GitHub release are
verified independently.

This closure changes release and documentation truth only. Stable Pages and
all exact docs.rs routes are verified independently as of 2026-08-19: the
registry validator passed with zero errors for all 34 exact packages, all 34
versioned docs.rs routes served HTTP 200, and the Pages site presented
`1.8.0 · Stable`. No AWS application
operation or production mutation occurred; live-provider, hosted-performance
and content-safety evidence remain absent or `NOT RUN`.

## Completed 1.8.0 candidate preparation

M14-T36 starts from exact merged `main`
`9e4e4c2b5b8e35457d4d45f94b4114236a775069`. It hardens and releases the
already merged direct object-transfer slice: authenticated upload/part/
complete/abort/download/metadata control operations, direct private S3 bytes,
range resume, immutable updates, private cache revalidation, quarantine and
structural cost accounting.

The application retains authorization, quotas, durable session state, logical
object pointers, retention and content inspection. The candidate adds no
default CDN, acceleration, scanner, scheduler, fixed compute or large-body
Lambda relay. The exact sealed source passes the complete macOS quality and
local-release gates from a clean JJ child: pinned assurance, additive SemVer
for all 34 packages, candidate load/recovery, package dry-runs, Plan/SAM and
Lambda builds, AppSync proof, owned PostgreSQL/Rustack runtime and Orders E2E.
Exact-head immutable security review, hosted Linux, merge, tag, OIDC
publication, registry, docs.rs, Pages, provider, deployment and production are
separate states.

## Completed 1.7.0 release closure

PR #163 reviewed exact source `22d62cb75a24011e2e83e9ccb3c4e07df4b02081`
and tree `31d279aca70e747ea934258ec2ce1548c66fd90d`, with zero unresolved
threads and passing clean-Linux run `31712458388`. It merged by guarded squash
as the same tree in commit `7773892792696ccf061ddbb49fa284e5ba7f6747`.
Merged-main clean-Linux run `31712808528`, authentication-only OIDC run
`31713263154` and publication run `31713475849` passed. Immutable tag
`v1.7.0`, all 34 exact non-yanked registry versions, a fresh public CLI install
and the GitHub release are verified independently.

The closure changes release and documentation truth only. Stable Pages and all
exact docs.rs routes remain separate until verified. No AWS application
operation or production mutation occurred; provider/performance and
model/human outcome lanes remain absent or `NOT RUN`.

## Completed 1.7.0 candidate preparation

M14-T32 advances the existing 34-package family and 19 official descriptors to
`1.7.0`, freezes the release manual and carries the already merged Apple-first
fresh local-service behavior into versioned release evidence. The immutable
previously published baseline was `v1.6.0`.

Existing receipts and exact owned resources remain authoritative. Docker is
still supported, and no automatic persistent-data migration or resource
deletion is part of the candidate. Local qualification, hosted clean Linux,
merge, tag, OIDC, registry, docs.rs, Pages, deployment and production proof are
separate states. The exact sealed source passes the complete macOS quality and
local-release gates from a clean JJ child: pinned measured assurance, additive
SemVer for all 34 packages, candidate load/recovery, package dry-runs,
Plan/SAM and Lambda builds, AppSync proof, owned PostgreSQL/Rustack runtime and
Orders E2E. No provider, registry, deployment or production claim is inferred.

## Active 1.6.0 release closure

PR #160 reviewed exact source `f47f28d696df9372a627c07b7590274e0da18dd9`
and tree `8747a5bf12991bc54263b635c1202912f729609d`, with zero unresolved
threads and passing clean-Linux run `31689050949`. It merged by guarded squash
as the same tree in commit `9abae9128dddc9bc32d099732e1421a0332e4785`.
Merged-main clean-Linux run `31689854658`, authentication-only OIDC run
`31689854606` and publication run `31690283715` passed. Immutable tag
`v1.6.0`, all 34 exact non-yanked registry versions and the GitHub release are
verified independently.

The closure changes release and documentation truth only. Stable Pages and all
exact docs.rs routes remain separate until verified. No AWS application
operation or production mutation occurred; provider/performance and
model/human outcome lanes remain absent or `NOT RUN`.

## Completed 1.6.0 candidate preparation

M14-T29 starts from merged audit source
`4bba904f498289bf2bfe6a4fa09a165e84e9d2e2`. It coordinates the additive
durable audit ledger, Orders golden slice, all 34 package versions, 19 official
descriptors, nine agent skills, upgrade guide and frozen 1.6 manual. The global
database default remains unchanged; the Orders DynamoDB profile is one
low-idle audit choice, not a universal default.

Candidate qualification, exact-head clean Linux, merge, immutable tag,
registry upload, docs.rs, Pages, provider, deployment and production proof are
separate. The task authorized candidate preparation and guarded source merge,
not tagging, publication or deployment.

## Previous 1.5.0 release closure

PR #157 reviewed exact head `0e6f02296ef69a84274eb74daed1dfaaccb50243`
and tree `6d7bd41cb1af0d83eb2e16324906a67b17643e0b`, with zero review
threads and passing clean-Linux run `31588777070`. It merged by guarded squash
as the same tree in commit `c3706559357510d33d046fa461f8550fbbd4c04c`.
Merged-main clean-Linux run `31593051123`, authentication-only OIDC run
`31593053757` and publication run `31593507996` passed. Immutable tag
`v1.5.0`, all 34 exact non-yanked registry versions and the GitHub release are
verified independently.

The closure changes release and documentation truth only. Stable Pages and all
exact docs.rs routes remain separate until verified. No AWS/Waffo application
operation or production mutation occurred; provider/performance and
model/human outcome lanes remain absent or `NOT RUN`.

## Completed 1.5.0 candidate preparation

M14-T23 starts from exact merged `main`
`ef7c3e30bebcae162d0c145ed4d9b6ba94cfc2f9` in the isolated
`/private/tmp/minco-task-m14-t23` JJ workspace. It packages only already merged
P0-P2 improvements: pinned measured local assurance, golden-topology cost
regression and five official typed side-effect fakes. It advances all 34
packages, official descriptors, nine AI skills and the frozen documentation in
lock-step without adding a provider capability or runtime resource.

The remaining application-specific agent-evaluation item is not silently
closed. No model was invoked and no human review effort was measured, so those
lanes remain `NOT RUN`. The exact source passed the complete macOS quality and
local-release gates from a clean JJ qualification child: pinned assurance, all
34 package archive dry-runs, SAM/Lambda builds, owned PostgreSQL/Rustack checks
and Orders E2E. Candidate preparation grants no authority to merge, tag,
publish, create a GitHub release, deploy, contact AWS/Waffo or mutate
production. Obtain clean-Linux, tag, registry, docs.rs and Pages evidence as
separate later gates.

## Active P0 assurance boundary

M14-T20 is isolated from exact merged `main`
`f48ead125b09699f1d7e8ab8bf02deeeb9dc6fb4`. It introduces a pinned local
assurance profile, deterministic release-identity projection, bounded
Plan/release mutation regressions and a private CLI command-schema module.
The exact base has 122 executable tests plus one doctest; four focused P0
regressions raise the final inventory to 126 plus one doctest. Base coverage
is 84.91%/80.98%; current coverage is 85.65% line/82.01% function. The 46 mutants have 43 caught, zero missed,
zero timeout and three unviable. Exact CLI help bytes are unchanged.

The P0 security review closed 23/23 diff worklist rows and found one Low/P3
evidence-integrity blocker. Its remediation authenticates every digest-addressed
private QA artifact using confined no-follow descriptors and makes the clean
release lane regenerate and verify ignored ephemeral assurance evidence instead
of trusting a frozen receipt without its private files.

Do not promote this local evidence into hosted Linux, AWS, Waffo, deployment,
production or SLO proof. Exact-tree hosted performance remains `NOT RUN` and
current live-provider evidence remains absent; M14-T20 stays active. This task
changes no workspace version, public API, Plan serialization, plugin
compatibility or supported provider set.

## Current 1.4.0 published boundary

Minco `1.4.0` is a published maintenance minor over the immutable 1.3.0
baseline. It contains the reviewed homepage presentation and reproducible
language/package refresh without changing the public Rust API, serialized
contracts, CLI, package inventory, static plugin selection or provider topology.
All 34 packages and official descriptors advance together; all nine packaged
Codex/Claude skills point to the frozen 1.4.0 manual and carry cumulative
maintenance-release coverage.

Exact local qualification, candidate and merged-main clean-Linux runs, immutable
tag, all 34 crates.io records, the GitHub release and all 34 exact docs.rs routes
are independently verified. Post-publication PR #152 passed exact-head
clean-Linux run `31482873533`, merged as
`9afd71cfa79362b98d9ff7497fc96e6235e1ce66`, and exact merged-main Pages
run `31483298491` deployed the stable site. Live browser acceptance covered the
root, 1.4.0, installation, plugins, Waffo, agent and versions routes.
Performance remains `NOT RUN`; no live Waffo call, AWS application mutation or
production SLO is implied.

## Completed ecosystem refresh

M14-T18 updates the direct Rust, uv, Node LTS, Playwright and immutable action
pins reviewed on 2026-08-11 without changing the published `1.3.0` API or
provider boundary. The complete macOS quality matrix and clean-JJ
`scripts/ci/local-release.sh` qualification pass, including package dry-runs,
archive consumers, SAM/Lambda builds, local Rustack and Orders E2E. The
repository retains VitePress `1.6.4` with tested Vite `6.4.3`; Vite 8 remains a
separate compatibility migration. Exact details and primary sources are in
`docs/research/language-package-ecosystem-review-2026-08.md`.

This maintenance qualification created no release, tag, upload, provider
contact or deployment. Hosted Linux performance remains `NOT RUN`, and current
live-provider evidence remains absent.

## Completed documentation correction

M14-T17 rebalances the homepage contract-to-cloud SVG without changing the
published crate family. Local browser, build and link checks passed, exact-head
clean-Linux run `31460666529` passed, PR #148 merged as
`21b70f1157f792ca20d70c724bf61974fa736695`, and merged-main Pages run
`31460937727` deployed successfully. The live SVG is byte-identical to reviewed
source and its 804 by 615 render retains the measured card and connector
spacing.

## Published 1.3.0 release boundary

Minco `1.3.0` is a published additive minor over the immutable 1.2.2 baseline.
It grows the lock-step family to 34 packages with one opt-in
Waffo Pancake beta plugin and grows the version-matched Codex/Claude bundle to
nine skills. Applications continue to own orders, subscriptions, entitlements
and payment projections; the plugin supplies only provider-specific checkout,
query and verified-webhook mechanics.

Exact source `e1fbb066e9332a2b6355b11a6f4b1c28806cc3e5` passed the complete
local macOS release gate and exact-main clean-Linux run `31451883403`.
Immutable tag `v1.3.0`, all 34 exact crates.io versions and the GitHub release
are independently verified, and all 34 exact docs.rs rustdoc routes return HTTP
200. Promotion PR #146 passed exact-head clean-Linux run `31457619990`, merged
as exact reviewed tree `3de7375ec5fdc5ec16ea240a4a142c33ff0a6c17` in main
commit `f46304d4c59061a1d4c118681eac45de748aadd4`, and merged-main Pages run
`31457889688` passed. The stable root, 1.3.0 manual, versions, Waffo payments,
local-development, files/static-sites, events/notifications/mail, plugins and
AI-agent routes return HTTP 200 with expected content. No live Waffo call, payment, AWS
deployment or production proof is authorised or implied by this handoff.

## Previous 1.2.2 release boundary

Minco `1.2.2` is a published, SemVer-compatible lock-step patch over the
immutable `1.2.1` baseline. It fixes homepage diagram overflow and operating
model alignment, carries those presentation checks into cumulative agent
release coverage, and changes no public Rust API, runtime or deployment
topology. Exact local qualification, PR-head and merged-main clean-Linux runs,
tag identity, OIDC upload, all 33 registry versions, the GitHub release, stable
Pages and docs.rs are verified separately.

Exact PR-head run `31395154514`, merged-main run `31395740260` and OIDC
publication run `31396167046` passed for source
`0496e6294b213c839af551a82858e2c1c3f7f45d`. Independent registry validation
found all 33 exact 1.2.2 versions present and non-yanked. No AWS application
resource was contacted or changed; current performance remains `NOT RUN` and no
production SLO is claimed.

Promotion PR #144 passed exact-head clean-Linux run `31399236714` and merged as
exact tree `92cad4c3e3cbd7912f0f711d44cfc375ddbc563e` in main commit
`62de61f7c8e510b93933e5337289a630e391b3e9`. Merged-main Pages run
`31399712561` passed. The root, frozen 1.2.2 manual and versions page return
HTTP 200 with 1.2.2 marked latest stable, and all 33 exact 1.2.2 docs.rs rustdoc
routes return HTTP 200.

## Closed release boundary

Minco `1.2.1` is published from immutable tag `v1.2.1` at exact qualified
commit `5f329ebbabef2840b01f10743f8dbb25a0b0dbe4`. Exact PR-head qualification
run `31378055301`, merged-main run `31378944090` and guarded OIDC publication
run `31379324388` passed. Independent post-upload validation found all 33 exact
versions present and non-yanked. GitHub release `v1.2.1` is published from the
same tag. No live AWS application
deployment was part of this crate release; the performance baseline stays
`NOT RUN`, current-provider evidence records no contact, and historical
provider rehearsals retain their exact source scope. Stable Pages and 1.2.1
docs.rs reachability are independently closed: promotion PR #141 passed
exact-head clean-Linux run `31383722610` and merged as exact tree
`2c0cb03598f879ae80cf5f60e8d106a7a910914f` in main commit
`140c7278c9c7f60cb7ce3be949583f17f0d71a17`; merged-main Pages run
`31384082079` passed, and all 33 exact 1.2.1 docs.rs rustdoc routes returned
HTTP 200. See `VERIFICATION.md` for the separate evidence lanes.

Post-release registry verification is:

```bash
uv run --locked python scripts/validate_publish.py \
  --expect-published --check-registry --require-registry
```

The command requires successful crates.io evidence for every exact workspace
version. It does not treat registry unavailability as a pass.

## Current product state

The 34 public packages are available together in the published compatible
`1.3.0` line. The release adds the Waffo plugin and a ninth packaged AI skill
while keeping cumulative release freshness fail closed. The stable 1.2 product
line added
browser/native HTTP metadata, verified direct uploads, rich observable mail,
owned local services, topology-aware Plan cost/validation, release-bound
Feedback task receipts, deterministic operational evidence and the
digest-approved handover command. The frozen 1.3.0 manual is the current stable
guide after its post-publication Pages change reached main. Documentation
checks remain independent from crates.io publication and live-provider
evidence.

For every later release, independently verify current crates.io OIDC configuration,
the exact merged-main qualification, immutable tag identity and exact registry
state. Ownership or a previous successful OIDC run is not future authentication
evidence.

One task owns one isolated JJ workspace. Each task follows public-interface
RED/GREEN/refactor cycles, focused checks, authoritative local qualification
and independent review before merge. The short manual clean-Linux workflow is
a distinct compatibility check when needed, not a substitute release matrix.

## CI and mutation boundary

`quality.toml`, `scripts/quality.sh` and `scripts/ci/local-release.sh` are
authoritative. GitHub Actions is limited to Pages, crates.io OIDC publication
and the short manually dispatched clean-Linux compatibility check. All-feature,
browser, security, generated-application, package, native Lambda, Rustack, E2E
and documentation matrices remain local.

No AWS apply, cleanup, domain change, later tag, later crates.io upload, GitHub
release or production mutation is implicitly authorised by local qualification.
Each requires its exact target, digest and applicable explicit gate.

## Recovery

Use the colocated primary checkout for GitHub transport and preserve unrelated
working copies:

```bash
cd /Users/xicao/Projects/minco
git fetch --all --tags --prune
jj git import
jj workspace list
```

Use `jj op log`, `jj undo` and `jj workspace update-stale` for recovery. Never
delete a task workspace until its exact merged state and retained evidence have
been verified.
