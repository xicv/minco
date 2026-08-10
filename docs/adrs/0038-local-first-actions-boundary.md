# ADR-0038: Reserve GitHub Actions for platform-only work

- Status: Accepted
- Date: 2026-08-10
- Supersedes: ADR-0013 hosted release profile

## Context

ADR-0013 correctly made local quality authoritative, but retained a full hosted
release profile. Later tasks also added task-specific workflows and a two-job
pull-request workflow for local service runtime qualification. GitHub evaluates
workflow files from the event's commit or ref, so a push to a task branch can
run a workflow that never reaches `main`. Deleting that file later also does
not change the already registered workflow state; stale registrations must be
disabled explicitly.

The 2026-08-10 audit found four workflow files on `main`, sixteen active remote
workflow registrations and 116 hosted-qualification runs between August 1 and
August 10 consuming about 1,503 runner wall-minutes. Sixty-nine caches occupied
10,693,043,757 bytes, above GitHub's default 10 GB repository cache allowance.
The Minco repository is public, so standard GitHub-hosted runners and Pages
runner use are currently free. Gross metered activity is therefore not the same
as a net charge, but queueing, cache pressure, failure noise and future private
repository cost remain real constraints.

Moving those jobs to a self-hosted GitHub runner would preserve workflow
sprawl, keep the GitHub control plane in the critical path and expose a durable
machine to untrusted public-repository code. GitHub recommends self-hosted
runners only for private repositories for that reason. Minco instead already
has a local authoritative gate and an exact, loopback-only Rustack boundary.

## Decision

The repository contains exactly three GitHub workflow files:

1. `docs-pages.yml` deploys the documentation product after path-filtered
   changes on `main` or a manual dispatch. Pages requires GitHub's deployment
   identity and its standard runner use is free.
2. `publish-crates.yml` obtains crates.io's short-lived OIDC token and uploads
   an exact tag. It verifies the tag, committed static/package/source-manifest
   evidence, first-publication rule and partial-publication registry complement.
   It does not repeat format, Clippy, workspace tests, generated applications,
   documentation or the complete quality matrix. The publish driver still
   performs its package dry-run, archive tests and external-consumer checks
   immediately before upload because those are part of safe publication.
3. `minco-manual.yml` is a read-only, `workflow_dispatch`-only clean-Linux
   compatibility check. It has no inputs, no cache, no artifacts and a
   20-minute timeout. It runs only `scripts/ci/hosted-essential.sh` after pinned
   toolchain setup.

Every substantive qualification command is local. `scripts/quality.sh` remains
authoritative. `scripts/ci/local-release.sh` adds AppSync proof, candidate
recovery/load, package dry-run, Plan/SAM and native Lambda builds, owned Docker
service lifecycle, Rustack S3/SQS/SSM/STS conformance and Orders E2E. The owned
runtime boundary is separately runnable through `scripts/ci/local-runtime.sh`.
These commands contact no public AWS endpoint and grant no publication,
deployment or promotion authority. The old full hosted release profile is
removed in its entirety, so 100% of that profile's qualification matrix now
runs locally; the retained clean-Linux check is a distinct compatibility slice.

`scripts/test/hosted_ci_policy.py` enforces the three-file allowlist, the small
manual workflow, the non-duplicating publisher and the retained local matrix.
`AGENTS.md` prohibits temporary or task-specific workflow files on every
branch. Remote stale-workflow and cache cleanup is an explicit maintainer
operation, not another scheduled cleanup workflow.

Repository-level workflow execution protections are a useful additional
defence when they can distinguish the Pages `main` push from arbitrary branch
pushes. Until that policy is configured and evaluated without blocking Pages,
the source allowlist, local policy test and disabled stale registrations are the
enforced boundary.

## Consequences

- Full release, runtime, Rustack and E2E work no longer consumes GitHub runner
  time or creates branch caches and artifacts.
- Linux compatibility remains available when that distinct evidence is useful,
  but it is not a routine merge gate and cannot select a full release profile.
- crates.io and Pages retain the GitHub identities their platforms require.
- Local evidence, clean-Linux compatibility, publication, real-provider smoke,
  deployment and production proof remain separate claims.
- A local machine must carry the pinned release tools and sufficient compute;
  the local release preflight reports missing tools before starting the matrix.
- The publisher trusts the reviewed exact tag's committed evidence and
  maintainer process instead of reproducing the full matrix on GitHub.

## Alternatives rejected

### Keep the hosted release profile but dispatch it less often

The observed 116 dispatches in ten days show that convention alone does not
bound usage. Removing the profile makes the policy executable.

### Register the development machine as a self-hosted runner

This retains Actions as the orchestrator and is unsafe for untrusted public
pull-request code. Direct local commands have the narrower authority boundary.

### Remove GitHub Actions completely

GitHub Pages deployment and crates.io trusted publishing need GitHub-provided
identity. A short clean-Linux compatibility check also detects environmental
assumptions that a macOS local gate cannot prove.

## References

- [GitHub Actions billing](https://docs.github.com/en/billing/concepts/product-billing/github-actions)
- [GitHub Actions dependency cache limits](https://docs.github.com/en/actions/reference/workflows-and-actions/dependency-caching)
- [GitHub workflow model](https://docs.github.com/en/actions/concepts/workflows-and-actions/workflows)
- [Workflow execution protections](https://docs.github.com/en/repositories/managing-your-repositorys-settings-and-features/actions-policies/workflow-execution-protections)
- [Adding self-hosted runners](https://docs.github.com/en/actions/how-tos/manage-runners/self-hosted-runners/add-runners)
