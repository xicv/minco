# Review status

The current review boundary is the Minco `0.4.0` release-source and package
candidate in M8-T07. It reconciles the accepted framework work through M10-T03,
the 28-package family, the four first-publish crates, upgrade guidance,
zero-provisioned-compute doctrine, Verified Review Loop and recurring truth
diagnostics.

This record separates:

- source review and local qualification;
- hosted checks on the exact pull-request head;
- pull-request merge and merged-main requalification;
- live AWS deployment/rollback evidence;
- exact tag creation;
- crates.io publication and independent registry/consumer/docs.rs proof.

The first tagged-stage correction passed exact PR-head hosted run
`30466012186`, merged as
`4bf245cae924e2d3c89d008cf291da8bf862cba4`, passed the full local suite and
AWS Plan/SAM validation, and passed exact-main hosted run `30467769879`.
Authorised live run `20260729t215737z-approved` proved the private PostgreSQL
migration, S3 visibility on the first bounded attempt, exact-source release
`minco.683d7abad93046f3b4476621` and an exact release-bound change set. Both
API Gateway stage creates then failed because AWS evaluated the dependent
`apigateway:TagResource` authorization as `apigateway:PUT` on the stage
collection ARN `/apis/<api-id>/stages`, not on the direct tagging API's
`/tags/*` namespace.

The current unmerged correction keeps the specialized
`apigateway:POST` and `apigateway:PUT` permissions on
`/apis/*/stages`. Each statement requires the three exact run-ownership
request tags and the same closed ten-key allowlist. The focused regression
failed before implementation and passes afterward. IAM custom-policy
simulation permits both required methods on the stage collection with exact
tags; a wrong run ID, an extra tag key and direct `PUT` on `/tags/*` are
implicit deny. Access Analyzer reports no findings for the two specialized
statements. The failed live run's application cleanup is all true, and the
second exact database cleanup verification confirms the delayed managed
secret, instance, stack, VPC, local secret files and synthetic data are all
absent. Bootstrap IAM and local temporary credentials are also absent.

Exact source qualification, hosted qualification, replacement live AWS proof,
exact tag creation and crates.io publication remain separate pending gates.
Current and historical command evidence is maintained in `VERIFICATION.md`;
Feedback-specific architecture evidence remains in
`FEEDBACK_REVIEW_STATUS.md`.
