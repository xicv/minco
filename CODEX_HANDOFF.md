# Minco 1.1.0 release-candidate handoff

Date: 2026-08-06
Published baseline: `1.0.0`
Current workspace version: `1.1.0`
Workspace release state: `candidate`
Published `1.0.0` source: `39a69e36b051724c383da75d5907a824cbd2765b`
Last completed tasks: `M13-T01` through `M13-T06`
Active task: `M14-T01`

## Closed release boundary

Minco `1.0.0` is tagged, published as a GitHub release and available across the
complete 33-package crates.io family. Exact PR-head and merged-main hosted
release qualification passed. Independent registry verification found all 33
exact versions non-yanked. No new AWS deployment was part of the `1.0.0`
publication; the separately authorised bounded rehearsals retain their exact
historical source scope. See `VERIFICATION.md` for exact evidence categories.

Post-release registry verification is:

```bash
uv run --locked python scripts/validate_publish.py \
  --expect-published --check-registry --require-registry
```

The command requires successful crates.io evidence for every exact workspace
version. It does not treat registry unavailability as a pass.

## Current candidate state

The workspace advances the same 33 public packages to `1.1.0` and adds the
agent-native CLI/skill layer. The published baseline remains `1.0.0` until the
exact candidate is merged, qualified, tagged, uploaded and independently
verified. README and the versioned candidate manual document the new workflows;
stable navigation is promoted only after publication.

Before upload, independently verify crates.io OIDC authentication, the exact
main release qualification, immutable tag identity and all 33 absent exact
registry versions. Do not infer trusted-publisher readiness from historical
ownership alone.

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
