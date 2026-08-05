# Connect a local read-only MCP client

Minco's MCP server gives a local agent the same bounded repository-native
`ProjectView` used by the CLI. It is a child process, uses newline-delimited
JSON-RPC over stdin/stdout, and opens no TCP or HTTP listener.

## Validate the view first

From a Minco project, run:

```bash
cargo minco mcp --check --json
```

The check does not start a server or contact a provider. It reports the source
digest, input usage, explicit limits, derived task summary, six tool names,
`transport: "stdio"`, `read_only: true`, and `listening_sockets: 0`.

Resolve the project path before configuring a client:

```bash
PROJECT_ROOT="$(pwd -P)"
cargo minco --root "$PROJECT_ROOT" mcp
```

The second command is a protocol server, so an interactive terminal appears to
wait for input. Minco requires the explicit `--root` on this serving path and
reserves stdout exclusively for MCP messages.

## Configure the client

Use a child-process stdio entry equivalent to:

```json
{
  "command": "cargo",
  "args": ["minco", "--root", "/absolute/canonical/project", "mcp"]
}
```

Run the command from a checkout that provides the `cargo minco` subcommand, or
install the matching `cargo-minco` version. Do not wrap it in a shell command or
pass secrets through its arguments.

## Choose the narrowest tool

The catalog is deterministic and contains exactly:

| Tool | Input | Result |
| --- | --- | --- |
| `minco.project_summary` | none | identity, derived summary, limits, input usage, diagnostics |
| `minco.operation_explain` | exact `operation_id` | one declared OpenAPI operation or `found: false` |
| `minco.task_readiness` | optional exact `task_id` | one or all derived readiness records |
| `minco.evidence` | optional six-value `lane` | one or all independent evidence lanes |
| `minco.feedback_context` | none | Feedback capability metadata and operation IDs only |
| `minco.project_view` | none | the complete bounded schema-versioned view |

Prefer a narrow tool when the complete graph is unnecessary. All results use
schema version 1 and structured JSON. A complete response that would exceed
the configured protocol-response limit fails closed; it is never truncated
into ambiguous JSON.

Every tool advertises read-only, non-destructive, idempotent, closed-world
annotations. Inputs have closed schemas. Filesystem paths, shell commands, SQL,
URLs, credentials, provider calls and write grants are not part of the tool
surface.

## Interpret evidence without upgrading it

The six lanes are `source`, `local_verification`, `hosted_verification`,
`deployment`, `runtime`, and `review`. Evidence in one lane does not prove a
different lane. An explicit `absent` item means that the repository snapshot did
not provide that evidence; it does not mean success or failure.

Raw roadmap statuses remain unchanged. The semantic display mapping is explicit
and derived, and an unknown raw status remains `unknown` with a diagnostic.

## Security and limit behavior

The reader follows only paths declared by `minco.toml` plus a fixed allowlist of
local verification records. It rejects traversal, absolute declared paths,
symlink components (including dangling optional evidence links), oversized
directory scans, files, aggregate input, graph sizes, text and responses.
Secret configuration defaults are represented as `redacted`; local override
files, environment variables, feedback records, attachments, databases and
provider services are not read.

The stdio transport accepts at most 256 KiB per newline-delimited client
message. The ProjectView response budget is 2 MiB and reserves space for the
request identity and MCP envelope. Any future network transport or write tool
requires a separate design and security review.

The transport and tool behavior follow the official
[MCP 2026-07-28 transport](https://modelcontextprotocol.io/specification/2026-07-28/basic/transports)
and [server tools](https://modelcontextprotocol.io/specification/2026-07-28/server/tools)
contracts.
