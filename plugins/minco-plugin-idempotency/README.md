# minco-plugin-idempotency

Official Minco idempotency primitives.

The crate defines validated idempotency keys, deterministic request
fingerprints, replay records, a use-case-neutral storage port, and an in-memory
implementation suitable for tests and local development. Durable applications
should implement the storage trait transactionally in their persistence layer.
