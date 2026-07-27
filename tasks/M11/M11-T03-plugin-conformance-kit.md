---
id: M11-T03
title: Publish one plugin conformance kit
milestone: M11
status: planned
priority: critical
area: plugins/testing
depends_on: [M11-T02]
operations: []
owned_paths:
  - crates/minco-test/**
  - crates/minco-core/**
  - plugins/**
  - extensions/**
  - examples/plugins/**
  - docs/how-to/**
  - docs/reference/**
  - tasks/M11/M11-T03-plugin-conformance-kit.md
checks:
  - cargo test -p minco-test -p minco-core --all-features --locked
  - cargo minco plugin test --all
  - cargo minco plugin validate
---

## Goal

Make official and third-party-style plugins use the same public tests for
descriptor validity, config defaults/unknown fields, graph/provenance, HTTP
ownership, migrations/seeds, health, resources/IAM/wake/cost, package contents,
docs examples, and provider leakage.

## Acceptance

- the kit is usable from outside the workspace against a published version;
- at least one intentionally minimal third-party-style fixture passes;
- negative fixtures emit stable diagnostics;
- provider/live integration requirements remain separately labelled;
- plugin success does not imply application or provider production readiness.

## Non-goals

- forcing identical backend semantics;
- executing remote calls during composition;
- certifying a plugin's business or privacy policy automatically.
