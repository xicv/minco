# Minco 1.0.0 release-candidate handoff

Date: 2026-08-05
Published baseline: `0.6.0`
Current workspace version: `1.0.0`
Workspace release state: `candidate`
Published `0.6.0` source: `2c4605b7d4abcd865035196ffc0484c4a0e82f1e`
Last completed tasks: `M12-T01` through `M12-T06`
Active task: none; the local 1.0 source candidate awaits separately authorised
hosted and release actions

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

## Current state

The workspace is an unpublished `1.0.0` source candidate with 32 publishable
packages. The four first-publication packages are `minco-plugin-realtime`,
`minco-project-view`, `minco-mcp` and `minco-workbench`. Exact local source,
package, generated-consumer, security, recovery, load, documentation and
artifact qualification is recorded under `verification/`. Hosted
qualification, merge, tag, registry publication, documentation publication,
live AWS deployment and production enablement remain separate gates; this
candidate does not reopen or rewrite the completed `0.6.0` evidence below.

The `0.6.0` release adds archive-visible plugin distribution records, one
public conformance kit and a detailed versioned documentation set. Tag,
registry, GitHub release, external-consumer, docs.rs and live stable-website
proof are complete. PR #74 merged the stable documentation as exact commit
`651a1886476556805991d83cbc71f9054f7703fe`; Pages run `30691854137`
deployed it and all 13 applicable live desktop/mobile browser checks passed.
M11-T08 is complete.

The remaining release path is:

1. obtain separately authorised hosted qualification for the exact resulting
   source after review/merge;
2. create and publish `v1.0.0` only under a separate explicit release
   authority, then verify every registry and documentation surface.

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
