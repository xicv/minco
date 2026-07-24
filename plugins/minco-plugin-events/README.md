# minco-plugin-events

Provider-neutral domain-event publishing and transactional-outbox contracts.
The plugin deliberately does not create a scheduler or poller. Applications can
publish immediately on the request path and add an explicitly costed recovery
trigger only when required.
