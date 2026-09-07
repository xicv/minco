//! Workspace, project and integration-profile isolation for Minco
//! compositions (ADR-0076).
//!
//! This plugin owns the transport-neutral scope model: bounded workspace,
//! project and profile identifiers; the workspace/project registry;
//! integration profiles as persisted policy (never credentials); and the
//! scope-resolution service that turns verified evidence into a checked,
//! immutable caller scope. Effective authority is always an intersection —
//! principal grants, profile ceilings, the resolved workspace/project
//! scope, and the consuming application's own rules — never a union, and
//! origins are restrictions, not authentication.
//!
//! The boundary rules this crate enforces:
//!
//! - Deployment binding: exactly one stable, persisted workspace identity
//!   per database, provisioned atomically; conflicting configuration,
//!   second workspaces, and concurrent conflicting provisioning fail
//!   closed, and restarts preserve identity.
//! - Fail-closed profiles: only supported kind/authentication-mode
//!   combinations may be enabled; reserved taxonomy is rejected at
//!   configuration time instead of falling through to a bearer path.
//! - Reconciliation over silent mutation: repeated provisioning is
//!   idempotent, drift is reported or rejected, and startup never
//!   deletes, disables, re-enables, or broadens anything.
//! - Ledger separation: the `SQLite` stream owns
//!   `_minco_workspace_migrations` exclusively and never touches the sqlx
//!   default ledger or another plugin's ledger.
#![forbid(unsafe_code)]

pub mod model;
#[cfg(feature = "sqlite")]
pub mod persistence;
pub mod service;
pub mod store;

mod plugin;

pub use model::{
    IntegrationProfile, InvalidIdentifier, InvalidOrigin, PROJECT_MAXIMUM, PrincipalGrant,
    ProfileAuthMode, ProfileId, ProfileKind, ProfileStatus, ProjectId, ProjectRegistration,
    ProjectScope, ResolvedScope, ScopeTokenError, ServicePrincipalScope, Workspace, WorkspaceId,
    WorkspaceScope, validate_exact_origin,
};
#[cfg(feature = "sqlite")]
pub use persistence::{SqliteWorkspaceStore, WORKSPACE_MIGRATION_LEDGER};
pub use plugin::WorkspacePlugin;
pub use service::{
    ProfileSpec, ProvisionReport, WorkspaceConfig, WorkspaceError, WorkspaceService,
};
pub use store::{
    MemoryWorkspaceStore, ProvisionPlan, WorkspaceStore, WorkspaceStoreError, WorkspaceStoreService,
};
