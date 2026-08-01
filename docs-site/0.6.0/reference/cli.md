---
title: CLI Reference
description: Minco 0.6.0 command groups, JSON interfaces, dry-run behavior, and mutation boundaries.
---

# CLI Reference

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
| Data | `db plan`, `status`, `verify`, `migrate`, `seed` |
| Plugins | `plugin list`, `enable`, `disable`, `new`, `validate`, `test` |
| Deployment | `deploy`, `package`, `release`, `promote` |
| Compatibility | `update`, `upgrade` |
| Work | `roadmap`, `task`, `vcs` |
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
```

Output is deterministic and bounded. Secret values, secret-reference names,
provider credentials, service values, and customer data do not belong in these
documents.

## Read-Only and Dry-Run Commands

Inspection, contract checks, source-only database planning, and deployment
planning do not contact a provider. Resource and operation generators expose a
plan first:

```bash
cargo minco make resource order --dry-run --json
```

`plugin new` writes a local plugin skeleton immediately; preview and dry-run
support for plugin mutations is planned work. `plugin list`, `validate`, and
`test --all` inspect local packages and never download or execute unknown code.
The broader add, init, explain, doctor, and safe remove workflows also remain
planned and are not documented as implemented.

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
