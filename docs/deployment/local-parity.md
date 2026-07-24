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

Run `./scripts/dev/test-rustack.sh` for level 3. The check enables only SSM,
creates and loads a `SecureString` through the standard AWS SDK endpoint override,
then removes the parameter. Rustack remains an adapter-behavior check; level 5 is
the AWS fidelity authority.
