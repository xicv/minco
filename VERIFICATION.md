# Minco verification and release evidence

Date: 2026-08-05
Current workspace version: `1.0.0`
Published baseline: `1.0.0`
Workspace release state: `published`
Purpose: retain exact release evidence and distinguish source, hosted,
registry, documentation and live-deployment proof.

## M12-T08 `1.0.0` publication and stable documentation promotion

The immutable `v1.0.0` tag resolves to exact merged main
`39a69e36b051724c383da75d5907a824cbd2765b`. Exact-head hosted release run
[`30986838335`](https://github.com/xicv/minco/actions/runs/30986838335) and
exact-main hosted release run
[`30990218161`](https://github.com/xicv/minco/actions/runs/30990218161) passed
every release gate before upload.

The complete dependency-ordered manual publication accepted all 33 public
crates, including the five first-publication packages. The independent
post-upload command
`uv run --locked python scripts/validate_publish.py --expect-published
--check-registry --require-registry` reported `errors: 0` and
`registry_checks_succeeded: 33`. GitHub release
[`v1.0.0`](https://github.com/xicv/minco/releases/tag/v1.0.0) is published from
the same tag. Establishing ownership does not by itself verify trusted
publisher configuration for the five new crates.

Pages run
[`30990196620`](https://github.com/xicv/minco/actions/runs/30990196620)
deployed the candidate site from exact main before registry publication. This
task promotes that already-qualified manual to stable in a separate source
change and retains its post-merge Pages deployment as a separate gate.

No live AWS application resource was created, modified, promoted or deleted by
the crate release or documentation promotion.

## M12-T07 `1.0.0` exact release closure

At the M12-T07 source boundary, the unpublished workspace contained the complete 33-package 1.0 family:
the published 28-package 0.6 baseline, realtime, ProjectView, local MCP,
Workbench, and the post-candidate DynamoDB package. M12-T07 requalifies the
exact descendant source instead of extending the earlier 32-package evidence
by narrative.

`verification/1.0-candidate-release-gates.json` records the source-tree digest,
file count, all mandatory command statuses, exit codes, durations and private
log digests. Its command set includes the complete quality/security matrix,
the standalone AppSync consumer proof, Feedback browser tests, HTTP E2E,
Rustack, the 33-package publish dry run and package inventory, recovery, load,
and a final source-manifest check. Recovery and load details remain separate
schema-validated exact-source records.

The checked-in CLI integration now proves the MCP 2026-07-28
`server/discover` lifecycle with per-request metadata. The standalone AppSync
workspace is locked to the 1.0 dependencies and is a mandatory local and hosted
release gate. The versioned documentation site contains a frozen 1.0.0 manual,
candidate navigation, all 18 catalog components, and dedicated realtime,
DynamoDB, ProjectView/MCP/workbench and complete deployment-lifecycle guidance.

Publication remains ordered and non-atomic. Five crates require manual first
publication, so the documented 1.0 procedure uses one short-lived manual token
for the complete exact-tag family. The OIDC workflow refuses `publish=true`
before requesting a token while repository truth lists a first-publication
crate; this prevents the existing family from being partially uploaded before
new-crate ownership exists.

That local source record did not itself claim a `v1.0.0` tag, crates.io upload,
GitHub release, stable documentation promotion, hosted exact-head run, AWS
deployment or application adoption. The first four later gates are recorded in
M12-T08 above; AWS deployment and application adoption remain unclaimed.

## M11-T11/M11-T12 `0.7.0` realtime candidate

The unpublished 29-package source candidate adds
`minco-plugin-realtime`, minimal AppSync Events planning/publication and a
subscriber-only browser facade. It does not claim a tag, crates.io publication
or production enablement. Exact local, hosted and merge evidence is recorded in
M11-T11; bounded disposable provider evidence is recorded in M11-T12. The
completed `0.6.0` evidence below remains the published baseline.

M11-T12 passed its exact-source live proof on 2026-08-05 at
`2d721700998ad9abf93e4ea855bf938cd1b3a27e`: Cognito connection and
claim-bound subscription, HTTP truth resynchronization before buffered event
delivery, two real IAM publications through the Rust adapter across a browser
reconnect, mismatched-channel rejection, and complete teardown all passed. The
exact stack and versioned artifact bucket were independently proved absent, and
the temporary proof permission was restored and reprovisioned to its exact
original value. This is bounded non-production verification, not standing AWS
deployment authority.

The first exact-head essential run
[`30893275719`](https://github.com/xicv/minco/actions/runs/30893275719)
failed at its final source-manifest check because a local ignored AppSync Plan
render had been hashed into the committed manifest but was absent from the
clean checkout. The manifest now excludes that exact reproducible generated
directory, and a regression keeps the clean/local source boundary stable.

## M11-T08 `0.6.0` release closure

The release includes the completed versioned documentation site, strict
plugin distribution manifests, public plugin conformance kit and detailed
framework/API/plugin/testing/deployment documentation added after `0.5.0`.
Local candidate qualification passed on 2026-08-01:

```text
./scripts/quality.sh
  static/repository/publish/deep-review validation: passed
  workspace test, Clippy, rustdoc and generated applications: passed
  Feedback Playwright: 40 passed
  VitePress build/link/browser: passed
  cargo audit: 0 vulnerabilities; 1 explicitly allowed upstream warning
  npm audit: 0 vulnerabilities
  gitleaks: no leaks

Orders SQLite HTTP E2E: passed
Orders disposable PostgreSQL 18 adapter tests: 4 passed
Rustack S3/SQS/SSM/STS and Minco adapter conformance: passed
Plan and SAM validation: passed
Orders ARM64 ZIP: 5,102,303 bytes
Orders SHA-256: 7864a2533e14dbb21abec1d7757e1ace047dc1c2b9c9b4c7e3081ff08288a5f7
Worker ARM64 ZIP: 574,199 bytes
Worker SHA-256: 80d7f8bb3c82a4ead305696437dcad88f5c1473b82373e8a606e5d61749b11f8

crates.io exact-version preflight: 28/28 exact 0.6.0 versions absent
coordinated cargo publish --dry-run: 28/28 packages
configured unpacked-archive tests: passed
external archive consumers: no-default/default/all-features/new-package passed
unpacked cargo-minco installation: minco 0.6.0
```

Candidate hosted run
[`30687931439`](https://github.com/xicv/minco/actions/runs/30687931439)
passed the Feedback browser suite but stopped at the final source-manifest
check. The static report had counted local generated
`docs-site/.vitepress/dist/release.json`, making its committed JSON-file metric
one larger than a clean checkout. Static validation now excludes the exact
VitePress cache/dist prefixes already excluded from the source manifest; the
regression keeps source `docs-site/release.json` in scope.

PR [`#73`](https://github.com/xicv/minco/pull/73) merged exact qualified head
`13840cb4dc507037e8d7fc7fbf66bc59597f91c1` as
`2c4605b7d4abcd865035196ffc0484c4a0e82f1e`. Exact PR-head hosted release run
[`30688694186`](https://github.com/xicv/minco/actions/runs/30688694186) and
merged-main run
[`30689722134`](https://github.com/xicv/minco/actions/runs/30689722134)
passed every release-quality, package, Plan/SAM/native ARM64, Rustack and E2E
gate. Immutable remote tag `v0.6.0` resolves to the merged commit.

Trusted publication run
[`30690519946`](https://github.com/xicv/minco/actions/runs/30690519946)
verified the exact tag, repeated the static/compiler/test/generated-app/docs
and 28-package archive gates, obtained a short-lived OIDC token, uploaded the
complete family and revoked the token. Independent post-publication proof is:

```text
exact non-yanked crates.io records: 28/28
owner xicv: 28/28
downloaded archive SHA-256 matched registry checksum: 28/28
downloaded archive bytes checked: 811,608
fresh cargo-minco installation: minco 0.6.0
external registry consumers: default/no-default/full passed
exact docs.rs library routes: 28/28 HTTP 200
```

The public GitHub release is
[`v0.6.0`](https://github.com/xicv/minco/releases/tag/v0.6.0).

Stable documentation PR
[`#74`](https://github.com/xicv/minco/pull/74) passed exact-head essential run
[`30691699436`](https://github.com/xicv/minco/actions/runs/30691699436) at
`c0dbcc2e0e4037c272ca8521e04f9c0717169f66` and merged as
`651a1886476556805991d83cbc71f9054f7703fe`. Manual branch Pages run
[`30691820803`](https://github.com/xicv/minco/actions/runs/30691820803)
passed the build and was correctly denied deployment because the protected
environment permits only `main`. Exact merged-main Pages run
[`30691854137`](https://github.com/xicv/minco/actions/runs/30691854137)
built and deployed successfully. Public verification then passed:

```text
https://xicv.github.io/minco/: HTTP 200; canonical root; latest stable 0.6.0
https://xicv.github.io/minco/0.6.0/: HTTP 200; canonical immutable release
https://xicv.github.io/minco/versions: HTTP 200
https://xicv.github.io/minco/sitemap.xml: HTTP 200
live Playwright: 13 passed; 1 intentional desktop skip
```

No AWS resource was created, modified, promoted or deleted for the `0.6.0`
release.

## `0.5.0` release closure

PR
[`#61`](https://github.com/xicv/minco/pull/61) merged exact qualified source
`485d67104a49f139820722eb73334415f69a653c`. Candidate hosted release run
[`30618708535`](https://github.com/xicv/minco/actions/runs/30618708535) and
merged-main release run
[`30623278437`](https://github.com/xicv/minco/actions/runs/30623278437)
passed. Immutable remote tag `v0.5.0` resolves to that exact source.

Trusted-publishing workflow corrections merged through PRs `#62` to `#65`.
The unsuccessful attempts failed before upload or were cancelled after exact
registry sweeps confirmed `0/28` packages at `0.5.0`. Final publication run
[`30632811070`](https://github.com/xicv/minco/actions/runs/30632811070)
passed all static, feature, Clippy, test, generated-application, rustdoc and
coordinated dry-run gates, obtained a short-lived OIDC token, uploaded all 28
packages and completed token revocation.

Independent post-publication verification passed:

```text
uv run --locked python scripts/validate_publish.py \
  --expect-published --check-registry --require-registry \
  --output verification/published-release-validation.json
  exact non-yanked registry records: 28/28

crates.io owner/archive verifier
  owner xicv: 28/28
  downloaded archive SHA-256 matched registry checksum: 28/28

cargo +1.97.1 install cargo-minco --version 0.5.0 --locked \
  --root target/minco/registry-install-0.5.0
  external Cargo subcommand: minco 0.5.0

external registry consumers
  default features: passed
  no default features: passed
  every published facade feature: passed

exact docs.rs library routes
  HTTP 200: 28/28
```

The GitHub release is
[`v0.5.0`](https://github.com/xicv/minco/releases/tag/v0.5.0). No AWS
resource was created, modified, promoted or deleted for this publication. The
separately authorised `0.4.0` disposable AWS rehearsal remains the latest live
deployment proof.

## M8-T09 `0.5.0` source and package qualification

Qualification ran in the isolated `/Users/xicao/Projects/minco-task-m8-t09`
JJ workspace. It includes the
accepted M9-T08 resource API convention, M9-T09 local/hosted CI boundary and
M10-T07 zero-idle cost evidence.

Local source qualification passed on 2026-07-31:

```text
./scripts/quality.sh
  repository/static/publish/deep-review validation: passed
  contract, architecture and complete workspace test/clippy/doc gates: passed
  Playwright: 40 passed
  generated SQLite and PostgreSQL applications: passed
  cargo audit: 0 vulnerabilities
  npm audit: 0 vulnerabilities
  gitleaks: no leaks
  source manifest: verified

MINCO_ORDERS_TEST_POSTGRES_URL='postgres://.../minco_release_m8_t09' \
  cargo test -p orders-adapters --features postgres --test postgres \
  --locked -- --ignored --test-threads=1
  4 passed

scripts/test/e2e.sh
  complete SQLite Orders resource lifecycle: passed

scripts/dev/rustack-smoke.sh
  S3, SQS, SSM and STS plus Minco adapters: passed

scripts/aws/plan.sh
scripts/aws/validate.sh
scripts/aws/build-lambda.sh
scripts/aws/build-worker-lambda.sh
  deterministic Plan and SAM validation: passed
  Orders ARM64 ZIP: 5,102,572 bytes
  Orders SHA-256:
    eac3fb8534f55dc576cc56310291b254a0770f2d997ca7b038d1a948a2999d7f
  worker ARM64 ZIP: 574,202 bytes
  worker SHA-256:
    dd53385da37c1069225210d6ebe714caf8f6fb2b77c744db1c556dd3e6860745
```

The clean-tree package boundary also passed:

```text
scripts/release/publish.sh --skip-quality
  coordinated cargo publish --dry-run: 28/28 packages
  configured unpacked-archive tests: passed
  default, no-default, all-feature and new-package consumers: passed
  unpacked cargo-minco installation: minco 0.5.0

scripts/release/package-list.sh
  release packages: 28
  manifests present: 28
```

Registry validation reached all 28 package records and proved exact `0.5.0`
absent before publication. A regression also proves an immutable yanked exact
version is still treated as occupied rather than reusable. The hosted,
publication and independent post-publication evidence is recorded above.

## `0.4.0` release closure

The remote tag `v0.4.0`, GitHub release, `main` and `origin/main` resolve to
exact source `65bf94045448bdbeedd37e10b1a004c926513508`. Exact-main manual
quality run
[`30579590089`](https://github.com/xicv/minco/actions/runs/30579590089)
completed successfully. The separately gated publication workflow
[`30585166476`](https://github.com/xicv/minco/actions/runs/30585166476)
completed successfully.

All 28 lock-step package versions are published at `0.4.0`. The post-release
registry command below requires successful crates.io evidence for every exact
workspace version; an absent version, HTTP failure or unavailable registry is
an error:

```bash
uv run --locked python scripts/validate_publish.py --expect-published
```

The final authorised disposable AWS rehearsal
`20260730t204904z-release040` bound deployment and hosted verification to one
exact release. Contract, readiness, authentication, smoke and artifact
identity checks all passed. Promotion succeeded by changing only the live
Lambda alias routing to the already verified version, with no rebuild or
replan. The final cleanup receipt proves application and temporary database
cleanup, temporary IAM-principal absence and local credential-file absence.
This is bounded release qualification, not production-runtime proof.

## M8-T07 `0.4.0` source and package candidate

Starting remote `main`:
`12839f3e802b2e47bf9088c82787a8aa9b1ec93d`. The task runs in the isolated
`/Users/xicao/Projects/minco-m8-t07` JJ workspace; the unrelated dirty primary
checkout is preserved.

Current source metadata declares 28 lock-step `0.4.0` publishable packages over
the independently published 24-package `0.3.1` baseline. First-publish crates
are `minco-config`, `minco-db`, `minco-dev` and `minco-deploy-aws`. Each is in
the unpacked-archive test set.

Baseline checks on untouched `main` passed:

```text
uv sync --locked --only-dev
uv run --locked python scripts/validate_static.py
uv run --locked python scripts/test/repository_truth.py
uv run --locked python scripts/validate_publish.py
uv run --locked python scripts/test/publish_validation.py
uv run --locked python scripts/deep_review.py
uv run --locked python scripts/test/deep_review_exclusions.py
cargo minco architecture
cargo minco inspect --json
cargo minco roadmap status
cargo minco task ready --json
cargo minco upgrade report --json
jj log -r 'conflicts()'
```

The literal baseline `git diff --check` was blocked in the secondary JJ
workspace with `fatal: not a git repository (or any of the parent directories):
.git`. A Git transport equivalent must run from the colocated primary
repository against the final exported commit; this blocker is not a pass.

The release reconciliation and authorised live gates found the following
fail-closed controller defects before publication:

1. publishing each `0.4.0` crate separately could not resolve unpublished
   lock-step dependencies from crates.io; the driver now performs one
   coordinated 28-package Cargo dry run;
2. unpacked archive tests inherited a lockfile that referred to the temporary
   registry, so `--locked` could not refresh that registry source; the
   coordinated family dry run remains locked while isolated archive tests use
   `--offline` plus patches to the other unpacked archives;
3. repeated Cargo Lambda ZIP builds embedded the build-time DOS timestamp, so
   byte-identical ARM64 binaries had different archive SHA-256 values. The
   shared Lambda packaging helper now accepts only `bootstrap` and the optional
   RDS CA bundle, normalizes timestamps and modes, writes entries in stable
   order and atomically replaces the ZIP only after successful validation. Both
   native build scripts also require the existing lockfile;
4. exact-head hosted run
   [`30367217262`](https://github.com/xicv/minco/actions/runs/30367217262)
   failed before its compatibility assertions because three CLI fixtures
   require JJ to create and read an `@-` baseline while the runner had no `jj`
   binary. The manual workflow now installs the current pinned `jj-cli 0.43.0`
   package, checks `jj --version`, and retains the real JJ-backed test rather
   than weakening it to `--vcs none`;
5. the next exact-head hosted run
   [`30368618149`](https://github.com/xicv/minco/actions/runs/30368618149)
   passed repository-truth checks and the JJ-backed compatibility fixtures,
   then failed with exit 127 at
   `scripts/test/generated_apps.sh: line 89: rg: command not found`. The
   runner image did not supply ripgrep even though the authoritative quality
   script requires it. The workflow now installs the current pinned
   `ripgrep 15.2.0` package and checks `rg --version` before quality;
6. exact-head hosted run
   [`30369804923`](https://github.com/xicv/minco/actions/runs/30369804923)
   passed source quality, the two-browser matrix and coordinated 28-package
   publication dry run, then failed after Plan generation and SAM validation
   because the source-installed Cargo Lambda did not install Zig. Cargo Lambda
   reported `Zig is not installed in your system` before either native ARM64
   archive was built. The workflow now uses the Cargo Lambda documentation's
   Zig `0.14.0` GitHub Actions baseline through immutable `setup-zig v2.2.1`
   commit `d1434d08867e3ee9daa34448df10607b98908d29`.
7. final review found that `--execute` verified the workspace-version tag in
   Git checkouts but accepted an untagged JJ-only workspace. The release driver
   now requires the exact tag on `@` or its clean parent in JJ workspaces, and
   regression fixtures prove both the accepted and fail-closed paths.
8. later evidence-only head
   `edcb42c916114dc0c7bc3ffb10bcf8555190b0f1` passed authoritative quality
   and the browser matrix in hosted run
   [`30411179583`](https://github.com/xicv/minco/actions/runs/30411179583),
   then failed while testing the unpacked `minco-dev` archive because
   `coordinated_shutdown_terminates_process_descendants` observed its PID file
   before the shell had completed the PID write. A local full-suite stress
   loop reproduced both an empty PID and the premature shutdown assertion.
   The fixture now waits for a complete numeric PID before resolving its
   shutdown future; the unchanged descendant-liveness assertion then passed
   600 repeated nine-test suite runs. No supervisor production code changed.
9. the first separately authorised live-AWS rehearsal on 2026-07-29 stopped
   before caller discovery or resource creation because macOS Bash rejected
   the bootstrap controller's own hyphenated default SSM parameter name. The
   escaped hyphen was not portable inside the bracket expression. Parameter
   validation now uses one shared predicate with the hyphen in the final
   character-class position, and a Mac-Bash regression accepts the generated
   default while retaining the relative-name, doubled-slash, trailing-slash
   and whitespace rejections.
10. after that correction merged, exact `main`
    `d34c0e49d881a5ababdc1e9576c046c867f45ab3` passed the full local suite and
    manual hosted run
    [`30422838559`](https://github.com/xicv/minco/actions/runs/30422838559).
    The next authorised live rehearsal migrated and verified its disposable
    private PostgreSQL database and built the native ARM64 Lambda, then Cognito
    rejected tagged user-pool creation because the bounded deployment role
    lacked `cognito-idp:TagResource`. Application cleanup passed immediately;
    the RDS-managed secret reached `ResourceNotFound` after the controller's
    initial bounded verification window, and the exact cleanup verifier then
    produced all-true application, database/VPC/secret and bootstrap-IAM
    receipts. The candidate correction grants only `TagResource` over the
    current Region/account user-pool namespace when all three exact run tags
    and no other tag keys are present. Its regression renders the actual role
    policy and asserts the whole statement rather than searching for an action
    string. The AWS IAM policy simulator returned `allowed` for those exact
    tags and `implicitDeny` when an additional tag key was supplied.
11. that least-privilege correction passed PR-head manual run
    [`30425328469`](https://github.com/xicv/minco/actions/runs/30425328469),
    merged as exact `main`
    `cd5b0049cd55f3ba7093a202eff9b668c825ed0b`, and passed the full local
    suite, AWS/SAM validation and exact-main hosted run
    [`30426089277`](https://github.com/xicv/minco/actions/runs/30426089277).
    Authorised replacement run `20260729t060221z-approved` then migrated and
    verified its disposable private PostgreSQL database, built and sealed the
    exact native ARM64 artifact, and stopped before application change-set
    creation. AWS CLI parsed the shorthand comma-delimited
    `LambdaSubnetIds` value as a nested list, but CloudFormation
    `ParameterValue` accepts only a string. Application cleanup passed
    immediately; after the RDS-managed secret reached `ResourceNotFound`, the
    exact verifier produced all-true application, database/VPC/secret and
    bootstrap-IAM receipts. The candidate correction serializes both
    deployment and promotion parameter lists as one JSON argument with typed
    string values. Its focused regression preserves comma-delimited values as
    strings, and AWS CLI `2.36.10` accepted the same shape with the
    non-contacting output-skeleton validator.
12. the JSON-parameter correction passed PR-head manual run
    [`30428780397`](https://github.com/xicv/minco/actions/runs/30428780397),
    merged as exact `main`
    `100ffa276163a2c02149321b2b7ffcc542edb4c5`, and passed the full local
    suite, AWS/SAM validation and exact-main hosted run
    [`30429829246`](https://github.com/xicv/minco/actions/runs/30429829246).
    Authorised replacement run `20260729t071107z-approved` migrated and
    verified its disposable private PostgreSQL database, built the 5,038,349
    byte native ARM64 artifact and created the unexecuted application change
    set. Parsing then stopped fail-closed because the real
    `describe-change-set` response omitted `ChangeSetType`, which is create
    input rather than a documented `DescribeChangeSet` response element. The
    initial cleanup removed every application resource but refused the empty,
    untagged `REVIEW_IN_PROGRESS` shell; after exact inspection proved one
    unexecuted change set and zero stack resources, the change set and shell
    were deleted. The RDS-managed secret subsequently reached
    `ResourceNotFound`, and the repository verifiers produced all-true
    application, database/VPC/secret and bootstrap-IAM receipts. The candidate
    parser now requires the caller's already-guarded type and rejects an
    optional contradictory provider value. Cleanup separately permits only an
    exact preflight-absent, untagged `REVIEW_IN_PROGRESS` stack with zero
    resources. Focused red/green tests cover the real missing-field shape,
    contradiction rejection and cleanup refusal when preflight absence, review
    status or zero-resource evidence is missing.
13. the guarded-type correction passed PR-head manual run
    [`30433187335`](https://github.com/xicv/minco/actions/runs/30433187335),
    merged as exact `main`
    `13be9b0a8d99281c98fec880b8d275a59c7499f9`, and passed the full local
    suite, AWS/SAM validation and exact-main hosted run
    [`30434365889`](https://github.com/xicv/minco/actions/runs/30434365889).
    The first replacement invocation `20260729t082443z-approved` stopped during
    IAM propagation before application or database creation; its temporary
    user, role, access key and local credential files were removed. Authorised
    replacement run `20260729t082616z-approved` then migrated and verified the
    private PostgreSQL database, sealed and verified the 5,038,349 byte native
    ARM64 release, created and re-read the application change set through the
    corrected parser, and attempted the exact digest-approved apply. Both API
    Gateway stages failed because CloudFormation propagated stack tags but the
    change set carried only Minco release tags while the bounded role required
    the three run-ownership tags. Rollback removed all stack resources.
    Tag-only cleanup correctly refused the remaining release-tagged rollback
    shell; exact preflight, stack ID, release digest and all-`DELETE_COMPLETE`
    resource evidence authorized its manual deletion. The RDS-managed secret
    reached `ResourceNotFound` after the initial verification window. A final
    cross-service sweep proved the application and RDS stacks, instance,
    secret, VPC, parameter, bucket, Cognito pool, Lambda/log group and
    bootstrap IAM identities absent. The candidate correction makes validated
    target stack tags part of the deterministic JSON change-set input, reserves
    Minco's three release keys and the `aws:` prefix, enforces provider limits,
    and generates the bounded smoke catalog with the exact run tags required
    by both stage authorization and cleanup. The authoritative local suite,
    AWS Plan/SAM validation, ShellCheck and AWS CLI `2.36.10` non-contacting
    shape validation pass.
14. the stack-tag correction passed PR-head hosted run
    [`30438686783`](https://github.com/xicv/minco/actions/runs/30438686783),
    merged as exact `main`
    `8dcc49e2cefec1b9a043da5ae50161ae1e2431d1`, and passed the full local
    suite, AWS Plan/SAM validation and exact-main hosted run
    [`30440072120`](https://github.com/xicv/minco/actions/runs/30440072120).
    Authorised replacement run `20260729t094817z-approved` proved the target
    stack carried the exact run tags, migrated and verified its disposable
    private PostgreSQL database, and sealed release
    `minco.28624a327fb2f9afaed5d1ac` from the exact merged source. API Gateway
    stage tagging still returned `AccessDenied` because CloudFormation adds
    `aws:cloudformation:stack-name`, `aws:cloudformation:stack-id` and
    `aws:cloudformation:logical-id`, while the policy's `aws:TagKeys`
    allowlist omitted those service-owned keys. AWS IAM custom-policy
    simulation reproduced `implicitDeny` with the real key set and returned
    `allowed` after adding only those three keys. Application rollback and
    cleanup passed; the delayed RDS-managed secret subsequently reached
    `ResourceNotFound`, and the exact cleanup verifier produced all-true
    application, database/VPC/secret and bootstrap-IAM receipts. The candidate
    correction names only the documented API Gateway V2 tagging IAM action
    `apigateway:POST`, retains the exact stage collection ARN, caller chain and
    run-tag value guards, and admits only the three documented CloudFormation
    system keys in addition to the already reviewed run, release and SAM keys.
    A replacement live rehearsal, tag and registry publication remain blocked
    pending exact-head merge and requalification.
15. that action/key correction passed PR-head hosted run
    [`30443671627`](https://github.com/xicv/minco/actions/runs/30443671627),
    merged as exact `main`
    `0f1271eec11bf2e4fd475f7093c04eddd8d47f6c`, and passed the full local
    suite, AWS/SAM validation and exact-main hosted run
    [`30444766607`](https://github.com/xicv/minco/actions/runs/30444766607).
    Authorised replacement run `20260729t105820z-approved` migrated and
    verified its disposable private PostgreSQL database, built the
    5,038,349-byte native ARM64 ZIP with SHA-256
    `ff9609127cedcf2aad6c563e1f524feda1258ec33f104f7973eccecaa80ea474`,
    and sealed exact-source release `minco.44a1623ffb1ec9bd0b037813` with
    digest
    `44a1623ffb1ec9bd0b0378136bd9931e8420f78762bc422f634f6a072a7199d9`.
    Both API Gateway stage creates still failed their dependent
    `TagResource` authorization. CloudTrail recorded the operations as
    `CreateStage`, the assumed run role, CloudFormation source and user agent,
    and the complete expected request tags. AWS documents the tagging endpoint
    as `POST /v2/tags/{resource-arn}` and its IAM resource as `/tags/*`; the
    specialized statement instead named the stage collection. The existing
    region-wide mutation statement already admits every API Gateway resource
    when `aws:CalledVia` is present, so the continued deny also proves the
    dependent tag evaluation cannot rely on that caller-chain context.
    Rollback completed, the delayed RDS-managed secret reached
    `ResourceNotFound`, exact user/role absence was independently rechecked,
    and all three cleanup receipts contain only true values.

    The candidate correction retains the CloudFormation-only mutation
    statement and grants the separate `apigateway:POST` tagging authorization
    only on `/tags/*`, requiring the three exact run-ownership request-tag
    values and the closed reviewed tag-key allowlist. The focused regression
    failed with `StopIteration` before the generated statement changed and
    passes afterward. IAM custom-policy simulation returns `allowed` for the
    exact request and `implicitDeny` for either an extra tag key or a wrong run
    ID. `./scripts/quality.sh`, `scripts/aws/validate.sh` and
    `scripts/aws/plan.sh` pass on the candidate. A replacement live rehearsal,
    tag and registry publication remain blocked pending exact-head hosted
    qualification, merge and exact-main requalification.
16. the `/tags/*` correction passed PR-head hosted run
    [`30448531978`](https://github.com/xicv/minco/actions/runs/30448531978),
    merged as exact `main`
    `edabc701ee86b4adfee27b978f8d4d6187d19f2e`, and passed the full local
    suite, AWS/SAM validation and exact-main hosted run
    [`30449710067`](https://github.com/xicv/minco/actions/runs/30449710067).
    Authorised replacement run `20260729t121408z-approved` migrated and
    verified its disposable private PostgreSQL database, built the same
    5,038,349-byte native ARM64 ZIP with SHA-256
    `ff9609127cedcf2aad6c563e1f524feda1258ec33f104f7973eccecaa80ea474`,
    and sealed exact-source release `minco.6fba6aee8d28ce4d9bece03b` with
    digest
    `6fba6aee8d28ce4d9bece03b2d5a260f3b4d43530ef4eb2f175881764fd59a43`.
    Both stage creates still failed the provider-reported `TagResource`
    dependency. CloudTrail records the actual operation as `CreateStage`,
    with the complete expected tags, against
    `arn:aws:apigateway:ap-southeast-2::/apis/oyjsik9b3l/stages`; no separate
    tagging event exists. This falsifies the `/tags/*` resource hypothesis.
    Application cleanup contains only true values. The delayed RDS-managed
    secret subsequently reached `ResourceNotFound`, the exact RDS verifier
    contains only true values, and the deterministic bootstrap user and role
    are independently absent.

    The replacement candidate retains the CloudFormation-only general mutation
    statement and grants the specialized `apigateway:POST` authorization only
    on `/apis/*/stages`, requiring the three exact run-ownership request-tag
    values and closed reviewed tag-key allowlist. The focused regression failed
    with `StopIteration` before the generated statement changed and passes
    afterward. IAM custom-policy simulation returns `allowed` for the exact
    observed request without `aws:CalledVia`, and `implicitDeny` for a wrong
    run ID or extra tag key. `./scripts/quality.sh`,
    `scripts/aws/validate.sh` and `scripts/aws/plan.sh` pass on the
    replacement candidate. Hosted qualification, a replacement live
    rehearsal, tag and registry publication remain blocked.
17. the stage-collection correction passed exact PR-head hosted run
    [`30453546940`](https://github.com/xicv/minco/actions/runs/30453546940),
    merged as `8593b47eaf691cace2bf32d3d07e3408f036ca46`, and passed the full
    local suite, AWS/SAM validation and exact-main hosted run
    [`30454760539`](https://github.com/xicv/minco/actions/runs/30454760539).
    Authorised run `20260729t132534z-approved` migrated and verified its
    disposable PostgreSQL database over TLS `verify-full`, removed the local
    `/32`, proved the database private, built the 5,038,349-byte native ARM64
    ZIP with SHA-256
    `ff9609127cedcf2aad6c563e1f524feda1258ec33f104f7973eccecaa80ea474`,
    and sealed exact-source release `minco.2b3857b9f12ff31ac32f183a` with
    digest
    `2b3857b9f12ff31ac32f183afb855975dea11d2a2fff385014a054b13613bb7e`.
    S3 accepted the run-owned bucket creation, public-access block and
    encryption calls. The cached build then reached the controller within
    seconds, and its immediate `HeadBucket` returned 404 before a change set
    was created. The application cleanup receipt contains only true values.
    The delayed managed secret subsequently reached `ResourceNotFound`; the
    exact RDS cleanup verifier, bootstrap IAM checks and local credential-file
    checks are consolidated in an all-true `final-cleanup.json`.

    The replacement candidate waits for the newly created bucket at the
    bounded smoke-script boundary. It retries only `404`, `NoSuchBucket` and
    `Not Found`, fails immediately for every other response, and stops after
    15 attempts. The focused regression failed with a missing helper before
    the implementation and now covers success after transient 404 responses,
    non-404 fail-fast behavior and bounded exhaustion. Exact-head hosted
    qualification, merge, a replacement live rehearsal, tag and registry
    publication remain blocked.
18. the bounded bucket-visibility correction passed exact PR-head hosted run
    [`30458112104`](https://github.com/xicv/minco/actions/runs/30458112104),
    merged as `dbe8a55f141c082a8329ec1871590c0199682eed`, passed the full local
    suite and AWS Plan/SAM validation, and passed exact-main hosted run
    [`30459913592`](https://github.com/xicv/minco/actions/runs/30459913592).
    Authorised run `20260729t143232z-approved` migrated and verified its
    disposable PostgreSQL database over TLS `verify-full`, removed the local
    `/32`, proved the database private, passed the new bucket-visibility guard
    on its first bounded attempt, built the 5,038,349-byte native ARM64 ZIP
    with SHA-256
    `ff9609127cedcf2aad6c563e1f524feda1258ec33f104f7973eccecaa80ea474`,
    and sealed exact-source release `minco.eefe49c4e87868c73164ecba` with
    digest
    `eefe49c4e87868c73164ecba8408ec5df76b741f15563c5856d072aea64cc79f`.
    Both API Gateway stage creates failed the provider-reported dependent
    `TagResource` authorization. CloudTrail recorded the two tagged
    `CreateStage` requests from exact temporary role
    `MincoSmoke-d93173c82d99`, including the expected ten-key closed tag set;
    no separate `TagResource` event exists.

    AWS's current API Gateway V2 operation mapping lists two permissions for
    tagged `CreateStage`: `apigateway:POST` for the stage collection and
    `apigateway:PUT` for the tag write. This proves that the prior retries
    alternated resource namespaces without ever granting the documented
    action/resource pair together. The current candidate adds only
    `apigateway:PUT` on `/tags/*`, with the same exact run-tag values and
    closed ten-key allowlist as the specialized `POST` statement on
    `/apis/*/stages`. The focused test failed with `StopIteration` before
    implementation and passes afterward. IAM custom-policy simulation returns
    `allowed` for only the expected `POST`/stage-collection and
    `PUT`/tag-namespace pairs. Crossed pairs, a wrong run ID and an extra tag
    key return `implicitDeny`.

    The application cleanup receipt contains only true values. After the
    RDS-managed secret reached `ResourceNotFound`, the exact database/VPC
    cleanup verifier, deterministic bootstrap user and role absence, and local
    credential-file absence were independently consolidated in an all-true
    `final-cleanup.json`. Exact-head hosted qualification, merge, a replacement
    live rehearsal, tag and registry publication remain blocked.
19. the first tagged-stage correction passed exact PR-head hosted run
    [`30466012186`](https://github.com/xicv/minco/actions/runs/30466012186)
    at `d7ffe82290ff2cfc215e737823e471226d661b56`, merged as
    `4bf245cae924e2d3c89d008cf291da8bf862cba4`, passed the full local suite
    and AWS Plan/SAM validation, and passed exact-main hosted run
    [`30467769879`](https://github.com/xicv/minco/actions/runs/30467769879).
    Authorised run `20260729t215737z-approved` migrated and verified its
    disposable PostgreSQL database over TLS `verify-full`, removed the local
    `/32`, proved the database private, passed S3 visibility on its first
    bounded attempt, and sealed exact-source release
    `minco.683d7abad93046f3b4476621` with digest
    `683d7abad93046f3b44766215f0ecea095bf9003e2fc4242b769db2f1deed30d`.
    It created the exact release-bound change-set receipt with digest
    `f32c48fb78964575188c2fe0035f053e0a4142d5e7030f08a19602284a209605`.

    Both API Gateway stage creates then failed. AWS reported that the temporary
    role was not authorized for `apigateway:TagResource` and identified the
    evaluated resource as
    `arn:aws:apigateway:ap-southeast-2::/apis/iaqgnlnghl/stages`.
    Custom-policy simulation reproduced the cause: `POST` on the stage
    collection was allowed, while `PUT` on that same collection was
    `implicitDeny`; the prior candidate had placed `PUT` on the separate
    direct tagging API namespace `/tags/*`.

    The current correction puts both specialized methods on
    `/apis/*/stages`, preserving the three exact run-ownership request tags and
    closed ten-key allowlist. The focused regression failed before the
    implementation and passes afterward. Custom-policy simulation permits
    exact-tag `POST` and `PUT` on the stage collection; a wrong run ID, an
    extra tag key and direct `PUT` on `/tags/*` are `implicitDeny`. Access
    Analyzer reports no findings for the two specialized statements.

    Application cleanup contains only true values. The exact database cleanup
    verifier subsequently confirmed the delayed managed secret, database
    instance, stack, VPC, local secret files and synthetic data are all absent.
    Bootstrap IAM and all temporary local credential/profile files are absent.
    Exact-head hosted qualification, merge, a replacement live rehearsal, tag
    and registry publication remain blocked.
20. the stage-collection correction passed exact PR-head hosted run
    [`30496875203`](https://github.com/xicv/minco/actions/runs/30496875203) at
    `cffb60520a9311c72cf287f94c8dcbfa762bf1e0`, merged as
    `36d09d5ce36242290ae99506afee64c1a2f0de91`, passed the full local suite
    and AWS Plan/SAM validation, and passed exact-main hosted run
    [`30498077062`](https://github.com/xicv/minco/actions/runs/30498077062).

    Authorised run `20260729t231646z-approved` stopped before application,
    database or release work. The fresh bootstrap key resolved on its first
    identity attempt to exact user
    `MincoSmokeBootstrap-ddf380d762c9`; the immediately following first
    `AssumeRole` returned `InvalidClientTokenId`. The script retried that
    reviewed fresh-key propagation failure during identity verification but
    not during role assumption.

    The current correction admits the same
    `InvalidClientTokenId`/invalid-security-token propagation class to the
    existing role-assumption retry loop, which remains capped at 15 attempts
    two seconds apart. It does not alter the exact role, principal, action or
    one-hour session. The bootstrap now marks application invocation before
    calling the runner; cleanup can therefore report a never-started
    application clean, while any started runner still requires its existing
    all-true receipt. The focused regression failed before implementation and
    passes afterward.

    Independent exact-name checks confirm both application and RDS stacks,
    bootstrap user and bootstrap role are absent. The cleanup receipt confirms
    temporary-database and local credential/profile cleanup are true. Exact-head
    hosted qualification, merge, another live rehearsal, tag and registry
    publication remain blocked.
21. the fresh-key correction passed exact PR-head hosted run
    [`30499941916`](https://github.com/xicv/minco/actions/runs/30499941916) at
    `579e240328b3415dd8a839535c2efd8dbc6fcd40`, merged as
    `fbba94496e14fce0629efef78d5bee4f71aa132a`, passed the full local suite
    and AWS Plan/SAM validation, and passed exact-main hosted run
    [`30500931722`](https://github.com/xicv/minco/actions/runs/30500931722).

    Authorised run `20260730t001031z-approved` proved the corrected
    fresh-credential propagation path, migrated and verified private
    PostgreSQL, built the 5,038,349-byte native ARM64 artifact with SHA-256
    `ff9609127cedcf2aad6c563e1f524feda1258ec33f104f7973eccecaa80ea474`,
    and sealed exact-source release `minco.d6168caadfd9d66f5d593c4d` with
    digest
    `d6168caadfd9d66f5d593c4d2afb751f330dcff3b62162debe92d7df565546fd`.
    The digest-approved application apply used change-set receipt
    `8ef973c492f41d89a934b8367278253d01edae50504568274c2dc41e7d02aeed`.

    Both API Gateway V2 stage creates failed because CloudFormation evaluated
    dependent authorization as `apigateway:TagResource` on
    `arn:aws:apigateway:ap-southeast-2::/apis/sefukjj5f2/stages`, while the
    specialized statement still granted `apigateway:PUT` on that correct
    collection resource. IAM custom-policy simulation returns `allowed` when
    the statement names the provider-evaluated `apigateway:TagResource`
    action.

    The current correction changes only that specialized action. The exact
    stage-collection ARN, three run-ownership request-tag values and closed
    ten-key allowlist are unchanged. Access Analyzer currently returns the
    exact stale error `The action apigateway:TagResource does not exist.` even
    though live IAM evaluation requires the action and IAM custom-policy
    simulation returns `allowed`. The bootstrap now accepts only that one
    `INVALID_ACTION` finding at the exact structurally verified statement
    index. Focused fixtures prove that an additional Analyzer error, a
    different finding location, a broader stage-tagging resource or an
    additional action wildcard remains fatal. Application cleanup contains
    only true values. The second exact RDS cleanup verifier confirms the
    delayed managed secret, database instance, stack, VPC, local secret files
    and synthetic data are absent. Independent exact-name checks also confirm
    the application stack, artifact bucket and bootstrap user/role are absent.
    Exact-head hosted qualification, merge, another live rehearsal, tag and
    registry publication remain blocked.
22. candidate `d9c2e541889aec007038bfe12cd60114ff863317`
    passed the authoritative quality and Feedback browser stages of exact-head
    hosted run
    [`30504351107`](https://github.com/xicv/minco/actions/runs/30504351107),
    then failed in the coordinated publication dry run while testing the
    unpacked `minco-dev` archive. The
    `coordinated_shutdown_terminates_process_descendants` fixture reported
    `descendant process 25049 survived shutdown`.

    The supervisor sends the whole process group `TERM`, reaps its direct
    child, sends the group `KILL`, and waits for every descendant-held log pipe
    to close. The fixture then used `kill -0`, which reports a Linux zombie PID
    as present even though the descendant is terminated and cannot execute.
    That made the assertion depend on the hosted runner's orphan-reaping
    timing rather than the supervisor's shutdown contract. The test-only
    correction inspects portable Unix `ps` state, treats only non-zombie
    processes as running, and applies the same helper to the lifecycle
    descendant case. No supervisor production code changed. The complete
    nine-test supervisor suite and 100 repeated focused shutdown runs pass
    locally. Exact-head hosted qualification must be repeated before merge;
    live AWS, tag and registry publication remain blocked.
23. corrected release candidate
    `bab0e8ca63ce4917251f7b5c75f0c17d37f4ccf2` passed exact-head hosted run
    [`30505833178`](https://github.com/xicv/minco/actions/runs/30505833178),
    merged as exact `main`
    `84598996a86067eb8b57015591a665445217af49`, and passed the complete local
    suite, AWS Plan/SAM validation and exact-main hosted run
    [`30506695053`](https://github.com/xicv/minco/actions/runs/30506695053).

    Authorised live run `20260730t020609z-approved` migrated and verified its
    disposable PostgreSQL database over TLS `verify-full`, removed the local
    `/32`, proved the database private, built the 5,038,349-byte native ARM64
    artifact with SHA-256
    `ff9609127cedcf2aad6c563e1f524feda1258ec33f104f7973eccecaa80ea474`,
    and sealed release `minco.1b974fc3ed8ee12979ac02dd` with digest
    `1b974fc3ed8ee12979ac02dd0d12d29ad5bfd9a2264806ed0b2309260de0e3fb`.
    The digest-approved application apply used change-set receipt
    `31f2b394721f437192c982d91aebfe7de9790d6b71f140722a5e74b06f3f789e`.
    Both tagged API Gateway stages reached `CREATE_COMPLETE`, proving the
    bounded `apigateway:TagResource` correction against the live provider.

    Hosted verification then stopped on its first request because
    `GET /health/live` returned API Gateway's
    `401 {"message":"Unauthorized"}` response. The generated definition
    contained contract-correct `security: []`, but exact AWS SAM translator
    `1.111.0` applies `Auth.DefaultAuthorizer` whenever the existing security
    value is falsey, so it replaced the empty list with the JWT authorizer.
    The renderer correction retains `Auth.Authorizers` only and emits explicit
    `JwtAuthorizer` security on authenticated operations. A focused renderer
    regression covers both route classes. An isolated transform with the exact
    `aws-sam-translator==1.111.0` dependency preserves `[]` for both health
    routes, emits `JwtAuthorizer` for both Orders routes and retains exactly
    one `JwtAuthorizer` security scheme.

    Application, artifact-bucket, Cognito, Lambda, API Gateway, log, SSM,
    RDS/VPC/database, managed-secret, bootstrap-IAM and local credential
    cleanup are independently absent. The aggregate cleanup receipt captured
    the managed secret during its short deletion-convergence window, but a
    subsequent exact-ARN `DescribeSecret` returns `ResourceNotFoundException`.
    A replacement exact-head qualification, merge, exact-main qualification
    and live rehearsal remain required. No tag or registry upload occurred.
24. The public-route correction passed exact-head hosted run
    [`30509848637`](https://github.com/xicv/minco/actions/runs/30509848637) at
    `b42909c17febb20109f1fa6cb66b419757130d23`, merged as exact `main`
    `d760b0d9f833cc88d23a34b852c4f79ffd5f9e0c`, and passed exact-main hosted
    run [`30511095728`](https://github.com/xicv/minco/actions/runs/30511095728).

    Authorised live runs `20260730t034110z-release040` and
    `20260730t040531z-diag` both reached the candidate integration and
    received API Gateway's generic `500` before Lambda created a log stream.
    The second run captured the deployed policies before cleanup. The only
    `lambda:InvokeFunction` statement was attached to the unqualified function
    ARN; the exact `candidate` qualifier returned
    `ResourceNotFoundException`. API Gateway invokes the qualified candidate
    ARN, so provider authorization rejected the request before application
    initialization.

    The renderer correction now gives stable `candidate` and `live` aliases
    separate API-scoped permissions. The initial sentinel makes both aliases
    resolve to the generated immutable version, while a promoted numeric
    `LiveFunctionVersion` keeps later infrastructure updates from moving live
    traffic. Promotion admits only one `LiveFunctionAlias`
    `AWS::Lambda::Alias` property modification and postchecks both alias
    versions and `CodeSha256`. Exact SAM translator `1.111.0` resolves
    `ApiFunction.Alias` and `ApiFunction.Version.Version` to the generated
    candidate alias and published version resources. The deterministic
    bootstrap renderer and checked-in SAM snapshot carry the same topology.

    Independent exact-name checks prove both failed runs left no application
    stack, RDS stack or instance, artifact bucket, Lambda function or log
    group, HTTP API, Cognito pool, SSM parameter, managed secret, bootstrap
    user/role, isolated profile or credential file. The aggregate cleanup
    receipts observed short S3/Secrets Manager convergence windows, but later
    exact-name provider calls returned absence. No tag or registry upload
    occurred. `./scripts/quality.sh`, `./scripts/aws/validate.sh`,
    `./scripts/aws/plan.sh`, and the regenerated source-manifest check pass in
    the isolated correction workspace. Exact-head hosted qualification,
    another live rehearsal, tag and publication remain blocked.
25. The qualified-alias correction passed exact PR-head hosted run
    [`30515135505`](https://github.com/xicv/minco/actions/runs/30515135505) at
    `5b269157f456591fb5167c32277067ee88c15bae`, merged as exact `main`
    `ccce06c180c29ba0f5c5471120b2d223a9baece9`, passed the complete local
    qualification again and passed exact-main hosted run
    [`30516228934`](https://github.com/xicv/minco/actions/runs/30516228934).
    Both hosted runs passed authoritative quality, Chromium/Firefox,
    coordinated 28-package dry-run, Plan/SAM and both native ARM64 artifacts,
    Rustack/SSM and Orders E2E.

    Authorised live run `20260730t053430z-release040` migrated and verified its
    disposable private PostgreSQL database, built the deterministic
    5,038,349-byte native ARM64 artifact with SHA-256
    `ff9609127cedcf2aad6c563e1f524feda1258ec33f104f7973eccecaa80ea474`,
    and sealed exact-source release `minco.81b8b9d9bb94a9e711c28d3f` with
    digest
    `81b8b9d9bb94a9e711c28d3fc691608ea6e3a3ee208a9fb8b8c4373c6c868339`.
    The smoke runner created, blocked and encrypted its run-owned artifact
    bucket, and its bounded visibility check passed. The deployment
    controller's immediately following `HeadBucket` returned 404 before SAM
    packaging or CloudFormation change-set creation.

    The replacement candidate applies the same bounded policy at that second
    CLI provider boundary: retry only `404`, `NoSuchBucket` and `Not Found`,
    fail immediately for every other response, and stop after 15 attempts. The
    focused regression first failed because the boundary was absent, then
    passed eventual success, authorization fail-fast and bounded exhaustion.
    `cargo test -p cargo-minco --all-targets --all-features --locked` passes
    all 54 CLI unit tests and 18 integration tests, and
    `scripts/test/aws_shell_portability.sh` passes.

    Application cleanup contains only true values. Later exact-name provider
    calls return stack-not-found for both stacks, 404 for the bucket,
    `ResourceNotFoundException` for the Cognito pool and managed secret,
    `ParameterNotFound` for the SSM parameter, `DBInstanceNotFound` for the
    database, and `NoSuchEntity` for both temporary IAM principals. No
    application change set, release tag or registry upload occurred.
26. The deployment-role bucket-visibility correction passed exact-head hosted
    run
    [`30519948680`](https://github.com/xicv/minco/actions/runs/30519948680) at
    `612dbf16fd998538d941308079e2b9437d4be87e`, merged as exact `main`
    `daae0595deffe945726df54c6f43ee82ff7bc7fd`, passed the complete local
    qualification again and passed exact-main hosted run
    [`30521267303`](https://github.com/xicv/minco/actions/runs/30521267303).
    Both hosted boundaries passed authoritative quality, the coordinated
    28-package dry run, Plan/SAM/native ARM64 Lambda, Rustack/SSM and explicit
    Orders E2E stages.

    Authorised live run `20260730t071445z-release040` migrated and verified its
    disposable private PostgreSQL database, built the deterministic
    5,038,349-byte native ARM64 artifact with SHA-256
    `ff9609127cedcf2aad6c563e1f524feda1258ec33f104f7973eccecaa80ea474`,
    and sealed exact-main release `minco.0b60a084c8c9029899e8fc27` with digest
    `0b60a084c8c9029899e8fc27eb50527ce0557367bc776bd1b927e90030c5ceb2`.
    The reviewed change-set receipt had digest
    `be526ee291f73d284af0d46e136dc79775fb7fa4d50a34924beaf3780e70add5`;
    its application stack reached `CREATE_COMPLETE`.

    Candidate `GET /health/live` then reached the Lambda and returned Minco's
    `x-request-id`, but Axum returned 404 with an empty body. Pinned
    `lambda_http 1.3.0` deliberately prefixes a named API Gateway stage into
    the request URI unless `AWS_LAMBDA_HTTP_IGNORE_STAGE_IN_PATH` is present,
    so the router received `/candidate/health/live` instead of the
    contract-owned `/health/live`.

    The replacement renderer sets that documented dependency switch to
    `'true'` for the Lambda environment. The focused SAM regression covers the
    setting alongside the isolated candidate/live alias topology, and the
    checked-in template is regenerated from the renderer. Application cleanup
    contains only true values. The aggregate database cleanup receipt observed
    the managed secret during its deletion-convergence window; a later
    exact-ARN lookup returns `ResourceNotFoundException`. Both stacks, RDS,
    VPC resources, bucket, SSM parameter, Cognito pool, Lambda/API resources,
    bootstrap IAM and local credentials are independently absent. The resource
    tagging index retained three stale ARNs after deletion; direct provider
    lookups for all three return not found. No release tag or registry upload
    occurred. Exact-head qualification, merge, exact-main qualification and a
    replacement live rehearsal remain required.
27. The stage-environment correction passed exact PR-head hosted run
    [`30526281458`](https://github.com/xicv/minco/actions/runs/30526281458) at
    `d5b4a76946a47bb4aeffb8be64b7460e1e61ce2d`, merged as exact `main`
    `83d1583e9a385070306c95665a5219700cbc1c5e`, passed the complete local
    qualification and passed exact-main hosted run
    [`30527357088`](https://github.com/xicv/minco/actions/runs/30527357088).
    Both hosted boundaries passed authoritative quality, the coordinated
    28-package dry run, Plan/SAM/native ARM64 Lambda, Rustack/SSM and explicit
    Orders E2E stages.

    Authorised live run `20260730t085318z-release040` migrated and verified its
    disposable private PostgreSQL database, reproduced the deterministic
    5,038,349-byte native ARM64 artifact with SHA-256
    `ff9609127cedcf2aad6c563e1f524feda1258ec33f104f7973eccecaa80ea474`,
    and sealed exact-main release `minco.faf23ae016624d15d0b8f11f` with digest
    `faf23ae016624d15d0b8f11f1e0d7cb20c13c3ea6d2ce247e416427e3b3e977e`.
    The reviewed change-set receipt had digest
    `3d349a2be71b1aa04491f61f388780bb5c8d973e756aa4296c388103a8f27443`;
    its application stack reached `CREATE_COMPLETE`.

    Candidate `GET /health/live` still reached Lambda and returned Minco
    `x-request-id` `1dcc9a69-cae5-4c68-ba8e-bac9fec24128`, but Axum returned
    404 with an empty body. The real API Gateway v2 event and pinned
    `lambda_http 1.3.0` fixture show that named stages are already included in
    `rawPath`. `AWS_LAMBDA_HTTP_IGNORE_STAGE_IN_PATH` suppresses additional
    prefix insertion but does not strip `/candidate`, so the prior SAM
    environment correction could not change the routed path.

    The replacement normalizes API Gateway v2 requests in
    `minco-aws-lambda` before Axum route matching. It removes only the exact
    non-default context stage, preserves URI authority and query data, rejects
    prefix lookalikes and leaves `$default` unchanged. A realistic named-stage
    event regression first exposes `/candidate/health/live`, then proves the
    wrapper reaches the contract-owned `/health/live` Router endpoint. The
    ineffective SAM environment setting is removed. Focused adapter tests and
    all-feature Clippy, plan tests and SAM validation pass. No promotion,
    release tag or registry upload occurred. The application cleanup receipt is
    all true. The initial database cleanup check caught the RDS-managed secret
    during AWS's asynchronous deletion window and failed closed; a bounded
    rerun then proved the exact temporary PostgreSQL stack, instance, managed
    secret and VPC absent, with synthetic data and local database secret files
    absent. The bootstrap user, role, profiles and credential files are absent.
    Exact-head hosted qualification, merge, exact-main qualification and
    another live rehearsal remain required.
28. The named-stage correction passed exact PR-head hosted run
    [`30532832860`](https://github.com/xicv/minco/actions/runs/30532832860) at
    `d7e5a1c6e9ff5f5c43c754bc145bdefd63c7b60e`, merged as exact `main`
    `73807d918bc860b60d592611f388bb63775d7c54`, passed the complete local
    qualification and passed exact-main hosted run
    [`30534601227`](https://github.com/xicv/minco/actions/runs/30534601227).
    Both hosted boundaries passed authoritative quality, Feedback browser
    evidence, the coordinated 28-package dry run, Plan/SAM/native ARM64
    Lambda, Rustack/SSM and explicit Orders E2E stages.

    Authorised live run `20260730t104626z-release040` migrated and verified
    private PostgreSQL, built the 5,039,398-byte native ARM64 artifact with
    SHA-256
    `92dc989125a6032e378eaa660303939a9fadc0920bb3b2d0606bc2bcaaf86d11`,
    sealed release `minco.789c2425846acb0fda2039f0` with digest
    `789c2425846acb0fda2039f0eca3179978a48ce2be8af34ebb9b4ab42593c7b7`,
    and applied its exact reviewed change set. Candidate liveness and
    readiness passed. The unauthenticated protected-order probe returned the
    expected 401 with `apigw-requestid` `BUQyKibkSwMEMJw=`, but the smoke
    verifier recognized only `x-request-id` and `x-amzn-requestid`. It
    therefore failed before the authenticated POST, database mutation or
    promotion.

    The correction centralizes response request-ID extraction and recognizes
    Minco's `x-request-id`, Lambda/API Gateway's `x-amzn-requestid`, and API
    Gateway's observed `apigw-requestid`. The executable AWS shell portability
    suite covers every supported spelling and rejects an unrelated-header
    response. Application cleanup contains only true values. The initial
    database aggregate caught the RDS-managed secret during its asynchronous
    deletion window; an exact bounded rerun then proved the temporary
    PostgreSQL stack, instance, managed secret and VPC absent, with synthetic
    data and local secret files absent. Bootstrap user, role, profiles and
    credentials are absent. No tag or registry upload occurred. Exact-head
    hosted qualification, merge, exact-main qualification and another live
    rehearsal remain required.
29. The request-header correction passed exact PR-head hosted run
    [`30539721321`](https://github.com/xicv/minco/actions/runs/30539721321) at
    `8e97b38ef22608f849d531145f13dbf0e3e7243e`, merged as exact `main`
    `30260209c49acb048f6549a31eb1e375fd1e923e`, passed the complete local
    release matrix and passed exact-main hosted run
    [`30542710147`](https://github.com/xicv/minco/actions/runs/30542710147).
    The hosted main run passed authoritative quality, all 40 Feedback browser
    checks, the coordinated 28-package dry run, Plan/SAM/native ARM64 Lambda,
    Rustack/SSM and Orders E2E.

    Authorised live run `20260730t124426z-release040` migrated and verified its
    private disposable PostgreSQL database, built the 5,039,398-byte native
    ARM64 artifact with SHA-256
    `92dc989125a6032e378eaa660303939a9fadc0920bb3b2d0606bc2bcaaf86d11`,
    sealed release `minco.761bb0f73b895275c78858ff` with digest
    `761bb0f73b895275c78858ffa2d04f72023c62a552d9bc99d3d22006319c5a36`,
    and applied its exact reviewed change set. Candidate liveness, readiness,
    unauthenticated 401, authenticated place/get and idempotent replay all
    passed. Strict hosted-report construction then rejected only the
    Authentication check because API Gateway's padded `apigw-requestid` did
    not satisfy the report's narrower request-ID character set. No promotion,
    tag or registry upload occurred.

    The correction accepts one or two `=` characters only as trailing
    request-ID padding. It retains the 128-byte limit and rejects empty IDs,
    internal padding and all previously unsupported characters. The focused
    regression first failed with `InvalidCheck { kind: Authentication }`; all
    13 hosted-verification tests pass after the change. Exact cleanup reruns
    prove the application stack, bucket, API, Lambda, log group, Cognito pool,
    SSM parameter, temporary PostgreSQL stack/instance/managed secret/VPC,
    synthetic data, bootstrap principals, profiles and local credential files
    absent. Exact-head hosted qualification, merge, exact-main qualification
    and another live rehearsal remain required.
30. The padded-request-ID correction passed exact PR-head hosted run
    [`30548150116`](https://github.com/xicv/minco/actions/runs/30548150116) at
    `ade67d7f6d2866ed6bfde610742cf53660fe8ec9`, merged as exact `main`
    `25ffdd4c38eba8e8a759cf7e83404fbfebd36e60`, passed the complete local
    release matrix and passed exact-main hosted run
    [`30550393414`](https://github.com/xicv/minco/actions/runs/30550393414).
    Both hosted runs passed authoritative quality, all 40 Feedback browser
    checks, the coordinated 28-package dry run, Plan/SAM/native ARM64 Lambda,
    Rustack/SSM and Orders E2E.

    Authorised live run `20260730t142515z-release040` migrated and verified its
    disposable private PostgreSQL database, built the 5,039,398-byte native
    ARM64 artifact with SHA-256
    `92dc989125a6032e378eaa660303939a9fadc0920bb3b2d0606bc2bcaaf86d11`,
    sealed release `minco.31235789f783406088906750` with digest
    `31235789f783406088906750b530cfe7e84bb7d959c65a809cf98158a0fd0556`,
    applied its exact reviewed change set and passed all candidate hosted
    checks. Promotion stopped before creating or executing a change set
    because the bounded deployment role lacked
    `cloudformation:DetectStackResourceDrift`; no live alias changed.

    AWS documents stack drift detection as requiring stack-scoped
    `DetectStackDrift` and `DetectStackResourceDrift`, plus wildcard-only
    `BatchDescribeTypeConfigurations` and drift-status polling. The focused
    policy regression failed before the correction and now compares both
    complete statements, retaining exact stack ARNs and adding no action
    wildcard. Application cleanup is all true. An exact bounded RDS cleanup
    rerun after the managed-secret deletion window is all true, and the
    recovery `final-cleanup.json` proves the temporary database, bootstrap
    principals, profiles and local credential files absent. Exact-head hosted
    qualification, merge, exact-main qualification and another live rehearsal
    remain required before tag or registry publication.
31. The bounded drift-policy correction passed exact PR-head hosted run
    [`30556566177`](https://github.com/xicv/minco/actions/runs/30556566177) at
    `541e61e6fbb23a582011244539b2befddcd38fbf`, merged as exact `main`
    `fbdcb002b5df7632e6233f3d08be97b13e571fb3`, passed the complete local
    release matrix and passed exact-main hosted run
    [`30558916893`](https://github.com/xicv/minco/actions/runs/30558916893).
    Both hosted runs passed authoritative quality, all 40 Feedback browser
    checks, the coordinated 28-package dry run, Plan/SAM/native ARM64 Lambda,
    Rustack/SSM and Orders E2E.

    Authorised live run `20260730t160831z-release040` migrated and verified its
    disposable private PostgreSQL database, reproduced the 5,039,398-byte
    native ARM64 artifact with SHA-256
    `92dc989125a6032e378eaa660303939a9fadc0920bb3b2d0606bc2bcaaf86d11`,
    sealed release `minco.2b93b493fa3a454d51a4cbcb` with digest
    `2b93b493fa3a454d51a4cbcb962a12c7d07942d0983f2060fd1352352b1c19ba`,
    applied reviewed change-set digest
    `3d68fbb34507f287fcc8f8d26cafcea21387a14124ba892d7fe2da3737376854`
    and passed every candidate hosted check. CloudFormation drift detection
    completed and the stack snapshot records `IN_SYNC`, but the controller
    rejected the polling response because it required
    `StackDriftDetectionStatus`. AWS returns that state in `DetectionStatus`.
    Promotion stopped before creating or executing a change set, and no live
    alias changed.

    A provider-shaped unit regression failed before the correction with
    `missing field StackDriftDetectionStatus`. The minimal field binding now
    consumes `DetectionStatus`; the regression passes, while failed, unknown
    and drifted responses retain their existing fail-closed behavior.
    Application cleanup is all true. The exact bounded RDS cleanup rerun after
    the managed-secret deletion window is all true, and recovery
    `final-cleanup.json` proves the temporary database, bootstrap principals,
    profiles and local credential files absent. Exact-head hosted
    qualification, merge, exact-main qualification and another live rehearsal
    remain required before tag or registry publication.
32. The response-field correction passed exact PR-head hosted run
    [`30563657881`](https://github.com/xicv/minco/actions/runs/30563657881) at
    `f952af63d3848333c8a56782fe3b42e73dd457fd`, merged as exact `main`
    `ff242141c98c4d555de3ed232dba4437ff59ee17`, passed the complete local
    release matrix and passed exact-main hosted run
    [`30565805289`](https://github.com/xicv/minco/actions/runs/30565805289).

    Authorised live run `20260730t174217z-release040` migrated and verified
    private PostgreSQL, reproduced the 5,039,398-byte native ARM64 artifact,
    sealed release `minco.b100be45a4972f08cb3a554f`, applied its reviewed
    change set and passed every candidate hosted check. Promotion failed
    closed before any change set or live alias mutation because
    CloudFormation resource drift inspection required
    `lambda:GetProvisionedConcurrencyConfig` for the run-owned function's
    versions and aliases plus wildcard-resource
    `logs:DescribeIndexPolicies`. No tag or registry upload occurred.

    The focused rendered-policy regression failed before the correction with
    `owned function policy misses the provider drift-read permission` and
    passes after it. The Lambda action is confined to the exact run-owned
    function ARN and qualified ARN pattern. AWS documents
    `DescribeIndexPolicies` as a wildcard-resource List action, so it is added
    to the existing log metadata discovery statement without an action
    wildcard. The application cleanup receipt and exact RDS recovery rerun are
    all true; `final-cleanup.json` also proves bootstrap principals, profiles
    and local credential files absent. Exact-head hosted qualification, merge,
    exact-main qualification and another live rehearsal remain required.
33. The drift-resource IAM correction passed exact PR-head hosted run
    [`30570766634`](https://github.com/xicv/minco/actions/runs/30570766634) at
    `367d04e0476e9225e64626966245313340d54a71`, merged as exact `main`
    `982bc9bf2e58597b9d7df2b7fe3e39d5a89f83b9`, passed the complete local
    release matrix and passed exact-main hosted run
    [`30573067627`](https://github.com/xicv/minco/actions/runs/30573067627).

    Authorised live run `20260730t191908z-release040` migrated and verified
    private PostgreSQL, reproduced the 5,039,398-byte native ARM64 artifact
    with SHA-256
    `92dc989125a6032e378eaa660303939a9fadc0920bb3b2d0606bc2bcaaf86d11`,
    sealed verified release `minco.30360dc26d7e73b91c2657fe` with digest
    `30360dc26d7e73b91c2657fe30e237941b35903a9f6b96f670bb4d68006e4802`,
    applied reviewed change-set receipt digest
    `6eb539a96f9a73ddaa14db2af91b3db37cdcb6a57c2c73bdb9a53d3cbe263ad5`
    and passed every candidate hosted check under verification digest
    `028a83bec858aa4622cd8b15c1ff5d75e0d03f99b1ba0b1411da7bb494d2af92`.
    Promotion failed closed before creating or executing a change set because
    CloudFormation drift inspection required
    `lambda:GetRuntimeManagementConfig` on the published function version. No
    live alias changed, and no tag or registry upload occurred.

    The focused rendered-policy regression failed before the correction with
    `owned function policy misses the version runtime drift-read permission`
    and passes after it. The new read remains confined to the exact run-owned
    function ARN and qualified ARN pattern; no action or resource wildcard was
    added. ShellCheck warning/error classes pass. The application cleanup
    receipt is all true. The first RDS cleanup check caught AWS's delayed
    managed-secret deletion and failed closed; the exact recovery rerun is all
    true, both run-specific stacks are independently absent, and
    `final-cleanup.json` proves bootstrap principals, profiles and local
    credential files absent. Exact-head hosted qualification, merge,
    exact-main qualification and another live rehearsal remain required.

Corrected pull-request head
`46be92f0b68e6759a897ef5e99c010d77c2bf32b` passed manual hosted run
[`30410242657`](https://github.com/xicv/minco/actions/runs/30410242657).
Every material stage passed: authoritative quality, Chromium/Firefox,
coordinated 28-package publication dry run, Plan/SAM and both native ARM64
Lambda artifacts, Rustack/SSM conformance and Orders E2E. No package upload or
live AWS mutation occurred.

Corrected exact head
`b211b5083b43a0c9a0de9cd28ca4f748dfbbeb51` then passed manual hosted run
[`30412849538`](https://github.com/xicv/minco/actions/runs/30412849538).
Every material stage passed again, including the coordinated package dry run
that exercises the corrected `minco-dev` fixture. No package upload, tag
creation or live AWS mutation occurred. M8-T07 is complete and the pull
request is ready for an exact-head guarded merge; the final evidence-only
record still requires its own exact-head qualification before merge.

Regression fixtures assert the coordinated command, archive-only patch paths,
offline archive-test boundary and external-consumer manifest. The controller
now compiles four consumers from unpacked archives (`minco` no-default,
default and full, plus the four first-publish crates), installs
`cargo-minco` from its unpacked archive and checks that the installed binary
reports `minco 0.4.0`. A partial recovery selection deliberately skips this
full-family consumer gate and cannot substitute for it.
The Lambda regression creates equivalent archives with different timestamps,
proves normalization yields the same digest and permissions, and proves an
unexpected entry leaves the original archive unchanged. Two consecutive real
Orders and worker builds reproduced the same normalized hashes.
The hosted-toolchain regression first failed with
`KeyError: 'Install pinned JJ'`, then passed after asserting the exact pinned
install and version-check commands. The ripgrep regression separately failed
with `KeyError: 'Install pinned ripgrep'` before its matching pinned install
and version check were added. The Zig regression failed with an empty
`zig_steps` list before asserting the exact immutable action and version. The focused
`cargo test -p cargo-minco --test compatibility_cli --locked` gate passed all
three JJ-backed tests locally. Skipped stages in all three failed hosted runs
are not counted as passes; in the third run Rustack and E2E were skipped.

Focused candidate gates passed:

```text
uv run --locked python scripts/validate_static.py
uv run --locked python scripts/test/repository_truth.py
uv run --locked python scripts/validate_publish.py --check-registry
uv run --locked python scripts/test/publish_validation.py
uv run --locked python scripts/test/lambda_artifact_reproducibility.py
uv run --locked python scripts/deep_review.py
uv run --locked python scripts/test/deep_review_exclusions.py
cargo fmt --all -- --check
cargo test -p cargo-minco --test compatibility_cli --locked
cargo check -p minco --no-default-features --locked
cargo check -p minco --locked
cargo check -p minco --features official-plugins --locked
cargo check -p minco --all-features --locked
cargo test -p minco-config --all-features --locked
cargo test -p minco-db --all-features --locked
cargo test -p minco-dev --all-features --locked
cargo test -p minco-deploy-aws --all-features --locked
cargo test -p minco --no-default-features --locked
cargo test -p minco --locked
cargo test -p minco --all-features --locked
cargo minco architecture
cargo minco inspect --json
cargo minco task ready --json
cargo minco roadmap status
scripts/aws/plan.sh
scripts/aws/validate.sh
scripts/aws/build-lambda.sh
scripts/aws/build-worker-lambda.sh
scripts/test/e2e.sh
scripts/dev/rustack-smoke.sh
npm run --prefix plugins/minco-plugin-feedback test:browser
scripts/release/package-list.sh
scripts/release/publish.sh --skip-quality
```

The browser gate used the repository lockfile with Node 24 after Node 26
browser-engine installation stalled. Chromium and Firefox completed all 40
tests. Orders E2E passed. Rustack completed S3, SQS, SSM and STS conformance
plus Minco adapter checks under account `000000000000`, then cleaned its
emulated resources. Neither gate contacted or mutated AWS.

The coordinated release dry run verified and staged all 28 archives, emitted
Cargo's expected dry-run upload abort for every package, ran the five configured
unpacked-archive suites, compiled all required archive consumers and installed
the archive-only CLI. `--execute` was never supplied.

Observed first complete archive set:

| Archive | Bytes | SHA-256 |
| --- | ---: | --- |
| `cargo-minco-0.4.0.crate` | 116394 | `0a42f971d445efdf30fb034823b1f3d3bf665268570b96923b903997052607e4` |
| `minco-0.4.0.crate` | 35218 | `41d722b94f9f7887ba8c0c796aba6a13cbcbf63063c9fbd9da533981aec73230` |
| `minco-aws-adapters-0.4.0.crate` | 50929 | `8effec677afdfdafed81187a5e3855a1a277c26c9b2cde6819f5bb660a31fd3c` |
| `minco-aws-lambda-0.4.0.crate` | 23598 | `13a10e273fbfb0292fd8b3e04af4f6dd4db2bb2bee5542ce9741b8c99c622591` |
| `minco-aws-worker-0.4.0.crate` | 19870 | `6b5afcf023a6d0548db4f3e125ed213ed6735aaa738d06a02eb2c630293d46f6` |
| `minco-config-0.4.0.crate` | 20387 | `47a96d6d1fe3e2cccfcdd47d4eb9a27d4f7ad6e0e2675a471a07b55607c0d20f` |
| `minco-contract-0.4.0.crate` | 28460 | `76121b309d0df3858bcc95f0f7f9185b0833be6e0e34e7b9f1fb72ec45b88e9f` |
| `minco-core-0.4.0.crate` | 25745 | `e78434810174282cc9900749cb8ac23b523b5ea491db1f91a57f57b6fbebde57` |
| `minco-db-0.4.0.crate` | 19749 | `fd38ab2a093473463ecc18bbc4712a425a1439b2d091b5c0afdd982785b7d33d` |
| `minco-deploy-aws-0.4.0.crate` | 30835 | `e74f557183d00ebd54477f04fe818f32d23803b15cd5f180fe62b248409c5358` |
| `minco-dev-0.4.0.crate` | 28735 | `ecfa93ac0166f1592c3a1ae04c11844459adeb48f4969ddae21ef12ee3e58828` |
| `minco-http-0.4.0.crate` | 20197 | `1c3c88240688a3d004e461f07a1de540fb7c840cd227a988a11d11cecbfe0225` |
| `minco-plan-0.4.0.crate` | 37318 | `80b315c749797b7173b9a4d967e639d4aabebf5de904c15d72a1d6584cf8ca58` |
| `minco-plugin-audit-0.4.0.crate` | 11780 | `8f044d45d04dcc77daa17cd7b24bd1213afb00a74134af7e56fc4e10a32c354a` |
| `minco-plugin-events-0.4.0.crate` | 13964 | `21d9cc5c206dc100cdd39d02f98ba512f0e69f12fe8ca47286e0c2940fb80eae` |
| `minco-plugin-feedback-0.4.0.crate` | 78770 | `a2cda3578d616ccf780389a071f3ff30b5ddb79cf895038064e222ce9faead0f` |
| `minco-plugin-health-0.4.0.crate` | 9817 | `95f80fda2f57fcee6d73758e75cde52c11736934a4aec17596047e2c4bafcc9b` |
| `minco-plugin-idempotency-0.4.0.crate` | 14330 | `7189a2aba4f0adbfdce17ed761bc0f68a2c819ed0830dedfe2a59997f7a8043a` |
| `minco-plugin-identity-0.4.0.crate` | 16872 | `ad76388253cab9acde022a1e32394e33fdab0a2cfdbd6db02166163a39e6700c` |
| `minco-plugin-notifications-0.4.0.crate` | 11788 | `696ea3b3a36f995b8a957db86b5c84a9dae1a19876c4e38f4b76c39198b7c4bf` |
| `minco-plugin-object-storage-0.4.0.crate` | 14321 | `fda540c4393529fea6bbc08779c7e81db7c12aee1bd65bde8c01e473e7a4a5b1` |
| `minco-plugin-observability-0.4.0.crate` | 10037 | `5c58f3074eaea9f86f79c3321d6626e3c15fc6c198968ac0b3ad64fa0299cea4` |
| `minco-plugin-sessions-0.4.0.crate` | 15115 | `09577095c7c4d0b69f467cbbbe74ff670135d668f86e83e654faa3ea49574f6d` |
| `minco-plugin-static-site-0.4.0.crate` | 12507 | `5dd42539129abf4bf9a86b1a0b7336b02bacb9cec72bfec3335c2457effa48c3` |
| `minco-release-0.4.0.crate` | 18242 | `25b2b21a7f018bfb8629946e2ddf26323c53e3bbd8cd3cf4a9d5572c89f7be25` |
| `minco-sqlx-postgres-0.4.0.crate` | 32082 | `4b5a192e5329b2251199936d536bc06168df1b30b2eee59f0e146ba6ed57159e` |
| `minco-sqlx-sqlite-0.4.0.crate` | 29987 | `b7a904945f0f39f12a42985657ba161bb753b3a705c80db8bf0313441b2a29fa` |
| `minco-test-0.4.0.crate` | 12666 | `5bae49c5588bc6dcc09dc70aec6c94b4d83c14f7f1dfb0e2c13f983fe777a119` |

The sorted archive-manifest digest is
`cbd9d81b24fd1c1ceba42a89952f97c76b0c063c9d3e456d34b2847a3d8bc0c5`.
The final clean-source run reproduced every archive byte count and SHA-256
exactly.

Facade dependency observations versus `0.3.1` are 16/105/118/300 normal
packages for no-default/default/official/all-feature profiles, with deltas
0/0/0/+10. Feature-tree line counts are 81/824/1050/3453. Initial cold/follow-on
facade build observations were 5.53 and 45.38 seconds. These are local samples,
not release budgets.

The exact-source native ARM64 Orders ZIP is 5,035,518 compressed /
11,048,288 uncompressed bytes with SHA-256
`42ae9c1056738dd2ccd39864a69965cb13b4de6eb1f3c4177bacc1575aafa04f`
and a 127.87-second cold build observation. The worker ZIP is 574,199 /
1,203,520 bytes with SHA-256
`c1508117d7329029aaedc85691b416f3321d1fa11831c5c162f9647465bd3a44`
and a 15.16-second follow-on build observation. Both are below the 10 MiB
compressed policy. The durable measurement report binds these observations to
the final source-tree digest.

The generated AWS plan and SAM static gates pass without provider contact.
Plan SHA-256 is
`b104438b8eb61dcef6a7585a7e2f35565dd59b83da3973a4adcde10125ce4c9d`;
template SHA-256 is
`e25a3c0d61ad8bddc795e92067def9728d102c8090e3355a511c414ed090e372`.
The minimal profile retains no NAT gateway, fixed compute, schedule or
provisioned concurrency. Minco promises zero provisioned application compute
at idle, not zero bill: storage, retained logs, DNS, secrets, database storage,
schedules and other fixed/request dimensions remain explicit and bounded.

The operator separately authorised the bounded live-AWS rehearsal and the
irreversible exact tag, crates.io publication and GitHub release on 2026-07-29.
The SSM-name, Cognito-tagging and JSON-parameter corrections passed exact-main
local and hosted qualification. The subsequent live controller invocation
reached a real unexecuted CloudFormation change set and exposed the documented
absence of `ChangeSetType` from `DescribeChangeSet`, plus the cleanup
controller's handling of an empty untagged review shell. Exact application,
database, VPC, secret and bootstrap-IAM absence is proven. Until the guarded
parser and review-shell cleanup corrections are reviewed, merged, requalified
and the replacement live rehearsal passes with cleanup proof, the release
verdict remains `live_deployment_gate_blocked`. No tag or registry upload has
occurred.

## M8-T03 trusted-publishing closure

On 2026-07-28, an authenticated crates.io preflight found no existing trusted
publisher and no conflicting configuration for any of the 24 packages already
published at `0.3.1`. Each package was then configured with the same exact
GitHub identity:

- repository: `xicv/minco`;
- workflow: `publish-crates.yml`;
- environment: `crates-io`.

The created crates.io configuration IDs are the contiguous range
`14327..=14350`. A separate authenticated read-back returned exactly one
matching configuration for each of the 24 packages and no errors. The
unpublished `minco-config` candidate was deliberately excluded: crates.io
requires its first release before a trusted publisher can be configured, and
M8-T03 does not authorize an upload.

The sole `xicv` owner remains intentional under the explicit single-maintainer
policy. There is no co-maintainer or required environment reviewer. Agent review
and the pinned, least-privilege, manual-only workflow controls documented in
`docs/development/publishing.md` are the release boundary.

The workflow change was developed with behavior-level red/green checks. The
initial check failed because no `authenticate` input or authentication-only job
existed. A second red check rejected the unnecessary `contents: read`
permission. The final structured YAML check proves that:

- `authenticate` defaults to false;
- authentication-only routing requires `authenticate=true` and
  `publish=false`;
- the authentication-only job has only `id-token: write`, contains no shell
  step, and uses the action pinned at
  `c6f97d42243bad5fab37ca0427f495c86d5b1a18`;
- the upload command remains separately gated by explicit `publish=true`.

Hosted workflow-dispatch run
[`30313972544`](https://github.com/xicv/minco/actions/runs/30313972544)
qualified commit `0a5dfb1397b240c5e1a92fdd64d34960a01b5f9c`. The
authentication action and its token-revocation post-step passed; the complete
release job was skipped. An independent post-run crates.io lookup found all 24
published packages still at maximum version `0.3.1` and `minco-config` still
returned HTTP 404. No crate upload occurred.

The task's registry command,
`uv run --locked python scripts/validate_publish.py --check-registry
--require-registry`, completed all 25 registry lookups and returned the expected
24 `PUBLISH-072` errors because every existing package version `0.3.1` is
immutable and already published. `minco-config` was the sole unpublished
candidate. This is an expected release-state rejection, not a passing
pre-release validator.

The final local `./scripts/quality.sh` suite passed. It covered repository
truth, static and deep review, publish metadata, formatting, the complete
feature matrix, strict workspace Clippy and tests, generated PostgreSQL and
SQLite consumer workspaces, Rustdoc and documentation, `cargo deny`,
`cargo audit`, npm audit, Gitleaks, and the final source-manifest check. The
required clean-workspace `scripts/release/publish.sh --skip-quality` command
also passed for all 25 current source candidates. It used Cargo's `--dry-run`
path; `--execute` was not supplied and every upload was aborted.

## `0.3.1` publication evidence

The patch release contains the text-only Feedback boundary merged in PR #15
and exact SQLx backend feature isolation merged in PR #16. It changes no public
Rust API or serialized contract shape and retains the same 24-package release
inventory as `0.3.0`. The larger multi-runtime Plan IR redesign remains outside
this release and is tracked separately as M6-T10.

The source-fix merge commit is
`cd679c74d44e04abe1655b71c8ca9b9381aa6f6b`. Hosted run
`30247725599` passed authoritative quality, the Chromium/Firefox Feedback
matrix, all-package publication dry run, Rustack/SSM conformance, and Orders
E2E on that exact merged `main` source before this release change began.

Release PR #17 exact head
`36b52a18893aded72284601503272fa0b444a403` passed hosted run
`30249418058`. Merge commit
`33719376b634e995c0bfdbe6c215f1c304cd6b5d` passed merged-main hosted run
`30249977158`. Both runs passed authoritative quality, the Chromium/Firefox
Feedback matrix, the 24-package publish dry run, Rustack/SSM conformance, and
Orders E2E. Remote tag `v0.3.1` resolves exactly to that merge commit.

Trusted-publisher run `30250487113` passed every source and packaging gate but
stopped before upload because crates.io had no trusted-publisher configuration
for `xicv/minco`. The documented authenticated fallback then published all 24
packages from a clean detached worktree at the exact tag without a partial
failure.

Independent post-publication verification downloaded every exact `.crate`
archive, matched all 24 crates.io SHA-256 checksums, confirmed every record is
not yanked, and confirmed owner `xicv`. A fresh locked
`cargo-minco 0.3.1` installation reports `minco 0.3.1`; a fresh external
consumer resolves and checks `minco = "=0.3.1"` with the declared Rust 1.97.1
toolchain.

All 24 exact docs.rs library routes return HTTP 200 directly. The final
`minco` facade build reports that all builds succeeded.

## `0.3.0` release boundary

The `0.3.0` release adds bounded registration provenance to the strengthened
plugin kernel published in `0.2.0`. It is a pre-1.0 minor release because it
changes public registrar return types and the `ServiceError::Duplicate`
payload. Publication is proven separately by the exact remote tag and
independent crates.io records; source metadata alone is not publication proof.

The release verification covers:

- Rust format/check/Clippy/test/Rustdoc gates across all targets and features;
- generated PostgreSQL and SQLite applications;
- real SQLite/PostgreSQL Feedback persistence;
- Chromium/Firefox widget E2E, cargo-deny, gitleaks and npm audit;
- native ARM64 Lambda ZIP packaging and all-package publication dry runs;
- deterministic Plan IR and SAM generation;
- graph-derived PostgreSQL/Rustack startup and isolated real Rustack
  S3/SQS/SSM/STS conformance through standard AWS endpoint variables,
  including `minco-aws-lambda` SecureString loading through the Rust SDK;
- SAM CLI linting plus read-only CloudFormation and IAM Access Analyzer
  validation.

The current adoption-readiness task creates no AWS resources. Earlier M5/M6
tasks contain bounded real-AWS adapter evidence and verified cleanup; this task
does not refresh or broaden that evidence. The local Docker API did not answer
read-only status calls during M6-T06, so its PostgreSQL and Rustack reruns are
environment-blocked rather than passed; earlier evidence remains historical.
Rustack proof is emulator proof even when executable.
The repository-wide Codex Security Deep Scan did not produce a canonical
completed report for the Feedback release; M6-T05 records the release-scoped
waiver and compensating checks. That waiver is not a scan pass and does not
automatically apply to a later release.

## M6-T07 plugin-registration provenance evidence

Base Git SHA:
`c5b7749cec295fddd795827733e2889d6f1f896b`.

The candidate now retains authoritative application/plugin ownership for
typed singleton services and ordered contributions. Plugin owners are opaque
and created only by `PluginManager`; direct application collections retain a
distinct application owner. Duplicate singleton diagnostics include the Rust
type, first owner and attempted owner. Frozen contribution summaries retain
global deterministic installation indices.

`ComposedApplication::registration_provenance()` and `cargo minco inspect
--json` serialize metadata only. Focused tests use service values with
deliberately sensitive `Debug` output and prove that neither values nor debug
content enter JSON. A compile-fail public API example plus runtime ownership
tests prove a plugin cannot supply another plugin's identity.

Passed:

```text
cargo fmt --all -- --check
cargo check -p minco-core -p cargo-minco --all-targets --all-features --locked
cargo clippy -p minco-core -p cargo-minco --all-targets --all-features --locked -- -D warnings
cargo test -p minco-core -p cargo-minco --all-features --locked
cargo check --workspace --all-targets --all-features --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-targets --all-features --locked
cargo doc --workspace --all-features --no-deps --locked
cargo minco inspect --json
scripts/aws/build-lambda.sh
cargo lambda build --release --arm64 --output-format zip -p minco-aws-worker --example sqs_worker --locked
```

The first focused strict-Clippy run failed because the manual `Debug`
implementations for the two mutable registries omitted newly added metadata
fields. They now report only counts and the next installation index; the exact
focused and workspace Clippy commands pass. No concrete registration values
were added to `Debug`.

The refreshed Orders ARM64 ZIP is 5,028,504 compressed / 11,043,648
uncompressed bytes. That is 15,502 bytes (0.3092%) above the immutable M6-T06
baseline and remains below the 10 MiB policy. The SQS worker remains 573,418 /
1,203,520 bytes. Cold local observations were 10.15 seconds for default facade
compilation, another 40.72 seconds for the all-feature increment, 110.28 seconds
for Orders Lambda and 12.78 seconds for the worker. These are single local
samples, not CI budgets. Both Cargo Lambda builds emitted the existing macOS
linker warning that deprecated optimization setting `1` was ignored; packaging
still succeeded.

Real-AWS, Rustack and PostgreSQL tests requiring explicitly configured external
environments remained ignored in the ordinary workspace test command. This
task does not refresh those provider proofs and does not create remote
resources.

The authoritative `./scripts/quality.sh` command passes, including generated
PostgreSQL and SQLite consumers, Rustdoc/docs, cargo-deny, RustSec audit,
Feedback npm audit and Gitleaks. The separate bounded inspection assertion,
official-plugin validation, package inventory, reverse-apply whitespace check,
source-manifest check and JJ conflict query pass. The 24-package publication
driver passes without `--execute`; Cargo verified every package tarball and
aborted every upload because of `--dry-run`.

The first publication dry run packaged all 24 crates and then failed during
packaged `minco-http` verification with `No space left on device`. Only this
isolated workspace's generated Cargo target was cleared; the unchanged
clean-source retry passed. No upload, tag, deployment, database or product
repository mutation occurred.

Exact commands, results and current limitations are recorded in
`FEEDBACK_REVIEW_STATUS.md` and `CODEX_HANDOFF.md`. The release history below
preserves the `0.1.x` evidence and records the current `0.2.0` boundary.

## Adoption footprint measurements

The durable machine-readable comparison is
`verification/adoption-measurements.json`. Dependency trees and native ARM64
artifacts were measured on the same pinned Rust/Cargo toolchain from isolated
cold targets.

| Facade selection | Baseline packages / feature lines | Candidate packages / feature lines |
|---|---:|---:|
| no default features | 16 / 81 | 16 / 81 |
| default features | 105 / 820 | 105 / 820 |
| `official-plugins` | 118 / 1040 | 118 / 1040 |
| all features | 290 / 3351 | 298 / 3424 |

The no-default, default and official-plugin surfaces do not grow. The
all-feature graph adds eight packages for the opt-in SQS Lambda runtime. Cold
baseline default and all-feature-increment builds measured 10.23 and 48.87
seconds. The current candidate report does not record corresponding general
build timings. Its isolated native ARM64 artifact builds recorded 21.15 seconds
for the Orders Lambda and 5.88 seconds for the SQS worker. These single local
wall-clock samples are observational and are not CI budgets.

The baseline Orders ARM64 Lambda ZIP was 5,013,002 compressed bytes and
11,000,744 uncompressed bytes. The candidate ZIP measured 5,030,945 compressed
bytes and 11,047,008 uncompressed bytes, a 17,943-byte (0.3579%) compressed
increase. The new opt-in SQS worker ZIP measured 573,415 compressed and
1,203,520 uncompressed bytes. The candidate report records exact SHA-256
digests for both ZIPs in addition to their compressed/uncompressed sizes.
`cargo-bloat` and `cargo-llvm-lines` were unavailable.

The committed baseline snapshot is bound to Git SHA
`6fe9121ea9284e2fa4e2dbfd76f21bd8a13e263a`; the candidate measurement is bound
to the immutable `source-tree-sha256` recorded in both the adoption report and
`verification/source-manifest.json`. The manifest excludes itself and the
adoption report to avoid self-reference, and `scripts/source_manifest.py
--check` recomputes every other distributable file without writing. The report
is regenerated by `scripts/measure_adoption.py`, which accepts both revisions,
timings and artifact paths and computes compressed/uncompressed sizes and
deltas rather than relying on a hand-edited comparison.

## M6-T06 exact-source local evidence

The authoritative `./scripts/quality.sh` entry point passed after the complete
change. It ran current static/truth/publish/deep-review fixtures; SQLite schema,
scaffold and dependency hygiene; no-default/default/official/worker/all-feature
facade checks; workspace all-target/all-feature check, strict Clippy and tests;
fresh generated PostgreSQL and SQLite application check/tests; Rustdoc/docs;
`cargo deny`, `cargo audit`, Feedback `npm audit`; and redacted full-source
Gitleaks. The generated-consumer target was changed to share the repository
Cargo cache and disable debug/incremental artifacts in the quality runner; an
earlier exact command failed with `No space left on device` and was not treated
as a pass.

Additional passed checks:

```text
cargo minco contract sync
cargo minco contract sync --check
scripts/test/e2e.sh
npm run --prefix plugins/minco-plugin-feedback test:browser
scripts/aws/plan.sh
scripts/aws/validate.sh
scripts/aws/build-lambda.sh
cargo lambda build --release --arm64 --output-format zip -p minco-aws-worker --example sqs_worker --locked
sam validate --lint --template-file infra/aws/generated/template.yaml
jj diff --git | git apply --reverse --check --whitespace=error-all
jj log -r 'conflicts()'
```

The browser matrix passed 38 Chromium/Firefox tests. The local Orders HTTP E2E
passed. The shared Docker daemon did not answer read-only status calls, so the
Docker-backed PostgreSQL and Rustack reruns are explicitly environment-blocked.
No Docker restart was attempted because it could disrupt unrelated user
containers. No AWS mutation, deployment, crate upload or tag occurred.

For the final hosted-controller correction, the repository's `get-api-docs`
workflow found no local Context package for Cargo Lambda and used the official
Cargo Lambda installation and GitHub Actions guidance. That guidance requires
Zig for the default cross-compiler and shows Zig `0.14.0` on Linux runners.

## Release history and current boundary

### 0.2.0 publication boundary

Remote tag `v0.2.0` resolves exactly to
`c5b7749cec295fddd795827733e2889d6f1f896b`. A review-time
`scripts/validate_publish.py --require-registry` lookup succeeded for all 24
package names and reported each exact `0.2.0` version as already present on
crates.io. This proves the version is immutable and cannot contain M6-T07.

That lookup did not refresh downloaded archive checksums, ownership, docs.rs,
installation, or a GitHub release object. Those remain separate evidence. The
M6-T07 workspace is therefore `0.3.0`; no tag, upload, release, or deployment
is performed by this change.

### 0.1.x release history

All 14 public packages were accepted by crates.io at version `0.1.0` on
2026-07-24 and are owned by `xicv`. The published CLI compiles, installs, and
runs, but its binary-only archive cannot satisfy docs.rs `cargo rustdoc --lib`.

Version `0.1.1` was the lock-step patch release containing the `M8-T04`
library documentation target and the local/hosted Rustdoc regression gate.

The sections below retain the original `M8-T02` pre-publication evidence. They
are historical evidence, not claims about the current registry state.

## M8-T05 publication evidence

Minco `0.1.1` was published from remote tag `v0.1.1`, which resolves exactly
to merge commit `3da298c094ef515a68dcc18ee6a2b867dcd4889e`.

Release gates:

- PR `#5` exact head `23afb15d8b2ec71baa5da203467fca9d7969be01`
  passed hosted run `30069887615`.
- The exact merged-main commit passed hosted run `30070145165`.
- The complete local quality suite, generated PostgreSQL and SQLite consumer
  compilation/tests, docs.rs-shaped Rustdoc command, and 14-package Cargo
  publish dry run passed before tagging.
- Cargo accepted all 14 uploads in dependency order without a partial failure.

Post-publication verification:

- all 14 exact `0.1.1` registry records exist and are not yanked;
- every downloaded `.crate` archive matches its registry SHA-256 checksum;
- `cargo owner --list` reports `xicv` for every package;
- `cargo install cargo-minco --version 0.1.1 --locked` succeeds from crates.io,
  and the executable reports `minco 0.1.1`;
- all 14 exact library documentation routes return HTTP 200 without redirect;
- the `cargo_minco 0.1.1` Rustdoc page renders the README-backed CLI usage from
  the new library target.

At the time of the `0.1.1` evidence capture, task `M8-T03` remained active for
ownership and GitHub OIDC trusted-publisher work. The 2026-07-28 closure section
above records the later single-maintainer decision and completed configuration.

## Publication shape

The workspace contains 19 Cargo packages:

- 14 public packages restricted to `crates-io`;
- 5 private Orders reference-application packages with `publish = false`.

The public family is published in this dependency order:

```text
minco-core
minco-contract
minco-http
minco-release
minco-test
minco-sqlx-postgres
minco-sqlx-sqlite
minco-plan
minco-plugin-health
minco-plugin-observability
minco-plugin-idempotency
minco-aws-lambda
minco
cargo-minco
```

The normal application dependency is the `minco` facade. The development control plane is the `cargo-minco` binary, exposed by Cargo as `cargo minco`.

## Performed and passed

### Static repository validation

Command:

```bash
python3 scripts/validate_static.py
```

Result:

```text
status:                 ok
errors:                 0
warnings:               0
workspace packages:     19
Rust source files:      47
OpenAPI operations:     4
OpenAPI schemas:        10
plugin catalog entries: 6
roadmap milestones:     9
task records:           18
```

The validator checks repository structure, TOML/YAML/JSON parsing, workspace member targets, the pinned toolchain declaration, OpenAPI profile rules, generated-contract drift, operation inventory, architecture boundaries, plugin selection and manifests, roadmap/task graphs, deployment-plan drift, structural cost/performance controls, SAM route coverage, placeholder detection, credential patterns, Python syntax, and shell syntax.

Evidence: `verification/static-validation.json`.

### crates.io publication-structure validation

Command:

```bash
python3 scripts/validate_publish.py --check-registry --require-registry
```

Result:

```text
status:               ok
errors:               0
warnings:             0
public packages:      14
private packages:     5
registry checks:      14
```

The validator confirms:

- complete crates.io metadata;
- dual-license files and explicit package-content allowlists;
- `publish = ["crates-io"]` for every public package;
- `publish = false` for private examples;
- lock-step version `0.1.0`;
- explicit version plus local path for every public internal dependency;
- a dependency-valid multi-package release order;
- the `minco` facade and feature matrix;
- the `cargo-minco` executable name and Cargo-argument normalization;
- local README and package-file presence.

Evidence: `verification/publish-validation.json`.

### Crate-name availability check

On 2026-07-24, exact crates.io API lookups returned `404` for all 14 proposed
names. This is evidence only; it is not a reservation and must be repeated
immediately before the first upload.

Evidence: `verification/crate-name-availability.json`.

### Generated application profiles

Command:

```bash
python3 scripts/test/scaffold_templates.py
scripts/test/generated_apps.sh
```

Passed for both generated profiles:

```text
postgres
sqlite
```

For each profile the static test renders and parses the layered workspace,
validates 11 TOML files, 2 YAML files, 8 Rust source files, 5 workspace
packages, migrations, and the two-operation OpenAPI contract. The compiler
test then generated fresh PostgreSQL and SQLite workspaces and successfully
ran both `cargo check --workspace --all-targets` and
`cargo test --workspace --all-targets`. The first compiler run found that
generated API DTOs used `chrono` and `uuid` without direct dependencies; the
scaffold manifests were repaired and both clean generations passed.

Evidence: `verification/scaffold-templates.json`.

### Deep static review

Command:

```bash
python3 scripts/deep_review.py
```

Result:

```text
status:   ok
errors:   0
warnings: 2
```

The two heuristic warnings count `expect` calls used after `writeln!` into
`String` in the contract and SAM renderers. Those writes are infallible by the
`fmt::Write for String` implementation, and strict Clippy plus renderer tests
pass. They are retained as visible review findings rather than suppressed.

Evidence: `verification/deep-review.json`.

### SQLite schema behavior

Command:

```bash
python3 scripts/test/sqlite_schema.py
```

The real SQLite engine executed the reference migration and verified foreign keys, JSON constraints, persistence behavior, and idempotency-key uniqueness.

Evidence: `verification/sqlite-schema.txt`.

### Deterministic non-Rust checks

Performed:

```text
Python py_compile over repository scripts
bash -n over every shell script
deterministic generation of Plan IR, SAM, roadmap and task graphs
source SHA-256 manifest generation
archive integrity and external checksum verification
```

Evidence is retained under `verification/`.

### Rust compiler and feature gates

The dedicated JJ workspace used the repository-pinned toolchain:

```text
rustc 1.97.1 (8bab26f4f 2026-07-14)
cargo 1.97.1 (c980f4866 2026-06-30)
rustfmt 1.9.0-stable
clippy 0.1.97
jj 0.43.0
```

`Cargo.lock` was generated by Cargo, reviewed, and contains 326 external
packages from the crates.io index only. The following exact gates passed:

```bash
cargo fmt --all -- --check
cargo check -p minco --no-default-features --locked
cargo check -p minco --locked
cargo check -p minco --all-features --locked
cargo check -p cargo-minco --locked
cargo test -p minco --no-default-features --locked
cargo test -p minco --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-targets --all-features --locked
scripts/test/generated_apps.sh
cargo doc --workspace --all-features --no-deps --locked
```

The compiler pass found and repaired source-assembly defects including a
missing direct `thiserror` dependency, feature-specific mutability, generated
Rustfmt drift, strict Clippy findings, and invalid Lambda error context
conversion. `./scripts/quality.sh` then passed end to end.

### Cargo package and publication dry run

From a clean JJ working-copy commit:

```bash
scripts/release/publish.sh
scripts/release/package-list.sh
cargo package --locked --package <all 14 release packages>
```

The dry-run driver re-ran the complete quality suite, completed 14 live
registry checks, normalized and extracted every package, compiled every
package against Cargo's temporary registry, and stopped each upload at Cargo's
dry-run boundary. No `--allow-dirty` or `--no-verify` option was used.

The retained `.crate` archives range from 8.8 KiB to 37.0 KiB compressed.
Their file counts, sizes, SHA-256 digests, and intended content review are
recorded in `verification/package-artifacts.txt`.

The driver originally failed closed because JJ 0.43 removed
`jj resolve --list`; its conflict guard now uses the repository-standard
`jj log -r 'conflicts()'` query.

## Not performed by M8-T02

No crate was uploaded. No crates.io token was used. No GitHub release, tag,
trusted publisher, or owner assignment was created. Those are task `M8-T03`
actions and remain outside this compiler/package task.

## Historical first-upload boundary

This read-only preflight also passed on 2026-07-24:

```bash
python3 scripts/validate_publish.py --expect-unpublished --require-registry
```

All 14 exact names were absent at check time. This is not a reservation and
must be repeated immediately before the first upload. Then follow
`docs/development/publishing.md`. The first version of every new crate must be
published by an authenticated owner. Configure protected OIDC trusted
publishing only after each crate exists and ownership has been established.

## M8-T02 conclusion

Minco `0.1.0` is **compiler-verified and Cargo dry-run verified** across the
complete 14-crate family. The generated PostgreSQL and SQLite applications
also compile and test successfully.

Task `M8-T03` remains the separate irreversible registry-release task. Nothing
was published in this task.
