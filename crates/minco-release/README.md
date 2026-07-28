# minco-release

Immutable release-manifest and deployment-receipt primitives for Minco.

A release records exact function artifacts, OpenAPI contract, configuration and
database-source digests, Plan IR, rendered template, Cargo lockfile, source
change, toolchain identity and optional attestations. A deployment receipt binds
that verified release to exact database plans and terminal verification
evidence. Promotion reuses the same artifacts rather than rebuilding source.

```rust,no_run
use minco_release::FileDigest;

let digest = FileDigest::from_path("target/lambda/bootstrap.zip")?;
digest.verify()?;
# Ok::<(), minco_release::ReleaseError>(())
```
