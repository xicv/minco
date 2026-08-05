---
name: minco-release
description: >-
  Prepare and verify an exact Minco framework or application release while
  preserving artifact, migration, hosted, registry, deployment, runtime, and
  review gates. Use when, and only when, the user makes an explicit user request
  to prepare or execute a release; never infer release authority from completed
  feature work.
---

# Prepare a Minco release

Confirm the explicit user request and its boundary: prepare, qualify, publish,
deploy, or some separately authorized combination.

1. Resolve the exact source revision, version, changelog batch, package order,
   compatibility decision, migrations, target, and rollback candidate.
2. Run read-only preflight and the repository's specifically authorized focused
   qualification commands.
3. Build once, seal the artifact and manifest, and verify digests independently.
4. Keep local, hosted, registry, deployment, runtime, and review evidence
   separate. Never rebuild during promotion.
5. Require an exact current approval at every publish, migration, deployment,
   promotion, rollback, cleanup, or production-enablement boundary.

Stop before any action not named in the explicit user request. A green test,
ready task, merged PR, built artifact, dry run, empty check list, or earlier
approval does not authorize the next boundary.

Read [workflow.md](references/workflow.md) for release state separation.
