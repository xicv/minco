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

Exact merged `main` `8dcc49e2cefec1b9a043da5ae50161ae1e2431d1`
passed the full local suite, AWS Plan/SAM validation and manual hosted run
`30440072120`. Authorised live run `20260729t094817z-approved` proved the
private PostgreSQL migration, native ARM64 artifact, sealed release and exact
run-owned stack tags. API Gateway stage tagging still failed because
CloudFormation's three automatic `aws:cloudformation:*` keys were absent from
the bounded `aws:TagKeys` allowlist. AWS IAM simulation reproduced the
`implicitDeny` and proved that adding only those keys makes the exact request
`allowed`. Rollback and the exact cleanup verifiers produced all-true
application, database/VPC/secret and bootstrap-IAM receipts.

The current unmerged correction narrows the stage action to API Gateway V2's
documented tagging IAM action, `apigateway:POST`, and admits only the three
provider-owned CloudFormation keys while retaining the exact resource,
call-chain and run-tag value guards. The focused shell regression passes. Full
local qualification, hosted qualification, replacement live AWS proof, exact
tag creation and crates.io publication remain separate pending gates. Current
and historical command evidence is maintained in `VERIFICATION.md`;
Feedback-specific architecture evidence remains in
`FEEDBACK_REVIEW_STATUS.md`.
