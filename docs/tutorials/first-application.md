# Walk the first Minco application

The Orders reference application demonstrates Minco's narrow contract-to-cloud
path without hiding application structure behind a runtime container. Start at
`examples/orders/openapi/openapi.yaml`; generated HTTP types, domain rules,
use-case-shaped ports, adapters, composition, plans, and evidence remain
separately inspectable.

## Features

Enable `contract`, `http`, `test`, and the `default-plugins` bundle. The domain
and application crates remain independent of Axum, SQLx, Lambda, and AWS SDKs.

## Provider assumptions

Contract inspection, operation explanation, domain tests, and application tests
are local. They require no database, network listener, AWS account, or provider
credential.

## Cost and wake behavior

Pure compilation and tests have `zero_compute` idle cost and no wake source.
Selecting a runtime or persistence profile later introduces its own visible
cost and wake assumptions.

## Follow the graph

Check the canonical contract and inspect the complete graph:

```bash
cargo minco contract check --json
cargo minco inspect --json
cargo minco explain placeOrder --json
```

`explain` binds `placeOrder` to its OpenAPI operation, generated source,
application use case, memory/PostgreSQL/SQLite adapters, HTTP handler, tests,
deployment function, and current configuration. It does not execute the use
case or contact a provider.

Exercise the pure layers directly:

```bash
cargo test --locked -p orders-domain -p orders-application
```

When adding an operation, change OpenAPI first, add one failing application test,
implement the invariant and use case, then add adapter and Axum contract proof.

## Verification

`scripts/test/examples/all.sh` executes `orders-contract`, `orders-explain`, and
`orders-application` as the checked-in proof for this tutorial.

## Unsupported gates

This tutorial does not qualify a real database, build a Lambda artifact, create
AWS resources, run migrations, deploy, promote, or prove production behavior.
