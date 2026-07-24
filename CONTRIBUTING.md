# Contributing to Minco

Minco is contract-first, JJ-first, and evidence-driven. Read `AGENTS.md`,
`docs/DECISIONS.md`, the relevant ADR, and the owning task before changing code.

## Development workflow

1. Install Jujutsu and initialise a colocated repository with
   `./scripts/jj/init.sh`.
2. Select a ready task with `cargo minco task ready`.
3. Create a dedicated workspace with
   `./scripts/jj/task-start.sh <TASK-ID>`.
4. Change the OpenAPI contract first for externally visible API behavior.
5. Add failing domain/application tests before infrastructure code.
6. Keep handlers free of SQL and provider logic.
7. Run `./scripts/quality.sh` and the relevant database/e2e runner.
8. Record command evidence in the task and describe the JJ change.
9. Create a bookmark and push it only after the gate passes.

## Change shape

Prefer small vertical slices. A public API operation should normally include:

- OpenAPI operation, examples, security, and problem responses;
- generated DTO/operation metadata;
- domain invariants and application use case;
- narrow application-owned ports;
- required adapters and migrations;
- application, adapter, HTTP, and deployment-plan tests;
- documentation and task evidence.

Do not introduce a generic repository, runtime service locator, dynamic library
plugin, hidden build-script code generation, or cloud abstraction that weakens
the settled architecture.

## Quality gate

The authoritative local gate is:

```bash
./scripts/quality.sh
```

The gate performs static repository validation, formatting, Clippy, workspace
tests, generated-artifact freshness, and deep review when the required tools are
available. Environment omissions are failures for release, not implicit passes.

## Commit and review guidance

Jujutsu changes should have an imperative description and remain logically
focused. Review generated changes alongside their source contract/plan. Any
change affecting deployment cost, IAM, migrations, authentication, or public
contracts requires an ADR update or an explicit statement that the existing ADR
still applies.
