# minco-plugin-jobs

Durable typed work for Minco: typed job contracts, an explicit handler
registry, a bounded versioned envelope, at-least-once dispatch ports,
lease-based execution, retry and permanent-failure semantics, overlap locks
and explicit scheduling contracts.

Jobs are commands with exactly one registered handler and are deliberately
distinct from domain events. The durable job row owns execution state, the
publication row owns pending transport delivery, and the queue message is
delivery, never truth. Business mutations and durable dispatch commit
atomically through the SQL adapters' transactional `enqueue_in`; rolling the
business mutation back leaves no job and no publication intent.

The plugin never schedules itself: dispatch is explicit and bounded, retries
are durable, and permanent failure is persisted before the delivery is
acknowledged. Delivery is at least once — duplicate deliveries are
neutralized by an atomic execution claim, and application effects must be
idempotent. Payloads, metadata values and secrets never appear in `Debug`.

`MemoryJobStore` and `FakeJobDispatcher` provide deterministic in-memory
reference semantics for tests without network access; the fake records every
valid dispatch attempt in order and consumes queued one-shot failures once.
