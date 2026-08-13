# Operation layer ownership

Release skill freshness is checked against the current Minco changelog and
versioned documentation before the bundle ships.

- OpenAPI owns the external HTTP contract.
- Domain owns pure invariants and transitions.
- Application owns authorization, validation, orchestration, and use-case ports.
- Adapters implement application-owned ports.
- HTTP handlers extract/map, call one use case, and map one response.
- The composition root selects concrete adapters and runtime plugins.
- Browser/native contract metadata remains frontend-neutral and shares one
  authoritative business API.
- Verified direct upload completion validates provider metadata before durable
  state; rich mail separates provider acceptance from mailbox delivery.

Prefer tests that observe public behavior:

- domain unit tests for invariants;
- application tests with fake boundary ports;
- adapter behavior tests against the real engine;
- Axum `oneshot` tests for the external contract; and
- plan/configuration tests for declared infrastructure consequences.

For side effects, prefer the official port-specific test doubles:
`FakeMessageHandler`, `FakeEventPublisher`, `FakeObjectStore`,
`FakeFeedbackStore`, and `FakeMailTransport`. Script only the failure needed by
the scenario, assert the public use-case outcome, and keep provider or mailbox
evidence in its separate lane. Do not introduce a generic mocking facade or put
customer data, credentials, tokens, object bytes, feedback text, or mail content
in committed fixtures.

Record the exact failed test before implementation and the focused passing
command afterward. Do not turn an unrun provider or hosted check into a pass.

At a maintenance release boundary, re-check version-matched documentation,
exact package/tool pins, public-contract compatibility and lane-specific evidence.

At the 1.5 assurance release boundary, prefer the official fake owned by the
operation's side-effect port, exercise it through that public interface and
retain adapter/provider qualification as a separate lane.

At the 1.6 durable audit ledger boundary, construct one privacy-aware semantic
audit action in the use case and prove that the adapter accepts it atomically
with the domain mutation, including duplicate and concurrent-write behavior.

At the 1.7 Apple Container default boundary, keep local dependency-runtime
selection outside domain and external API semantics. An operation may require
ready dependencies, but it must not silently migrate, replace or delete their
runtime resources.
