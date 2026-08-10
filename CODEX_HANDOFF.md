# Minco 1.2.1 release handoff

Date: 2026-08-10
Published baseline: `1.2.1`
Current workspace version: `1.2.1`
Workspace release state: `published`
Published `1.2.1` source: `5f329ebbabef2840b01f10743f8dbb25a0b0dbe4`
Published source-tree digest: `4207fb168ee9c71eb7291efbf4dc03464a9009f7ae5889d34e09f030fca2caf3`
Published release task: `M14-T14`
Active evidence task: `M14-T10`

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
docs.rs reachability remain post-merge closeout gates. See `VERIFICATION.md`
for the separate evidence lanes.

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
digest-approved handover command. The frozen 1.2.1 manual is prepared for
stable promotion; live Pages and all exact docs.rs routes must still be checked
independently from crates.io publication and live-provider evidence.

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
