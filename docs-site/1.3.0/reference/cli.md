---
title: Current CLI
description: Minco 1.3.0 command groups, JSON interfaces, dry-run behavior, and mutation boundaries.
---

# Current CLI

The binary is `cargo-minco`; Cargo exposes it as `cargo minco`. Global options
are `--root PATH` and `--json`.

## Command Groups

| Area | Commands |
|---|---|
| Project | `new`, `doctor`, `check`, `architecture` |
| Local development | `dev` |
| Contract | `contract check`, `sync`, `diff` |
| Generation | `make`, `stubs` |
| Inspection | `inspect`, `explain`, `config`, `cost`, `perf` |
| Coding agents | `agent plan`, `sync`, `doctor`, `context`, `eval` |
| Data | `db plan`, `status`, `verify`, `migrate`, `seed` |
| Plugins | `plugin list`, `add`, `explain`, `doctor`, `init`, `remove`, `enable`, `disable`, `new`, `validate`, `test` |
| Deployment | `deploy`, `destroy`, `package`, `release`, `promote`, `rollback` |
| Compatibility | `update`, `upgrade` |
| Work | `roadmap`, `task`, `vcs` |
| Local interfaces | `mcp`, `workbench` |
| Feedback | `feedback` |

## Machine-Readable Interfaces

Prefer JSON for tooling and AI agents:

```bash
cargo minco inspect --json
cargo minco explain placeOrder --json
cargo minco config schema --json
cargo minco task show M11-T07 --json
cargo minco deploy plan --stdout --json
cargo minco plugin test --all --json
cargo minco agent context --operation placeOrder --json
cargo minco agent eval --target all --json
```

Output is deterministic and bounded. Secret values, secret-reference names,
provider credentials, service values, and customer data do not belong in these
documents.

## Coding Agent Projections

```bash
cargo minco agent plan --target codex|claude|all --json
cargo minco agent sync --target codex|claude|all \
  --expect-plan-digest <sha256> --json
cargo minco agent doctor --target codex|claude|all --json
cargo minco agent context [--operation ID | --task ID] --json
cargo minco agent eval --target codex|claude|all --json
```

Plan is read-only; sync requires the exact current plan digest and writes only
fixed, manifest-owned project paths. Doctor and eval are read-only. Context
reuses the bounded `ProjectView`, rejects path-like selectors, caps output at
64 KiB, and performs no commands, network requests, writes, provider calls, or
database queries. Eval checks installed bytes, cross-client parity, and
scenario contracts without invoking a model.

See [Develop with Codex and Claude Code](../guides/agent-development) for
projection ownership, existing-file preservation, workflow selection, and
authority boundaries.

## Read-Only and Dry-Run Commands

Inspection, contract checks, source-only database planning, and deployment
planning do not contact a provider. Resource and operation generators expose a
plan first:

```bash
cargo minco make resource order --dry-run --json
```

Plugin mutations expose a plan before writing. `plugin add`, `init`, `remove`,
`enable`, `disable`, and `new` accept `--dry-run`; `plugin list`, `explain`,
`doctor`, `validate`, and `test --all` inspect local state. These commands use
the reviewed catalog and statically linked packages. They do not download or
dynamically load unknown code.

## Guarded Mutations

Database migration, seed application, CloudFormation apply, and promotion
require exact environment, identity, digest, and receipt inputs. Their help
text is the authority for the current source checkout:

```bash
cargo minco db migrate --help
cargo minco deploy apply --help
cargo minco promote --help
```

Shell history is not a safe place for credentials or password-bearing database
URLs. Target commands accept the name of an environment variable that contains
the direct URL.

## Task and JJ Workflow

```bash
cargo minco task ready --json
./scripts/jj/task-start.sh M11-T07
cargo minco task verify M11-T07 --json
./scripts/jj/task-finish.sh M11-T07 "docs(site): deepen current documentation" --push
```

One workspace owns one task. A planned task is not ready merely because a
dependency merged.

For exhaustive option-level detail, run `cargo minco COMMAND --help` against
the exact source or release you are using.
