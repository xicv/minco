---
title: Inspect ProjectView with MCP and Workbench
description: Read one bounded repository-native project model through JSON, modern MCP stdio, or an accessible local workbench without creating a second source of truth.
---

# Inspect ProjectView with MCP and Workbench

`ProjectView` projects declared repository authorities into one deterministic,
schema-versioned read model. It preserves raw task status and six independent
evidence lanes: source, local verification, hosted verification, deployment,
runtime, and review. Evidence in one lane never upgrades another.

## Validate Without Writing or Listening

```bash
cargo minco mcp --check --json
cargo minco workbench --check --json
```

Both commands report the bounded source digest, limits, diagnostics, and
derived summary without opening a listener, writing output, or contacting a
provider.

## Connect a Modern Read-only MCP Client

Configure a child-process stdio command with an absolute canonical root:

```json
{
  "command": "cargo",
  "args": ["minco", "--root", "/absolute/canonical/project", "mcp"]
}
```

The server supports the MCP `2026-07-28` `server/discover` lifecycle and
per-request `_meta`, while retaining legacy initialization compatibility. Its
six deterministic tools expose summaries, operation explanation, task
readiness, independent evidence, Feedback capability context, and the complete
bounded view. Every tool is annotated read-only, non-destructive, idempotent,
and closed-world; filesystem paths, shell commands, SQL, URLs, secrets,
provider calls, and write grants are absent.

## Export or Serve the Workbench Explicitly

```bash
cargo minco workbench export --format json --output target/workbench-json
cargo minco workbench export --format mermaid --output target/workbench-mermaid
cargo minco workbench export --format static --output target/workbench-static

cargo minco --root /absolute/canonical/project --json workbench serve --port 0
```

Exports are create-only, project-relative, outside canonical inputs, and
installed with a no-clobber boundary after symlink and filesystem-identity
checks. The server binds IPv4 loopback, accepts the exact host, rejects foreign
origins, serves six fixed routes, and writes nothing. Do not proxy or tunnel it.

The accessible workbench provides textual status, keyboard navigation,
small-screen modes, deterministic downloads, and optional browser read-aloud
over already displayed text. Minco calls no speech provider and stores no voice
data.

## Keep the Sources Authoritative

ProjectView is a presentation projection, not a dashboard-owned state machine.
Unknown raw statuses remain unknown with diagnostics. Missing evidence remains
absent. Local source and test results never become hosted, deployment, runtime,
or product-review proof.
