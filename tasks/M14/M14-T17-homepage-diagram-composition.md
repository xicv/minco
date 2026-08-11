---
id: M14-T17
title: Rebalance the homepage architecture diagram
milestone: M14
status: complete
priority: high
area: docs/presentation
depends_on: [M14-T16]
operations: []
owned_paths:
  - CODEX_HANDOFF.md
  - VERIFICATION.md
  - docs-site/public/minco-system.svg
  - docs-site/tests/docs.spec.mts
  - tasks/M14/M14-T17-homepage-diagram-composition.md
  - verification/1.3-performance-baseline.json
  - verification/operational-evidence-validation.json
  - verification/provider-evidence.toml
  - verification/source-manifest.json
checks:
  - scripts/docs/test-browser.sh
  - bash scripts/docs/build.sh
  - scripts/docs/check-links.sh
  - uv run --locked python scripts/validate_operational_evidence.py --check-output verification/operational-evidence-validation.json
  - uv run --locked python scripts/source_manifest.py --check
---

# M14-T17 - Rebalance the homepage architecture diagram

## Goal

Correct the public homepage's contract-to-cloud illustration at the reported
804 by 615 viewport so its application label, connector geometry and runtime
cards have deliberate spacing and the lower evidence row no longer overwhelms
the primary flow.

## Acceptance

- the application label and runtime connector have a visible gutter;
- every runtime label remains inside its card;
- the diagram uses a wider, balanced composition without clipping at the
  reported viewport, normal desktop and mobile widths;
- browser regression coverage measures the SVG geometry rather than relying on
  a source-string assertion;
- the docs build, link check and browser suite pass; and
- Pages deploys and the live root serves the corrected exact SVG bytes.

## Non-goals

- changing the Minco product, public Rust API, Plan IR or runtime behavior;
- changing the published `1.3.0` crate family or immutable tag;
- publishing crates, creating a GitHub release or contacting a provider; or
- changing the surrounding homepage information architecture.

## Evidence

The geometry regression first failed against the published SVG because the
application card had no stable node selector and the image exposed no intrinsic
size. After the correction, the focused 804 by 615 test passed and the complete
browser suite passed on desktop Chromium and Pixel 7 emulation: 38 passed and 2
desktop-only/mobile-only cases skipped as designed. `bash scripts/docs/build.sh`
and `scripts/docs/check-links.sh` passed; the link checker covered 1,137
internal links, 14 external links and 291 canonical pages.

PR [#148](https://github.com/xicv/minco/pull/148) retained exact head
`982ae8705cf47a88a21606c7777f705ccf8eb722`. Clean-Linux run
[`31460666529`](https://github.com/xicv/minco/actions/runs/31460666529)
passed that SHA before merge. The PR merged as
`21b70f1157f792ca20d70c724bf61974fa736695` with reviewed tree
`a54b9b6cd6e0efdca9c3ffc7c47d152d198d6c86`, and merged-main Pages run
[`31460937727`](https://github.com/xicv/minco/actions/runs/31460937727)
built and deployed successfully.

The live root and SVG return HTTP 200. The live SVG SHA-256 is
`e78294ce451525a5af8a966fb41021ca5b5eecb2756a23220df5b0f3ad9e1a8b`,
byte-identical to the reviewed source, and the final 804 by 615 render preserves
the measured spacing and containment. M14-T17 is complete. No crate, tag,
release or provider action was part of this docs-only correction.
