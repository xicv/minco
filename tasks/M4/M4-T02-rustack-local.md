---
id: M4-T02
title: Exercise declared AWS seams through Rustack
milestone: M4
status: complete
priority: medium
area: local-infrastructure
depends_on: [M4-T01]
operations: []
owned_paths:
  - infra/local/**
  - scripts/dev/**
  - extensions/minco-aws-lambda/Cargo.toml
  - extensions/minco-aws-lambda/tests/**
  - .github/workflows/minco-manual.yml
  - docs/development/quickstart.md
  - docs/development/testing.md
  - docs/deployment/local-parity.md
  - README.md
  - CODEX_HANDOFF.md
  - VERIFICATION.md
checks:
  - docker compose -f infra/local/compose.yaml config
  - scripts/dev/test-rustack.sh
---

## Goal

Start only the local PostgreSQL and AWS-compatible services required by the selected application graph.

## Evidence

- `docker compose -f infra/local/compose.yaml config`
- `./scripts/dev/up.sh` with the default graph: no local services selected
- `./scripts/dev/test-rustack.sh`: only Rustack/SSM started; SecureString
  put/load/delete passed through `minco-aws-lambda` and the standard AWS SDK
