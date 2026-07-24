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
  -> promote exact release
```

## Environment classes

| Environment | Purpose | Data and mutation policy |
|---|---|---|
| local | Fast developer loop | Synthetic/local data; reset allowed. |
| dev | Real AWS integration | Synthetic data; guarded recreate allowed. |
| staging | Release acceptance | Persistent; reset is exceptional and explicit. |
| production | Live traffic | No demo seed/reset; protected account/role. |

The same contract, router, release artifact, and Plan IR move forward. Environment config
selects credentials and resource settings; it does not rebuild business code.
