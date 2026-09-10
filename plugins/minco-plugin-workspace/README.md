# `minco-plugin-workspace`

Workspace, project and integration-profile isolation for Minco
compositions (ADR-0076). The plugin owns the transport-neutral scope
model — bounded workspace/project/profile identifiers, the workspace
registry, integration profiles as persisted policy, and scope/grant
resolution — while concrete credential verification stays in the
composition root and ticket-specific authorization stays in the
consuming application.

## Boundary rules

- **Deployment binding.** Provisioning binds exactly one stable
  workspace identity to a database, atomically and repeatably. A pinned
  identity that differs from the bound one, a second workspace, and
  concurrent conflicting provisioning all fail closed. Restarts and
  restores preserve identity; nothing rewrites ownership.
- **Intersection, never union.** Effective authority is the intersection
  of the authenticated principal's grants, the profile's ceiling, the
  resolved workspace/project scope, and the application's own rules. A
  profile never widens a human identity, and its service principal never
  replaces the human requester.
- **Fail-closed profiles.** `kind` is classification, never a verifier:
  each enabled profile declares one concrete authentication mode, and
  only supported combinations (`service_api`/`service_bearer_token`,
  `portal`/`portal_session`, `email`/`inbound_email`) may be enabled in
  this release. Reserved taxonomy is rejected at configuration time —
  it never falls through to a service bearer path.
- **Reconciliation over silent mutation.** Repeated provisioning is
  idempotent. Persisted profiles missing from configuration are
  reported, not deleted; drifted policy digests, foreign ownership, and
  attempts to re-enable a disabled profile require explicit
  reconciliation. Startup never broadens permissions.
- **Ledger separation.** The SQLite stream owns the
  `_minco_workspace_migrations` ledger exclusively. It never runs under
  the sqlx default `_sqlx_migrations` ledger, never renames another
  plugin's ledger, and never disables checksum validation.

## Usage

```rust,ignore
use minco_plugin_workspace::{
    MemoryWorkspaceStore, ProfileAuthMode, ProfileId, ProfileKind, ProfileSpec, ProjectId,
    WorkspaceConfig, WorkspaceService, WorkspaceStoreService,
};
use std::sync::Arc;

let service = WorkspaceService::new(
    WorkspaceStoreService::new(Arc::new(MemoryWorkspaceStore::new())),
    WorkspaceConfig {
        projects: vec![ProjectId::try_from("desk".to_owned())?],
        profiles: [(
            ProfileId::try_from("desk-agent".to_owned())?,
            ProfileSpec {
                kind: ProfileKind::ServiceApi,
                auth_mode: ProfileAuthMode::ServiceBearerToken,
                bound_project: "desk".try_into()?,
                service_subject: "desk-agent".into(),
                secret_reference: Some("DESK_AGENT_TOKEN".into()),
                ..Default::default()
            },
        )]
        .into_iter()
        .collect(),
        ..Default::default()
    },
)?;
service.provision().await?; // explicit phase, after migration
let scope = service
    .resolve_service_principal_scope(&"desk-agent".try_into()?)
    .await?;
```

With the `sqlite` feature, `SqliteWorkspaceStore::migrate()` applies the
crate's migrations under its own ledger and the plugin registers a
critical `workspace-store` health check. The plugin contributes no HTTP
module: compositions resolve a scoped caller before calling application
use cases (see ADR-0076 for the route inventory classes).

## Evidence

`cargo test -p minco-plugin-workspace --all-features --locked`
