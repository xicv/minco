---
id: M11-T05
title: Build an exercised examples and recipes matrix
milestone: M11
status: planned
priority: high
area: documentation/examples
depends_on: [M10-T05, M11-T01, M11-T04]
operations: []
owned_paths:
  - examples/**
  - docs/tutorials/**
  - docs/how-to/**
  - scripts/test/examples/**
  - tasks/M11/M11-T05-examples-recipes.md
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
