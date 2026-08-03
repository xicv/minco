# Generate a standalone Minco application

The scaffold proof creates PostgreSQL and SQLite applications outside the Minco
workspace, patches only their dependency resolution back to the current source
tree, and compiles/tests their public package boundary.

## Features

The generated profiles exercise `contract`, `http`, database-specific
`sqlx-postgres` or `sqlx-sqlite`, and the optional `aws-lambda` composition.
Generators for modules, migrations, seeders, workers, adapters, operations, and
plugins are exercised after the base application compiles.

## Provider assumptions

Temporary workspaces and local Cargo builds are used. No PostgreSQL server,
AWS account, registry publication, or deployed endpoint is required.

## Cost and wake behavior

Generation and compilation have `zero_compute` idle cost and no wake source.
The generated application's selected runtime/database profile determines later
cost and wake behavior.

Run the complete disposable-workspace proof:

```bash
scripts/test/generated_apps.sh
```

The generated operation deliberately leaves application and HTTP business TODO
tests failing after the scaffold compiles. This prevents generated shape from
being mistaken for implemented business behavior.

## Verification

The matrix check is `generated-applications`; the task also runs
`scripts/test/generated_apps.sh` as an independent acceptance command.

## Unsupported gates

The scaffold is not a finished product. It proves public APIs and generated
structure, not requirements, authorization, persistence durability, provider
integration, deployment, or production readiness.
