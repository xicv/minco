---
name: minco-operation
description: >-
  Add or change one Minco external API operation through the complete
  OpenAPI-first vertical slice. Use when implementing endpoint behavior,
  request or response schemas, authorization, domain invariants, persistence,
  HTTP mapping, generated bindings, or operation-level verification.
---

# Add a Minco operation

Follow one test-driven vertical slice at a time.

1. Read the canonical OpenAPI document and run
   `cargo minco explain <operationId> --json` when the operation exists.
2. Change OpenAPI first, including examples, security, success responses,
   stable Problem responses and the same browser/native contract metadata when
   both clients consume the operation. Do not create a second business API.
3. Run `cargo minco contract check --json`; sync generated bindings rather than
   editing `// @generated` files.
4. Add one failing application test through a use-case-shaped port.
5. Implement domain invariants and the application use case without Axum,
   SQLx, AWS SDK, Lambda, or Minco HTTP/plan dependencies.
6. Add an adapter only when persistence or an external boundary is required.
7. Add an in-process Axum contract test for status, media type, headers, IDs,
   body, authorization, and fail-before-persistence behavior.
8. When applicable, model a verified direct upload as authorization-first
   issuance plus provider-metadata completion, and rich mail as validated
   submission plus a separate acceptance/delivery observation boundary.
9. Update Plan, IAM, cost, wake-source, configuration, migration, and seed
   implications when the operation changes them.
10. Run focused checks, then confirm `cargo minco explain <operationId> --json`
   traces the completed slice.

Never add SQL to a handler, a generic CRUD repository, or fake business
behavior merely to make generated tests pass.

Read [workflow.md](references/workflow.md) for layer ownership and observable
test boundaries.
