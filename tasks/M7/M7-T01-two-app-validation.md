---
id: M7-T01
title: Validate Minco with GarmentIQ and CGSP slices
milestone: M7
status: planned
priority: critical
area: stabilization
depends_on: [M6-T02]
operations: []
owned_paths:
  - docs/adoption/**
checks:
  - cargo test --workspace --all-features
---

## Goal

Port one real vertical slice from each existing application before stabilizing public framework APIs.
