# minco-plugin-events

Provider-neutral domain-event publishing and transactional-outbox contracts.
The plugin deliberately does not create a scheduler or poller. Applications can
publish immediately on the request path and add an explicitly costed recovery
trigger only when required.

`FakeEventPublisher` records every valid typed publication attempt and consumes
queued infrastructure failures once. Use it with the real application service
or `EventServices` to prove retry and failed-outbox behavior without network
access. Its `Debug` output omits event payloads and metadata values.
