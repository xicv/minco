# Webhook workflow

Treat webhook handling as an explicit application slice:

1. Preserve the exact request bytes and signature header.
2. Enforce the configured body-size bound.
3. Verify the signature and timestamp before JSON decoding.
4. Reject the wrong Waffo environment and expected store.
5. Atomically claim the delivery/event deduplication key in durable application storage.
6. Apply an idempotent application-owned projection or domain command.
7. Record received, handled, ignored, retryable-failure, and terminal-failure outcomes.
8. Return success only after the chosen durability boundary is satisfied.

The plugin verifies provider authenticity; it must not become the owner of product-specific subscription tables, access rules, invoices, or order state. Typed application events are useful, but they follow successful verification and durable deduplication.

Never parse and re-serialize the body before signature verification. Never use a checkout return URL as the authoritative payment signal.
