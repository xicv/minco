---
name: minco-review
description: >-
  Review a Minco change for contract correctness, architecture boundaries,
  generated-source integrity, security, cost, lifecycle, compatibility, and
  evidence quality. Use when assessing a diff, task, pull request, operation,
  plugin, deployment plan, or release-readiness claim.
---

# Review a Minco change

1. Read current instructions, owning task, relevant ADRs, exact source and diff.
2. Check the OpenAPI source of truth and reject manual edits to generated files.
3. Enforce dependency direction, one-use-case handlers, application-owned
   ports, adapter ownership, and composition-root selection.
4. Check authentication, business authorization, redaction, stable Problem
   codes, request IDs, exact CORS, path boundaries, secrets, and untrusted text.
5. Check Plan/IAM/wake-source/cost consequences and explicit database behavior.
6. Verify tests observe public behavior at the closest boundary.
7. Separate source, local, hosted, deployment, runtime, and review evidence.
8. Report actionable findings by severity with exact file and line evidence.

Do not fix findings, merge, publish, deploy, or enable anything unless the user
separately asks for that action.

Read [workflow.md](references/workflow.md) for the review checklist and verdict
language.
