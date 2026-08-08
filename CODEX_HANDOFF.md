# Minco 1.2.0 candidate handoff

Date: 2026-08-07
Published baseline: `1.1.0`
Current workspace version: `1.2.0`
Workspace release state: `candidate`
Published `1.1.0` source: `4d81543f7c5adb773655f23278abfe084de9f3e0`
Last completed release tasks: `M14-T01`, `M14-T04`, and `M14-T05`
Active candidate task: `M14-T07`

## Closed release boundary

Minco `1.1.0` is published from immutable tag `v1.1.0` at exact qualified
commit `4d81543f7c5adb773655f23278abfe084de9f3e0`. Exact PR-head and merged-main
release qualification passed. The guarded OIDC publication recovered from an
independently reconciled five-present/28-absent registry state, and independent
post-upload validation found all 33 exact versions non-yanked. GitHub release
`v1.1.0` is published from the same tag. No live AWS application deployment was
part of this crate release; bounded provider rehearsals retain their exact
historical source scope. See `VERIFICATION.md` for separate evidence lanes.

Post-release registry verification is:

```bash
uv run --locked python scripts/validate_publish.py \
  --expect-published --check-registry --require-registry
```

The command requires successful crates.io evidence for every exact workspace
version. It does not treat registry unavailability as a pass.

## Current release state

The 33 public packages advance together in the workspace to the unpublished
`1.2.0` candidate. M14-T07 adds topology-aware Plan cost/validation,
release-bound Feedback task receipts, deterministic operational evidence and
the digest-approved handover command without changing the published `1.1.0`
baseline. The frozen stable manual remains `1.1.0`; the `1.2.0` manual is
candidate-only. Stable Pages deployment remains a post-merge exact-SHA gate,
distinct from crates.io and docs.rs availability.

For a later release, independently verify current crates.io OIDC configuration,
the exact merged-main qualification, immutable tag identity and exact registry
state. Ownership or a previous successful OIDC run is not future authentication
evidence.

One task owns one isolated JJ workspace. Each task follows public-interface
RED/GREEN/refactor cycles, focused checks, relevant local qualification,
independent review and essential exact-head hosted checks before merge.

## CI and mutation boundary

`quality.toml` and the local qualification command are authoritative. GitHub
Actions should run only a small essential pull-request gate, plus separately
dispatched release/authentication workflows. Expensive all-feature, browser,
security, generated-application, package, native Lambda, Rustack, E2E and
documentation matrices remain local unless a later release gate explicitly
requires hosted evidence.

No AWS apply, cleanup, domain change, tag, crates.io upload, GitHub release or
production mutation is implicitly authorised by local qualification. Each
requires its exact target, digest and applicable explicit gate.

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
