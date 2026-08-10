# Diagnostic routing

Release skill freshness is checked against the current Minco changelog and
versioned documentation before the bundle ships.

- Contract mismatch: OpenAPI plus `contract check` and generated binding state.
- Slice ownership: `explain <operationId>`.
- Graph/capability issue: `inspect` or `project_summary`.
- Task readiness: `task readiness` from CLI/MCP.
- Configuration: `config check/explain/diff`; values remain redacted.
- Local process issue: DevPlan, owned local services, exact resource identity,
  loopback binding and process output.
- Infrastructure issue: selected topology-aware cost, Plan/review/receipt,
  without provider mutation.
- Claim mismatch: inspect only the relevant release-bound evidence lane and
  exact subject.

An explicit absent evidence record means the snapshot did not provide proof; it
is neither a pass nor a failure. Preserve freshness and exact revision limits.
