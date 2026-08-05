---
id: M14-T01
title: Prepare and publish the Minco 1.1.0 release candidate
milestone: M14
status: complete
priority: critical
area: release/1.1
depends_on: [M13-T06]
operations: []
owned_paths:
  - Cargo.toml
  - Cargo.lock
  - README.md
  - CHANGELOG.md
  - CODEX_HANDOFF.md
  - PUBLISHING.md
  - VERIFICATION.md
  - crates/minco-cli/assets/agent/**
  - crates/minco-cli/src/agent_cmd.rs
  - crates/minco-cli/tests/agent_cli.rs
  - crates/minco-cli/tests/agent_skills.rs
  - crates/minco-cli/tests/plugin_cli.rs
  - plugins/*/minco-plugin.json
  - extensions/*/minco-plugin.json
  - docs/**
  - docs-site/**
  - examples/orders/api/src/generated.rs
  - examples/plugins/third-party-minimal/**
  - proofs/realtime-appsync/aws-handler/Cargo.lock
  - roadmap/**
  - tasks/M14/**
  - verification/**
checks:
  - cargo test -p cargo-minco --test agent_cli --test agent_skills --test plugin_cli --locked
  - uv run --locked python scripts/test/agent_workflows.py
  - uv run --locked python scripts/validate_static.py
  - uv run --locked python scripts/validate_publish.py --check-registry --require-registry
  - npm --prefix docs-site run build
  - npm --prefix docs-site run test:browser
  - scripts/release/publish.sh --skip-quality
---

## Goal

Release the additive agent-native development layer and all previously
published Minco functionality as one compatible `1.1.0` lock-step family,
with a complete versioned manual and exact evidence boundaries.

## Acceptance

- all 33 publishable packages and internal registry requirements resolve to
  `1.1.0`, while independently versioned capability contracts remain stable;
- agent assets and documentation identifiers derive from the exact CLI package
  version instead of retaining a `1.0.0` implementation constant;
- README, changelog, adoption guide, current manual, frozen `1.1.0` manual and
  CLI reference document Codex/Claude setup, bounded context and evaluation;
- full local package and documentation gates plus exact-head and exact-main
  hosted release qualification pass before tagging;
- immutable tag, GitHub release, trusted publication and complete registry
  verification are recorded as distinct actions; and
- no live AWS application deployment, promotion, database mutation or feature
  enablement is inferred from package publication.

## Non-goals

- changing the read-only MCP tool catalog or client-owned configuration;
- adding a hosted agent runtime, dynamic plugin loading or implicit authority;
- claiming docs.rs completion before each exact build succeeds; or
- deploying any live Minco application as part of the crate release.

## Evidence

Started on 2026-08-06 in the isolated `minco-task-m14-t01` JJ workspace from
exact merged main `9ef9c469532ec2fa3e7b0675baafa83aa3febafe`. The six M13
pull requests were merged in dependency order, and the final main tree exactly
matched the previously qualified cumulative M13 head. Release, registry and
documentation-publication evidence will be appended only after each action is
independently verified.

The first exact-main hosted release run, GitHub Actions run `31054629951`,
passed repository/static tests and then stopped at ten Clippy diagnostics in
the newly merged agent command. The release candidate fixes those diagnostics
without broad formatting and advances all archive-visible official plugin core
compatibility ranges to `^1.1.0`, matching their linked descriptors.

Local candidate evidence on 2026-08-06:

- Rustfmt 1.9.0 checked only the five modified Rust files with edition 2024;
- targeted Clippy for the modified `cargo-minco` binary and three integration
  tests passed with warnings denied;
- agent CLI, skill and plugin CLI suites passed `42/42` tests;
- deterministic agent qualification passed all 16 scenario contracts for
  Codex and Claude, with zero model invocations and zero network requests;
- static validation and deep review completed with zero errors, retaining only
  pre-existing review warnings outside this task's changed Rust paths;
- VitePress built successfully and Playwright passed 19 rendered desktop/mobile
  cases with one intentionally skipped desktop instance of a mobile-only case;
  and
- registry validation reached all 33 crate records and proved every exact
  `1.1.0` version absent before publication.
- the clean-tree coordinated publish dry run packaged and compiled all 33
  archives, passed selected packaged tests and external consumer checks for
  no-default, default, all-feature and newly added packages, then installed the
  archived CLI and reported `minco 1.1.0`; no archive was uploaded.
