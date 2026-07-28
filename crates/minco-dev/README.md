# minco-dev

`minco-dev` owns Minco's deterministic local service/process plan and
coordinated child-process supervision. Plans are graph-derived, serializable,
and safe to inspect without starting infrastructure or application processes.

The crate exposes pure `DevPlan` derivation plus a supervisor with:

- explicit service start/stop and migration/seed lifecycle commands;
- API, worker and optional frontend processes;
- loopback-only HTTP readiness and process readiness events;
- labelled stdout/stderr events with sensitive environment-value redaction;
- Unix process-group termination and reverse-order service cleanup.

Applications normally use this through `cargo minco dev`.
