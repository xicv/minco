---
id: M14-T51
title: Make store selection explicit and bootstrap capability claims truthful
milestone: M14
status: active
priority: high
area: plugins/ticketing/truthfulness
depends_on: [M14-T50]
operations: []
owned_paths:
  - docs/DECISIONS.md
  - docs/adrs/0053-truthful-bootstrap-and-explicit-store-selection.md
  - plugins/minco-plugin-ticketing/**
  - tasks/M14/M14-T51-truthful-bootstrap-and-store-selection.md
  - verification/1.9-performance-baseline.json
  - verification/source-manifest.json
  - verification/operational-evidence-validation.json
  - verification/static-validation.json
  - verification/publish-validation.json
  - verification/deep-review.json
  - docs/reference/generated/plugins.md
checks:
  - cargo test -p minco-interaction -p minco-plugin-ticketing --all-features --locked
  - cargo clippy -p minco-interaction -p minco-plugin-ticketing --all-targets --all-features --locked -- -D warnings
  - cargo minco plugin validate
  - cargo package -p minco-plugin-ticketing --locked
  - cargo minco check --with-cargo
---

# M14-T51 - Make store selection explicit and bootstrap capability claims truthful

Closes the two remaining Stage-B-line correctness gaps before the Jobs
bridge: accidental memory selection and hard-coded capability claims.

## Goal

- Remove `Default for TicketingPlugin` so no code path can silently select
  the memory store; constructors stay explicit (`memory()`, `sqlite(pool)`,
  `new(store)`), and the facade's dev registration remains an explicit
  `memory()` call.
- The support bootstrap stops claiming unimplemented capabilities:
  `screenshot_enabled`, `voice_enabled` and `file_enabled` report `false`
  until real upload operations exist, and a new additive `capabilities`
  object reports per-feature truth — `portal_sessions` true only when the
  sessions and CSRF services are actually registered, `history` true,
  files/screenshots/voice/knowledge/email/automation false.
- The capability object is a ticketing-local type serialized alongside the
  interaction bootstrap shape (serde flatten); extending the published
  `minco-interaction` type would fail package verification against the
  registry, so the extension stays inside this plugin.

## Acceptance

- No `TicketingPlugin: Default` exists; compile of all dependents passes.
- Bootstrap tests prove: portal sessions capability flips only with the
  services registered; unimplemented capture toggles are false; the
  response contains no secrets.
- OpenAPI schema/example updated truthfully; parity holds (30 operations,
  unchanged).

## Non-goals

- Implementing attachment/screenshot/voice operations (later stages).
- Facade-level storage configuration (application-owned composition).
- Any Jobs dependency.

## Evidence

Run 2026-08-24 in the `minco-task-m14-t51` workspace:

- `cargo test -p minco-plugin-ticketing --all-features --locked` — ok,
  **53 passed** (real temporary SQLite included; +1 bootstrap truthfulness
  proof).
- Feature matrix — `--no-default-features` (20), sqlite-only (30),
  `--features full` (53) — all ok.
- `cargo clippy -p minco-plugin-ticketing --all-targets --all-features
  --locked -- -D warnings` — clean; `rustfmt --check` over changed files —
  clean.
- `cargo minco plugin validate` — `[]`; `cargo package
  -p minco-plugin-ticketing --locked` — verified (this caught the
  unpublished-interaction-dependency problem: the capability type is
  ticketing-local, serialized beside the interaction bootstrap shape via
  serde flatten); `contract sync --check` passes; docs reference current.
- `cargo minco check --with-cargo` — result recorded at finish.

Behavioral proofs: `TicketingPlugin` has no `Default`; the bootstrap
reports `screenshot_enabled/voice_enabled/file_enabled` false and a
`capabilities` object whose `portal_sessions` flips to true only when the
sessions and CSRF services are registered, `history` true, all
not-yet-implemented features false; the response contains no secrets.
