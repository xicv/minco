# minco-release

Immutable release-manifest and digest verification primitives for Minco.

A release records the exact application artifact, OpenAPI contract, migration
set, deployment plan, Cargo lockfile, source change, and toolchain identity.
Promotion reuses the same artifact rather than rebuilding source.

```rust,no_run
use minco_release::FileDigest;

let digest = FileDigest::from_path("target/lambda/bootstrap.zip")?;
digest.verify()?;
# Ok::<(), minco_release::ReleaseError>(())
```
