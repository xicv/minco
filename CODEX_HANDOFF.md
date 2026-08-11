# Minco 1.4.0 published-release handoff

Date: 2026-08-11
Published baseline: `1.4.0`
Current workspace version: `1.4.0`
Workspace release state: `published`
Published `1.4.0` source: `2b02bf956eed3ef2a17bae6d10970dff1408e231`
Published source-tree digest: `21ff73906bdfa441dcb44d5c8e9700332757b348b7f7e310c4e2cbddf51255f2`
Published release task: `M14-T19`
Latest release task: `M14-T19` (`active`)
Active evidence task: `M14-T10`

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
are independently verified. The post-publication stable Pages deployment remains
open at this snapshot. Performance remains `NOT RUN`; no live Waffo call, AWS
application mutation or production SLO is implied.

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
