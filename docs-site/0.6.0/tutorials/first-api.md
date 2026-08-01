---
title: Build your first API
description: Generate, inspect, test, and run a Minco 0.6.0 application.
minco_version: 0.6.0
rust_version: 1.97.1
---

# Build your first API

In this tutorial you will generate a layered SQLite application, inspect its
contract-to-cloud graph, run its tests, and start the local HTTP service.

## Before you begin

Install the exact stable CLI:

```bash
cargo +1.97.1 install cargo-minco --version 0.6.0 --locked
```

## 1. Generate the application

```bash
cargo minco new hello-minco --database sqlite
cd hello-minco
cp .env.example .env
```

The generator creates separate domain, application, adapter, API, and
composition crates. It also creates OpenAPI, migrations, tests, deployment
configuration, tasks, and local quality commands. JJ with colocated Git is the
default version-control profile.

## 2. Check the contract and graph

```bash
cargo minco contract check
cargo minco contract sync --check
cargo minco inspect --json
cargo minco explain healthLive --json
```

OpenAPI is the external API source of truth. Generated bindings are checked in
and deterministic; do not edit files marked `@generated`.

## 3. Run the tests

```bash
cargo minco test unit
cargo minco test feature
cargo minco check --with-cargo
```

The generated application starts with real liveness and platform examples.
New operation generators deliberately create failing specifications rather than
inventing business behavior.

## 4. Start the service

```bash
cargo run -p hello-minco-service --bin hello-minco-local
```

In another terminal:

```bash
curl --fail --show-error http://127.0.0.1:3000/health/live
```

You now have a locally running Minco API. No AWS resource was created.

## What to do next

- [Build a standardized resource API](../how-to/resource-api).
- [Configure environments](../how-to/configure-environments).
- [Plan the AWS deployment](../how-to/plan-deployment).
