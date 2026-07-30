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

The fresh-key correction passed exact PR-head hosted run `30499941916` at
`579e240328b3415dd8a839535c2efd8dbc6fcd40`, merged as
`fbba94496e14fce0629efef78d5bee4f71aa132a`, passed the full local suite and
AWS Plan/SAM validation, and passed exact-main hosted run `30500931722`.
Authorised replacement run `20260730t001031z-approved` proved the corrected
bootstrap retry, private PostgreSQL migration and verification, deterministic
native ARM64 artifact, exact-source release manifest and digest-approved
application change set. Both API Gateway V2 stages then rolled back because
CloudFormation's dependent authorization was evaluated as
`apigateway:TagResource` on `/apis/${ApiId}/stages`, while the specialized
statement still named `apigateway:PUT`.

The current unmerged correction changes only that specialized action to the
provider-evaluated `apigateway:TagResource`. The exact stage-collection ARN,
three run-ownership request-tag values and closed ten-key allowlist are
unchanged. IAM custom-policy simulation returns `allowed` for that exact
action/resource pair, although Access Analyzer currently returns one stale
`INVALID_ACTION` finding for the same literal action. The bootstrap accepts
only that exact finding at the exact structurally verified statement; every
other Analyzer error remains fatal. Focused regressions fail for an additional
error, a different finding location, any broader tagging resource or an
additional wildcard action. Application cleanup is all true; the second exact
RDS verifier confirms the delayed managed secret, instance, stack, VPC, local
secret files and synthetic data are absent. Independent exact-name checks also
confirm the application stack, artifact bucket and bootstrap user/role are
absent.

Exact candidate `d9c2e541889aec007038bfe12cd60114ff863317`
passed the authoritative quality and browser stages of hosted run
`30504351107`, then its unpacked `minco-dev` archive test treated a terminated
Linux zombie as a live descendant because the fixture used `kill -0`.
Supervisor cleanup already waits for descendant-held log pipes to close, but
`kill -0` reports a zombie PID as present until the runner's process reaper
collects it. The fixture now inspects portable Unix process state and treats
only non-zombie processes as running; both descendant-shutdown cases use the
same helper. The nine-test supervisor suite and 100 repeated focused runs pass
locally. A new exact-head hosted run remains required before merge.

Replacement live AWS proof, exact tag creation and crates.io publication remain
separate pending gates. Current and historical command evidence is maintained
in `VERIFICATION.md`; Feedback-specific architecture evidence remains in
`FEEDBACK_REVIEW_STATUS.md`.
