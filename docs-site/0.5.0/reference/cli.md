---
title: CLI reference
description: Stable Minco 0.5.0 command groups and machine-readable interfaces.
---

# CLI reference

The installed binary is `cargo-minco`; Cargo exposes it as `cargo minco`.
Global options are `--root PATH` and `--json`.

| Area | Commands |
|---|---|
| Project | `new`, `doctor`, `check`, `architecture`, `inspect`, `explain` |
| Contract | `contract check`, `contract sync`, `contract diff` |
| Generate | `make module`, `operation`, `resource`, `migration`, `seeder`, `worker`, `adapter`, `test`, `plugin` |
| Configure | `config check`, `explain`, `diff`, `schema` |
| Database | `db plan`, `status`, `migrate`, `verify`, `seed` |
| Deploy | `deploy plan`, `render-sam`, `changeset`, `apply`, `verify`, `promote` |
| Evidence | `cost`, `perf`, `package`, `release create`, `release verify` |
| Plugins | `plugin list`, `enable`, `disable`, `new`, `validate` |
| Work | `roadmap status`, `task ready`, `task show`, `vcs task-start`, `vcs task-finish` |

## Machine-readable inspection

```bash
cargo minco inspect --json
cargo minco explain placeOrder --json
cargo minco task show M11-T01 --json
cargo minco deploy plan --stdout --json
```

These interfaces expose bounded metadata, identities, and diagnostics. They do
not expose service values, credentials, secret-reference names, or customer
data.

For every option and safety boundary, use the
[complete 0.5.0 CLI reference](https://github.com/xicv/minco/blob/v0.5.0/docs/reference/cli.md).
