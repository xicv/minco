# Updating Minco

`minco update` is intentionally conservative. It updates a source workspace; it
does not download an unsigned replacement binary.

## Check mode

```bash
cargo minco update check --json
```

This reports Rustup/toolchain state, Cargo dependency resolution, lockfile
metadata, JJ availability, and deferred actions without writing the source
workspace. Any failed inspection command makes the check fail; locked Cargo
metadata is summarized rather than embedding the complete package graph in the
report.

## Apply mode

```bash
cargo minco update apply \
  --toolchain \
  --dependencies \
  --run-checks \
  --yes
```

Apply mode requires a clean JJ working-copy change (or clean colocated Git tree
when JJ is unavailable), explicit `--yes`, at least one explicitly selected
action, and the complete requested tools. The example installs the pinned
toolchain, updates `Cargo.lock`, and runs validation. If neither JJ nor Git is
available, apply mode fails because it cannot prove workspace cleanliness.

## Review discipline

Dependency updates must be isolated in a JJ task workspace. Review direct and
transitive version changes, security advisories, MSRV/toolchain changes,
generated contracts, deployment plans, artifact size, and database migrations.
A future binary self-update mechanism requires signed releases, verified checksums,
rollback metadata, and a separate ADR before it can become a default action.
