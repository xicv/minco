---
name: minco-web-application
description: >-
  Orchestrate a complete Minco web application change across contract, domain,
  application, adapters, HTTP, local lifecycle, frontend integration, and
  evidence. Use when building or substantially extending a web application that
  is already a Minco project or should be created with Minco.
---

# Build a Minco web application

Start from project truth and route each slice to the focused Minco skill.

## Establish the mode

1. Run `cargo minco doctor --json` and `cargo minco inspect --json`.
2. If no Minco manifest exists, discuss `cargo minco new` before writing files.
3. Treat an ordinary generated application as application mode. Do not apply
   Minco framework task, JJ, publication, or repository-release instructions.
4. Use `$minco-framework-task` only when the project is the Minco framework and
   an owning task is dependency-ready.

## Build vertical slices

1. Confirm the user-visible journey and operation boundaries.
2. Use `$minco-operation` for each external API operation.
3. Use `$minco-plugin` only for a reusable statically linked capability.
4. Use `$minco-lifecycle` for configuration, migrations, seeds, local services,
   frontend startup, and verification.
5. Re-run `cargo minco inspect --json` and exact operation explanations.
6. Report source, local, hosted, deployment, runtime, and review evidence
   separately.

Stop before commit, push, release, publication, deployment, provider access, or
production enablement unless the user explicitly requests that separate action.

Read [workflow.md](references/workflow.md) when decomposing a multi-slice web
application request.
