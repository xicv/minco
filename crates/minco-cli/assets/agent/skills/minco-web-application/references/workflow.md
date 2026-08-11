# Web application routing

Release skill freshness is checked against the current Minco changelog and
versioned documentation before the bundle ships.

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
application. Use one browser/native contract and generated client boundary
rather than duplicating request/response types. Start only selected owned local
services. Route product guidance through version-matched Signal documentation.
For versioned documentation presentation changes, test computed styles and
desktop/mobile geometry so prose-theme defaults cannot reintroduce markers,
spacing drift or overflow.
Realtime subscriptions resynchronize through authoritative HTTP reads; they do
not replace the API contract.

Always identify the current evidence lane. A local browser journey is not
hosted, deployed, production-runtime, or product-review proof.

At a maintenance release boundary, re-check version-matched documentation,
exact package/tool pins, public-contract compatibility and lane-specific evidence.
