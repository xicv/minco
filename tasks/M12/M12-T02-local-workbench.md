---
id: M12-T02
title: Build the optional local developer workbench and project views
milestone: M12
status: complete
priority: medium
area: ai/workbench
depends_on: [M12-T01]
operations: []
owned_paths:
  - Cargo.lock
  - Cargo.toml
  - crates/minco-workbench/**
  - crates/minco-cli/**
  - docs/how-to/**
  - docs/reference/**
  - roadmap/tasks.mmd
  - scripts/test/workbench_browser.sh
  - tasks/M12/M12-T02-local-workbench.md
  - verification/adoption-measurements.json
  - verification/deep-review.json
  - verification/publish-validation.json
  - verification/repository-truth.toml
  - verification/rust-dependency-hygiene.json
  - verification/source-manifest.json
  - verification/static-validation.json
checks:
  - cargo test -p minco-workbench -p cargo-minco --all-features --locked
  - cargo clippy -p minco-workbench -p cargo-minco --all-targets --all-features --locked -- -D warnings
  - cargo minco workbench --check --json
  - scripts/test/workbench_browser.sh
---

## Goal

Build an optional local dashboard and deterministic export surface from the
M12-T01 `ProjectView` for OpenAPI exploration, application/resource graphs,
feature and task progress, local process status, migrations/seeds, request
traces, cost/deployment previews, evidence lanes, accessible narration and
Feedback.

## Acceptance

- the workbench is local-only and opt-in;
- it reuses stable read models rather than creating a second application graph;
- `--check`, `export --format json|mermaid|static`, and loopback-only `serve`
  preserve the ADR-0030 read/write and evidence boundaries;
- export accepts only a new project-relative destination whose existing
  components are non-symlink directories beneath the canonical project root
  and whose parent is outside canonical inputs;
- export retains the verified parent identity, exclusively creates a private
  staging directory through that handle, never adopts a pre-existing staging
  entry, writes through the retained staging handle, installs with an atomic
  no-clobber primitive and fails closed if any component, staging entry, parent
  or destination changes before publication;
- `serve` binds directly to loopback, rejects non-loopback `Host` values and
  cross-origin browser access, enables no permissive CORS, serves only local
  assets under a restrictive Content Security Policy and marks project-view
  responses `Cache-Control: no-store`;
- visual progress retains raw status, explains derived totals and keeps source,
  local, hosted, deployment, runtime and review/UAT evidence separate;
- diagrams, keyboard navigation, screen-reader structure, accessible text and
  explicit client-side read-aloud controls cover desktop and small screens;
- the Minco repository and separately authorized first-party application
  evidence consume the same schema before an adapter boundary is frozen;
- secret/redaction and response bounds match the MCP/CLI contracts;
- static assets add no default facade dependency;
- accessibility and small-screen behavior are tested;
- export tests cover symlinked ancestors, parent-identity swaps, pre-existing
  staging entries, staging-name collisions, canonical input overlap, a
  concurrently created destination and unsupported safe installation
  primitives.

## Non-goals

- a production admin UI;
- hosted telemetry collection;
- a text-to-speech provider, stored voice data or generated audio;
- performing deployment or database writes by default.

## Evidence

Implementation and local qualification were completed on 2026-08-05 in the
isolated `minco-task-m12-t02` JJ workspace:

- `minco-workbench` consumes the schema-1 `ProjectView` directly and adds no
  second repository graph or default `minco` facade dependency;
- `cargo minco workbench --check --json` reported `read_only: true`, zero
  listeners, zero writes, the source digest, 126 nodes, 314 edges, all raw task
  statuses, six independent evidence-lane counts, configured bounds and actual
  input usage;
- JSON, Mermaid and static export use a new normalized project-relative
  destination outside the complete CLI-declared input set. Descriptor-relative
  tests prove no-follow ancestor traversal, canonical-input rejection, private
  staging collision handling, parent and staging identity changes, concurrent
  destination creation, unavailable no-clobber support, post-install failure
  cleanup and preservation of unrelated replacement entries;
- one cleanup regression was caught red before the fix: replacing the staging
  name could make error cleanup remove an unrelated entry. Cleanup now removes
  a staging or installed directory name only while its device/inode still
  matches the retained owned directory. A second injected post-install red
  (`assertion failed: !canonical_root.join(destination).exists()`) proved that
  parent-sync failure left an empty destination; the installed-state cleanup
  regression now passes;
- serve binds `127.0.0.1` directly, requires the exact bound Host, rejects
  different and non-UTF-8 Origin values, exposes no permissive CORS, serves only
  bundled assets and bounded snapshots, and returns CSP, no-store, no-sniff,
  deny-frame and no-referrer headers. The invalid-Origin regression first
  returned 200 instead of 403 and now passes;
- the accessible workbench preserves raw statuses, derived-total explanation
  and all six evidence lanes; uses landmarks, textual diagrams, focus styles,
  primary-view arrow navigation, roving keyboard tabs, explicit browser speech
  and JSON export; and switches Graph, Tasks and Evidence into exclusive modes
  below 720 px;
- visual QA compared generated desktop and mobile concepts with real Chromium
  renders at 1536x1024 and 390x844. The fidelity ledger covered the five-value
  summary, graph/task separation, six-lane evidence matrix, cobalt/mint/amber
  semantics, compact square geometry, responsive actions and small-screen mode
  exclusivity. The initial mobile journey failed with `mobile Evidence view
  left Graph visible`; the corrected journey has no horizontal overflow or
  browser-console errors;
- the preferred in-app browser transport was attempted twice and failed with
  `tool call failed for node_repl/js` caused by `Transport closed`. The approved
  Playwright fallback then passed desktop rendering, keyboard navigation,
  read-aloud start/stop, JSON download, small-screen mode/keyboard switching,
  six evidence lanes and screenshot creation. Screenshots were kept outside the
  repository for inspection only;
- targeted locked tests and Clippy passed for `minco-workbench` and
  `cargo-minco`: six descriptor-race unit tests, five export integration tests,
  one loopback security test and four end-to-end CLI workbench tests are part of
  the passing suite; package validation reports 32 publishable packages with
  zero errors and warnings, and generated CLI/package references are current;
- the workbench introduces no public application adapter boundary. ADR-0030's
  separately authorized first-party schema-consumption proof and any adapter
  freeze remain explicit M12-T03/M12-T04 gates rather than being inferred from
  this Minco-only implementation; and
- no AWS, database, hosted workflow, deployment, release, registry,
  documentation-site, push or merge mutation was performed by this task.
