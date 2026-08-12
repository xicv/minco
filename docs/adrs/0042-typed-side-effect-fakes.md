# ADR 0042: Keep side-effect fakes typed and application-test scoped

## Status

Accepted.

## Context

Minco already provides successful in-memory adapters for events, objects,
feedback and mail. They are useful for local composition, but application tests
also need to prove retry, fallback, partial-batch and fail-before-persistence
behavior. Hand-written test doubles repeatedly reimplemented the same public
ports, often without an observable call order or a precise one-shot failure.

A generic mocking layer would obscure which application-owned port is being
exercised and would encourage tests to assert internal calls instead of public
behavior. Provider sandboxes are too slow and costly for this test tier and do
not replace bounded live evidence.

## Decision

The owning packages publish five explicit fakes:

- `FakeMessageHandler` for the SQS `MessageHandler` contract;
- `FakeEventPublisher` for domain-event publication;
- `FakeObjectStore` for object put/get/delete behavior;
- `FakeFeedbackStore` for feedback persistence; and
- `FakeMailTransport` for rich-mail submission and fallback.

Each fake implements its real public port, records attempts in call order and
consumes explicitly queued failures once. Tests exercise the real worker,
service or port interface; no facade, macro-generated mock, service locator or
provider SDK is introduced. Successful object and feedback behavior delegates
to the existing memory implementations so their invariants remain aligned.

Captured values remain accessible to the test through typed snapshots when the
assertion requires them. Fake and attempt `Debug` output is privacy-bounded:
message bodies, object bytes, feedback content, token hashes, recipients, mail
bodies, attachments and metadata values are not rendered. Test authors remain
responsible for using synthetic fixtures and must never commit customer data or
credentials.

## Compatibility

The new types and methods are additive public Rust APIs. Existing memory
adapters, traits, plugin descriptors, serialized contracts, CLI output, Plan IR,
IAM and provider renderers are unchanged. No fake is selected by a production
composition root unless an application explicitly injects it.

## Cost, performance and evidence

The fakes perform no network access, provider contact, sleep, polling,
background work or resource creation. Their locks are test-process state and
make no production-performance claim. A passing fake-based application test is
behavior evidence only; AWS, mailbox, storage durability, latency and cleanup
still require their separate provider lanes.

## Alternatives rejected

- A generic expectation/mocking framework: weakens port vocabulary and couples
  tests to implementation detail.
- Extending production adapters with test switches: mixes provider behavior
  with test authority.
- Making failures permanent by default: makes retries hard to model and hides
  whether one attempt consumed the intended outcome.
- Relying only on successful memory adapters: cannot prove failure policy.
- Requiring provider sandboxes for application tests: slow, costly and unable
  to provide deterministic failure ordering.
