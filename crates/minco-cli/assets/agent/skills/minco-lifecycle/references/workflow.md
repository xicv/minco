# Lifecycle decision table

Release skill freshness is checked against the current Minco changelog and
versioned documentation before the bundle ships.

| Need | Inspect first | Possible explicit action |
|---|---|---|
| Effective config | `config check/explain/diff` | edit authoritative config |
| Migration | `db plan/status` | `db migrate` with digest and environment guards |
| Seed | seed plan/dry run | `db seed` with classification and preservation guards |
| Owned local services | `dev --dry-run` plus resource identity | `dev` with selected PostgreSQL, Rustack or Mailpit profile |
| Rich mail | transport/capture/delivery state | explicitly authorized local or provider action |
| Infrastructure | `deploy plan/review` | change set/apply under deployment authority |
| Exact release | package/release verification | promote only the verified artifact |
| Cleanup | destroy dry run | guarded exact-identity cleanup |

Do not infer a live state from a plan or generic charges from an unselected
topology-aware cost dimension. Do not infer deployment from hosted CI, or
production behavior from a local process.

At a maintenance release boundary, re-check version-matched documentation,
exact package/tool pins, public-contract compatibility and lane-specific evidence.

At the 1.5 assurance release boundary, use the reviewed topology-cost baseline
as provider-free regression evidence only; it is not a provider price,
deployment result or production budget.

At the 1.6 durable audit ledger boundary, monitor retained bytes and journal
lag, and require an explicit verified archive/legal-hold decision before TTL,
partition pruning or sealed-segment deletion.

At the 1.7 Apple Container default boundary, treat an existing receipt and its
exact owned resources as stronger evidence than the fresh-install default.
Require explicit runtime selection, a verified data backup or export and exact
ownership before migration or deletion.

At the 1.9 API Gateway traffic policy boundary, prefer the managed stage and
route throttling rendered onto both the `$default` and candidate stages before
adding any application-side limiter. Treat it as best-effort ingress
protection, never as authorization, a per-user quota or a hard spend cap.
