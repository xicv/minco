# Minco 1.0.0 release handoff

Date: 2026-08-05
Published baseline: `1.0.0`
Current workspace version: `1.0.0`
Workspace release state: `published`
Published `1.0.0` source: `39a69e36b051724c383da75d5907a824cbd2765b`
Last completed tasks: `M12-T01` through `M12-T08`
Active task: none

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

## Current state

The workspace matches the published `1.0.0` version and contains 33 public
packages. Realtime, ProjectView, MCP, Workbench and DynamoDB crossed their
first-publication ownership boundary. Exact local source, package,
generated-consumer, security, recovery, load and documentation qualification
is recorded under `verification/`; hosted qualification, registry publication,
GitHub release and stable documentation remain separately evidenced.

The next release-hardening recommendation is to configure and independently
verify crates.io trusted publishing for the five packages first published in
1.0.0. That work needs a new owning task and must not infer OIDC readiness from
package ownership alone.

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
