# Lifecycle decision table

| Need | Inspect first | Possible explicit action |
|---|---|---|
| Effective config | `config check/explain/diff` | edit authoritative config |
| Migration | `db plan/status` | `db migrate` with digest and environment guards |
| Seed | seed plan/dry run | `db seed` with classification and preservation guards |
| Local topology | `dev --dry-run` | `dev` with selected profile/processes |
| Infrastructure | `deploy plan/review` | change set/apply under deployment authority |
| Exact release | package/release verification | promote only the verified artifact |
| Cleanup | destroy dry run | guarded exact-identity cleanup |

Do not infer a live state from a plan. Do not infer deployment from hosted CI,
or production behavior from a local process.
