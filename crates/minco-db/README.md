# minco-db

`minco-db` models migration identity, ownership, ordering, digests, destructive
risk, target status and verification without depending on SQLx or a database
runtime. It loads attributable `.minco-migrations.toml` sidecars and rejects
ambiguous history ownership before target access.

PostgreSQL and SQLite adapters consume the provider-neutral sets. The
`cargo-minco` control plane owns credential indirection, exact-digest
acknowledgement, durable receipts and operator-facing JSON. Production
application startup never runs migrations.
