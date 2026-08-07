# M14-T03 — Waffo Pancake payments plugin

Status: in_progress
Milestone: M14
Owner: framework
Provider contract reviewed: `df098331cf5ea7d43ad79ab223d9eda6d4ac8e5f`

## Goal

Add an opt-in, statically composed Waffo Pancake payment plugin that preserves
Minco's zero-idle-cost, AWS-native, contract-first and AI-automatable design,
while making the common checkout and webhook workflow concise.

## Implemented

- [x] Signed Waffo actions, hosted checkout, read-only GraphQL, and raw-body webhook verification.
- [x] Typed configuration with unresolved `env:` / `ssm:` secret references.
- [x] Explicit provider and Minco idempotency claims for mutating actions.
- [x] Bounded bodies, environment validation in the CLI, a production-write guard, and no hidden Waffo retries.
- [x] Dedicated stable-JSON CLI for config checks, doctor checks, actions, checkout, GraphQL, webhook registration, and webhook verification.
- [x] Cashier-inspired fluent guest-checkout value object and direct `minco-waffo checkout` command.
- [x] Exact Waffo product short-ID validation and typed common checkout fields.
- [x] Plugin-local agent guidance for checkout, webhook projection, and offline testing.

## Required before ready for review

- [ ] Reconcile the branch with the immutable published Minco `1.1.0` baseline and open the correct later candidate version/package boundary.
- [ ] Apply the exact short-ID contract to merchant and store configuration as well as checkout products.
- [ ] Bind every application client and verifier construction path to Minco `EnvironmentClass` before secret resolution.
- [ ] Bind webhook verification and deduplication scopes to the configured store and mode.
- [ ] Preserve complete ordered provider warnings and errors, including untrusted AI hints and GraphQL locations/path.
- [ ] Scope local idempotency by provider environment and canonical API origin.
- [ ] Replace the handwritten GraphQL scanner with a maintained parser.
- [ ] Canonicalize/restrict generic production actions and return JSON for normal CLI parse failures.
- [ ] Add an injectable no-network transport fake with endpoint and redacted-request assertions.
- [ ] Add authenticated checkout through the reviewed Waffo customer-session-token endpoint.
- [ ] Pass exact-head focused and authoritative repository qualification.

## Evidence

Historical run `31069913728` passed the earlier implementation with 16 tests.
It is not exact-head evidence for the current Cashier-inspired update. The
permanent focused workflow is `.github/workflows/waffo-payments.yml`.

Only modified or newly created Waffo Rust files are formatted directly. Linting
is restricted to the new plugin targets. No provider credentials, live payment
mutations, AWS changes, deployment, tag, release, or registry publication are
authorised by this task.
