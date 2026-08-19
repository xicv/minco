---
title: Identity and Sessions
description: Compose verified claims, authorization policy, and revocable sessions without hiding the provider boundary.
---

# Identity and Sessions

Identity and session management are separate plugins. Identity maps verified
provider claims into application principals, scopes, and permissions. Sessions
issue and revoke provider-neutral session records. Business authorization stays
inside application use cases.

Minco does not provide a login or sign-in screen and does not replace an
identity provider. The selected ingress or provider completes authentication;
Minco maps its verified claims and applies application-owned session and
authorization policy.

## Enable the components

```bash
cargo minco plugin enable identity --dry-run --json
cargo minco plugin enable sessions --dry-run --json
```

Review the plan, apply it without `--dry-run`, then diagnose the compile-time and
runtime selection:

```bash
cargo minco plugin enable identity --json
cargo minco plugin enable sessions --json
cargo minco plugin doctor --json
```

The corresponding facade features are `plugin-identity` and
`plugin-sessions`. The commands never download or dynamically load code.

## Keep verification explicit

The identity plugin consumes claims that a selected ingress or provider adapter
has already verified. For native AWS HTTP deployment, API Gateway JWT authorizer
claims are the production input. Development headers are a separate local-only
policy and must be disabled by default.

Do not accept an unsigned client payload merely because it resembles the
principal shape.

## Authorize in the use case

Handlers extract a principal and call one application use case. The use case
checks the required permission before validation that might reveal protected
state and before any persistence port is called.

```text
verified claims -> Principal -> application authorization
                              -> domain rule
                              -> use-case-shaped port
```

This keeps authorization consistent across HTTP, worker, test, and future
delivery adapters.

## Session lifecycle

The sessions plugin defines issuance, lookup, expiry, and revocation ports. The
application selects a concrete store and clock. It must decide cookie policy,
token transport, retention, concurrent-session limits, and revocation behavior.

Session state can create storage cost at idle even when application compute
scales to zero. Declare retention and the selected adapter in deployment
evidence.

## HTTP policy

Installed HTTP plugins contribute only their exact request, exposed, and
sensitive headers. CORS remains exact; wildcard origins or headers fail
configuration. Sensitive authorization, cookie, session, and development
identity material must be redacted from logs and diagnostics.

## Test the boundary

- application tests: missing/insufficient permissions fail before ports;
- HTTP tests: missing, malformed, expired, and valid principals map to stable
  Problem/status responses;
- session adapter tests: expiry and revocation use a controlled clock;
- production smoke: only the configured issuer/audience and ingress path are
  accepted.

Local fake claims are not provider verification evidence.
