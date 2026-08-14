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

At the 1.5 assurance release boundary, use official typed fakes through their
owning ports for application behavior and keep provider, model-driven and human
review outcomes explicit rather than inferred.

At the 1.6 durable audit ledger boundary, expose only permission-gated bounded
resource history, keep audit storage independent of operational schemas, and
make retention, archive and provider-cost limits visible to operators.

At the 1.7 Apple Container default boundary, keep the native application
process outside the dependency container. Apple selection or Docker fallback
changes local dependency lifecycle only, not application, API or production
semantics.

At the 1.8 resumable object transfer boundary, use the API as an authorized
bounded control plane and the private provider as the byte plane. Mobile retry,
stop/resume and private caching must retain strong revision identity, while
content remains quarantined until application inspection accepts it.
