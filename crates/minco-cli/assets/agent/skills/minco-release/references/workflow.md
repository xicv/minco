# Release states

Report these independently:

- source selected and clean;
- local checks passed;
- hosted checks passed for the exact revision;
- artifact and manifest built and digest-verified;
- tag or registry publication completed;
- deployment applied to the named target;
- runtime behavior observed;
- product/reviewer acceptance recorded; and
- rollback and cleanup readiness proven.

Before release, verify release skill freshness: the current changelog section,
feature-to-skill mapping, versioned documentation and packaged skill markers
must agree. Preserve the local-first release boundary and topology-aware cost
review even when a clean hosted compatibility check passes.

Treat versioned documentation presentation as release content: build and check
the exact manual, verify responsive layout at supported viewports, and keep
rendered-page evidence separate from source or package qualification.

Publication is non-atomic. If transport becomes ambiguous, query authoritative
state before retrying. Never place credentials, endpoints, account identifiers,
customer data, or secret values in committed receipts.

At a maintenance release boundary, re-check version-matched documentation,
exact package/tool pins, public-contract compatibility and lane-specific evidence.

At the 1.5 assurance release boundary, bind additive typed fakes and measured
provider-free gates to exact candidate source while leaving model, human-review,
hosted-performance and live-provider evidence at their recorded states.

At the 1.6 durable audit ledger boundary, verify SemVer against the immutable
1.5 baseline, exact transaction/cursor tests, retained-growth and incomplete-
price truth, and separate tag, publication, provider and deployment authority.

At the 1.7 Apple Container default boundary, verify additive SemVer against the
immutable 1.6 baseline and qualify Apple selection, receipt precedence, exact
resource ownership and Docker fallback. Candidate, merge, tag, publication and
runtime proof remain separate release states.

At the 1.8 resumable object transfer boundary, qualify the maximum multipart
manifest, range/cache validators, quarantine and structural cost claims against
the immutable 1.7 baseline. Keep local, hosted, registry, provider, deployment
and production evidence as separate states.
