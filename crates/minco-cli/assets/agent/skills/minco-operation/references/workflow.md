# Operation layer ownership

- OpenAPI owns the external HTTP contract.
- Domain owns pure invariants and transitions.
- Application owns authorization, validation, orchestration, and use-case ports.
- Adapters implement application-owned ports.
- HTTP handlers extract/map, call one use case, and map one response.
- The composition root selects concrete adapters and runtime plugins.

Prefer tests that observe public behavior:

- domain unit tests for invariants;
- application tests with fake boundary ports;
- adapter behavior tests against the real engine;
- Axum `oneshot` tests for the external contract; and
- plan/configuration tests for declared infrastructure consequences.

Record the exact failed test before implementation and the focused passing
command afterward. Do not turn an unrun provider or hosted check into a pass.
