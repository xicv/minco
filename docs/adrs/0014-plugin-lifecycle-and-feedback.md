# ADR 0014: Typed plugin finalization and first-class feedback loops

Status: Accepted

## Context

GarmentIQ and CGSP required independently composed health checks, identity,
sessions, storage, events/outbox, notifications, audit, and client-review
workflows. A single-binding service registry was insufficient for aggregate
registries, while runtime plugin discovery or a global service locator would
weaken Rust's compile-time guarantees.

Client review also depended on external chat and manual screenshot/task handoff,
which made requirement clarification difficult to trace and slow to turn into
AI-ready development context.

## Decision

1. Keep plugins statically linked and explicitly registered.
2. Separate authoritative single services from ordered multi-contributions.
3. Validate plugin configuration and the full application graph before install.
4. Run a deterministic, side-effect-free finalization pass after all installs.
5. Ship Feedback as an official stable plugin with a committed OpenAPI contract,
   embeddable widget, screenshot/file/voice capture, threaded clarification,
   explicit workflow states, persistence adapters, notifications/audit/events,
   and deterministic AI export.
6. Keep provider adapters explicit; memory implementations are references and
   must not be represented as production durability.
7. Identity administration and static-site publication are provider-neutral
   typed ports. Their memory implementations exist only for deterministic
   composition tests and local orchestration; `minco-aws-adapters` supplies
   Cognito and S3/CloudFront as explicit production selections.
8. Feedback stability covers its public API, project isolation, authorization,
   data handling, and operational contract. Independently selected provider
   plugins and adapters retain their own catalog stability labels.
9. M6-T05 may use a release-scoped owner waiver when the external Deep Security
   Scan repeatedly terminates an authorized defensive review without canonical
   artifacts. The waiver is not a scan pass: partial candidates must be
   manually validated, the independent local security matrix must pass, and
   the exception does not carry to a later release.

## Consequences

- Aggregate registries such as readiness can be assembled without load-order
  hacks or a global locator.
- Plugin descriptors are more detailed and configuration mistakes fail early.
- Feedback can be embedded in any frontend without adopting a frontend stack.
- Larger multipart limits become explicit HTTP-module contributions.
- Production deployments must select durable storage, notification, audit, and
  transcription adapters and establish retention/privacy policy.
- Provider-specific AWS adapters can evolve without changing core or Feedback.
- A failed external assurance service remains visible as accepted residual risk;
  it cannot silently become a no-findings result.
