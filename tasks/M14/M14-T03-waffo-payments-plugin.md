# M14-T03 — Waffo Pancake payments plugin

Status: in_progress
Milestone: M14
Owner: framework

## Goal

Add an opt-in, statically composed Waffo Pancake payment plugin that preserves
Minco's zero-idle-cost, AWS-native, contract-first and AI-automatable design.

## Scope

- `plugins/minco-plugin-payments-waffo/**`
- root workspace membership, dependencies and release order
- `minco` facade feature, re-export and static registration
- official plugin catalog entry
- 1.1 package-family publication boundary

## Acceptance

- [x] Research current Waffo authentication, idempotency, checkout, GraphQL and webhook contracts before implementation.
- [x] Keep secret values outside configuration, graphs, diagnostics and command-line arguments.
- [x] Require explicit Waffo and Minco idempotency claims for every mutating action.
- [x] Fail closed on environment mismatch and require a persisted production-write guard.
- [x] Verify webhook signatures against untouched bounded request bytes before deserialization.
- [x] Provide a dedicated JSON CLI with config-check, doctor, action, checkout, query, webhook registration and webhook verification commands.
- [ ] Produce a reviewed `Cargo.lock` and pass targeted Rust formatting, tests, Clippy and facade composition checks on the pinned toolchain.
- [ ] Record final hosted qualification evidence in the pull request.

## Verification

Only modified or newly created Rust files are formatted or linted. No provider
credentials or live payment mutations are used by automated qualification.
