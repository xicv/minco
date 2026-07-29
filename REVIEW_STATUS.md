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

The stage-collection correction passed exact PR-head hosted run `30496875203`
at `cffb60520a9311c72cf287f94c8dcbfa762bf1e0`, merged as
`36d09d5ce36242290ae99506afee64c1a2f0de91`, passed the full local suite and
AWS Plan/SAM validation, and passed exact-main hosted run `30498077062`.
Authorised replacement run `20260729t231646z-approved` stopped before any
application, database or release work: the fresh bootstrap access key resolved
to the exact run-owned user, but its immediately following first
`AssumeRole` call returned `InvalidClientTokenId`.

The current unmerged correction handles that same fresh-key propagation error
in the already bounded role-assumption loop: at most 15 attempts, two seconds
apart, with no widened action, principal, role or credentials lifetime. It
also records whether the application runner was invoked so early bootstrap
failure can truthfully report the never-started application clean while still
requiring the existing all-true cleanup receipt after any invocation. The
focused regression failed before implementation and passes afterward. Exact
application and RDS stack checks, bootstrap IAM checks and local temporary
credential checks for the failed run are all absent/clean.

Exact source qualification, hosted qualification, replacement live AWS proof,
exact tag creation and crates.io publication remain separate pending gates.
Current and historical command evidence is maintained in `VERIFICATION.md`;
Feedback-specific architecture evidence remains in
`FEEDBACK_REVIEW_STATUS.md`.
