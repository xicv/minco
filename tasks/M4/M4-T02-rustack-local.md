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
  - docs/deployment/local-parity.md
  - docs/development/testing.md
  - CHANGELOG.md
  - VERIFICATION.md
checks:
  - python3 scripts/dev/test_topology.py
  - docker compose -f infra/local/compose.yaml config
  - scripts/dev/up.sh --dry-run
  - scripts/dev/rustack-smoke.sh
---

## Goal

Start only the local PostgreSQL and AWS-compatible services required by the selected application graph.

## Evidence

- `scripts/dev/topology.py` derives PostgreSQL plus Rustack `ssm,sts` from the
  selected deployment graph and exposes only standard database/AWS environment
  variables to the local application.
- Provider-neutral `events` and `object-storage` plugins do not silently select
  SQS or S3. Their application adapters remain owned by `M6-T04`.
- `scripts/dev/rustack-smoke.sh` passes isolated real S3, SQS, SSM SecureString
  and STS operations against the pinned Rustack 0.9.1 image, including a
  SecureString load through the real `minco-aws-lambda` SDK adapter.
- The manual hosted workflow runs the same Rustack/adapter boundary by default
  and exposes an explicit `run_rustack` override.
- The normal `up.sh`, explicit migration and `run.sh` path passes liveness,
  readiness, order creation, idempotent replay and order retrieval. Port 4567
  and an isolated database were used because unrelated local work owned port
  4566 and the existing development database had historical migration drift;
  neither resource was removed or rewritten.
