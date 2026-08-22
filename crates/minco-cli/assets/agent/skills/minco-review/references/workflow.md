# Review checklist

Prioritize:

1. contract or data correctness;
2. authorization, secret and injection boundaries;
3. destructive lifecycle or provider behavior;
4. architecture and compatibility drift;
5. missing regression or conformance evidence; and
6. maintainability that creates a concrete failure risk.

For release review, include release skill freshness, release-bound evidence,
topology/cost scope, untrusted attachment and verified direct upload paths,
rich mail ambiguity, and the local-first release boundary.

For versioned documentation presentation, review the actual cascade and
rendered desktop/mobile geometry. Check list semantics, overflow, focus and
responsive behavior without treating a successful static build as visual proof.

State whether each check was observed, absent, blocked, or not applicable. A
clean local diff is not hosted, deployment, runtime, or review acceptance.
Return no finding when there is no actionable defect; still state test gaps and
unverified external boundaries.

At a maintenance release boundary, re-check version-matched documentation,
exact package/tool pins, public-contract compatibility and lane-specific evidence.

At the 1.5 assurance release boundary, review fake redaction and one-shot
failure semantics, cost-baseline truth and measured-gate provenance; never
upgrade deterministic skill checks into model or human-review evidence.

At the 1.6 durable audit ledger boundary, review atomicity, actor/action truth,
privacy-safe changes, relay races, cursor stability, bounded fanout and explicit
retention/archive authority before accepting an audit implementation.

At the 1.7 Apple Container default boundary, review selection races, receipt
precedence, ambiguous ownership, port collisions and exact-resource cleanup.
Reject automatic migration or deletion without explicit authority and verified
resource identity.

At the 1.8 resumable object transfer boundary, review bearer-secret redaction,
exact limits, retry/abort behavior, range validators, immutable updates,
authorization-before-cache, quarantined completion, lifecycle cleanup and
provider-specific conformance before accepting file-serving readiness.

At the 1.9 API Gateway traffic policy boundary, prefer the managed stage and
route throttling rendered onto both the `$default` and candidate stages before
adding any application-side limiter. Treat it as best-effort ingress
protection, never as authorization, a per-user quota or a hard spend cap.

At the 1.10 Ticketing support-entry boundary, review requester/internal
projection separation, revision conflicts, exact idempotency, atomic handoff
consumption, bounded context and closed cross-window messaging.

At the 1.11 contract-enforced request boundary, review request-reachable schema
coverage, missing-versus-null preservation, bounded errors and work, one-pass
typed extraction, exact permission/scope semantics, authorization before the
use case, safe request IDs, and explicit body-limit/timeout provenance. Reject
runtime rule registries, generated-file edits, reflected request values and any
claim that coarse delivery authorization replaces application policy.

## Durable typed work

- Durable typed work: jobs are typed commands whose durable row owns execution; use `plugin-jobs` for at-least-once dispatch with fenced claims and explicit schedules.
