# cargo-minco

Cargo subcommand for the Minco framework.

Install it separately from the `minco` library crate:

```bash
cargo install cargo-minco --locked
cargo minco new example-api --database postgres
cd example-api
cargo minco doctor
```

`cargo minco new` creates a layered, contract-first workspace with a local and
Lambda composition root, `PostgreSQL` or `SQLite` migration path, quality gates,
roadmap/tasks, plugin catalog, and JJ initialization by default.

The remaining commands operate on a repository containing `minco.toml` and provide
contract checks and generation, plugin selection and scaffolding, local quality
and test runners, deployment planning, database cost analysis, release
manifests, roadmap/task views, updates, and JJ-first task workflows.

Publishing and mutating deployment actions remain explicit; the CLI does not
silently upload crates or change cloud resources.

`cargo minco deploy plan --stdout --json` emits canonical Plan IR without
writing a repository artifact. Local topology tooling uses this mode so plugin
selection and Rustack service startup consume the same validated graph as
deployment planning.

Plan IR schema 2 adds explicit worker artifacts, queues, mappings, DLQs and
reviewed schedules while retaining schema 1 API-only input. `cost --json`
exposes runtime wake/request dimensions and `perf --json` reports each
function's artifact digest when built. See the repository's
`docs/deployment/plan-schema-v2-migration.md` before adopting the likely 0.4
serialized boundary.
