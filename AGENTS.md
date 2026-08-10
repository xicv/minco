# Minco Agent Operating Contract

This file is authoritative for AI-assisted work in this repository.

## Read first

Before changing code, read:

1. `docs/DECISIONS.md` and the relevant ADR;
2. `roadmap/roadmap.yaml`;
3. the owning task under `tasks/`;
4. the relevant contract, plugin and deployment documentation.

Use `cargo minco inspect --json`, `cargo minco explain <operationId> --json`,
`cargo minco task show <id> --json` and `cargo minco deploy plan` rather than
inferring hidden structure.

## Non-negotiable architecture

1. `examples/orders/openapi/openapi.yaml` is the reference external API source of truth.
2. Never edit `// @generated` files manually; sync and commit deterministic output.
3. Domain crates do not depend on Axum, SQLx, Lambda, AWS SDKs or Minco HTTP/plan crates.
4. Application crates do not depend on Axum, SQLx, Lambda or AWS SDKs.
5. HTTP handlers extract/map, call one use case, and map a response; they contain no SQL.
6. Application ports are use-case-shaped, not generic CRUD repositories.
7. Adapters implement ports owned by the application layer.
8. The composition root is the only place selecting concrete adapters/runtime plugins.
9. Plugins are statically linked and explicitly selected. Do not add runtime scanning,
   a global service locator, facades or dynamic-library loading.
10. The minimal AWS profile adds no NAT Gateway, fixed compute, schedule or provisioned concurrency.
11. Production migrations are explicit release operations; Lambda startup does not migrate.
12. Promotion uses the exact release artifact and manifest; it never rebuilds source.
13. Database profiles must expose correctness, wake-source, connection and cost assumptions.
14. DynamoDB requires access-pattern-specific ports/adapters; do not emulate relational SQL semantics.

## JJ-first task workflow

Use JJ for mutations and a colocated Git repository for GitHub transport.

```bash
cargo minco task ready --json
./scripts/jj/task-start.sh <TASK-ID>
cd ../minco-task-<task-id>
jj status
```

One workspace owns one task. Respect `owned_paths`, dependencies, goals and non-goals.
Before finishing:

```bash
./scripts/quality.sh
jj log -r 'conflicts()'
./scripts/jj/task-finish.sh <TASK-ID> 'type(scope): description' --push
```

Resolve or explicitly split conflicts before release. Use `jj op log`, `jj undo` and
`jj workspace update-stale` for recovery. Never report a task complete merely because
source was written.

## GitHub Actions boundary

The workflow allowlist is exactly `docs-pages.yml`, `minco-manual.yml` and
`publish-crates.yml`. Never create temporary, task-specific or branch-only GitHub
workflows. Run full quality, release, local runtime, Rustack and E2E qualification
locally with `scripts/ci/local-release.sh`; GitHub provides only Pages deployment,
crates.io OIDC publication and the short manual clean-Linux compatibility check.

## Required workflow for a new operation

1. Change OpenAPI, including examples, security, success and Problem responses.
2. Run contract check/sync.
3. Add a failing application test with fake ports.
4. Implement domain invariants and application use case.
5. Add adapters/migrations only when persistence is required.
6. Add in-process Axum contract tests.
7. Add Plan IR/cost/IAM implications.
8. Confirm `minco explain` traces the slice.
9. Run `./scripts/quality.sh` and relevant database/e2e checks.
10. Record exact evidence in the task.

## Plugin workflow

A plugin must supply a real descriptor, typed services, explicit configuration,
capabilities/dependencies, health/resource/cost behavior and tests. Use:

```bash
cargo minco plugin new <id>
cargo minco plugin validate
```

Core changes made solely for one plugin are usually a design smell. Prove any new extension
point with at least two implementations and an ADR.

## Testing expectations

- Domain: pure unit tests for invariants/transitions.
- Application: fake-port tests proving authorization, validation and fail-before-persistence.
- Adapter: behavioral/transaction tests against the real engine.
- HTTP: Axum `oneshot` tests for status, media type, headers, IDs and bodies.
- Plugin/core: graph, dependency, injection, selection and deterministic-order tests.
- Deployment: Plan/SAM snapshots, structural cost/performance checks and bounded AWS smoke.
- Release: digest and exact-artifact verification.

Static validation is not compiler verification. If a required tool is unavailable, record the
exact command and error in `VERIFICATION.md`; do not convert it into a pass.

## Security and data

Never commit credentials, password-bearing database URLs, tokens or customer data. Keep
CORS exact, redact sensitive headers, preserve request IDs, return stable problem codes and
keep business authorization in application use cases. Generated plans/manifests may contain
secret **names**, never secret values.
