---
id: M1-T02
title: Verify the local SQLite HTTP journey
milestone: M1
status: complete
priority: high
area: testing
depends_on: [M1-T01, M3-T02]
operations: [getLive, getReady, placeOrder, getOrder]
owned_paths:
  - scripts/test/e2e.sh
checks:
  - bash -n scripts/test/e2e.sh
  - scripts/test/e2e.sh
---

## Goal

Start the real local binary against a temporary SQLite database, place and replay an order, retrieve it, and terminate cleanly.

## Evidence

On 2026-07-24, the script built and started the real local binary on a
dynamically allocated loopback port with a temporary file-backed SQLite
database. It verified liveness, dependency readiness, authenticated order
creation and retrieval, exact original-result replay, idempotency conflict
handling with RFC 9457 media type, and fail-closed unauthenticated access. The
trap terminated the directly launched binary and removed its isolated
temporary database.
