---
id: M4-T02
title: Exercise declared AWS seams through Rustack
milestone: M4
status: ready
priority: medium
area: local-infrastructure
depends_on: [M4-T01]
operations: []
owned_paths:
  - infra/local/**
  - scripts/dev/**
checks:
  - docker compose -f infra/local/compose.yaml config
  - scripts/dev/up.sh
---

## Goal

Start only the local PostgreSQL and AWS-compatible services required by the selected application graph.
