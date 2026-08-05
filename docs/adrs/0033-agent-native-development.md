# ADR 0033: Version-matched agent-native development projections

## Status

Accepted

## Context

Minco 1.0 already exposes stable repository paths, JSON CLI commands, an
OpenAPI-to-application graph, task readiness, `ProjectView`, six independent
evidence lanes, a local read-only MCP server and the optional workbench. These
surfaces let an agent inspect the framework without inventing hidden runtime
structure, but a developer still has to teach each coding client how to choose
the right Minco workflow and commands.

Research on 2026-08-05 compared the current Encore AI integration and skills,
the SRS multi-client skill tree, the Agent Skills specification, Codex project
skills and `AGENTS.md`, Claude Code project skills and MCP, and MCP 2026-07-28.
The useful shared pattern is a small instruction layer, focused progressively
loaded skills, deterministic CLI actions and a bounded live project model.

Encore demonstrates a strong compiler-derived application model and runtime
verification loop, but its current rule generator downloads a large
instruction payload from a moving branch and may reserialize existing client
configuration. Its MCP surface also includes database and runtime actions that
are intentionally outside Minco's read-only boundary. SRS demonstrates focused
documentation/code-map routers, shared skills and negative evaluations, but
uses symlinked client projections that are not a sufficient cross-platform
distribution contract. Neither project is a source of Minco truth.

Minco must support two distinct users:

- an application developer building a Minco web application; and
- a framework contributor changing the Minco repository itself.

Generated applications must not inherit Minco's internal JJ task workflow,
release policy or repository-only paths merely because a skill was installed.

## Decision

Minco adds an agent-native development layer as deterministic projections over
existing authorities. It does not add an autonomous agent runtime, hosted
control plane, second application graph or new source of project state.

### Layered contract

The layer has five responsibilities:

1. project instructions retain small source-of-truth and safety rules;
2. portable Agent Skills select focused Minco workflows;
3. `ProjectView`, versioned local documentation and existing CLI inspection
   commands provide bounded project facts;
4. `cargo minco` remains the only framework action surface; and
5. source, local, hosted, deployment, runtime and review evidence remain
   independent and attributable.

Skills never upgrade evidence or treat a successful command as proof in an
unrelated lane.

### Canonical assets and client projections

Canonical skill assets are shipped inside the version-matched `cargo-minco`
package. Each skill follows the open Agent Skills format and uses only `name`
and `description` in shared YAML front matter. Detailed procedures and examples
use one-level `references/` files so metadata and `SKILL.md` remain concise.

`cargo minco agent` materializes managed copies for supported clients:

- Codex: `.agents/skills/minco-*/`;
- Claude Code: `.claude/skills/minco-*/`; and
- Claude project instructions: an optional thin `CLAUDE.md` that imports an
  existing `AGENTS.md`, created only when the destination is absent.

Copies, rather than required symlinks, are the portable baseline. Every managed
file is recorded in `.minco/agent-manifest.json` with asset schema, Minco
version, client target, relative path and SHA-256 digest. The manifest is a
projection receipt, not project truth. An unchanged managed projection may be
replaced atomically; a missing, edited, unowned, symlinked or type-changed
destination fails closed instead of being adopted or overwritten.

Existing `AGENTS.md`, `CLAUDE.md`, `.mcp.json`, `.codex/config.toml`, client
settings and non-Minco skills are never parsed and reserialized. If integration
needs a user-owned file that already exists, the plan reports a manual action
and does not change the file.

### CLI contract

The additive CLI surface is:

```text
cargo minco agent plan --target codex|claude|all --json
cargo minco agent sync --target codex|claude|all --expect-plan-digest SHA256
cargo minco agent doctor --json
cargo minco agent context [--operation ID | --task ID] --json
cargo minco agent eval --target codex|claude|all --json
```

`plan` is read-only. It resolves the canonical project root, inventories only
fixed client destinations and emits a deterministic list of creates, safe
managed updates, unchanged files, conflicts and manual actions. The digest
binds target, Minco/asset version, input identities and intended bytes.

`sync` requires the exact current plan digest. It rechecks all input and
destination identities, creates private staging files beneath retained safe
parents and publishes with no-clobber or exact-owned replacement semantics. It
never accepts an arbitrary source path, remote URL or shell command and never
deletes an unmanaged path. A stale plan is rejected.

`doctor` is read-only and reports discovery, version, digest, drift, ownership,
client projection and local MCP configuration state. It does not install a
global skill, client plugin or user-level configuration.

`context` returns a bounded schema-versioned projection of existing
`ProjectView`, operation explanation or task readiness data plus applicable
versioned documentation identifiers. It does not synthesize source state,
execute a check or read outside the existing ProjectView allowlist.

`eval` validates skill format, target projection parity and checked-in scenario
contracts. It does not invoke a hosted model by default. Model-driven forward
tests require a separate explicit invocation and retain their own evidence.

### Workflow skills

The initial bundle contains focused workflows for:

- building a Minco web application;
- adding an OpenAPI-first operation;
- adding or changing a statically linked plugin;
- using the explicit application lifecycle;
- diagnosing a Minco project from bounded facts;
- reviewing a Minco change and its evidence;
- contributing to the Minco framework through its declared task/JJ workflow;
  and
- preparing release work only after an explicit user request.

The web-application skill is a router, not a replacement for the focused
skills. Framework-contributor instructions activate only when the project view
identifies the Minco framework repository and the owning task exists.

The release skill is explicit-invocation-only in its portable instructions and
contains no standing authorization to commit, push, merge, publish, deploy,
promote, migrate, enable a feature or contact a provider.

### MCP and future runtime tools

The initial implementation reuses ADR-0030's six read-only MCP tools without
expanding their schemas or tool catalog. Skills prefer narrow existing tools
and version-matched local CLI documentation.

An additional local documentation tool is considered only if cross-client
evaluations prove that the packaged references and `agent context` are
insufficient. Any MCP tool input or catalog change receives compatibility
review. Database queries, endpoint calls, application process control,
deployment and provider actions remain outside the default MCP server and
require a separate ADR, explicit local capability grant and independent
security review.

### Evaluation and versioning

Checked-in evaluations cover positive and negative triggering, correct workflow
ordering, framework/application mode separation, source authority, generated
file handling, evidence-lane separation, existing-file preservation and
forbidden implicit actions. Projection tests cover absent/existing destinations,
edited managed files, symlinks, path races, stale plan digests, line-ending
preservation and identical canonical content across clients.

Skill assets are released with `cargo-minco` and never fetched from a mutable
branch at generation time. Asset-schema changes are explicit. A compatible
patch may correct instructions without changing CLI behavior; renamed skills,
changed trigger semantics, manifest changes or new mutation authority require
normal compatibility review and release notes.

## Consequences

- Codex and Claude receive the same Minco workflows without a global install.
- Skills stay small because live facts come from existing project projections.
- Client-specific layout is replaceable without making either client an
  architectural dependency of Minco core.
- Generated applications receive application guidance without framework-only
  task or release policy.
- The CLI gains a managed local file-writing surface whose path, ownership,
  race and no-clobber guarantees require executable boundary tests.
- A Codex or Claude plugin marketplace package can be added later as a thin
  distribution adapter over the same canonical assets; it is not required for
  repository-scoped use.

## Compatibility

The new `agent` command is additive after Minco 1.0. Its serialized plans,
manifest, context, doctor and evaluation results are schema-versioned. Existing
projects remain unchanged until an operator runs `agent sync` with an exact
reviewed plan digest. Removing or renaming a command, skill, result field or
managed-path rule requires the ordinary post-1.0 compatibility process.

## Safety

All repository text, skill references, MCP results and client configuration are
untrusted data. The implementation resolves one explicit canonical project
root, accepts only fixed project-relative destinations, rejects traversal and
symlink components, enforces file/count/aggregate limits and never embeds
credentials, secret values, customer data or user-level client state.

Planning performs no write. Synchronization is restricted to manifest-owned
Minco projections, is digest-bound, stages privately, revalidates identity
before publication and fails closed on races or ownership ambiguity. It neither
runs generated skill commands nor grants their described actions.

No skill or command in this layer authorizes commit, push, merge, release,
publication, deployment, database mutation, provider access or production
feature enablement. Those actions retain their existing explicit Minco and user
approval gates.
