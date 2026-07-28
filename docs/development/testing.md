# Testing and Quality Gates

Minco tests itself at several boundaries. The local runner is authoritative;
the optional GitHub workflow merely invokes the same commands.

Repository Python dependencies are declared in `pyproject.toml` and pinned in
`uv.lock`. Run `uv sync --locked --only-dev` after checkout; quality commands
use `uv run --locked` and fail rather than rewriting the lock.

## Test tiers

| Tier | Command | Purpose |
|---|---|---|
| Unit | `./scripts/test/unit.sh` | Domain invariants, graph validation, parsing, cost formulas, plugin ordering. |
| Feature | `./scripts/test/feature.sh` | Application use cases, HTTP `oneshot`, adapter behavior, generated plan/contract checks. |
| Local topology | `python3 scripts/dev/test_topology.py` | `cargo minco dev` default/SQLite plans, graph-derived Compose selection, standard endpoint wiring, and override behavior. |
| Rustack conformance | `./scripts/dev/rustack-smoke.sh` | Isolated real S3, SQS, SSM SecureString, STS, and Minco SSM SDK-adapter operations against Rustack. |
| E2E | `./scripts/test/e2e.sh` | Local service over TCP with contract requests; optional PostgreSQL/Rustack dependencies. |
| All | `./scripts/test/all.sh` | Runs the three tiers and deep review. |
| Quality | `./scripts/quality.sh` | Static checks, format, Clippy, all workspace targets, generation freshness, review. |

## Stability requirements

The framework's own tests cover:

- identifier and graph invariants;
- duplicate/missing plugin capabilities and dependency cycles;
- typed service registration and injection;
- OpenAPI profile validation and deterministic generation;
- route/operation bijection;
- cost and performance policy diagnostics;
- SAM rendering and public/authenticated route behavior;
- release digest verification;
- middleware, request correlation, error media types, and authentication;
- idempotency replay/conflict behavior;
- domain and application fail-before-persistence behavior;
- memory, PostgreSQL, and SQLite adapter semantics;
- CLI manifest/task/plugin/update helpers.
- deterministic generator plans, fail-closed paths, app-owned stubs, and
  compiler-visible PostgreSQL/SQLite generated-application journeys;
- graph-derived local services and standard AWS endpoint configuration;
- isolated Rustack S3, SQS, SSM and STS compatibility, including the Minco SSM
  SDK adapter.

A release additionally requires database-backed conformance, native Lambda build,
SAM validation, and a bounded real-AWS smoke deployment. Static or unit checks do
not substitute for those gates.

## Optional GitHub Actions

`.github/workflows/minco-manual.yml` is deliberately `workflow_dispatch` only.
It does not run until a maintainer explicitly invokes it. The workflow installs
the exact uv release and locked Python dependency group used locally. This avoids
making hosted CI a prerequisite while retaining a reproducible runner config.

## Evidence

Commands write evidence under `target/minco/`. Completed tasks should reference
the exact command, result, tool versions, and artifact digest. Do not replace a
failed/missing tool with a claimed pass.
