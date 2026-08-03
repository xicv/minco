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
Lambda composition root, `PostgreSQL` or `SQLite` migration and safe demo-seed
paths, quality gates, roadmap/tasks, plugin catalog, and JJ initialization by
default.

The remaining commands operate on a repository containing `minco.toml` and provide
graph-driven local supervision, contract checks and generation, plugin
selection and scaffolding, local quality and test runners, deployment planning,
database cost analysis, release manifests, roadmap/task views, updates, and
JJ-first task workflows.

Inspect or run only the local services and processes declared by the selected
graph:

```bash
cargo minco dev --dry-run --json
cargo minco dev
```

Seeds, non-default workers and frontend commands remain explicit options.
Ctrl-C terminates process groups and stops selected Compose services together;
dry-run resolves no secret values.

Publishing and mutating deployment actions remain explicit; the CLI does not
silently upload crates or change cloud resources.

When the typed `static-site` plugin is selected, `package` binds a deterministic
asset manifest into the release. `deploy static-site plan` is local;
`deploy static-site apply` requires the exact release digest and publishes
through the reviewed private S3/CloudFront stack. `deploy verify --static-site`
then binds current S3, `CloudFront`, certificate, DNS, cache, and pricing evidence
into the ordinary deployment receipt.

Database migration is also explicit and digest-bound:

```bash
cargo minco db plan --set orders-postgres --json
cargo minco db status \
  --set orders-postgres \
  --database-url-env MINCO_MIGRATION_DATABASE_URL \
  --json
cargo minco db migrate \
  --set orders-postgres \
  --database-url-env MINCO_MIGRATION_DATABASE_URL \
  --expected-plan-digest reviewed-plan-digest \
  --receipt target/minco/migration-receipt.json
cargo minco db verify \
  --set orders-postgres \
  --database-url-env MINCO_MIGRATION_DATABASE_URL \
  --json
```

Only the environment-variable name appears in arguments; its database URL
value is not serialized into plans or receipts. See
`docs/deployment/database-lifecycle.md` in the Minco repository for sidecar
metadata, locks, risk gates and receipt semantics.

Database seeding is separately classified and digest-bound:

```bash
cargo minco db seed \
  --profile demo \
  --environment local \
  --set orders-postgres-seeds \
  --dry-run \
  --json
cargo minco db seed --verify --json
```

Applying a seed plan additionally requires a named database URL environment
variable, the exact dry-run digest and a new receipt path. Demo/test seeds fail
closed in production. Bootstrap execution requires an exact environment
acknowledgement. Target verification is database-enforced read-only.

`cargo minco deploy plan --stdout --json` emits canonical Plan IR without
writing a repository artifact. Local topology tooling uses this mode so plugin
selection and Rustack service startup consume the same validated graph as
deployment planning.

Plan IR schema 2 adds explicit worker artifacts, queues, mappings, DLQs and
reviewed schedules while retaining schema 1 API-only input. `cost --json`
exposes runtime wake/request dimensions and `perf --json` reports each
function's artifact digest when built. See the repository's
`docs/deployment/plan-schema-v2-migration.md` before adopting schema 2.

The preview Verified Review Loop is explicit and dry-run first:

```bash
cargo minco --json deploy plan --environment preview --stdout
cargo minco --json deploy review --environment preview --dry-run
cargo minco --json destroy --environment preview --dry-run
```

After an exact deployment and hosted verification, `deploy review` can create
an immutable review manifest using read-only AWS inspection. `destroy` requires
that manifest's exact digest as separate approval, rechecks current account,
role, stack, resources, retention and termination protection, then uses standard
`CloudFormation` deletion and records an absence-verified receipt. No default
scheduler, force deletion, or persistent-target cleanup is available. See
`docs/deployment/preview-review-loop.md` in the Minco repository.

Rollback assessment and optional API canaries reuse the same exact-artifact
boundary:

```bash
cargo minco rollback --dry-run --json
cargo minco promote --canary --dry-run --json
```

`rollback` compares successful current and target promotion receipts and never
contacts AWS, rebuilds, replans, reverses SQL, repairs data, or rewires workers.
An exact data-compatibility decision can be supplied as strict release-bound
JSON. A compatible result is still only qualification: the exact older artifact
must be redeployed as the current candidate without rebuilding or replanning,
hosted verification must be repeated, and ordinary `promote` then owns live
routing. Historical candidate evidence is not reused as current evidence.

`promote --canary` requires an opt-in persistent-target policy, a numeric live
version, exact hosted candidate evidence, reviewed `CloudWatch` metric alarms and the
same live approval as immediate promotion. Live execution uses routing-only
`CloudFormation` change sets, records a canary receipt, waits through alarm
monitoring, verifies the weighted alias, restores the previous unweighted alias,
and only then performs ordinary full promotion. Workers remain unchanged and no
provisioned concurrency is introduced.
