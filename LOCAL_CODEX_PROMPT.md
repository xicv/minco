# Ready-to-paste Codex prompt: finish and publish the Minco Feedback change

Use the prompt below in Codex after extracting the accompanying Minco ZIP or
overlaying it onto a clean clone of `https://github.com/xicv/minco`.

---

You are working on the Minco Rust framework repository. Treat this as a
production framework change, not a prototype.

## Objective

Finish, compiler-verify, and publish a draft GitHub pull request for the current
core/plugin-system audit and the new official `minco-plugin-feedback` vertical
slice.

The supplied source snapshot was prepared on branch
`agent/feedback-plugin-and-core-audit` against base commit
`e0f6f5a7b86a3bea1fcc1f018c93f812114f774d`. It has passed deterministic static,
publication-structure, deep-review, Feedback-contract, JavaScript syntax,
Python syntax, shell syntax, whitespace, UTF-8, and line-ending checks. It has
not been compiled because the assembly environment did not contain Rust.

## Non-negotiable architecture

Preserve these invariants:

1. OpenAPI is the authoritative external HTTP contract.
2. `minco-core` remains provider-neutral and does not depend on Axum, SQLx,
   Lambda, or AWS SDK crates.
3. Plugins are statically linked ordinary Rust crates with explicit
   registration. Do not add dynamic-library loading, automatic filesystem
   discovery, global facades, or a string-key service locator.
4. Plugin descriptors declare semantic version, Minco core compatibility,
   dependencies, required/provided versioned capabilities, configuration,
   operations, migrations, health checks, resources, wake sources, idle-cost
   class, data sensitivity, stability, and documentation.
5. Plugin graph validation must complete before service construction.
6. Use typed single bindings for one authoritative service and deterministic
   ordered multi-contributions for zero-to-many extension points.
7. Keep install and finalization deterministic and free of migrations, remote
   calls, background workers, or other uncontrolled side effects.
8. Business rules stay in domain/application layers. Adapters implement narrow
   use-case ports. Do not add an ORM or generic repository abstraction.
9. Do not imply atomicity across independently configured stores. Feedback
   persistence remains authoritative; notification/audit/event failures are
   warnings unless an application supplies a transaction-integrated adapter.
10. No hidden polling schedule, NAT Gateway, provisioned concurrency, or fixed
    compute in the minimal profile.
11. JJ is the default mutation workflow; Git remains the GitHub transport in a
    colocated repository.
12. Do not weaken lints, delete meaningful tests, add `allow` suppressions merely
    to pass Clippy, or represent unperformed checks as successful.

## Read first

Read these files in order:

```text
AGENTS.md
REVIEW_STATUS.md
VERIFICATION.md
README.md
docs/architecture/capability-audit.md
docs/architecture/extensions.md
docs/architecture/plugin-authoring.md
docs/architecture/feedback-loop.md
docs/adrs/0014-plugin-lifecycle-and-feedback.md
plugins/minco-plugin-feedback/README.md
plugins/minco-plugin-feedback/openapi/feedback.openapi.yaml
tasks/M6/M6-T02-essential-plugins.md
tasks/M6/M6-T03-feedback-loop.md
tasks/M6/M6-T04-aws-plugin-adapters.md
tasks/M8/M8-T02-compiler-package-gates.md
```

## Repository setup

Prefer a colocated JJ/Git repository:

```bash
jj git init --colocate .
jj status
jj log -r 'all()' -n 10
```

When working from a clone whose `main` contains only the initial license, create
or keep the feature workspace and import the supplied snapshot there. Do not
commit directly on `main`.

A recommended workspace flow is:

```bash
jj workspace add ../minco-feedback-review -r main \
  -m 'feat: strengthen plugins and add Feedback loop'
cd ../minco-feedback-review
```

If the extracted snapshot already contains the source files, copy them into that
workspace while preserving `.git`/`.jj` from the clean clone.

## Toolchain

Install the pinned toolchain and required local tools:

```bash
rustup toolchain install 1.97.1 \
  --profile minimal \
  --component rustfmt \
  --component clippy

rustup override set 1.97.1
node --version
python3 --version
```

Install Docker, JJ, Cargo Lambda, SAM CLI, AWS CLI, and PostgreSQL client tooling
when required by the later gates. Do not substitute a newer unreviewed Rust
version for the pinned release without an ADR and full compatibility review.

## Phase 1: reproduce the non-compiler gates

Run:

```bash
python3 scripts/validate_static.py --output verification/static-validation.json
python3 scripts/validate_publish.py --output verification/publish-validation.json
python3 scripts/deep_review.py
cp target/minco/deep-review.json verification/deep-review.json
python3 scripts/test/feedback_contract.py > verification/feedback-contract.json
node --check plugins/minco-plugin-feedback/assets/widget.js
python3 -m py_compile $(find scripts -type f -name '*.py' | sort)
while IFS= read -r script; do bash -n "$script"; done \
  < <(find scripts -type f -name '*.sh' | sort)
git diff --check
```

Do not proceed until these remain green.

## Phase 2: dependency and formatting gate

Generate the real lockfile:

```bash
cargo generate-lockfile
```

Review `Cargo.lock` for unexpected dependency, licensing, duplicate-version, or
MSRV changes. Then run:

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets --all-features --locked
```

Fix compiler errors at their architectural source. Pay particular attention to:

- trait-object safety and `Send + Sync` bounds in plugin contributions;
- `Arc` downcasting and typed service/contribution registries;
- deterministic plugin dependency ordering and finalization;
- Axum 0.8 router state types and multipart/body-limit behavior;
- SQLx 0.9 executor and transaction APIs;
- Reqwest 0.13 multipart APIs;
- Lambda HTTP/API Gateway request-context APIs;
- feature-gated imports and docs.rs all-feature builds.

## Phase 3: focused plugin tests

Run each new plugin independently before the entire workspace:

```bash
cargo test -p minco-core --all-features --locked
cargo test -p minco-http --all-features --locked
cargo test -p minco-plugin-health --all-features --locked
cargo test -p minco-plugin-observability --all-features --locked
cargo test -p minco-plugin-idempotency --all-features --locked
cargo test -p minco-plugin-sessions --all-features --locked
cargo test -p minco-plugin-identity --all-features --locked
cargo test -p minco-plugin-object-storage --all-features --locked
cargo test -p minco-plugin-events --all-features --locked
cargo test -p minco-plugin-notifications --all-features --locked
cargo test -p minco-plugin-audit --all-features --locked
cargo test -p minco-plugin-static-site --all-features --locked
```

Test Feedback in every supported feature shape:

```bash
cargo test -p minco-plugin-feedback --no-default-features --locked
cargo test -p minco-plugin-feedback --locked
cargo test -p minco-plugin-feedback --features client --locked
cargo test -p minco-plugin-feedback --features postgres --locked
cargo test -p minco-plugin-feedback --features sqlite --locked
cargo test -p minco-plugin-feedback --features openai-transcription --locked
cargo test -p minco-plugin-feedback --features command-transcription --locked
cargo test -p minco-plugin-feedback --all-features --locked
cargo test -p cargo-minco --locked
```

Add or repair tests where compiler feedback reveals uncovered assumptions. Do
not merely make the code compile.

## Phase 4: full quality gate

Run:

```bash
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-targets --all-features --locked
cargo doc --workspace --all-features --no-deps --locked
scripts/test/generated_apps.sh
```

Also run the facade matrix:

```bash
cargo check -p minco --no-default-features --locked
cargo check -p minco --locked
cargo check -p minco --features official-plugins --locked
cargo check -p minco --features plugin-feedback --locked
cargo check -p minco --all-features --locked
```

## Phase 5: database and browser verification

Use disposable PostgreSQL and SQLite databases. Verify:

- migrations apply from an empty database;
- client token hashes, revisions, workflow states, message ordering, attachment
  metadata, and optimistic-concurrency conflicts;
- client projections never expose internal notes, object keys, developer-only
  metadata, or raw token hashes;
- failed attachment persistence cleans up already-uploaded objects where the
  adapter can do so;
- notification/audit/event failures do not lose a committed feedback mutation;
- PostgreSQL and SQLite behavior matches the memory reference contract where
  portability is claimed.

Exercise the widget in a real browser and check:

- all four configurable FAB positions;
- keyboard navigation, focus order, labels, contrast, reduced motion, and screen
  reader behavior;
- Shadow DOM style isolation;
- screenshot flow with browser consent and cancellation;
- microphone flow with consent, cancellation, unsupported-browser behavior, and
  bounded recording size;
- session-storage default and explicit local-storage option;
- page-context query redaction;
- attachment count/type/size failures;
- CSRF/CORS/auth behavior in the host application;
- no use of `innerHTML`, `eval`, cookies, or silent capture.

Use Playwright or the repository's preferred browser test harness and commit the
E2E tests if they are missing.

## Phase 6: architecture acceptance

Review `docs/architecture/capability-audit.md` against the actual compiled API.
The conclusion should remain:

- core/plugin architecture covers the reusable capabilities from GarmentIQ and
  CGSP;
- concrete AWS provider adapters still tracked by `M6-T04` are not falsely
  described as implemented;
- product-specific Mapbox, ERP, courier, report, and invitation policy remains in
  application adapters/use cases.

Verify exact operation ownership with:

```bash
cargo minco plugin validate
cargo minco architecture
```

Verify the developer loop manually:

```bash
cargo minco feedback inbox --endpoint <local-api> --token <developer-token>
cargo minco feedback show <feedback-id> --endpoint <local-api> --token <developer-token>
cargo minco feedback reply <feedback-id> --body 'Please clarify the expected result.' \
  --endpoint <local-api> --token <developer-token>
cargo minco feedback status <feedback-id> ready_for_development \
  --endpoint <local-api> --token <developer-token>
cargo minco feedback pull <feedback-id> --output tasks/feedback/<feedback-id>.md \
  --endpoint <local-api> --token <developer-token>
```

Use synthetic feedback only in tests and screenshots.

## Phase 7: package and publication dry run

Do not publish. Run:

```bash
scripts/release/publish.sh
scripts/release/package-list.sh
```

Review every `.crate` include list and compressed package size. Verify the new
plugin crates are in dependency-valid publication order and that the Orders
reference packages remain private.

## Phase 8: update evidence and task state

Update:

```text
VERIFICATION.md
REVIEW_STATUS.md
CODEX_HANDOFF.md
verification/static-validation.json
verification/publish-validation.json
verification/deep-review.json
verification/feedback-contract.json
tasks/M6/M6-T02-essential-plugins.md
tasks/M6/M6-T03-feedback-loop.md
tasks/M8/M8-T02-compiler-package-gates.md
```

Only mark tasks complete when their exact acceptance commands have actually
passed. Record tool versions and command output. Keep `M6-T04` planned unless the
AWS adapters are truly implemented and verified.

## Phase 9: commit, push, and draft PR

Inspect the final change:

```bash
jj status
jj diff --summary
jj diff
```

Resolve all JJ conflicts. Describe and bookmark the reviewed change:

```bash
jj describe -m 'feat: strengthen plugins and add Feedback review loop'
jj bookmark create agent/feedback-plugin-and-core-audit -r @
# or move the existing bookmark to @ if already present
jj git push --bookmark agent/feedback-plugin-and-core-audit
```

Open a **draft** pull request against `main` with a body that covers:

- plugin-kernel improvements;
- capability audit against GarmentIQ and CGSP;
- official plugin additions and explicit provider-adapter gaps;
- Feedback widget, screenshot, voice/transcription, discussion, persistence,
  notifications, audit/events, and AI handoff;
- security/privacy boundaries;
- test and verification evidence;
- remaining non-blocking follow-up tasks.

Suggested PR title:

```text
feat: strengthen plugin architecture and add Feedback review loop
```

Do not claim real AWS, browser, database, Cargo, or publication gates that have
not actually run.

---
