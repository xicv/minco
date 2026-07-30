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

The deployment-role bucket-visibility correction passed exact-head hosted run
`30519948680` at `612dbf16fd998538d941308079e2b9437d4be87e`,
merged as exact `main`
`daae0595deffe945726df54c6f43ee82ff7bc7fd`, passed the complete local
qualification again and passed exact-main hosted run `30521267303`. All
authoritative quality, browser, 28-package dry-run, Plan/SAM/native ARM64,
Rustack/SSM and explicit Orders E2E stages passed.

Authorised live run `20260730t071445z-release040` migrated and verified its
disposable private PostgreSQL database, built the deterministic 5,038,349-byte
native ARM64 artifact with SHA-256
`ff9609127cedcf2aad6c563e1f524feda1258ec33f104f7973eccecaa80ea474`,
sealed exact-main release `minco.0b60a084c8c9029899e8fc27`, and applied the
reviewed change-set receipt. Candidate `GET /health/live` reached Lambda and
returned Minco's request ID but Axum returned 404: pinned `lambda_http 1.3.0`
prefixes named API Gateway stages into the URI unless
`AWS_LAMBDA_HTTP_IGNORE_STAGE_IN_PATH` is set.

The current correction sets that dependency switch in generated Lambda
environment configuration and adds a focused SAM renderer regression. The
checked-in template is regenerated. Exact provider checks prove all
application, database/VPC, managed-secret, bucket, Cognito, SSM, Lambda/API
and bootstrap-IAM resources absent. The tag index temporarily retained three
stale deleted ARNs; direct provider calls for each return not found. No tag or
registry upload occurred.

Replacement live AWS proof, exact tag creation and crates.io publication remain
separate pending gates. Current and historical command evidence is maintained
in `VERIFICATION.md`; Feedback-specific architecture evidence remains in
`FEEDBACK_REVIEW_STATUS.md`.
