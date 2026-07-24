---
id: M0-T03
title: Lock Python tooling for local and hosted quality gates
milestone: M0
status: complete
priority: high
area: developer-experience
depends_on: [M0-T02]
operations: []
owned_paths:
  - pyproject.toml
  - uv.lock
  - .gitignore
  - quality.toml
  - scripts/**
  - crates/minco-cli/src/main.rs
  - crates/minco-cli/src/update.rs
  - .github/workflows/**
  - docs/development/testing.md
  - docs/development/quickstart.md
  - docs/development/publishing.md
  - docs/development/update.md
  - README.md
  - PUBLISHING.md
  - tasks/M0/M0-T03-python-tooling.md
checks:
  - uv lock --check
  - uv sync --locked --only-dev
  - uv run --locked python scripts/validate_static.py
  - uv run --locked python scripts/test/feedback_contract.py
  - cargo test -p cargo-minco --locked
  - shellcheck scripts/quality.sh scripts/bootstrap.sh scripts/aws/validate.sh scripts/package.sh scripts/release/publish.sh
---

## Goal

Make every repository quality gate that imports third-party Python packages
reproducible on a clean Mac and in optional hosted workflows without modifying
the system Python environment.

## Acceptance

- Python development dependencies use a committed cross-platform `uv.lock`.
- Local validation fails if dependency metadata would change the lockfile.
- `cargo minco doctor` treats `uv` as a required local quality tool.
- Manual GitHub workflows install the pinned uv release and sync the locked
  development group before validation.
- Third-party GitHub actions in touched workflows are immutable commit pins.
- Hosted CI remains manual; local and Rustack validation remain authoritative.

## Current evidence

Official uv documentation and the 2026-07-23 release were reviewed before
implementation. The local uv installation was upgraded from 0.8.17 to 0.11.32.
`pyproject.toml` declares the PEP 735 development group and exact supported uv
version; the canonical cross-platform `uv.lock` resolves only the virtual Minco
development project and PyYAML 6.0.3.

`uv lock --check`, `uv sync --locked --only-dev`, and both online and cached
offline `uv run --locked` validation pass. A fresh temporary environment proved
that static validation and all 13 Feedback contract operations pass without any
global Python package. The lock SHA-256 remained
`f17e423cc1d6b378958c3a33fee6c072d86a4651a662d38b1472fb69ccd6bd41`
before and after locked execution.

Static, publish, deep-review, SQLite-schema, scaffold-template, source-manifest,
and Feedback validators pass. `.venv` is excluded from static/deep-review/source
discovery and source archives, with a regression test. A complete package-script
smoke included `pyproject.toml` and `uv.lock`, excluded `.venv`, passed archive
integrity, and matched its SHA-256 sidecar. The package script now validates
whitespace from JJ workspaces as well as colocated Git repositories.

The two manual GitHub workflows parse under Actionlint. Every action is pinned
to an immutable commit, including checkout 7.0.1, setup-uv 9.0.0, rust-cache
2.9.1, and crates.io auth 1.0.5. Hosted uv caching is disabled and neither
workflow was dispatched.

`cargo-minco` passes all 12 tests and strict scoped Clippy. Doctor reports uv
and Clippy as explicitly required; `minco update check` successfully runs both
`uv lock --check` and the non-mutating `uv lock --upgrade --dry-run`. All touched
shell scripts pass ShellCheck and all touched Python files compile.

Cloud-touch record: no authenticated AWS API or AWS resource operation occurred.
One local `sam validate --lint` ran before telemetry was explicitly disabled;
the CLI may have emitted anonymous usage telemetry, but it did not use the AWS
account or create/read resources. The final script sets `SAM_CLI_TELEMETRY=0`,
and its local SAM lint passes.
