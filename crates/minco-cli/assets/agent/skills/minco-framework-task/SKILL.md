---
name: minco-framework-task
description: >-
  Execute an owning task in the Minco framework repository through its ADR,
  roadmap, task, JJ workspace, test, evidence, and handoff contracts. Use when
  changing the Minco framework itself and an exact dependency-ready task exists;
  never use for an ordinary generated Minco application.
---

# Execute a Minco framework task

First prove this is the Minco framework repository, not a generated application.

1. Read `AGENTS.md`, `docs/DECISIONS.md`, the relevant ADR,
   `roadmap/roadmap.yaml`, and the owning task under `tasks/`.
2. Run `cargo minco task show <id> --json` and confirm ownership, dependencies,
   goals, non-goals, and checks.
3. Refresh the exact base and create one isolated physical JJ workspace with
   the repository's declared task-start workflow.
4. Work only within owned paths. Use one failing observable test followed by
   the minimum implementation for each vertical slice.
5. Update coupled generated evidence and Signal documentation owned by the
   task; never edit `// @generated` files directly.
6. Run the task's focused checks, inspect `@ & conflicts()`, and record exact
   limitations without converting unavailable tools into passes.
7. For release work, prove release skill freshness against the current
   changelog and preserve the local-first release boundary between full local
   qualification and bounded hosted compatibility.
8. Use the declared task-finish/push workflow only when its checks stay within
   the user's authorized validation boundary.

Stop before merge, release, publication, deployment, provider access, or
workspace cleanup unless each action is separately authorized and proven safe.

Read [workflow.md](references/workflow.md) for mode and handoff gates.
