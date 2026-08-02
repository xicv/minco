# Static-site deployment on AWS

Minco treats a frontend as an immutable release artifact served from a private
S3 origin through CloudFront. It does not run Node in production, expose an S3
website endpoint, or assume a frontend framework. The default profile has no
provisioned compute while idle; S3 storage, CloudFront requests/transfer and
optional Route 53 charges remain explicit.

## 1. Select and configure the plugin

Enable the plugin with `cargo minco plugin enable static-site`, then configure
the application-owned build output in `minco.toml`:

```toml
[plugins]
enabled = ["health", "observability", "idempotency", "static-site"]

[plugins.configuration.static-site]
source_directory = "dist"
index_document = "index.html"
spa_fallback = true
immutable_cache_seconds = 31536000
html_cache_seconds = 0
price_class = "price_class100"
ipv6_enabled = true
custom_domain = "app.example.com"
manage_dns_alias = true
```

`source_directory` and `index_document` accept only normalized relative paths.
The build must not contain symlinks, non-UTF-8 paths, traversal, or files below
the reserved `.minco/` prefix. A filename with an eight-or-more-character hex
token receives immutable caching; entrypoints receive the configured short
cache plus `must-revalidate`.

The application remains responsible for producing `dist`. Put that build in
the existing `[commands].package` sequence alongside the Lambda build. Minco
does not add npm, Vite, Vue, React or another build tool implicitly.

## 2. Review target-owned AWS inputs and cost evidence

Custom aliases use a certificate that already exists in `us-east-1`. Managed
DNS uses a public hosted zone that already owns the hostname. Store identifiers
and dated commercial evidence—not secret values—in the reviewed deployment
target:

```toml
[environments.production]
enabled = true
expected_account_id = "123456789012"
expected_region = "ap-southeast-2"
expected_role_arn = "arn:aws:iam::123456789012:role/minco-production"
stack_name = "minco-production"
artifact_bucket = "minco-production-artifacts"
database_url_parameter_name = "/minco/production/database-url"
static_site_certificate_arn = "arn:aws:acm:us-east-1:123456789012:certificate/REVIEWED-ID"
static_site_hosted_zone_id = "Z1234567890ABC"
static_site_pricing_checked_on = "2026-08-02"
static_site_pricing_source = "https://aws.amazon.com/cloudfront/pricing/"
static_site_billing_model = "request_and_transfer"
static_site_flat_rate_eligibility = "ineligible"
```

Allowed flat-rate evidence is `ineligible`, `eligible_not_selected`, or
`eligible_selected`. Live verification requires an evaluated account result:
omitting the pricing fields is valid for local planning, but `not_evaluated`
is never accepted as release evidence. `flat_rate` billing requires
`eligible_selected`; request/transfer billing must not claim the plan was
selected. Refresh the date and source when commercial assumptions are
re-reviewed. The chosen `PriceClass_100`, `PriceClass_200`, or
`PriceClass_All` remains in the exact release plan.

## 3. Package the exact bytes

```bash
cargo minco package
cargo minco release verify target/minco/release.json
```

When the plugin is selected, packaging automatically writes
`target/minco/static-site-release.json` and attaches its file digest to the
release. It records sorted path, byte count, SHA-256, content type and cache
metadata for every asset. Changed bytes after packaging fail before a provider
call.

Render and review the normal combined SAM template. Static resources include a
private encrypted bucket, OAC, explicit cache policy, distribution, bucket
policy and optional A/AAAA aliases. Certificate and hosted-zone parameters are
required only by the selected domain profile.

## 4. Review and apply infrastructure

Use the normal exact-release change-set and apply workflow described in
[`release.md`](release.md). A dry run reports missing certificate, hosted-zone
or pricing evidence without contacting AWS. CloudFormation apply creates no
object content and leaves the generic deployment receipt `started`.

Live AWS creation is cost-bearing and requires the same explicit change-set
approval as every other infrastructure change. A generated template or dry run
is not authorization and is not live proof.

## 5. Plan and publish assets

Inspect the publication first:

```bash
cargo minco deploy static-site plan \
  --manifest target/minco/release.json \
  --deployment-receipt target/minco/deployment-receipt.json
```

Then approve the exact release digest:

```bash
release_digest="$(jq -er '.release_digest' target/minco/release.json)"
cargo minco deploy static-site apply \
  --manifest target/minco/release.json \
  --deployment-receipt target/minco/deployment-receipt.json \
  --approve-release-digest "$release_digest"
```

Apply rechecks source, target, caller and stack, then acquires the conditional
S3 `.minco/deployment-lock`. It uploads each object with SHA-256, verifies
provider size/checksum/media/cache metadata, deletes stale objects only after
all new objects pass, invalidates `/*`, waits for `Completed`, releases the lock
and writes `target/minco/static-site-publication.json`.

The dedicated generated bucket is published at its root. Prefix-based adapter
use is also bounded; root publication requires explicit opt-in and is safe only
for a bucket dedicated to that site.

## 6. Verify API and static hosting together

```bash
cargo minco deploy verify --static-site \
  --manifest target/minco/release.json \
  --receipt target/minco/deployment-receipt.json \
  --static-site-publication target/minco/static-site-publication.json
```

The configured API hosted checks still run. Static verification additionally
re-reads every S3 checksum and metadata field, downloads every path through the
reviewed CloudFront URL with redirects disabled and identity encoding, bounds
the response by the released size, and verifies its hash, content type, and
cache metadata. It also checks the deployed distribution/OAC and completed
invalidation, confirms an
`ISSUED` `us-east-1` certificate covers the exact alias, and confirms a public
hosted zone plus A/AAAA aliases target the distribution. The deployment receipt
becomes `succeeded` only when all enabled evidence passes.

Use `cargo minco deploy verify --static-site --dry-run` to list blockers. It
makes no AWS or HTTP call and performs no receipt transition.

## Rollback and removal

Rollback publishes a previously verified static-site release from its exact
source revision using a new `deploy static-site apply` approval. The same lock,
checksums and invalidation gates apply. Receipts are immutable and are never
rewritten to pretend that an older publication occurred later.

If publication fails before stale deletion, the previous entrypoint and stale
objects remain. Content-addressed uploads may remain and are reconciled by the
next successful publication. Every failed publication retains the control lock
because provider state may be partial. Do not delete it until you have proved
that no publisher is running and identified the exact generated bucket from
the reviewed stack:

```bash
aws s3api head-object \
  --bucket REVIEWED_STATIC_BUCKET \
  --key .minco/deployment-lock

# Separate, explicit recovery action after the ownership check:
aws s3api delete-object \
  --bucket REVIEWED_STATIC_BUCKET \
  --key .minco/deployment-lock
```

Removing the plugin makes distribution, cache-policy and DNS removals visible
in the CloudFormation change set. The S3 bucket is retained; objects and storage
cost remain until a separately reviewed inventory and deletion action. Minco
never interprets plugin removal as permission to delete retained data or a
registered domain.
