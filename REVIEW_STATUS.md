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

The stage-environment correction passed exact-head hosted run `30526281458` at
`d5b4a76946a47bb4aeffb8be64b7460e1e61ce2d`, merged as exact `main`
`83d1583e9a385070306c95665a5219700cbc1c5e`, passed the complete local
qualification and passed exact-main hosted run `30527357088`. All
authoritative quality, browser, 28-package dry-run, Plan/SAM/native ARM64,
Rustack/SSM and explicit Orders E2E stages passed.

Authorised live run `20260730t085318z-release040` migrated and verified its
disposable private PostgreSQL database, reproduced the deterministic
5,038,349-byte native ARM64 artifact with SHA-256
`ff9609127cedcf2aad6c563e1f524feda1258ec33f104f7973eccecaa80ea474`,
sealed exact-main release `minco.faf23ae016624d15d0b8f11f`, applied reviewed
change-set receipt
`3d349a2be71b1aa04491f61f388780bb5c8d973e756aa4296c388103a8f27443`,
and reached `CREATE_COMPLETE`. Candidate `GET /health/live` still reached
Lambda and returned Minco request ID
`1dcc9a69-cae5-4c68-ba8e-bac9fec24128`, but Axum returned an empty 404.

The live event refines the root cause: API Gateway v2 already places
`/candidate` in `rawPath`, so
`AWS_LAMBDA_HTTP_IGNORE_STAGE_IN_PATH` leaves the prefixed path unchanged. The
current correction normalizes the exact non-default API Gateway context stage
in `minco-aws-lambda` before Axum route matching, preserves authority/query,
rejects prefix lookalikes and leaves `$default` unchanged. A realistic event
regression reaches the contract-owned `/health/live` route in-process. The
ineffective SAM environment setting is removed. No promotion, tag or registry
upload occurred. The application cleanup receipt is all true; a bounded
follow-up check after AWS's asynchronous deletion window also proves the exact
temporary PostgreSQL stack, instance, managed secret and VPC absent, with
synthetic data and local database secret files absent. The bootstrap user, role,
profiles and credential files are absent.

Replacement live AWS proof, exact tag creation and crates.io publication remain
separate pending gates. Current and historical command evidence is maintained
in `VERIFICATION.md`; Feedback-specific architecture evidence remains in
`FEEDBACK_REVIEW_STATUS.md`.

The named-stage correction passed PR-head hosted run `30532832860` at
`d7e5a1c6e9ff5f5c43c754bc145bdefd63c7b60e`, merged as exact `main`
`73807d918bc860b60d592611f388bb63775d7c54`, and passed both the complete
local qualification and exact-main hosted run `30534601227`.

Authorised live run `20260730t104626z-release040` then migrated and verified
private PostgreSQL, sealed exact release `minco.789c2425846acb0fda2039f0`,
and applied its reviewed change set. Candidate liveness and readiness passed;
the protected-order probe returned the expected 401 with API Gateway header
`apigw-requestid`. The verifier rejected that valid provider request ID because
it recognized only `x-request-id` and `x-amzn-requestid`, so no authenticated
mutation or promotion ran. Application cleanup is all true. A bounded exact
rerun after the asynchronous RDS secret window proves the temporary database,
managed secret and VPC absent; bootstrap principals, profiles and credentials
are also absent.

The current correction centralizes response request-ID extraction and adds
executable positive fixtures for all three supported provider/application
headers plus a negative unrelated-header fixture. Exact-head hosted
qualification, merge, exact-main qualification and another live rehearsal are
required before tag creation or crates.io publication.
