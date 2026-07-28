# Minco 0.4.0 release handoff

Date: 2026-07-28
Task: `M8-T07`
Published baseline: `0.3.1`
Candidate source: `0.4.0`
Starting `main`: `12839f3e802b2e47bf9088c82787a8aa9b1ec93d`
Workspace: `/Users/xicao/Projects/minco-m8-t07`

## Current boundary

M8-T07 reconciles the coordinated 28-package `0.4.0` source and package
candidate. It includes accepted framework work through M10-T03 and prepares one
reviewable release pull request.

Included:

- lock-step version/dependency metadata and four first-publish archive tests;
- changelog and `0.3.1` to `0.4.0` guide;
- Plan IR 2, typed configuration, database/seed lifecycle, graph-driven dev,
  generators/stubs, compatibility reports, packaging, change-set/apply
  receipts, hosted verification and exact-artifact promotion documentation;
- zero-provisioned-compute doctrine, residual cost vocabulary and the
  repository-native Verified Review Loop decision;
- recurring repository-truth diagnostics and generated roadmap/evidence;
- exact local and hosted source/package qualification evidence once complete.

Excluded without separate explicit approval:

- pull-request merge;
- any AWS resource mutation, hosted endpoint contact or alias promotion;
- `v0.4.0` creation/push;
- crates.io upload, ownership/trusted-publisher mutation or docs.rs claims.

Deferred product work:

- M10-T04 rollback/canary;
- M10-T05 static-site/custom-domain completion;
- M10-T06 review-environment lifecycle;
- M10-T07 database/service/cost research;
- M11 documentation/plugin ecosystem;
- M12 AI workbench and 1.0 freeze.

## Continuation gates

Source/package readiness does not authorize a live deployment rehearsal.
Before any later AWS phase, obtain explicit approval for the exact account,
Region, role, environment, change set, migration target and cleanup plan. Bind
all mutation to the exact merged-main release manifest and retain separate
deployment, hosted-verification, promotion and rollback evidence.

Before tag or publication, merge the reviewed pull request, rerun the complete
hosted workflow on the exact resulting `main` SHA, independently confirm all 28
`0.4.0` registry versions are absent, then obtain explicit tag/publication
approval. Publication is non-atomic and the four new crates require
first-release handling before trusted publishers can be configured.

## Recovery and safety

Use the primary colocated Git checkout only for transport:

```bash
cd /Users/xicao/Projects/minco
git fetch --all --tags --prune
jj git import
```

Continue source work only in `/Users/xicao/Projects/minco-m8-t07`. Preserve the
unrelated dirty `codex/production-closure` change in the primary checkout.
Check available disk before broad Rust/package/Lambda builds and clean only
this task workspace's disposable `target/` output when necessary.

## Exact evidence

Focused source, compiler, archive, external-consumer, generated-application,
browser, Orders E2E, Rustack and static AWS gates pass locally. The guarded
release controller performs one coordinated locked 28-package dry run, tests
the five configured unpacked archives offline, compiles the required facade
and new-package consumers from those archives, and installs the archive-only
CLI. No package was uploaded.

`VERIFICATION.md` records commands, measurements, hashes, diagnostic failures
and residual limits. The complete quality suite, clean final-source package
rerun and manual hosted workflow must still pass on the exact pull-request
head; neither historical green runs nor this local source state qualify a
different commit automatically.

The handoff verdict after those source/package gates is
`live_deployment_gate_pending`: no AWS account, role, Region, change set,
migration target, budget or cleanup authority was approved for the separately
bounded live rehearsal.
