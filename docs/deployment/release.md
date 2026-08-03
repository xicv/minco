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

When the static-site plugin is selected, `cargo minco package` automatically
adds a deterministic asset manifest attestation. See
[`static-site.md`](static-site.md) for the private S3/CloudFront publication,
domain, rollback and byte-verification stages.

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

`deploy verify --static-site` keeps every API check above and additionally
requires the exact static publication receipt plus current S3, CloudFront,
certificate and DNS evidence. The generic deployment receipt binds both report
files; promotion still requires exactly one hosted API report and rejects any
unknown evidence kind.

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

## Compatibility-checked rollback

Rollback starts with two successful promotion receipts: the current release and
the older target release. The command is local and non-mutating:

```bash
cargo minco rollback \
  --current-promotion target/minco/current/promotion-receipt.json \
  --target-promotion target/minco/previous/promotion-receipt.json \
  --dry-run
```

Each historical promotion is first reverified through its exact release,
successful deployment, target-config and change-set receipt chain. Account,
Region, role and stack are part of environment identity; labels alone do not
match. The result separately classifies the environment, current-to-target OpenAPI
contract, configuration digest, deployment-plan digest, migration catalog,
seed catalog, exact migration/seed plan bindings applied by each deployment,
persisted-data evidence, API versions and worker artifacts. It is
`compatible`, `operator_decision_required`, or `incompatible`, with exact codes
and reasons. Missing data evidence is never inferred as safe.

When data compatibility has been reviewed, bind the decision to both exact
release IDs:

```json
{
  "schema_version": 1,
  "current_release_id": "minco.CURRENT_DIGEST_PREFIX",
  "target_release_id": "minco.TARGET_DIGEST_PREFIX",
  "decision": "compatible",
  "reviewed_by": "release-owner",
  "reason": "The older application read/write paths were rehearsed against the current schema and representative data."
}
```

Pass that normalized project-relative file with
`--data-compatibility-evidence`. A compatible assessment authorizes no provider
mutation by itself. A historical hosted report proves its historical candidate,
not the alias currently serving the newest deployment. From a clean checkout at
the target release's source change, redeploy the exact sealed release without a
rebuild or replan, run hosted verification again against the newly published
candidate version, then use ordinary promotion with that new report and exact
approval. Rollback never invents reverse SQL, repairs data, or rewires worker
event sources.

## Alarm-guarded API canary

Canary routing is absent by default. Add it only to a reviewed persistent
deployment target:

```toml
[environments.production.canary]
initial_traffic_percent = 10
monitoring_minutes = 15
alarm_arns = [
  "arn:aws:cloudwatch:ap-southeast-2:111122223333:alarm:minco-api-errors",
]
```

The alarm list must contain one through five unique, sorted metric-alarm ARNs
from the exact target account and Region. Composite alarms are outside this v1
shape because CloudFormation requires their distinct rollback-trigger type;
failing closed keeps that type part of any future reviewed contract. The policy
is fixed to a weighted `live` API alias,
preserves current worker event sources, and refuses provisioned concurrency.
Inspect the non-contacting plan first:

```bash
cargo minco promote \
  --manifest target/minco/release.json \
  --receipt target/minco/deployment-receipt.json \
  --verification target/minco/hosted-verification.json \
  --approve-verification-digest "$verification_digest" \
  --canary \
  --dry-run
```

Remove `--dry-run` only with separate live AWS authority. Minco creates a
routing-only CloudFormation change set with the concrete candidate weight and
exact rollback alarms. It first proves that every configured metric alarm exists in
the reviewed account and Region and is currently `OK`, and that the two
function versions have the same execution role and dead-letter configuration,
writes
`target/minco/canary-receipt.json` as `started`, and waits through the monitoring
window. After the window, Minco re-reads every exact metric alarm and requires
`OK`; missing or `INSUFFICIENT_DATA` evidence is treated as a failed post-traffic
check and the cleanup restores the old route. An alarm stops and reverses the shift;
Minco records `reversed` only after the previous unweighted alias is observed.
On success it verifies the weighted alias, restores and verifies the previous
unweighted alias through a second routing-only change set, records `succeeded`,
then runs the ordinary all-traffic promotion.

The canary creates no persistent resource, schedule, provisioned concurrency,
or fixed compute. Existing CloudWatch alarm charges are external and therefore
reported as incomplete pricing. API traffic shifts; worker aliases and event
sources do not.
