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

Exact merged `main` `13be9b0a8d99281c98fec880b8d275a59c7499f9`
passed the full local suite, AWS/SAM validation and manual hosted run
`30434365889`. Authorised live run `20260729t082616z-approved` proved the
private PostgreSQL migration, native ARM64 artifact, sealed release and real
change-set parser, then failed during application stack creation because the
change set omitted the run-ownership tags required by the bounded API Gateway
stage policy. Rollback and an exact manual cleanup removed all run resources;
a cross-service absence sweep passed.

The current unmerged correction makes validated target stack tags part of the
deterministic change-set input and binds the bounded smoke tags to the IAM and
cleanup contract. The authoritative local suite, AWS Plan/SAM validation,
ShellCheck and non-contacting AWS CLI validation pass. Hosted qualification,
replacement live AWS proof, exact tag creation and crates.io publication
remain separate pending gates. Current and historical command evidence is
maintained in `VERIFICATION.md`; Feedback-specific architecture evidence
remains in `FEEDBACK_REVIEW_STATUS.md`.
