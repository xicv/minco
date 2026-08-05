# Framework task gates

Required before mutation:

- repository identity is the Minco framework;
- exact base and workspace state are known;
- the task is ready/active and dependencies are complete;
- owned paths include source, tests, docs and coupled evidence; and
- current ADR and compatibility boundaries are understood.

Required before handoff:

- focused task checks pass;
- no conflict touches the task change;
- evidence names the exact revision and lane;
- unavailable or broader user-disallowed gates are recorded literally; and
- the pushed bookmark contains no unrelated workspace changes.

A task implementation, push, PR, hosted check, merge, release, publication,
deployment, and runtime proof are separate states.
