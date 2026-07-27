---
id: M12-T06
title: Prepare the Minco 1.0 release candidate
milestone: M12
status: planned
priority: critical
area: release/1.0
depends_on: [M8-T03, M12-T02, M12-T05]
operations: []
owned_paths:
  - Cargo.toml
  - Cargo.lock
  - CHANGELOG.md
  - VERIFICATION.md
  - CODEX_HANDOFF.md
  - docs/**
  - tasks/M12/M12-T06-minco-1-0-rc.md
  - verification/**
checks:
  - ./scripts/quality.sh
  - scripts/release/package-list.sh
  - scripts/release/publish.sh --skip-quality
  - cargo install cargo-minco --path crates/minco-cli --locked
  - uv run --locked python scripts/source_manifest.py --check
---

## Goal

Prepare an exact, reviewable 1.0 release candidate after all completion,
adoption, compatibility, ownership, and qualification gates pass.

## Acceptance

- workspace version, lock-step internal dependencies, changelog, migration
  guides, docs, source manifest, package inventory, and candidate evidence
  agree;
- a fresh external generated application and facade consumer compile and test;
- the candidate source, package archives, docs, and artifact digests are exact;
- hosted exact-head qualification is ready for a separately authorised tag and
  publication task;
- no release claim is made before registry and tag actions actually occur.

## Non-goals

- uploading crates or creating the final tag without explicit authority;
- bypassing a blocked ownership, security, provider, or documentation gate;
- calling an RC a production release.
