# Release and Promotion

A Minco release manifest includes:

- immutable source commit and digest-derived release identity;
- every function artifact path, size, and SHA-256;
- OpenAPI path, size, and SHA-256;
- deployment Plan IR path, size, and SHA-256;
- rendered deployment template path, size, and SHA-256;
- Cargo.lock path, size, and SHA-256;
- deterministic configuration, migration-catalog and seed-catalog digests;
- Rust, Minco and artifact-builder toolchain versions;
- optional repository-relative offline signatures or attestations.

Declare the artifact build under `[commands]`:

```toml
[commands]
package = ["scripts/aws/build-lambda.sh"]
```

Package and verify:

```bash
cargo minco package
cargo minco release verify target/minco/release.json
```

`package` refuses conflicted JJ commits, dirty Git workspaces, changed source
revisions, missing artifacts, and package outputs outside ignored `target/`.
The manifest stores repository-relative paths, so a clean checkout can verify
the same release without the builder's absolute filesystem paths. A different
manifest cannot replace an existing release path. Promotion selects a verified
manifest and deploys its exact artifacts and rendered template. It never
replans or recompiles source in staging or production.

Database migration remains a separate, digest-bound release operation. Review
`cargo minco db plan`, acknowledge that exact digest to `db migrate`, and retain
the resulting receipt before deployment or promotion. The receipt records
before/after history and schema verification without storing the database URL.
See [`database-lifecycle.md`](database-lifecycle.md).

Deployment receipts are distinct from the release manifest. The controller
must persist `started` before mutation and then persist exactly one terminal
`failed` or `succeeded` state. Writers serialize the transition through a
process-safe lock adjacent to the receipt, so competing controllers re-read the
terminal state instead of overwriting it. A recorded failure cannot be replaced
by a success, and success is invalid without repository-relative verification
evidence. Migration and seed bindings include both the catalog/plan digests and
the exact plan file. Receipts contain no credentials, database URLs, secret
values, or authorization headers.
