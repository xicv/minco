---
id: M4-T04
title: Drive Plan IR and local topology from one application graph
milestone: M4
status: complete
priority: critical
area: deployment-planning
depends_on: [M2-T01, M4-T01, M4-T02]
operations: [getLive, getReady, placeOrder, getOrder]
owned_paths:
  - Cargo.toml
  - Cargo.lock
  - crates/minco-core/**
  - crates/minco-plan/**
  - crates/minco-cli/**
  - crates/minco/**
  - scripts/dev/**
  - docs/deployment/local-parity.md
  - infra/aws/generated/**
  - tasks/M4/M4-T04-graph-derived-plan.md
checks:
  - cargo test -p minco-core -p minco-plan -p cargo-minco --locked
  - cargo clippy -p minco-core -p minco-plan -p cargo-minco --all-targets --locked -- -D warnings
  - python3 scripts/dev/test_topology.py
  - cargo minco deploy plan --stdout --json
---

## Goal

Serialize the configured static application graph into Plan IR and make local
dependency selection consume that exact plan, so plugin selection, deployment
planning and Rustack services cannot drift across parallel configuration
parsers.

## Evidence

On 2026-07-24, `cargo minco deploy plan --stdout --json` serialized the
manifest-selected, statically linked plugin graph and derived the deterministic
local AWS service set consumed by `scripts/dev/topology.py`. Planning was proved
read-only: it validates configuration, dependencies and resources without
installing services or invoking lifecycle hooks. Checked-in plan output matched
fresh canonical output byte-for-byte, contained descriptor metadata but no
secret defaults, and `sam validate --lint` accepted the unchanged generated
template.

The scoped Rust tests, Clippy with warnings denied, docs, Python topology tests,
Python compilation, Ruff when available, Bash syntax, ShellCheck, static
validation and publish validation all passed. A local Rustack smoke proved S3,
SQS, SSM, STS and the Minco SSM SDK adapter, then removed its Compose container
and network. No real AWS API was called for this task.

A focused post-task review found and fixed one offline-plan boundary: topology
now rejects unsupported Rustack service names before configuring local
dependencies. The final focused review found no remaining correctness,
architecture, security or cleanup finding within M4-T04 scope.
