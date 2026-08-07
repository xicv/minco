# Testing payments safely

Keep the default test suite offline.

- Build checkout requests and assert exact serialized JSON.
- Use exact 22-character Waffo short-ID fixtures.
- Generate ephemeral RSA keys for signing and webhook tests.
- Verify tampering, stale timestamps, environment mismatch, store mismatch, duplicate delivery, and same-idempotency-key/different-body conflicts.
- Exercise `config-check`, `doctor`, and command help without contacting Waffo.
- Test application webhook projections against fake ports and durable idempotency-store behavior.
- Separate provider sandbox evidence from local conformance and production evidence.

A future transport test double should queue endpoint-specific responses and retain redacted requests for assertions, analogous to a billing fake, without adding a global singleton or allowing unregistered network requests.
