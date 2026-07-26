# CLI Reference

The published binary is `cargo-minco`; Cargo exposes it as `cargo minco`. The
workspace also defines a local `cargo minco` alias. Global options are
`--root PATH` and `--json`.

## Create an application

```text
cargo minco new <name> [--directory PATH] [--database postgres|sqlite] [--vcs jj|none]
```

The default creates a colocated JJ/Git repository and a layered workspace with
domain, application, adapter, API, and composition crates. `new` is the only
command that does not require an existing `minco.toml`.

## Repository and quality

```text
cargo minco doctor
cargo minco check [--with-cargo] [--with-optional]
cargo minco architecture
cargo minco inspect
cargo minco explain <operationId>
```

`cargo minco inspect --json` includes bounded registration provenance for the
manifest-selected, statically linked plugin graph. Service records contain only
Rust type and application/plugin owner; contribution records add deterministic
installation indices. Registered values, configuration values and provider
diagnostics are not emitted.

## Contract

```text
cargo minco contract check
cargo minco contract sync [--check]
```

## Deployment, cost and performance

```text
cargo minco deploy plan [--config PATH] [--output PATH]
cargo minco deploy render-sam [--config PATH] [--output PATH]
cargo minco cost [--config PATH]
cargo minco perf [--config PATH]
```

## Plugins

```text
cargo minco plugin list
cargo minco plugin enable <id>
cargo minco plugin disable <id>
cargo minco plugin new <id>
cargo minco plugin validate
```

## Tests

```text
cargo minco test unit
cargo minco test feature
cargo minco test e2e
cargo minco test all
```

## Roadmap and tasks

```text
cargo minco roadmap status
cargo minco roadmap render [--format mermaid|json] [--output PATH]
cargo minco task list
cargo minco task ready
cargo minco task next
cargo minco task show <id>
cargo minco task graph [--output PATH]
cargo minco task verify <id>
```

## Database and release

```text
cargo minco db migrate
cargo minco release create --artifact PATH [--plan PATH] [--output PATH]
cargo minco release verify <manifest>
```

`cargo minco db migrate` executes the application-relative
`commands.database_migrate` entry from `minco.toml`. The application owns its
adapter-specific environment contract—for example a pooled/direct PostgreSQL
URL pair or a persistent SQLite path—rather than the CLI guessing database
environment variables.

## Update

```text
cargo minco update check
cargo minco update apply --yes
```

Apply mode updates the pinned toolchain and dependencies and reruns checks. A
clean JJ working-copy change is required.

## Jujutsu VCS

```text
cargo minco vcs init
cargo minco vcs status
cargo minco vcs task-start <id> [--destination PATH]
cargo minco vcs task-finish <id> --message TEXT [--push]
```

`task-start` creates the new task change on top of the current change. Run it
from the reviewed prerequisite workspace when tasks must remain ordered.
