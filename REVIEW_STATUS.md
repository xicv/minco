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

The bounded S3-visibility correction passed exact PR-head hosted run
`30458112104`, merged as
`dbe8a55f141c082a8329ec1871590c0199682eed`, passed the full local suite and
AWS Plan/SAM validation, and passed exact-main hosted run `30459913592`.
Authorised live run `20260729t143232z-approved` proved the private PostgreSQL
migration, the visibility guard, the 5,038,349-byte native ARM64 artifact and
exact-source release `minco.eefe49c4e87868c73164ecba`. Both API Gateway stage
creates then failed the provider-reported dependent `TagResource`
authorization. CloudTrail recorded the tagged `CreateStage` requests from the
exact temporary role and no separate `TagResource` event.

The current unmerged correction follows AWS's current API Gateway V2 operation
mapping: tagged `CreateStage` requires `apigateway:POST` on
`/apis/*/stages` and `apigateway:PUT` on `/tags/*`. Each specialized statement
requires the three exact run-ownership request tags and the same closed
ten-key allowlist. The focused regression failed before implementation and
passes afterward. IAM custom-policy simulation permits only those two
action/resource pairs; crossed pairs, a wrong run ID and an extra tag key are
all implicit deny. The failed live run's application cleanup is all true; once
the delayed managed secret reached `ResourceNotFound`, exact database/VPC,
bootstrap IAM and local credential-file checks were independently consolidated
in an all-true `final-cleanup.json`.

Exact source qualification, hosted qualification, replacement live AWS proof,
exact tag creation and crates.io publication remain separate pending gates.
Current and historical command evidence is maintained in `VERIFICATION.md`;
Feedback-specific architecture evidence remains in
`FEEDBACK_REVIEW_STATUS.md`.
