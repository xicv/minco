# Minco post-0.4.0 handoff

Date: 2026-07-31
Published baseline: `0.4.0`
Current workspace version: `0.4.0`
Exact release source: `65bf94045448bdbeedd37e10b1a004c926513508`
Last completed task: `M8-T08`
Next ready task: `M10-T07`

## Closed release boundary

Minco `0.4.0` is tagged, published as a GitHub release and available across the
complete 28-package crates.io family. Exact-main local and hosted
qualification passed. The final authorised disposable AWS rehearsal passed
contract, readiness, authentication, smoke and artifact-identity checks,
promoted the exact verified Lambda version without rebuilding, and retained
all-true cleanup evidence. See `VERIFICATION.md` for exact commands and
evidence categories.

Post-release registry verification is:

```bash
uv run --locked python scripts/validate_publish.py --expect-published
```

The command requires successful crates.io evidence for every exact workspace
version. It does not treat registry unavailability as a pass.

## Approved continuation

The post-`0.4.0` program targets a qualified `0.5.0` candidate:

1. close repository truth and release-state documentation;
2. make comprehensive qualification local and source-bound;
3. retain only essential merge-safety CI in GitHub Actions;
4. complete zero-idle research, evidence and the deep AWS application profile;
5. complete rollback/canary, static delivery and Verified Review Loop cleanup;
6. complete plugin conformance and bounded two-application adoption evidence;
7. build the tested, versioned Laravel-inspired documentation product last;
8. prepare, but do not publish, the exact `0.5.0` candidate.

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
