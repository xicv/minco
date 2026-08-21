# Framework task gates

Required before mutation:

- repository identity is the Minco framework;
- exact base and workspace state are known;
- the task is ready/active and dependencies are complete;
- owned paths include source, tests, docs and coupled evidence; and
- current ADR and compatibility boundaries are understood.

Required before handoff:

- focused task checks pass;
- no conflict touches the task change;
- evidence names the exact revision and lane;
- generated references, Signal documentation and release skill freshness are
  current when the task changes a shipped feature or release;
- unavailable or broader user-disallowed gates are recorded literally; and
- the pushed bookmark contains no unrelated workspace changes.

A task implementation, push, PR, local-first release boundary, hosted check,
merge, release, publication, deployment, and runtime proof are separate states.

At a maintenance release boundary, re-check version-matched documentation,
exact package/tool pins, public-contract compatibility and lane-specific evidence.

At the 1.5 assurance release boundary, keep typed fakes, cost regression and
measured local assurance inside their owning task and leave model/human outcome
evidence `NOT RUN` until actually exercised.

At the 1.6 durable audit ledger boundary, keep application semantics, SQL
journal/ledger adapters, DynamoDB transaction shape, documentation, cost and
retention evidence in explicit owned paths and tasks.

At the 1.7 Apple Container default boundary, own runtime-selection code,
receipts, exact-resource tests, migration guidance and fallback documentation
together. Preserve Docker support and keep automatic data migration or resource
deletion outside the task unless separately authorised.

At the 1.11 contract-enforced request boundary, own OpenAPI authority,
request-reachable analysis, deterministic generation, typed extraction,
authorization policy, middleware provenance, in-process contract tests,
versioned documentation and compatibility evidence together. Keep application
business policy and provider topology outside that delivery-boundary change.
