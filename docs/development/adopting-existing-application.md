# Adopting Minco in an existing application

Minco can wrap one boundary without becoming the application's architecture.
Keep the migration reversible and make the existing runtime authoritative
until each Minco seam proves parity.

1. Pin an exact published Minco version. For an unreleased pilot, pin an exact
   Git commit rather than a moving branch.
2. Run `cargo minco contract check` and deterministic generation first. Do not
   change routing, persistence, deployment or production traffic at this step.
3. Add a small composition shell that supplies the application's existing,
   use-case-shaped ports to Minco's typed graph.
4. Keep existing domain and application crates unchanged. Transport and
   provider adapters may depend inward; business crates do not depend on Axum,
   SQLx, Lambda, AWS SDKs or Minco Plan types.
5. Bridge verified product identities into Minco's `Principal`; retain product
   authorization and row-level rules in application use cases.
6. Adopt health, observability and idempotency conventions before moving a
   business endpoint. Prove request IDs, redaction and retry behavior.
7. If useful, pilot Feedback as an additive vertical slice with its own
   feature switch, persistence choice and removal path.
8. Keep product migrations, tenant boundaries and database row-level security
   authoritative. Minco startup never runs production migrations.
9. Keep the current deployment, release and rollback tooling authoritative
   until exact-artifact, IAM, cost, smoke and rollback parity is evidenced.
10. Move one OpenAPI operation at a time behind a compatibility switch. Record
    contract tests, old/new behavior, data effects and a rehearsed rollback.
11. Initially compare Minco Plan IR and SAM with existing infrastructure as
    advisory output; do not let it replace live infrastructure automatically.
12. Give every temporary bridge an owner, expiry condition and removal test.
    A bridge without deletion criteria is architecture, not a migration aid.

Start with the `contract` facade feature, then add only the HTTP, adapter,
runtime or plugin feature needed by the selected slice. The detailed feature
matrix and `0.1.1` to `0.2.0` candidate notes are in
[`../adoption/incremental-adoption.md`](../adoption/incremental-adoption.md).

The first real pilot must be a separate task with application ownership,
rollback authority, data classification and provider budgets. This guide does
not authorize a CGSP or GarmentIQ migration.
