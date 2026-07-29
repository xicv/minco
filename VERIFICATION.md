# Minco verification and release evidence

Date: 2026-07-29
Current workspace version: `0.4.0`
Published baseline: `0.3.1`
Purpose: qualify the M8-T07 source/package candidate while preserving published
M8 evidence and the independently qualified `0.3.1` release history.

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

The release reconciliation found eight fail-closed controller defects before
qualification:

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
The corrected SSM-name source passed exact-main local and hosted qualification.
The subsequent live controller invocation found the bounded Cognito tagging
permission defect after database migration, then proved every run-owned
application, database, VPC, secret and bootstrap-IAM resource absent. Until the
least-privilege policy correction is reviewed, merged, requalified and the
replacement live rehearsal passes with cleanup proof, the release verdict
remains `live_deployment_gate_blocked`. No tag or registry upload has occurred.

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
