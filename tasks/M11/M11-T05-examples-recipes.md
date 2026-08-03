---
id: M11-T05
title: Build an exercised examples and recipes matrix
milestone: M11
status: complete
priority: high
area: documentation/examples
depends_on: [M10-T05, M11-T01, M11-T04]
operations: []
owned_paths:
  - examples/**
  - docs/tutorials/**
  - docs/how-to/**
  - scripts/test/examples/**
  - scripts/ci/hosted-essential.sh
  - scripts/quality.sh
  - scripts/test/hosted_ci_policy.py
  - tasks/M11/M11-T05-examples-recipes.md
  - verification/adoption-measurements.json
  - verification/deep-review.json
  - verification/source-manifest.json
  - verification/static-validation.json
checks:
  - scripts/test/examples/all.sh
  - scripts/test/generated_apps.sh
  - cargo test --workspace --all-targets --all-features --locked
---

## Goal

Exercise supported application, plugin, database, runtime, worker, local,
deployment, static-site, and adoption recipes without turning every combination
into a default dependency.

## Acceptance

- each recipe states exact features, provider assumptions, cost/wake behavior,
  checks, and unsupported gates;
- code and command snippets compile or execute;
- PostgreSQL-only, SQLite-only, memory, API, and worker dependency graphs stay
  isolated;
- at least one external-style plugin/application recipe uses published APIs;
- examples contain no fake deployment or credentials.

## Non-goals

- exhaustive Cartesian combinations;
- product business modules;
- using examples as a substitute for bounded provider proof.

## Implementation

- `examples/recipes.toml` is the schema-1 authority for ten deliberately small
  application, CRUD, SQLite, PostgreSQL/provider-comparison, SQS worker,
  zero-provisioned-compute AWS, static-site, third-party plugin, generated app,
  and verified-feedback recipes.
- Every entry records exact features, runtime, database, provider assumptions,
  ADR 0025 cost classes, public wake-source vocabulary, bounded check IDs, and
  unsupported gates. Closed vocabularies and strict fields make typos or
  undeclared schema growth fail until the schema version changes.
- `scripts/test/examples/validate.py` resolves those check IDs through a
  reviewed argv-only registry. Matrix content cannot supply shell commands;
  paths remain repository-contained, Markdown Bash fences must parse, and each
  guide names every bound check.
- The runner strips every inherited `AWS_*` value, disables metadata, replaces
  shared configuration/credentials with empty files, and routes any accidental
  AWS endpoint use to a closed loopback port. It also removes configured
  PostgreSQL URLs, so the default recipe proof cannot silently become a live
  provider test.
- Fast matrix validation is part of hosted essential CI. Local quality also
  runs the behavioral, path, vocabulary, documentation, command-registry, and
  provider-environment regressions. The full public runner remains the explicit
  task acceptance command because its generated-consumer proof is intentionally
  broader than bounded hosted essential CI.

## Local evidence

- `scripts/test/examples/all.sh` passed all 19 unique declared checks. This
  covered the Orders contract/explain/application/resource layers, real SQLite,
  PostgreSQL-only compilation, SQLx feature isolation, four structural database
  cost profiles, worker runtime/Plan behavior, zero-idle Plan, static-site
  contracts, a standalone third-party-style plugin, generated PostgreSQL and
  SQLite applications, and the Feedback review loop.
- Generated PostgreSQL and SQLite workspaces compiled and tested through their
  public package boundary; generated TODO specifications failed explicitly as
  designed. The standalone plugin passed against versioned public APIs plus
  repository path overrides.
- `uv run --locked python scripts/test/examples/test_recipes.py`: 11 passed,
  including rejection of arbitrary check IDs, invalid schema vocabulary,
  malformed Bash, undisclosed checks, and inherited AWS credentials/endpoints.
- `uv run --locked python scripts/test/hosted_ci_policy.py`: 3 passed with the
  new bounded hosted validator and authoritative local-quality commands.
- The configured PostgreSQL integration suite remained explicitly ignored
  because no disposable database URL was supplied. No AWS API, deployment,
  migration apply, registry, release, or public-site mutation occurred.
