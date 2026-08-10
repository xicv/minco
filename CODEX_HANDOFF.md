# Minco 1.2.2 release handoff

Date: 2026-08-10
Published baseline: `1.2.2`
Current workspace version: `1.2.2`
Workspace release state: `published`
Published `1.2.2` source: `0496e6294b213c839af551a82858e2c1c3f7f45d`
Published source-tree digest: `c548cdb7c2aa967b2dcc1aa441d8a07861caecff46d33970b5b0bf80f73bf2a6`
Published release task: `M14-T15`
Active release task: `M14-T15`
Active evidence task: `M14-T10`

## Current release boundary

Minco `1.2.2` is a published, SemVer-compatible lock-step patch over the
immutable `1.2.1` baseline. It fixes homepage diagram overflow and operating
model alignment, carries those presentation checks into cumulative agent
release coverage, and changes no public Rust API, runtime or deployment
topology. Exact local qualification, PR-head and merged-main clean-Linux runs,
tag identity, OIDC upload, all 33 registry versions and the GitHub release are
verified separately. Stable Pages and docs.rs remain closeout gates until the
post-publication truth change reaches `main`.

Exact PR-head run `31395154514`, merged-main run `31395740260` and OIDC
publication run `31396167046` passed for source
`0496e6294b213c839af551a82858e2c1c3f7f45d`. Independent registry validation
found all 33 exact 1.2.2 versions present and non-yanked. No AWS application
resource was contacted or changed; current performance remains `NOT RUN` and no
production SLO is claimed.

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

The 33 public packages are available together in the published compatible
`1.2.1` line. The patch refreshes all eight packaged AI skills and makes their
release freshness fail closed. The stable 1.2 product line adds
browser/native HTTP metadata, verified direct uploads, rich observable mail,
owned local services, topology-aware Plan cost/validation, release-bound
Feedback task receipts, deterministic operational evidence and the
digest-approved handover command. The frozen 1.2.1 manual is available for
stable use, and every exact 1.2.1 docs.rs rustdoc route is reachable. Those
documentation checks remain independent from crates.io publication and
live-provider evidence.

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
