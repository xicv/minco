# Set up Codex and Claude for a Minco application

Minco packages the same version-matched workflow skills for Codex and Claude.
Setup is repository-scoped, explicit and plan-first: creating an application
does not install global skills, rewrite existing client instructions, enable an
MCP server, commit, push, release or deploy anything.

## Inspect the application instructions

A fresh PostgreSQL or SQLite application contains `AGENTS.md` in application
mode. It routes application work to `$minco-web-application`, identifies OpenAPI
as the HTTP source of truth and preserves Minco's architecture, test and
deployment-authority boundaries. These are application instructions; the Minco
framework's JJ task and release workflow does not apply.

Use bounded context instead of asking an agent to infer repository structure:

```text
cargo minco agent context --json
cargo minco agent context --operation getPlatform --json
```

Context reads the schema-versioned ProjectView. It runs no commands or network
requests, performs no writes and does not upgrade source state into test,
hosted, deployment or production evidence.

## Review the exact projection

From the application root, produce a read-only plan:

```text
cargo minco agent plan --target all --json
```

The Codex projection uses `.agents/skills/`; the Claude projection uses
`.claude/skills/`. For Claude, Minco may also plan a three-line `CLAUDE.md` that
imports the application-owned instructions with `@AGENTS.md`.

Review `safe`, every `actions` entry, `conflicts`, `manual_actions` and the
`plan_digest`. In particular:

- a missing destination may be created;
- a file recorded in `.minco/agent-manifest.json` may be updated only when its
  current digest still matches the receipt;
- edited managed files and unsafe path entries are conflicts;
- an existing unowned `CLAUDE.md` stays untouched and produces a manual action;
  and
- a missing or unsafe `AGENTS.md` prevents Minco from creating a dangling
  Claude import.

## Apply only the reviewed plan

With approval for those exact repository writes, bind sync to the reported
digest:

```text
cargo minco agent sync \
  --target all \
  --expect-plan-digest <sha256> \
  --json
```

Sync recomputes the plan and refuses stale or ambiguous state. It owns only the
files listed in the receipt and never deletes neighboring files.

Re-run discovery and drift diagnosis without writing:

```text
cargo minco agent doctor --target all --json
```

MCP configuration deliberately remains manual and user-owned. Review the
client's project configuration before adding Minco's local read-only MCP server;
skill synchronization does not grant mutation authority to that server or to
an agent session.
