# Dev-to-Deploy Lifecycle

```text
contract change
  -> deterministic bindings
  -> TDD implementation
  -> local dependencies
  -> complete quality gate
  -> build immutable artifact
  -> create Plan IR and SAM
  -> explicit database migration
  -> apply infrastructure
  -> hosted verification
  -> optional alarm-guarded API canary
  -> promote exact release
```

The evidence boundaries are intentionally separate:

| Boundary | Evidence | What it does not prove |
|---|---|---|
| Local qualification | tests, lint, Plan/SAM validation | hosted runtime behavior |
| Infrastructure apply | immutable change-set and started deployment receipt | candidate acceptance or live routing |
| Hosted verification | request IDs, status codes, readiness/auth/smoke results, exact candidate version and artifact | live or production behavior |
| Canary qualification | weighted live-alias change sets, exact alarms, post-window alias proof and terminal canary receipt | worker rollback or future production behavior |
| Promotion | routing-only change set and terminal promotion receipt | production runtime acceptance |
| Production proof | separately captured live requests and operational evidence | future release correctness |

## Environment classes

| Environment | Purpose | Data and mutation policy |
|---|---|---|
| local | Fast developer loop | Synthetic/local data; reset allowed. |
| dev | Real AWS integration | Synthetic data; guarded recreate allowed. |
| staging | Release acceptance | Persistent; reset is exceptional and explicit. |
| production | Live traffic | No demo seed/reset; protected account/role. |

An explicit `preview` or `preview-*` environment is a fifth, disposable class:
it retains the same release and verification boundaries but adds owner, TTL,
exact resource/cost/retention evidence, Feedback linkage, and separately
approved cleanup. Expiry creates no default schedule or deletion authority.
See [Preview Verified Review Loop](preview-review-loop.md).

The same contract, router, release artifact, and Plan IR move forward. Environment config
selects credentials and resource settings; it does not rebuild business code.

Compatibility-checked rollback moves backward through the same evidence chain:
it compares successful current and target promotion receipts, reports contract,
configuration, resources, migration, data, API and worker reasons, and then
reuses exact-artifact promotion only when the result is compatible. See
[Release and Promotion](release.md#compatibility-checked-rollback).
