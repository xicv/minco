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

Exact merged `main` `edabc701ee86b4adfee27b978f8d4d6187d19f2e`
passed the full local suite, AWS Plan/SAM validation and manual hosted run
`30449710067`. Authorised live run `20260729t121408z-approved` proved the
private PostgreSQL migration, the 5,038,349-byte native ARM64 artifact and
sealed exact-source release `minco.6fba6aee8d28ce4d9bece03b`. Both API Gateway
stage creates still failed their dependent `TagResource` authorization.
CloudTrail records the actual operations as `CreateStage` on
`/apis/${ApiId}/stages`, with the complete reviewed tag sets and no separate
tagging event. IAM simulation confirms that the current `/tags/*` statement
cannot admit that stage-collection resource.

The current unmerged correction leaves general API Gateway mutations behind
the CloudFormation caller-chain statement. It separately permits
`apigateway:POST` only on `/apis/*/stages`, with the three exact run-ownership
request-tag values and the closed run, release, SAM and CloudFormation
system-key allowlist. IAM simulation returns `allowed` for the exact observed
request without `aws:CalledVia`, and `implicitDeny` for a wrong run ID or extra
tag key; the whole-statement regression failed before the correction and
passes afterward. Application cleanup is all true. The delayed RDS-managed
secret subsequently reached `ResourceNotFound`, the exact RDS cleanup verifier
is all true, and the deterministic bootstrap user and role are independently
absent. The authoritative local quality suite and AWS Plan/SAM validation pass
on the replacement candidate. Hosted qualification, replacement live AWS
proof, exact tag creation and crates.io publication remain separate pending
gates. Current and historical command evidence is maintained in
`VERIFICATION.md`; Feedback-specific architecture evidence remains in
`FEEDBACK_REVIEW_STATUS.md`.
