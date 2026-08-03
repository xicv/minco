# minco-deploy-aws

Fail-closed environment guards, deterministic CloudFormation change-set review,
strict hosted-verification reports, and digest-sealed promotion receipts for
Minco's AWS deployment controller.

The crate models review and authorization; the CLI owns provider and HTTP
execution. Hosted reports require contract, readiness, authentication, smoke,
request-ID, status, published-version, and exact-artifact evidence. Promotion
accepts only one ordinary property update to the expected live API Gateway
stage. Neither CloudFormation completion nor promotion claims production
runtime proof.

Optional static-site evidence follows the same boundary. An immutable
publication receipt binds the exact release manifest, private bucket,
distribution, completed invalidation, and public URL. Hosted verification then
requires exact S3 and CloudFront bytes/metadata, a Region-matching private S3
origin with OAC, certificate and DNS ownership when configured, and dated
official pricing evidence with evaluated account eligibility. Promotion
rejects unknown evidence kinds or a static report for another release.

Preview review manifests bind the exact source, release, artifacts, successful
deployment, change-set target, provider resource/retention inventory, ownership,
TTL, cost confidence, verification, delivery trace, and untrusted Feedback
IDs/digests. Cleanup receipts bind that immutable review and permit one
started-to-terminal transition. Persistent targets, mismatched target
configuration, enabled or unproved termination protection, provider drift, and
concurrent receipt writers fail closed.
