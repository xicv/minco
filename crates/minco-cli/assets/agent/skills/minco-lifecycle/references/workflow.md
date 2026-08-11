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
