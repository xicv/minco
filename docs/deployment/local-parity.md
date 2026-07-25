# Local-to-Production Fidelity

Minco uses an explicit fidelity ladder rather than claiming that local AWS emulation equals
AWS.

| Level | Runtime | Purpose |
|---|---|---|
| 0 | Application use case + fake ports | Business rules and deterministic TDD. |
| 1 | Real PostgreSQL or SQLite | SQL, constraints, migrations, transactions. |
| 2 | Native Axum router | HTTP extraction, middleware, contract behavior. |
| 3 | Rustack AWS seams | Fast SDK adapter behavior for declared services. |
| 4 | SAM/event fixtures or secondary emulator | API Gateway/Lambda envelopes and unsupported local services. |
| 5 | Disposable real AWS dev | IAM, authorizers, edge, provider semantics, cold starts. |

A PostgreSQL production profile should use PostgreSQL locally. SQLite is a separate
first-class profile, not an invisible development substitute.

`scripts/dev/rustack-smoke.sh` exercises level 3 through standard AWS endpoint
variables. In addition to direct S3, SQS, SSM and STS compatibility, it creates
and loads an SSM SecureString through the real
`minco-aws-lambda::load_secure_parameter` SDK adapter. Disposable real AWS
remains the fidelity authority.

`scripts/dev/topology.py` consumes canonical Plan IR from
`cargo minco deploy plan --stdout --json`; it does not independently infer
plugin selection from `minco.toml` or `plugins/catalog.toml`. Set
`MINCO_DEPLOYMENT_PLAN` to a reviewed plan file for deterministic offline CI.
The plan contains descriptor metadata and derived local AWS service names, but
never plugin secret values.
