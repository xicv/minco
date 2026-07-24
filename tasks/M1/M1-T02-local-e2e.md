---
id: M1-T02
title: Verify the local SQLite HTTP journey
milestone: M1
status: ready
priority: high
area: testing
depends_on: [M1-T01, M3-T02]
operations: [getLive, getReady, placeOrder, getOrder]
owned_paths:
  - scripts/test/e2e.sh
checks:
  - scripts/test/e2e.sh
---

## Goal

Start the real local binary against a temporary SQLite database, place and replay an order, retrieve it, and terminate cleanly.
