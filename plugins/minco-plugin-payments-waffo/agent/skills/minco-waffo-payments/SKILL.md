---
name: minco-waffo-payments
description: Build and review Minco Waffo Pancake checkout, signed actions, GraphQL queries, and verified webhooks. Use when a Minco application enables payments-waffo or invokes minco-waffo.
---

# Minco Waffo payments

Use this skill only after reading the application's `AGENTS.md`, Minco graph, and the
`payments-waffo` configuration. The provider contract was reviewed against Waffo Go SDK
revision `df098331cf5ea7d43ad79ab223d9eda6d4ac8e5f`.

## Route the task

- Common hosted checkout: read `references/checkout.md`.
- Webhook registration, verification, projection, or replay safety: read `references/webhooks.md`.
- Tests, fixtures, failure simulation, or provider-contract review: read `references/testing.md`.

## Invariants

1. The application owns orders, entitlements, subscriptions, and transaction projections.
2. A checkout redirect is not proof of payment; verified webhook state is authoritative.
3. Use `orderMerchantExternalId` or string metadata to correlate provider events with an application-owned identifier.
4. Never place private keys or webhook public-key values in TOML, arguments, logs, prompts, receipts, or generated plans. Use `env:` locally and exact `ssm:` references in AWS.
5. Reuse the same provider idempotency key for an explicit retry of the same request.
6. Do not infer production readiness from offline conformance or a successful test checkout.
7. Do not add ORM models, global billing state, hidden polling, or scheduled reconciliation to the plugin. Add application-owned ports and explicit workers only when the product requires them.

## Start with machine-readable checks

```bash
minco-waffo --config minco.waffo.toml config-check
minco-waffo --config minco.waffo.toml doctor
minco-waffo idempotency-key
```

`config-check` resolves no secrets. `doctor` parses configured keys but contacts no Waffo API.
