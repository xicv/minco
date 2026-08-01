# Minco 0.6.0 release handoff

Date: 2026-08-01
Published baseline: `0.6.0`
Current workspace version: `0.6.0`
Workspace release state: `published`
Published `0.6.0` source: `2c4605b7d4abcd865035196ffc0484c4a0e82f1e`
Last completed tasks: `M11-T01`, `M11-T02`, `M11-T03`, `M11-T07`
Active task: `M11-T08`

## Closed release boundary

Minco `0.6.0` is tagged, published as a GitHub release and available across the
complete 28-package crates.io family. Exact PR-head and merged-main hosted
release qualification passed. Independent registry metadata, owner, checksum,
archive, installation, external consumer and docs.rs checks passed. No new AWS
deployment was part of the `0.6.0` publication; the separately authorised
`0.4.0` disposable AWS rehearsal remains the latest live proof. See
`VERIFICATION.md` for exact evidence categories.

Post-release registry verification is:

```bash
uv run --locked python scripts/validate_publish.py \
  --expect-published --check-registry --require-registry
```

The command requires successful crates.io evidence for every exact workspace
version. It does not treat registry unavailability as a pass.

## Current closeout work

The `0.6.0` release adds archive-visible plugin distribution records, one
public conformance kit and a detailed versioned documentation set. Tag,
registry, GitHub release, external-consumer and docs.rs proof are complete.
M11-T08 remains active only until the stable website commit is deployed and
verified from its exact source.

The remaining planned program is:

1. complete bounded two-application adoption evidence;
2. complete rollback/canary, static delivery and Verified Review Loop cleanup;
3. continue the planned AI workbench and 1.0 compatibility-freeze program.

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
