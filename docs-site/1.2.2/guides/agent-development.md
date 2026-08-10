---
title: Develop with Codex and Claude Code
description: Install version-matched Minco skills, inspect bounded project context, and preserve explicit mutation authority.
---

# Develop with Codex and Claude Code

Minco 1.2.2 packages the same eight focused application workflows for Codex
and Claude Code. The skills select Minco commands and source authorities; they
do not add a hosted agent runtime or replace the application graph. The 1.2.2
candidate retains all eight across the complete 1.2 product boundary without
changing their names, triggers, projection paths or mutation authority.

## Start with a read-only plan

From a generated or existing Minco application:

```bash
cargo minco agent plan --target all --json
```

Review the fixed project-relative destinations and retain the returned
`plan_digest`. Planning writes nothing. It classifies files as creates,
unchanged managed files, safe managed updates, conflicts, or manual actions.

Install only the exact reviewed plan:

```bash
cargo minco agent sync \
  --target all \
  --expect-plan-digest <sha256> \
  --json
```

Sync rechecks the project and destination identities before every write. A
stale digest, edited managed file, unowned destination, symlink, non-regular
file, or path race fails closed. Neighboring files are never deleted.

## Client projections

| Client | Managed project paths |
|---|---|
| Codex | `.agents/skills/minco-*/` |
| Claude Code | `.claude/skills/minco-*/` |
| Both | `.minco/agent-manifest.json` ownership receipt |

For Claude, Minco may also create a three-line `CLAUDE.md` that imports
`@AGENTS.md`. It does so only when `AGENTS.md` is a regular file and the Claude
file is absent or already Minco-managed. An existing user-owned `CLAUDE.md` is
preserved byte-for-byte and reported as a manual action.

Minco does not parse or rewrite `.mcp.json`, `.codex/config.toml`, global client
settings, or non-Minco skills.

## Choose a focused workflow

The package contains eight skills:

- web-application routing;
- OpenAPI-first operations;
- statically linked plugins;
- explicit application lifecycle;
- bounded diagnosis;
- evidence-aware review;
- framework contribution through the owning task and JJ workspace; and
- explicitly requested release preparation.

Generated applications receive application guidance. They do not inherit the
Minco framework repository's internal task, JJ, or release policy.

## Inspect bounded context

Use the project model instead of asking an agent to infer hidden structure:

```bash
cargo minco agent context --json
cargo minco agent context --operation placeOrder --json
cargo minco agent context --task M0-T01 --json
```

The response is schema-versioned and capped at 64 KiB. It reuses the bounded
`ProjectView` allowlist and reports zero writes, child commands, network
requests, and arbitrary file reads. Unknown IDs return `found: false` with a
stable diagnostic; Minco does not guess.

## Diagnose and evaluate

```bash
cargo minco agent doctor --target all --json
cargo minco agent eval --target all --json
```

Doctor reports discovery, exact version, ownership, drift, projection parity,
and manual MCP configuration state without modifying the project. Eval checks
portable skill format, exact installed bytes, Codex/Claude parity, and 16
positive/negative workflow contracts. It performs no writes, commands, network
requests, or model invocations.

A passing evaluation is local projection evidence. It is not model quality,
hosted CI, review, deployment, runtime, or production proof.

## Release skill freshness

Every release maps each top-level changelog note to a stable product feature,
current versioned documentation and the packaged skills that must teach it.
Rust bundle tests and static mutation tests reject stale changelog digests,
missing skill markers, unknown mappings, documentation escapes and incomplete
coverage. Deterministic workflow qualification then checks the exact canonical
receipt without writing during verification.

This is a release-content gate, not a model-quality score. It does not download
mutable skills or grant release, registry, deployment or production authority.

## Authority remains explicit

Skills can explain commands, but they never authorize commit, push, merge,
release, registry publication, deployment, database mutation, provider access,
promotion, or production feature enablement. Those actions keep their ordinary
Minco evidence and user-approval gates.

For the underlying read model, see [ProjectView, MCP, and the workbench](./project-view).
For exact command syntax, see the [CLI reference](../reference/cli).
