---
title: Installation
description: Install the Minco 0.5.0 CLI and add the feature-gated facade.
---

# Installation

Minco 0.5.0 requires Rust 1.97.1. Install the Cargo subcommand at the exact
published version:

```bash
rustup toolchain install 1.97.1 --component clippy,rustfmt
cargo +1.97.1 install cargo-minco --version 0.5.0 --locked
cargo minco --version
```

The final command should print:

```text
minco 0.5.0
```

## Add the framework

Applications normally depend on the feature-gated `minco` facade:

```bash
cargo add minco@0.5.0
```

Add only the capabilities you use:

```bash
# PostgreSQL API on native Lambda
cargo add minco@0.5.0 --features sqlx-postgres,aws-lambda,plan,release,test

# Local or single-process SQLite API
cargo add minco@0.5.0 --features sqlx-sqlite,test

# Provider-neutral core only
cargo add minco@0.5.0 --no-default-features
```

## Verify the environment

From a Minco application:

```bash
cargo minco doctor
cargo minco check --with-cargo
```

Next, [build your first API](./tutorials/first-api).
