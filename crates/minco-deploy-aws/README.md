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
