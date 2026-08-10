# Minco 1.2.1 candidate handoff

Date: 2026-08-10
Published baseline: `1.2.0`
Current workspace version: `1.2.1`
Workspace release state: `candidate`
Published `1.2.0` source: `48df3cc0ebb8990061b60d9383ced63532941079`
Published source-tree digest: `07846817724cca504b7deff8c80006a00930cf4d37513cc88b8aeac285a15933`
Published release task: `M14-T13`
Candidate release task: `M14-T14`
Active evidence task: `M14-T10`

## Closed release boundary

Minco `1.2.0` is published from immutable tag `v1.2.0` at exact qualified
commit `48df3cc0ebb8990061b60d9383ced63532941079`. Exact PR-head qualification,
merged-main qualification and guarded OIDC publication passed. Independent
post-upload validation found all 33 exact versions present and non-yanked.
GitHub release `v1.2.0` is published from the same tag. No live AWS application
deployment was part of this crate release; the performance baseline stays
`NOT RUN`, current-provider evidence records no contact, and historical
provider rehearsals retain their exact source scope. Post-publication PR #138
merged as `8f9ec1e566df1fa496909775c87b4ca23c07421e`; Pages run `31367645402`
passed and all 33 exact docs.rs routes returned HTTP 200. See `VERIFICATION.md`
for the separate evidence lanes.

Post-release registry verification is:

```bash
uv run --locked python scripts/validate_publish.py \
  --expect-published --check-registry --require-registry
```

The command requires successful crates.io evidence for every exact workspace
version. It does not treat registry unavailability as a pass.

## Current product state

The 33 public packages remain available together in the published `1.2.0`
line. The workspace advances them together to an unpublished compatible
`1.2.1` candidate that refreshes all eight packaged AI skills and makes their
release freshness fail closed. The stable 1.2 product line adds
browser/native HTTP metadata, verified direct uploads, rich observable mail,
owned local services, topology-aware Plan cost/validation, release-bound
Feedback task receipts, deterministic operational evidence and the
digest-approved handover command. The frozen stable manual is live at `1.2.0`,
and all exact docs.rs routes are reachable. Pages and docs.rs remain independent
from crates.io publication and from live-provider evidence.

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
