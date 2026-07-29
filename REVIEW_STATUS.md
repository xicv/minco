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

Exact merged `main` `0f1271eec11bf2e4fd475f7093c04eddd8d47f6c`
passed the full local suite, AWS Plan/SAM validation and manual hosted run
`30444766607`. Authorised live run `20260729t105820z-approved` proved the
private PostgreSQL migration, the 5,038,349-byte native ARM64 artifact and
sealed exact-source release `minco.44a1623ffb1ec9bd0b037813`. Both API Gateway
stage creates still failed their dependent `TagResource` authorization.
CloudTrail recorded the calls as `CreateStage` from CloudFormation with the
complete reviewed tag sets. AWS documents the dependent tagging operation as
`apigateway:POST` on `/tags/*`, not the stage collection used by the current
policy. The existing CloudFormation-wide mutation statement would already
admit that resource if the dependent evaluation carried `aws:CalledVia`; IAM
simulation reproduces `implicitDeny` when that missing context remains
required.

The current unmerged correction leaves all API Gateway mutations behind the
CloudFormation caller-chain statement. It separately permits
`apigateway:POST` only on the documented `/tags/*` namespace, with the three
exact run-ownership request-tag values and the closed run, release, SAM and
CloudFormation system-key allowlist. IAM simulation returns `allowed` for the
exact request and `implicitDeny` for an extra tag key or wrong run ID. The
focused red/green regression, authoritative local quality suite, AWS Plan/SAM
validation and ShellCheck pass. Hosted qualification, replacement live AWS
proof, exact tag creation and crates.io publication remain separate pending
gates. Current and historical command evidence is maintained in
`VERIFICATION.md`; Feedback-specific architecture evidence remains in
`FEEDBACK_REVIEW_STATUS.md`.
