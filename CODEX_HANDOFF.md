# Minco 0.5.0 published handoff

Date: 2026-07-31
Published baseline: `0.5.0`
Current workspace version: `0.5.0`
Workspace release state: `published`
Published `0.5.0` source: `485d67104a49f139820722eb73334415f69a653c`
Last completed tasks: `M9-T09`, `M10-T07`, `M8-T09`
Next task: `M11-T01`

## Closed release boundary

Minco `0.5.0` is tagged, published as a GitHub release and available across the
complete 28-package crates.io family. Exact candidate and merged-main hosted
release qualification passed. Independent registry metadata, owner, checksum,
archive, installation, external consumer and docs.rs checks passed. No new AWS
deployment was part of the `0.5.0` publication; the separately authorised
`0.4.0` disposable AWS rehearsal remains the latest live proof. See
`VERIFICATION.md` for exact evidence categories.

Post-release registry verification is:

```bash
uv run --locked python scripts/validate_publish.py --expect-published
```

The command requires successful crates.io evidence for every exact workspace
version. It does not treat registry unavailability as a pass.

## Approved continuation

The published `0.5.0` program contains the standardized resource API,
local-authoritative CI split and zero-idle cost research. M11-T01 is the next
bounded step: build and deploy the tested, versioned, Laravel-inspired
documentation product without changing the published source tag.

The remaining planned program is:

1. build and deploy the tested, versioned documentation product;
2. complete plugin conformance and bounded two-application adoption evidence;
3. complete rollback/canary, static delivery and Verified Review Loop cleanup;
4. continue the planned AI workbench and 1.0 compatibility-freeze program.

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
