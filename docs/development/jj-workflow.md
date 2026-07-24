# Jujutsu-First Development Workflow

Minco uses Jujutsu (`jj`) as its default version-control interface. The default
repository is **colocated**: `.jj/` provides Jujutsu's operation/change model and
`.git/` preserves GitHub and tool compatibility.

## Why JJ

Jujutsu automatically snapshots the working copy, tracks change identity across
rewrites, records repository operations, represents conflicts as first-class
state, and supports multiple working copies backed by one repository. Minco
uses those properties to isolate tasks without serialising all development
through one mutable branch.

## Initialise

```bash
./scripts/jj/init.sh
# equivalent CLI path
cargo minco vcs init
```

Use `jj` for mutating operations. Read-only Git commands remain acceptable for
tools that require Git metadata. Avoid mixing mutating Git and JJ commands;
colocation imports/exports refs automatically but detached Git HEAD and divergent
changes are otherwise easy to misunderstand.

## One workspace per task

```bash
cargo minco task ready
./scripts/jj/task-start.sh M3-T01
```

The command creates a sibling workspace such as `../minco-task-m3-t01`, gives it
a dedicated working-copy change, and describes the task. A second task can use a
second workspace while tests run in the first.

Suggested workflow inside the task workspace:

```bash
jj status
jj diff
jj describe -m 'feat(postgres): implement adapter conformance'
./scripts/quality.sh
jj bookmark set task/m3-t01 -r @
jj git push --bookmark task/m3-t01
```

## Conflicts

Conflicts may remain in commits and be resolved later. Prefer creating a new
working-copy change above the conflicted change, resolve only the required
sections, inspect with `jj diff`, then `jj squash` the resolution into the
conflicted change. `jj resolve` may invoke an external merge tool.

Before merging or deploying, Minco requires a conflict-free selected release:

```bash
jj log -r 'conflicts()'
```

A non-empty result blocks the quality/release path.

## Operations and recovery

```bash
jj op log
jj undo
jj op restore <operation-id>
```

The operation log is the preferred recovery mechanism for accidental rewrites or
mutating Git commands. In a workspace made stale by another workspace rewrite,
run `jj workspace update-stale`.

## Deployment identity

Release manifests record the immutable commit ID, not a mutable bookmark name.
Bookmarks are used for GitHub collaboration and release selection; the artifact,
contract, migrations, plan, lockfile, and toolchain hashes remain the promotion
boundary.
