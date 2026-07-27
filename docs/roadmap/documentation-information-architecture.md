# Documentation Information Architecture

Status: Accepted map; mechanical migration is deferred to `M11-T01`
Planning task: `M9-T01`

## Goal

Minco documentation becomes a versioned product organised by reader intent.
The existing Markdown remains in place until a mechanical move can preserve
links and pass deterministic validation. This document defines the target map;
it does not rename the whole documentation tree.

## Diátaxis structure

```text
docs/
  tutorials/
  how-to/
  reference/
  explanation/
```

Each page has one primary mode:

- tutorials provide a complete learning experience;
- how-to guides solve one concrete task;
- reference describes exact machinery;
- explanation develops rationale and trade-offs.

A page may link to another mode but should not try to perform all four jobs.

## Tutorials

Initial complete journeys:

```text
tutorials/
  first-api.md
  postgres-application.md
  deploy-to-aws.md
  build-a-plugin.md
```

Tutorials start from a clean generated application, use the published CLI, and
finish with a visible result. The AWS tutorial distinguishes local/SAM proof
from an explicitly authorised live deployment.

## How-to guides

```text
how-to/
  add-an-operation.md
  configure-an-environment.md
  migrate-and-seed.md
  add-a-worker.md
  add-authentication.md
  use-feedback.md
  add-a-custom-domain.md
  promote-and-rollback.md
  adopt-an-existing-application.md
  publish-a-plugin.md
```

Each guide states prerequisites, exact commands, mutation boundaries,
verification, rollback/removal, and links to the relevant reference.

## Reference

```text
reference/
  cli.md
  minco-toml.md
  configuration-schema.md
  plugin-descriptor.md
  plugin-catalog.md
  capability-resource-model.md
  plan-ir.md
  diagnostics.md
  cargo-features.md
  database-profiles.md
  compatibility-policy.md
  supported-matrix.md
```

CLI, feature, plugin, package, configuration, Plan IR, and diagnostic
inventories must be generated from authoritative metadata or checked against it.
README summaries link to reference instead of maintaining another exhaustive
list.

## Explanation

```text
explanation/
  contract-first-architecture.md
  modular-monolith.md
  static-plugin-model.md
  five-plane-application-graph.md
  cost-and-wake-model.md
  database-portfolio.md
  dev-to-deploy-lifecycle.md
  build-once-promotion.md
  jj-workflow.md
  ai-native-design.md
```

Accepted ADRs remain the decision record. Explanation pages make those decisions
approachable without weakening their authority.

## Entry paths

### Application developers

Start with `first-api`, then use operation, configuration, database, worker, and
deployment how-to guides. Link directly to CLI, feature, and compatibility
reference.

### Plugin authors

Start with `build-a-plugin`, then use plugin descriptor, distribution,
conformance, configuration, migration/seed, and publication reference.

### Operators

Start with `deploy-to-aws`, then use environment, migration, change-set,
promotion, rollback, preview cleanup, cost, and database-profile material.

### Framework contributors

Start with `AGENTS.md`, `docs/DECISIONS.md`, this roadmap, JJ workflow, quality
gates, task records, and the relevant subsystem explanation/reference.

### AI coding agents

Start with `AGENTS.md` and stable machine-readable interfaces:

```text
cargo minco inspect --json
cargo minco explain <operationId> --json
cargo minco task show <id> --json
cargo minco deploy plan --stdout --json
```

Agent material must identify canonical sources, generated files, mutation
boundaries, evidence semantics, and secret-redaction rules. It must not encode
hidden framework behavior that humans cannot inspect.

## Versioning

- The site defaults to the latest stable release.
- Published release documentation is immutable except for clearly labelled
  errata.
- `next` documents unreleased behavior and links to the compatibility note.
- Code examples identify the Minco and Rust versions they target.
- A release does not claim documentation complete until versioned navigation,
  generated reference, examples, and links pass at the exact tag.

## Source-of-truth policy

| Documentation output | Authority |
|---|---|
| Package inventory and order | `[workspace.metadata.minco.release]` |
| Workspace version and MSRV | root `Cargo.toml` |
| Facade features | `crates/minco/Cargo.toml` |
| Plugin IDs/kinds/features/stability | `plugins/catalog.toml` plus validated descriptors |
| CLI commands/options | Clap command model and generated help |
| Roadmap and task graphs | `roadmap/roadmap.yaml` and task front matter |
| OpenAPI operations | canonical OpenAPI documents |
| Plan IR schema and diagnostics | `minco-plan` public schema and tests |
| Verification claims | exact task evidence and `VERIFICATION.md` |

Generated reference is checked in only when deterministic and reviewed. A stale
generated page fails validation. Human-authored tutorials and explanations link
to generated reference rather than repeating complete inventories.

## Migration plan

1. Inventory every existing page, owner, audience, mode, inbound link, and
   version sensitivity.
2. Create the site shell and redirects without rewriting content.
3. Generate CLI, feature, package, plugin, and diagnostic reference.
4. Move one documentation mode at a time with link checks.
5. Compile or exercise snippets and commands.
6. Add release-version navigation and archive policy.
7. Shorten README to product identity, first success, core guarantees, and
   authoritative links.

`M11-T01` owns the site/migration. `M11-T06` owns generated reference and drift
checks. No broad move belongs in the framework-definition PR.

## Quality gates

The documentation product must provide:

- deterministic site generation;
- internal and external broken-link checks;
- compiled or exercised Rust, shell, TOML, YAML, and JSON snippets where
  practical;
- CLI help/reference drift checks;
- feature/plugin/package/catalog drift checks;
- version and canonical-link checks;
- docs.rs links for every public crate;
- accessibility and small-screen navigation review;
- a generated-artifact freshness check in local and hosted quality.

## External reference

The four-mode model follows
[Diátaxis](https://diataxis.fr/start-here/), checked on 2026-07-27. Minco's
versioning, generation, and evidence rules remain repository decisions.
