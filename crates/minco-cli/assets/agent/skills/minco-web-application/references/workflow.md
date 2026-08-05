# Web application routing

Use the smallest workflow that owns the requested behavior:

| Request | Route |
|---|---|
| External HTTP behavior | `$minco-operation` |
| Reusable capability or provider seam | `$minco-plugin` |
| Configuration, database, seed, dev process | `$minco-lifecycle` |
| Failure investigation | `$minco-diagnose` |
| Change assessment | `$minco-review` |
| Minco framework repository task | `$minco-framework-task` |
| Explicit release preparation | `$minco-release` |

For a frontend plus API journey, keep browser state and product policy in the
application. Use the OpenAPI contract and generated client boundary rather than
duplicating request/response types. Realtime subscriptions resynchronize
through authoritative HTTP reads; they do not replace the API contract.

Always identify the current evidence lane. A local browser journey is not
hosted, deployed, production-runtime, or product-review proof.
