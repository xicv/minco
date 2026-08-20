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

At the 1.8 resumable object transfer boundary, keep upload/download handlers
thin and call one application use case that owns authorization, durable session
state, immutable pointer updates, quarantine and retention. Send large bytes
directly to the selected private provider rather than through the JSON API.

At the 1.9 API Gateway traffic policy boundary, prefer the managed stage and
route throttling rendered onto both the `$default` and candidate stages before
adding any application-side limiter. Treat it as best-effort ingress
protection, never as authorization, a per-user quota or a hard spend cap.

At the 1.9 negotiated response compression boundary, serve eligible known-size
responses through the standard negotiated gzip layer and keep the per-response
`DisableResponseCompression` marker for representations that combine secrets
with attacker-controlled reflection. Do not add application decompression or
dynamic Brotli without explicit measured evidence.

At the 1.10 Ticketing support-entry boundary, keep requester identity and
permissions server-derived, bound untrusted browser context, and issue
single-use handoffs only after the durable ticket result exists.
