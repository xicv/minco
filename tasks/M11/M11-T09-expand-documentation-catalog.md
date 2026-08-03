---
id: M11-T09
title: Expand the framework documentation catalog
milestone: M11
status: complete
priority: high
area: documentation/product
depends_on: [M11-T08]
operations: []
owned_paths:
  - docs-site/**
  - scripts/docs/**
  - roadmap/tasks.mmd
  - verification/adoption-measurements.json
  - verification/deep-review.json
  - verification/source-manifest.json
  - verification/static-validation.json
  - tasks/M11/M11-T09-expand-documentation-catalog.md
checks:
  - scripts/docs/build.sh
  - scripts/docs/check-links.sh
  - scripts/docs/check-snippets.sh
  - scripts/docs/test-browser.sh
  - ./scripts/quality.sh
  - uv run --locked python scripts/validate_static.py
---

## Goal

Make the current-development documentation useful as a complete framework
manual: progressively organized, searchable, example-led, and explicit about
Minco's shipped features, built-in plugins, CLI workflows, evidence boundaries,
and low-idle-cost AWS operating model.

## Acceptance

- the `next` sidebar has clear paths for new application developers, experienced
  users, plugin authors, and AWS operators without changing immutable `0.6.0`
  pages;
- current shipped features and official built-in plugins are discoverable from
  authoritative repository metadata and are described without implying runtime
  package discovery, an ORM, or a hosted Minco control plane;
- practical guides and cookbooks show complete, copyable workflows for the
  golden path, configuration/data lifecycle, HTTP resource APIs, background
  work, common plugins, local development, testing, and safe deployment;
- pages distinguish local, compiled, hosted, and provider-backed evidence and
  preserve the precise zero-provisioned-compute boundary;
- search, navigation, links, snippets, accessibility, responsive layout, and
  production rendering are covered by executable checks;
- the live Pages site is published and verified only after exact-head source and
  hosted qualification pass.

## Non-goals

- changing framework runtime behavior, public Rust APIs, serialized schemas, or
  plugin distribution records;
- backporting new content into immutable `0.6.0` documentation;
- creating, changing, promoting, or deleting AWS resources;
- publishing a crate release solely for documentation content;
- presenting planned M12 work or unverified provider behavior as shipped.

## Evidence

Implemented and locally qualified on 2026-08-03 in the isolated
`task-m11-t09` JJ workspace based on exact `main` commit
`a51af9fecc9709943246e275262ed47d62931b74`:

- the TDD content gate first failed with 12 current-development pages and named
  all 16 missing required pages; the completed site now has 28 `next` pages;
- the sidebar now provides Start Here, Essentials, Application Services,
  Plugins and Extensions, Deploy and Operate, Cookbook, and Reference paths;
- shipped behavior is documented from the current Cargo feature table, the
  checked 16-component catalog/distribution records, generated CLI/reference,
  accepted ADRs, and exercised Orders/Feedback sources;
- `scripts/docs/check-snippets.sh` passed 181 fenced blocks,
  `scripts/docs/build.sh` produced the VitePress 1.6.4 production site, and
  `scripts/docs/check-links.sh` passed 121 internal links, 13 external links,
  and 65 canonical pages;
- `scripts/docs/test-browser.sh` passed 15 applicable desktop/mobile Chromium
  journeys with the desktop-only mobile-viewport case intentionally skipped.
  The checks cover the expanded landing paths, full component catalog, local
  search, responsive containment, version warning, semantics, and browser
  errors;
- desktop overview/component-catalog and mobile cookbook production renders
  were visually reviewed after hydration;
- `uv run --locked python scripts/validate_static.py` passed with zero errors or
  warnings, and `./scripts/quality.sh` passed repository truth, generated
  reference, formatting, all-feature Clippy/tests, generated PostgreSQL/SQLite
  applications, Feedback browser tests, Rustdoc, dependency/license/advisory
  policy, secret scanning, and source-manifest verification;
- the review found no actionable correctness, security, accessibility, or
  release blocker in the documentation diff. Existing deep-review warnings in
  untouched Rust/SQL source remain separately visible.

This documentation-only task created or changed no AWS resource and ran no
provider-backed smoke. Configured PostgreSQL and AWS/Rustack tests retained
their explicit ignored/not-run state. Hosted exact-head qualification, merge,
Pages deployment, and live-site checks remain separate delivery evidence.
