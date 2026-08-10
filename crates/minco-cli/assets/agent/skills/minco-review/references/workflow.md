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

State whether each check was observed, absent, blocked, or not applicable. A
clean local diff is not hosted, deployment, runtime, or review acceptance.
Return no finding when there is no actionable defect; still state test gaps and
unverified external boundaries.
