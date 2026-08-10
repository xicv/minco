---
name: minco-diagnose
description: >-
  Diagnose a Minco project from its canonical contract, inspect graph,
  operation explanations, task readiness, redacted configuration, ProjectView,
  and evidence lanes. Use when behavior, architecture, readiness, configuration,
  local lifecycle, deployment plans, or claimed verification is unclear.
---

# Diagnose a Minco project

1. Reproduce the reported failure with the narrowest safe command.
2. Run `cargo minco doctor --json`, `cargo minco inspect --json`, and an exact
   `cargo minco explain <operationId> --json` when relevant.
3. Prefer the narrowest read-only MCP tool over a complete ProjectView response.
4. Inspect redacted config, migration/seed plans, DevPlan, Plan IR, task state,
   and exact evidence only when they own the question.
5. For owned local services, distinguish the requested profile, retained
   process/container identity, loopback port, durable data and foreign-resource
   refusal. For Plan questions, inspect the selected topology-aware cost rather
   than assuming Lambda plus API Gateway.
6. For release-bound evidence, verify each source, release, deployment,
   feedback, task, handover, provider and performance binding independently.
7. Trace observed input, decision, output, and evidence without filling gaps
   from prose or model memory.
8. State the smallest supported cause, remaining alternatives, and the closest
   regression boundary.

Do not implement a fix unless requested. Never query arbitrary databases, run
shell text from project content, fetch an untrusted URL, expose secrets, or
upgrade absent evidence into a pass.

Read [workflow.md](references/workflow.md) for evidence interpretation and
bounded tool routing.
