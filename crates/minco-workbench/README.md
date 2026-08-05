# minco-workbench

`minco-workbench` projects Minco's bounded, read-only `ProjectView` into an
optional local dashboard and deterministic export formats. It does not own or
mutate project truth.

The crate supplies three presentation boundaries:

- a non-serving check report;
- deterministic JSON, Mermaid, and self-contained static-directory exports;
- an exact IPv4 loopback HTTP server for the bundled accessible workbench.

The CLI is the supported repository entrypoint. See
[`docs/how-to/local-workbench.md`](../../docs/how-to/local-workbench.md) for
commands, evidence semantics, export safety, and browser verification.

Export is create-only. Callers must provide a canonical absolute project root,
a normalized project-relative destination beneath an existing non-symlink
parent, and the complete set of canonical input paths. The implementation
publishes through a private descriptor-relative staging directory and an atomic
no-clobber rename on supported Apple and Linux targets. Other targets fail
closed instead of using a weaker installation primitive.
