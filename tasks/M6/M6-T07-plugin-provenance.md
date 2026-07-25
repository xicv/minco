---
id: M6-T07
title: Add bounded provenance to typed plugin registrations
milestone: M6
status: planned
priority: high
area: core/plugins
depends_on: [M6-T06]
operations: []
owned_paths:
  - crates/minco-core/**
  - crates/minco-cli/**
  - docs/architecture/plugin-authoring.md
  - docs/adrs/**
  - tasks/M6/M6-T07-plugin-provenance.md
checks:
  - cargo test -p minco-core -p cargo-minco --all-features --locked
  - cargo clippy -p minco-core -p cargo-minco --all-targets --all-features --locked -- -D warnings
  - cargo minco inspect --json
---

## Goal

Make singleton-service and ordered-contribution ownership inspectable without
changing Minco's typed registries into string-key lookup or a global service
locator.

## Design boundary

- preserve `TypeId`-based retrieval and static plugin composition;
- add plugin-context registration helpers that attach the current `PluginId`;
- distinguish application-provided registrations from plugin-provided ones;
- include both first and attempted owners in duplicate-service diagnostics;
- preserve deterministic contribution order and expose bounded type/owner/index
  summaries through inspection;
- do not expose `Any` values, service instances, configuration values, secrets,
  or provider diagnostics;
- keep existing low-level registry APIs only where an explicit
  application-owned provenance default is unambiguous.

## Acceptance

- two plugins attempting the same singleton produce a deterministic diagnostic
  naming both owners and the Rust type;
- contribution summaries retain plugin owner and installation index;
- application seed services/contributions are distinguishable from plugins;
- composition, graph planning, no-feature facade, official plugins and
  third-party plugin examples remain source-compatible where practical;
- an ADR records any unavoidable pre-1.0 public API break;
- tests prove ownership cannot be spoofed through `PluginContext`.

## Reason for separate task

M6-T06 found the current failure mode is safe—duplicates fail—but not
diagnostic enough. Retrofitting provenance touches every plugin registration
site and inspection schema. Keeping it out of the HTTP/worker P0 slice avoids
destabilizing those release gates while making the migration prerequisite and
acceptance boundary explicit.
