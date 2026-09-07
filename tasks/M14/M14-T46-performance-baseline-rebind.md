---
id: M14-T46
title: Re-bind the performance baseline to the ticketing Stage A tree
milestone: M14
status: active
priority: high
area: verification/evidence
depends_on: [M14-T45]
operations: []
owned_paths:
  - tasks/M14/M14-T46-performance-baseline-rebind.md
  - verification/1.9-performance-baseline.json
  - verification/source-manifest.json
  - verification/operational-evidence-validation.json
  - verification/static-validation.json
  - verification/publish-validation.json
checks:
  - uv run --locked python scripts/validate_operational_evidence.py --check-output verification/operational-evidence-validation.json
  - cargo minco check --with-cargo
---

# M14-T46 - Re-bind the performance baseline to the ticketing Stage A tree

## Goal

Unblock `cargo minco check --with-cargo` (the `task-finish` gate) for the
ticketing branch after Stage A: `verification/1.9-performance-baseline.json`
bound its source digest to the published main tree, so every source change
failed `PERF-BASELINE-003`. Follow the exact precedent of commit `8b02db9`
(the M14-T44 task): keep `status: "NOT RUN"`, `production_slo: false`,
`provider_contact: false`, state the truthful reason, and re-bind
`source_tree_sha256` to the current deterministic source-manifest digest.
No measurements are claimed or fabricated.

## Evidence

- `uv run --locked python scripts/source_manifest.py` — wrote digest
  `6b19da944f9a269c346ed930709ed21e093c787faef7e9fc20d3cbf0105f46de`
  (1841 files); re-run is byte-stable, and the perf baseline itself is
  excluded from the digest, so binding is a fixed point.
- `uv run --locked python scripts/validate_operational_evidence.py --output
  verification/operational-evidence-validation.json` — `status: PASS`,
  `errors: 0, warnings: 2` (the two warnings are the truthful
  no-current-provider-evidence and hosted-performance NOT RUN statements).
- `cargo minco check --with-cargo` — every static and Python gate passes
  after this task's re-bind; the gate's `cargo test --workspace
  --all-targets --all-features --locked` step fails on exactly one
  environment-caused test, identically on pristine published main:

  ```
  crates/minco-dev supervisor: http_process_is_reported_ready_only_after_its_local_probe_succeeds
  panicked: supervision should stop cleanly after readiness: ReadinessTimeout { id: "api" }
  ```

  Root cause (proven by process sampling): the test starts
  `python3 -m http.server`, whose `HTTPServer.server_bind` calls
  `socket.getfqdn("127.0.0.1")`; on this development Mac the loopback
  reverse-DNS path hangs inside `mdns_hostbyaddr` (`mDNSResponder` is
  wedged), so the child never reaches `LISTEN` and the readiness probe times
  out. `nc -l` on the same port listens and accepts fine, confirming the
  repository code and test are correct and the failure is host-specific.
  Fix requires an operator restart of mDNSResponder (sudo), after which the
  full gate is expected to pass unchanged. Recorded here rather than
  converted to a pass or worked around in code.

## Non-goals

- Running hosted Linux performance qualification (provider contact,
  release-task scope).
- Any measurement claim: the baseline remains explicitly NOT RUN.
