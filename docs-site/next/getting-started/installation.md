---
title: Installation
description: Install the Minco toolchain and choose a small, explicit framework feature set.
---

# Installation

Minco uses ordinary Rust packages plus the `cargo minco` control plane. The
published baseline is 1.3.0. Minco 1.3.0 and current development require Rust
1.97.1. Use exact 1.3.0 packages for applications and the repository-pinned
workspace binary only when deliberately evaluating unreleased source.

## Install the CLI

```bash
rustup toolchain install 1.97.1 --component clippy,rustfmt
cargo +1.97.1 install cargo-minco --version 1.3.0 --locked
cargo minco --version
```

The last command should print `minco 1.3.0`. Contributors reviewing unreleased
source use the repository-pinned toolchain and workspace binary:

```bash
git clone https://github.com/xicv/minco.git
cd minco
cargo minco --version
```

## Add Minco to an application

The facade's defaults are deliberately small: OpenAPI contracts, HTTP
conventions, health, observability, and idempotency.

```bash
cargo add minco@1.3.0
```

Add only the capabilities the application needs:

```bash
# PostgreSQL API running on native Lambda
cargo add minco@1.3.0 --features sqlx-postgres,aws-lambda,plan,release,test

# Local or single-process SQLite API
cargo add minco@1.3.0 --features sqlx-sqlite,test

# Provider-neutral kernel only
cargo add minco@1.3.0 --no-default-features
```

See [Cargo feature flags](../reference/feature-flags) before enabling `full`.
Features compile code; they do not create cloud resources or discover plugins
at runtime.

## Verify the environment

Run these from an application containing `minco.toml`:

```bash
cargo minco doctor
cargo minco config check
cargo minco contract check
cargo minco check --with-cargo
```

`doctor` checks tools and project structure. `check` adds framework validation;
`--with-cargo` makes compiler evidence explicit rather than treating static
inspection as compilation.

## Optional tools

- Docker is needed for the local PostgreSQL and Rustack topology, not for an
  SQLite-only application.
- Jujutsu is the recommended task workflow; generated projects initialize a
  colocated JJ/Git repository unless `--vcs none` is selected.
- AWS credentials are needed only for an explicitly authorized provider-backed
  command. Planning, rendering, local testing, and Rustack do not require them.

Next: [build your first application](./first-application).
