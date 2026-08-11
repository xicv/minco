---
id: M14-T18
title: Refresh the language and package ecosystem
milestone: M14
status: complete
priority: high
area: quality/tooling
depends_on: [M14-T17]
operations: []
owned_paths:
  - .github/workflows/docs-pages.yml
  - .github/workflows/minco-manual.yml
  - .github/workflows/publish-crates.yml
  - Cargo.toml
  - Cargo.lock
  - CHANGELOG.md
  - CODEX_HANDOFF.md
  - VERIFICATION.md
  - crates/minco-cli/Cargo.toml
  - crates/minco-cli/src/agent_cmd.rs
  - crates/minco-cli/src/delivery_evidence.rs
  - crates/minco-cli/src/main.rs
  - crates/minco-cli/src/service_runtime.rs
  - crates/minco-cli/tests/agent_cli.rs
  - crates/minco-cli/tests/agent_skills.rs
  - crates/minco-config/Cargo.toml
  - crates/minco-config/src/graph.rs
  - crates/minco-contract/Cargo.toml
  - crates/minco-contract/src/generate.rs
  - crates/minco-contract/src/validate.rs
  - crates/minco-db/Cargo.toml
  - crates/minco-db/src/lib.rs
  - crates/minco-deploy-aws/Cargo.toml
  - crates/minco-deploy-aws/src/lib.rs
  - crates/minco-project-view/Cargo.toml
  - crates/minco-project-view/src/reader.rs
  - crates/minco-project-view/tests/project_view.rs
  - crates/minco-release/Cargo.toml
  - crates/minco-release/src/lib.rs
  - crates/minco-test/Cargo.toml
  - docs/development/quickstart.md
  - docs/reference/generated/diagnostics.md
  - docs/research/language-package-ecosystem-review-2026-08.md
  - docs-site/package.json
  - docs-site/package-lock.json
  - examples/orders/application/Cargo.toml
  - examples/orders/application/src/lib.rs
  - extensions/minco-aws-adapters/Cargo.toml
  - extensions/minco-aws-adapters/src/s3.rs
  - extensions/minco-aws-adapters/src/webhook.rs
  - extensions/minco-aws-adapters/tests/real_aws_s3.rs
  - extensions/minco-aws-adapters/tests/rustack.rs
  - plugins/minco-plugin-feedback/Cargo.toml
  - plugins/minco-plugin-feedback/package.json
  - plugins/minco-plugin-feedback/package-lock.json
  - plugins/minco-plugin-feedback/src/store.rs
  - plugins/minco-plugin-idempotency/Cargo.toml
  - plugins/minco-plugin-idempotency/src/lib.rs
  - plugins/minco-plugin-notifications/Cargo.toml
  - plugins/minco-plugin-object-storage/Cargo.toml
  - plugins/minco-plugin-object-storage/src/base.rs
  - plugins/minco-plugin-object-storage/src/uploads.rs
  - plugins/minco-plugin-payments-waffo/Cargo.toml
  - plugins/minco-plugin-payments-waffo/src/client.rs
  - plugins/minco-plugin-sessions/src/lib.rs
  - plugins/minco-plugin-static-site/Cargo.toml
  - plugins/minco-plugin-static-site/src/lib.rs
  - proofs/realtime-appsync/aws-handler/Cargo.toml
  - proofs/realtime-appsync/aws-handler/Cargo.lock
  - proofs/realtime-appsync/browser/package.json
  - proofs/realtime-appsync/browser/package-lock.json
  - proofs/realtime-pusher/aws-handler/Cargo.toml
  - proofs/realtime-pusher/aws-handler/Cargo.lock
  - proofs/realtime-pusher/browser/package.json
  - proofs/realtime-pusher/browser/package-lock.json
  - proofs/realtime-pusher/rust/Cargo.toml
  - proofs/realtime-pusher/rust/Cargo.lock
  - proofs/realtime-pusher/rust/src/main.rs
  - pyproject.toml
  - scripts/test/hosted_ci_policy.py
  - scripts/test/rust_dependency_hygiene.py
  - tasks/M14/M14-T18-language-ecosystem-refresh.md
  - verification/1.3-performance-baseline.json
  - verification/deep-review.json
  - verification/operational-evidence-validation.json
  - verification/publish-validation.json
  - verification/provider-evidence.toml
  - verification/source-manifest.json
  - verification/static-validation.json
checks:
  - uv lock --check
  - cargo check --workspace --all-targets --all-features --locked
  - cargo test --workspace --all-targets --all-features --locked
  - cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
  - uv run --locked python scripts/test/hosted_ci_policy.py
  - npm audit --prefix docs-site --audit-level=high
  - npm audit --prefix plugins/minco-plugin-feedback --audit-level=high
  - npm audit --prefix proofs/realtime-appsync/browser --audit-level=high
  - npm audit --prefix proofs/realtime-pusher/browser --audit-level=high
  - scripts/ci/local-release.sh
---

# M14-T18 - Refresh the language and package ecosystem

## Goal

Perform a dated, reproducible refresh of Minco's Rust, Python, Node,
documentation, browser-test and GitHub Action dependency surfaces. Adopt the
latest stable versions available on 2026-08-11 where Minco owns the dependency,
including intentional major-version migrations, while preserving the published
1.3.0 public API and runtime architecture.

## Acceptance

- the repository-pinned Rust release is verified against the current stable
  toolchain and every direct Cargo requirement is reviewed against the registry;
- the root workspace and every independent proof/example lockfile resolve
  deterministically on Rust 1.97.1;
- `base64` 0.23, `hmac` 0.13 and `sha2` 0.11 compile and preserve Minco's exact
  encoding, signing and digest behavior;
- uv, Node LTS, Playwright, VitePress, Vite, Nano ID and action commit pins are
  current, synchronized and covered by fail-closed policy tests;
- all four npm trees retain zero high or critical audit findings;
- generated evidence and the source manifest bind the final tree; and
- the complete provider-free local release qualification passes.

## Non-goals

- changing Minco's published 1.3.0 API, Plan IR or static plugin boundary;
- adding an AWS service, workflow, provider abstraction, poller or control plane;
- creating a tag, GitHub release, crates.io publication or deployment; or
- treating dependency freshness or local tests as live-provider evidence.

## Evidence

Completed on 2026-08-11 from isolated JJ workspace
`/Users/xicao/Projects/minco-task-m14-t18`, change ID `tylmxtnmzlyo`.

- `uv lock --check`, every reviewed `npm outdated`, and all four `npm audit`
  commands passed; the npm audits reported zero vulnerabilities.
- Root workspace check/tests/Clippy and the three independent Rust proof
  workspaces passed on Rust `1.97.1`; modified Rust files pass isolated
  `rustfmt --check`.
- `uv run --locked python scripts/test/hosted_ci_policy.py` passed 12 tests,
  including fail-closed synchronization for the toolchain and package pins.
- Feedback browser tests passed 40 of 40. Documentation browser tests passed
  38 with 2 project-specific desktop skips; 344 snippets, 1,137 internal links,
  14 external links and 291 canonical pages passed.
- `./scripts/quality.sh` passed, including the full workspace, generated
  Postgres/SQLite applications, publication policy, RustSec, dependency policy
  and secret scanning.
- The first `scripts/ci/local-release.sh` attempt was intentionally not counted:
  it reached the publication dry-run and stopped because the source change was
  the active JJ working copy. After creating an empty child, the exact parent
  source passed `scripts/ci/local-release.sh` with exit status zero, including
  candidate recovery/load, all 34 package dry-runs and archive consumers,
  AppSync proof, SAM/native Lambda builds, local runtime, Rustack and E2E.
- The source manifest and operational receipt are regenerated after this final
  evidence update. Performance remains `NOT RUN`; current live-provider
  evidence remains absent. No tag, release, publication, provider contact or
  deployment occurred.
