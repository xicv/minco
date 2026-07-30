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

CloudFormation review is a separate immutable boundary. `cargo minco deploy
changeset` binds the provider preview, packaged template, exact release,
reviewed target and drift evidence without executing it. `cargo minco deploy
apply` requires the exact review-receipt digest plus successful migration
evidence, rechecks live guards, and persists the deployment receipt before
executing the exact change-set ARN. Successful infrastructure apply leaves the
receipt in `started`; hosted verification owns the eventual `succeeded`
transition, while execution or wait errors write terminal `failed`.

## Hosted verification

The API function is published behind stable `candidate` and `live` Lambda
aliases. Each API Gateway stage invokes its matching alias, and each alias has
its own API-scoped resource policy. `LiveFunctionVersion` selects the published
version behind `live`; the initial `candidate` sentinel points `live` at the
same generated version without granting unqualified invocation.
Infrastructure apply does not make the new release live on a stack that
already has this boundary; ordinary updates must preserve the previous numeric
parameter value. A pre-boundary existing stack is rejected instead of silently
resetting live routing. A new stack initially has no prior live release, so
both aliases begin on the same version until the first numeric promotion
anchors `live`.

Declare one hosted verification command:

```toml
[commands]
hosted_verify = ["scripts/aws/smoke.sh"]
```

Then verify the applied candidate:

```bash
cargo minco deploy verify \
  --manifest target/minco/release.json \
  --receipt target/minco/deployment-receipt.json \
  --output target/minco/hosted-verification.json
```

The controller re-verifies the release, source, deployment/change-set
receipts, account, role, Region, current stack outputs, candidate function
version, and Lambda `CodeSha256`. The configured command must provide redacted
contract, readiness, authentication, smoke, and artifact-identity results.
Every HTTP check includes a bounded request ID and status. A missing, duplicate,
invalid, or failed check makes the deployment receipt terminal `failed`;
success binds the immutable hosted report and makes that receipt terminal
`succeeded`.

## Exact-artifact promotion

Promotion requires human approval of the exact hosted report file digest:

```bash
verification_digest="$(
  shasum -a 256 target/minco/hosted-verification.json | awk '{print $1}'
)"
cargo minco promote \
  --manifest target/minco/release.json \
  --receipt target/minco/deployment-receipt.json \
  --verification target/minco/hosted-verification.json \
  --approve-verification-digest "$verification_digest"
```

`promote` does not rebuild, repackage, or replan. It rechecks current source,
caller, clean stack drift, candidate endpoint/version/digest, and every bound
file. It creates an unexecuted CloudFormation update from the original packaged
template with all parameters preserved except `LiveFunctionVersion`. Execution
is allowed only when the provider reports exactly one ordinary property
modification to `LiveFunctionAlias`, the live `AWS::Lambda::Alias`; any
function, permission, API, replacement, deletion, import, dynamic, or
provider-sync change is rejected.

The promotion receipt is persisted `started` before execution and makes one
terminal transition after the stack parameter plus candidate and live alias
identities are rechecked. Local qualification, hosted candidate verification,
routing promotion, and production runtime proof remain separate evidence.
Promotion does not synthesize production proof.
