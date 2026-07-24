# Prompt for continuing Minco in Codex desktop

Copy the prompt below into Codex after extracting this archive.

---

You are continuing the Minco Rust web-framework repository. Work from the repository root and treat the existing source as an unverified implementation that must earn compiler/runtime evidence. Do not claim a check passed unless you executed it and captured the result.

## Project intent

Minco is contract-first, AI-native, AWS-native, performance-aware, deployment-oriented and JJ-first. It uses a small provider-neutral core, statically linked plugins, explicit typed services and ordinary Rust application code. It must preserve SOLID boundaries, separation of concerns, no ORM requirement, no global service locator, no dynamic plugin ABI and no hidden scheduled infrastructure.

This change reviews the core/plugin system against GarmentIQ and CGSP and adds the official `minco-plugin-feedback` vertical slice.

## Read first

Read these files in order:

```text
AGENTS.md
FEEDBACK_REVIEW_STATUS.md
VERIFICATION.md
docs/architecture/capability-audit.md
docs/architecture/extensions.md
docs/architecture/plugin-authoring.md
docs/architecture/feedback-loop.md
docs/adrs/0014-plugin-lifecycle-and-feedback.md
plugins/minco-plugin-feedback/README.md
plugins/minco-plugin-feedback/openapi/feedback.openapi.yaml
plugins/minco-plugin-feedback/src/plugin.rs
plugins/minco-plugin-feedback/src/service.rs
plugins/minco-plugin-feedback/src/http.rs
plugins/minco-plugin-feedback/assets/widget.js
```

## Preserve these settled decisions

1. OpenAPI remains the external HTTP source of truth.
2. Domain/application code must not depend on Axum, SQLx or AWS SDKs.
3. Plugins are statically linked Rust crates installed explicitly.
4. Plugin dependencies and capabilities are semver-checked before construction.
5. `ServiceCollection` is typed; do not add string-based service lookup.
6. Multi-provider contributions are ordered and finalized deterministically.
7. Plugin install/finalize must not perform remote I/O, migrate databases or start background workers.
8. Feedback persistence is authoritative; notifications/events/audit are secondary outcomes with surfaced warnings.
9. Client feedback does not become a development specification until explicitly transitioned to `ready_for_development`.
10. AI export is deterministic context, not an autonomous code-change trigger.
11. No hidden polling schedule is allowed in the default feedback/outbox design.
12. Do not weaken limits, authorization, privacy or architecture checks merely to make compilation pass.

## Recommended overlay into `xicv/minco`

The GitHub repository currently contains only its initial license commit. The safest
path is to clone it with JJ, then overlay this archive while preserving VCS metadata:

```bash
jj git clone https://github.com/xicv/minco.git minco
cd minco
jj new main -m 'feat: strengthen plugin kernel and add feedback workflow'
cd ..
mkdir -p /tmp/minco-feedback-source
unzip /path/to/minco-feedback-core-review-0.1.0.zip -d /tmp/minco-feedback-source
rsync -a --delete \
  --exclude .git \
  --exclude .jj \
  --exclude target \
  /tmp/minco-feedback-source/ minco/
cd minco
jj status
```

If the extracted archive itself will become the repository, use the initialization
flow below instead. Do not copy `.git` or `.jj` metadata from another workspace.

## Initialize JJ correctly

This archive intentionally excludes `.git` and `.jj` metadata. Initialize a colocated JJ/Git repository:

```bash
./scripts/jj/init.sh
jj status
jj describe -m 'feat: strengthen plugin kernel and add feedback plugin'
```

Use a dedicated workspace for verification if preferred:

```bash
jj workspace add ../minco-feedback-verify -r @ -m 'verify: feedback plugin and core audit'
cd ../minco-feedback-verify
```

Use JJ for mutations and conflict resolution. Git should primarily remain the GitHub transport.

## Install the pinned Rust toolchain

```bash
rustup toolchain install 1.97.1 \
  --profile minimal \
  --component rustfmt \
  --component clippy

cargo generate-lockfile
```

Review `Cargo.lock` before continuing. Do not fabricate or hand-edit a lockfile to bypass dependency resolution.

## First validation wave

Run and retain exact output:

```bash
python3 scripts/validate_static.py
python3 scripts/validate_publish.py
python3 scripts/deep_review.py
python3 scripts/test/feedback_contract.py
node --check plugins/minco-plugin-feedback/assets/widget.js
git diff --check

cargo fmt --all -- --check
cargo check -p minco --no-default-features --locked
cargo check -p minco --locked
cargo check -p minco --all-features --locked
cargo check -p cargo-minco --locked
```

Fix compiler errors and formatting issues. Prefer narrow type/API corrections. Do not suppress meaningful lints globally.

## Full Rust quality gate

```bash
cargo test -p minco --no-default-features --locked
cargo test -p minco --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-targets --all-features --locked
cargo doc --workspace --all-features --no-deps --locked
scripts/test/generated_apps.sh
```

Pay special attention to:

- object safety and `Send + Sync` boundaries in plugin services;
- Axum 0.8 extractor/router state types;
- SQLx 0.9 transaction executor usage;
- feature-gated OpenAI transcription code;
- ordered contribution downcasting;
- plugin finalize ordering and duplicate registrations;
- error conversions that could leak secrets or internal details;
- public API documentation and crates.io package contents.

## Feedback database verification

Run SQLite tests first, then PostgreSQL.

```bash
python3 scripts/test/sqlite_schema.py
```

Create executable Rust tests for both `MemoryFeedbackStore`, `SqliteFeedbackStore` and `PostgresFeedbackStore` if coverage is missing. Verify:

- create/read/list;
- client-token authorization;
- client replies;
- developer replies and internal-note privacy;
- state transitions;
- optimistic revision conflicts;
- attachment metadata;
- deterministic AI context;
- transaction rollback on failure;
- PostgreSQL/SQLite migration parity.

Use an isolated PostgreSQL container for integration tests. Do not require a developer's existing database.

## Browser/widget verification

Add or run browser tests against a real local API. Cover:

1. FAB renders in each configured position.
2. Keyboard accessibility, focus handling and reduced-motion behavior.
3. Widget config and token storage modes (`session` default, optional `local`).
4. Text-only feedback submission.
5. Screenshot capture with browser permission mocked or controlled.
6. Microphone recording and upload.
7. Optional transcription success, disabled state and provider failure.
8. Client token survives page refresh according to configured storage.
9. Client can continue a clarification thread.
10. Internal developer notes never appear in client responses.
11. Attachment download authorization and security headers.
12. Oversized body, invalid type and attachment-count rejection.

Do not add DOM/session replay. Preserve native browser consent.

## Core/plugin-system tests

Add or complete tests proving:

- explicit plugin disable beats default enable;
- dependency auto-enable works;
- explicitly disabling a required dependency fails;
- missing/duplicate capabilities fail before install;
- incompatible core/plugin versions fail;
- dependency cycles fail deterministically;
- duplicate typed singleton services fail;
- ordered multi-contributions preserve order;
- finalize sees all installed contributions;
- duplicate HTTP routes or operation IDs fail;
- OpenAPI and plugin HTTP inventories are bijective;
- an application can replace an official provider through typed injection without modifying core.

## Provider adapter review

The architecture audit identifies these remaining official gaps:

```text
S3 object store and signer
SQS event publisher + PostgreSQL outbox
SES and signed webhook notifications
persistent session/idempotency/audit adapters
Cognito administrative invitation/user adapter
S3/CloudFront static-site renderer
IAM derivation and real-AWS conformance
```

Do not pretend these are implemented. Create or refine roadmap tasks and only implement them in this change if they are needed to compile or to make Feedback's declared default profile usable. Keep provider dependencies outside `minco-core`.

## Security review for Feedback

Review these threats explicitly:

- bearer client-token theft and storage policy;
- developer/operator-token timing attacks;
- cross-project access;
- attachment content-type spoofing;
- malicious filenames;
- oversized multipart bodies;
- orphaned objects after database failure;
- transcription-provider data exposure;
- prompt injection in feedback text or transcript;
- internal-note disclosure;
- object URL lifetime and cache controls;
- screenshot/audio retention and deletion;
- notification retries and duplicate delivery.

Add tests and documentation for material findings. Treat feedback text and transcript as untrusted data when passed to an LLM. AI context must clearly delimit user-controlled content and instruct consumers not to execute embedded instructions.

## Packaging/publication gate

After the workspace is green:

```bash
scripts/release/publish.sh
scripts/release/package-list.sh
```

These should remain dry-run actions. Inspect package contents and size for all 22 public packages. Do not publish crates or create a GitHub release as part of this task unless the repository owner explicitly requests the irreversible action.

## Commit and open a draft pull request

When checks are green:

```bash
jj status
jj diff --stat
jj describe -m 'feat: strengthen plugin kernel and add feedback workflow'
```

Create a bookmark suitable for GitHub, export/push through the colocated Git repository, and open a **draft** PR targeting `main`. Suggested title:

```text
feat: strengthen plugin kernel and add official feedback workflow
```

The PR body must contain:

- core/plugin-system changes;
- capability coverage against GarmentIQ and CGSP;
- Feedback UX and API behavior;
- security/privacy decisions;
- provider gaps that remain;
- exact test commands and results;
- any compiler/API changes made during verification;
- screenshots of the FAB and feedback dialog if browser tests are available.

Do not mark the PR ready for review until Cargo, Clippy, tests, docs, generated apps, SQLite/PostgreSQL tests and browser tests are green, or until remaining limitations are explicitly accepted by the owner.

## Expected final response

Report:

1. Files changed and why.
2. Compiler/test failures found and how they were fixed.
3. Exact checks executed with pass/fail status.
4. Remaining provider or runtime gaps.
5. JJ change ID and bookmark.
6. Draft PR URL, or the precise blocker if a PR could not be opened.

---
