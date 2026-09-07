//! `SQLite` persistence for the workspace registry.
//!
//! The migration stream owns exactly one explicit, fixed, non-colliding
//! ledger table (`_minco_workspace_migrations`). It never runs under the
//! sqlx default `_sqlx_migrations` ledger, never renames another plugin's
//! ledger, and never disables checksum or missing-migration validation. The
//! desk composition applies it as an explicit provisioning phase after the
//! ticketing and plugin-storage migrations (ADR-0076).

use crate::{
    model::{
        IntegrationProfile, PrincipalGrant, ProfileAuthMode, ProfileId, ProfileKind, ProfileStatus,
        ProjectId, Workspace, WorkspaceId,
    },
    store::{ProvisionPlan, WorkspaceStore, WorkspaceStoreError, WorkspaceStoreService},
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::sync::Arc;

/// The one ledger table this migration stream owns.
pub const WORKSPACE_MIGRATION_LEDGER: &str = "_minco_workspace_migrations";

/// SQLite-backed workspace registry.
#[derive(Clone)]
pub struct SqliteWorkspaceStore {
    pool: sqlx::SqlitePool,
}

impl std::fmt::Debug for SqliteWorkspaceStore {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SqliteWorkspaceStore")
            .finish_non_exhaustive()
    }
}

impl SqliteWorkspaceStore {
    #[must_use]
    pub const fn new(pool: sqlx::SqlitePool) -> Self {
        Self { pool }
    }

    /// Apply this crate's migrations under its own ledger. Repeated
    /// application is idempotent; the ledger name is fixed and never
    /// collides with `_sqlx_migrations` or any other plugin ledger.
    pub async fn migrate(&self) -> Result<(), sqlx::migrate::MigrateError> {
        let mut migrator = sqlx::migrate!("./migrations/sqlite");
        migrator.dangerous_set_table_name(WORKSPACE_MIGRATION_LEDGER);
        migrator.run(&self.pool).await
    }

    #[must_use]
    pub const fn pool(&self) -> &sqlx::SqlitePool {
        &self.pool
    }
}

fn database_error(error: sqlx::Error) -> WorkspaceStoreError {
    if let sqlx::Error::Database(ref database) = error
        && database.is_unique_violation()
    {
        return WorkspaceStoreError::Conflict(database.message().to_owned());
    }
    WorkspaceStoreError::other(error)
}

fn parse_timestamp(value: &str) -> Result<DateTime<Utc>, WorkspaceStoreError> {
    DateTime::parse_from_rfc3339(value)
        .map(|timestamp| timestamp.with_timezone(&Utc))
        .map_err(|error| {
            WorkspaceStoreError::Other(format!("stored timestamp is invalid: {error}"))
        })
}

fn json_set(value: &str) -> Result<std::collections::BTreeSet<String>, WorkspaceStoreError> {
    serde_json::from_str(value)
        .map_err(|error| WorkspaceStoreError::Other(format!("stored set is invalid: {error}")))
}

#[derive(sqlx::FromRow)]
struct BindingRow {
    workspace_id: String,
    display_name: String,
    provisioned_at: String,
}

#[derive(sqlx::FromRow)]
struct ProfileRow {
    profile_id: String,
    workspace_id: String,
    kind: String,
    auth_mode: String,
    status: String,
    bound_project_id: String,
    service_subject: String,
    permission_ceiling_json: String,
    allowed_origins_json: String,
    resource_types_json: String,
    secret_reference: Option<String>,
    policy_digest: String,
}

#[derive(sqlx::FromRow)]
struct GrantRow {
    workspace_id: String,
    subject: String,
    project_id: String,
    granted_at: String,
}

fn parse_profile(row: ProfileRow) -> Result<IntegrationProfile, WorkspaceStoreError> {
    let workspace = WorkspaceId::try_from(row.workspace_id.clone())
        .map_err(|error| WorkspaceStoreError::Other(error.to_string()))?;
    Ok(IntegrationProfile {
        id: ProfileId::try_from(row.profile_id)
            .map_err(|error| WorkspaceStoreError::Other(error.to_string()))?,
        workspace,
        kind: parse_kind(&row.kind)?,
        auth_mode: parse_mode(&row.auth_mode)?,
        status: parse_status(&row.status)?,
        bound_project: ProjectId::try_from(row.bound_project_id)
            .map_err(|error| WorkspaceStoreError::Other(error.to_string()))?,
        service_subject: row.service_subject,
        permission_ceiling: json_set(&row.permission_ceiling_json)?,
        allowed_origins: json_set(&row.allowed_origins_json)?,
        resource_types: json_set(&row.resource_types_json)?,
        secret_reference: row.secret_reference,
        policy_digest: row.policy_digest,
    })
}

fn parse_kind(value: &str) -> Result<ProfileKind, WorkspaceStoreError> {
    match value {
        "web_bff" => Ok(ProfileKind::WebBff),
        "portal" => Ok(ProfileKind::Portal),
        "widget" => Ok(ProfileKind::Widget),
        "browser_extension" => Ok(ProfileKind::BrowserExtension),
        "native_handoff" => Ok(ProfileKind::NativeHandoff),
        "native_oidc" => Ok(ProfileKind::NativeOidc),
        "email" => Ok(ProfileKind::Email),
        "service_api" => Ok(ProfileKind::ServiceApi),
        "webhook" => Ok(ProfileKind::Webhook),
        "mcp" => Ok(ProfileKind::Mcp),
        "a2a" => Ok(ProfileKind::A2a),
        other => Err(WorkspaceStoreError::Other(format!(
            "stored profile kind is unknown: {other}"
        ))),
    }
}

fn parse_mode(value: &str) -> Result<ProfileAuthMode, WorkspaceStoreError> {
    match value {
        "service_bearer_token" => Ok(ProfileAuthMode::ServiceBearerToken),
        "portal_session" => Ok(ProfileAuthMode::PortalSession),
        "support_handoff" => Ok(ProfileAuthMode::SupportHandoff),
        "inbound_email" => Ok(ProfileAuthMode::InboundEmail),
        other => Err(WorkspaceStoreError::Other(format!(
            "stored profile mode is unknown: {other}"
        ))),
    }
}

fn parse_status(value: &str) -> Result<ProfileStatus, WorkspaceStoreError> {
    match value {
        "enabled" => Ok(ProfileStatus::Enabled),
        "disabled" => Ok(ProfileStatus::Disabled),
        other => Err(WorkspaceStoreError::Other(format!(
            "stored profile status is unknown: {other}"
        ))),
    }
}

#[async_trait]
impl WorkspaceStore for SqliteWorkspaceStore {
    async fn binding(&self) -> Result<Option<Workspace>, WorkspaceStoreError> {
        let row = sqlx::query_as::<_, BindingRow>(
            "SELECT workspace_id, display_name, provisioned_at
             FROM workspace_deployment_binding WHERE singleton = 1",
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(database_error)?;
        row.map(|row| {
            Ok(Workspace {
                id: WorkspaceId::try_from(row.workspace_id)
                    .map_err(|error| WorkspaceStoreError::Other(error.to_string()))?,
                display_name: row.display_name,
                provisioned_at: parse_timestamp(&row.provisioned_at)?,
            })
        })
        .transpose()
    }

    async fn commit_provision(&self, plan: &ProvisionPlan) -> Result<(), WorkspaceStoreError> {
        let mut transaction = self.pool.begin().await.map_err(database_error)?;
        let bound: Option<String> = sqlx::query_scalar(
            "SELECT workspace_id FROM workspace_deployment_binding WHERE singleton = 1",
        )
        .fetch_optional(&mut *transaction)
        .await
        .map_err(database_error)?;
        let workspace = if let Some(binding) = &plan.binding {
            if let Some(existing) = &bound {
                return Err(WorkspaceStoreError::Conflict(format!(
                    "database already bound to workspace {existing}; refusing to bind {}",
                    binding.id
                )));
            }
            sqlx::query(
                "INSERT INTO workspace_deployment_binding
                 (singleton, workspace_id, display_name, provisioned_at)
                 VALUES (1, ?, ?, ?)",
            )
            .bind(binding.id.as_str())
            .bind(&binding.display_name)
            .bind(binding.provisioned_at.to_rfc3339())
            .execute(&mut *transaction)
            .await
            .map_err(database_error)?;
            binding.id.as_str().to_owned()
        } else {
            bound.ok_or_else(|| {
                WorkspaceStoreError::Other(
                    "provision plan carries no binding for an unbound database".into(),
                )
            })?
        };
        for registration in &plan.projects {
            if registration.workspace.as_str() != workspace {
                return Err(WorkspaceStoreError::Conflict(format!(
                    "project {} belongs to a foreign workspace",
                    registration.project
                )));
            }
            sqlx::query(
                "INSERT INTO workspace_projects (workspace_id, project_id, registered_at)
                 VALUES (?, ?, ?)",
            )
            .bind(&workspace)
            .bind(registration.project.as_str())
            .bind(registration.registered_at.to_rfc3339())
            .execute(&mut *transaction)
            .await
            .map_err(database_error)?;
        }
        for profile in &plan.profiles {
            if profile.workspace.as_str() != workspace {
                return Err(WorkspaceStoreError::Conflict(format!(
                    "profile {} belongs to a foreign workspace",
                    profile.id
                )));
            }
            sqlx::query(
                "INSERT INTO workspace_profiles (
                    profile_id, workspace_id, kind, auth_mode, status, bound_project_id,
                    service_subject, permission_ceiling_json, allowed_origins_json,
                    resource_types_json, secret_reference, policy_digest, created_at
                 ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(profile.id.as_str())
            .bind(&workspace)
            .bind(profile.kind.as_str())
            .bind(profile.auth_mode.as_str())
            .bind(profile.status.as_str())
            .bind(profile.bound_project.as_str())
            .bind(&profile.service_subject)
            .bind(
                serde_json::to_string(&profile.permission_ceiling)
                    .map_err(WorkspaceStoreError::other)?,
            )
            .bind(
                serde_json::to_string(&profile.allowed_origins)
                    .map_err(WorkspaceStoreError::other)?,
            )
            .bind(
                serde_json::to_string(&profile.resource_types)
                    .map_err(WorkspaceStoreError::other)?,
            )
            .bind(&profile.secret_reference)
            .bind(&profile.policy_digest)
            .bind(Utc::now().to_rfc3339())
            .execute(&mut *transaction)
            .await
            .map_err(database_error)?;
            // Provision history is retained permanently (round 1
            // finding 2): INSERT OR IGNORE keeps the first provisioning
            // timestamp while tolerating re-seeds after explicit
            // reconciliation.
            sqlx::query(
                "INSERT OR IGNORE INTO workspace_profile_history
                 (profile_id, workspace_id, provisioned_at)
                 VALUES (?, ?, ?)",
            )
            .bind(profile.id.as_str())
            .bind(&workspace)
            .bind(Utc::now().to_rfc3339())
            .execute(&mut *transaction)
            .await
            .map_err(database_error)?;
        }
        for grant in &plan.grants {
            if grant.workspace.as_str() != workspace {
                return Err(WorkspaceStoreError::Conflict(format!(
                    "grant for {} belongs to a foreign workspace",
                    grant.subject
                )));
            }
            sqlx::query(
                "INSERT INTO workspace_grants (workspace_id, subject, project_id, granted_at)
                 VALUES (?, ?, ?, ?)",
            )
            .bind(&workspace)
            .bind(&grant.subject)
            .bind(grant.project.as_str())
            .bind(grant.granted_at.to_rfc3339())
            .execute(&mut *transaction)
            .await
            .map_err(database_error)?;
        }
        transaction.commit().await.map_err(database_error)?;
        Ok(())
    }

    async fn projects(
        &self,
        workspace: &WorkspaceId,
    ) -> Result<Vec<ProjectId>, WorkspaceStoreError> {
        let rows: Vec<String> = sqlx::query_scalar(
            "SELECT project_id FROM workspace_projects
             WHERE workspace_id = ? ORDER BY project_id",
        )
        .bind(workspace.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(database_error)?;
        rows.into_iter()
            .map(|project| {
                ProjectId::try_from(project)
                    .map_err(|error| WorkspaceStoreError::Other(error.to_string()))
            })
            .collect()
    }

    async fn profile(
        &self,
        id: &ProfileId,
    ) -> Result<Option<IntegrationProfile>, WorkspaceStoreError> {
        let row = sqlx::query_as::<_, ProfileRow>(
            "SELECT profile_id, workspace_id, kind, auth_mode, status, bound_project_id,
                    service_subject, permission_ceiling_json, allowed_origins_json,
                    resource_types_json, secret_reference, policy_digest
             FROM workspace_profiles WHERE profile_id = ?",
        )
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(database_error)?;
        row.map(parse_profile).transpose()
    }

    async fn profiles(
        &self,
        workspace: &WorkspaceId,
    ) -> Result<Vec<IntegrationProfile>, WorkspaceStoreError> {
        let rows = sqlx::query_as::<_, ProfileRow>(
            "SELECT profile_id, workspace_id, kind, auth_mode, status, bound_project_id,
                    service_subject, permission_ceiling_json, allowed_origins_json,
                    resource_types_json, secret_reference, policy_digest
             FROM workspace_profiles WHERE workspace_id = ? ORDER BY profile_id",
        )
        .bind(workspace.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(database_error)?;
        rows.into_iter().map(parse_profile).collect()
    }

    async fn profile_history(
        &self,
        workspace: &WorkspaceId,
    ) -> Result<std::collections::BTreeSet<String>, WorkspaceStoreError> {
        let rows: Vec<String> = sqlx::query_scalar(
            "SELECT profile_id FROM workspace_profile_history
             WHERE workspace_id = ? ORDER BY profile_id",
        )
        .bind(workspace.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(database_error)?;
        Ok(rows.into_iter().collect())
    }

    async fn grants_for_subject(
        &self,
        workspace: &WorkspaceId,
        subject: &str,
    ) -> Result<Vec<PrincipalGrant>, WorkspaceStoreError> {
        let rows = sqlx::query_as::<_, GrantRow>(
            "SELECT workspace_id, subject, project_id, granted_at
             FROM workspace_grants WHERE workspace_id = ? AND subject = ?
             ORDER BY project_id",
        )
        .bind(workspace.as_str())
        .bind(subject)
        .fetch_all(&self.pool)
        .await
        .map_err(database_error)?;
        rows.into_iter()
            .map(|row| {
                Ok(PrincipalGrant {
                    workspace: WorkspaceId::try_from(row.workspace_id)
                        .map_err(|error| WorkspaceStoreError::Other(error.to_string()))?,
                    subject: row.subject,
                    project: ProjectId::try_from(row.project_id)
                        .map_err(|error| WorkspaceStoreError::Other(error.to_string()))?,
                    granted_at: parse_timestamp(&row.granted_at)?,
                })
            })
            .collect()
    }

    async fn grants(
        &self,
        workspace: &WorkspaceId,
    ) -> Result<Vec<PrincipalGrant>, WorkspaceStoreError> {
        let rows = sqlx::query_as::<_, GrantRow>(
            "SELECT workspace_id, subject, project_id, granted_at
             FROM workspace_grants WHERE workspace_id = ? ORDER BY subject, project_id",
        )
        .bind(workspace.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(database_error)?;
        rows.into_iter()
            .map(|row| {
                Ok(PrincipalGrant {
                    workspace: WorkspaceId::try_from(row.workspace_id)
                        .map_err(|error| WorkspaceStoreError::Other(error.to_string()))?,
                    subject: row.subject,
                    project: ProjectId::try_from(row.project_id)
                        .map_err(|error| WorkspaceStoreError::Other(error.to_string()))?,
                    granted_at: parse_timestamp(&row.granted_at)?,
                })
            })
            .collect()
    }

    async fn ready(&self) -> Result<(), WorkspaceStoreError> {
        sqlx::query("SELECT 1 FROM workspace_deployment_binding LIMIT 1")
            .execute(&self.pool)
            .await
            .map_err(database_error)?;
        Ok(())
    }
}

impl From<Arc<SqliteWorkspaceStore>> for WorkspaceStoreService {
    fn from(store: Arc<SqliteWorkspaceStore>) -> Self {
        Self::new(store)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        model::ProjectRegistration,
        service::{ProfileSpec, WorkspaceConfig, WorkspaceError, WorkspaceService},
        store::WorkspaceStoreService,
    };
    use std::collections::BTreeSet;

    async fn store() -> (tempfile::TempDir, Arc<SqliteWorkspaceStore>) {
        let directory = tempfile::tempdir().expect("scratch directory");
        let url = format!(
            "sqlite://{}?mode=rwc",
            directory.path().join("workspace.sqlite").display()
        );
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(4)
            .connect(&url)
            .await
            .expect("sqlite pool");
        let store = Arc::new(SqliteWorkspaceStore::new(pool));
        store.migrate().await.expect("workspace migrations");
        (directory, store)
    }

    fn config() -> WorkspaceConfig {
        WorkspaceConfig {
            display_name: "Default workspace".into(),
            workspace_id: Some(WorkspaceId::try_from("ws-test".to_owned()).expect("valid")),
            projects: vec![
                ProjectId::try_from("desk".to_owned()).expect("valid"),
                ProjectId::try_from("legacy Prj".to_owned()).expect("valid"),
            ],
            profiles: std::iter::once((
                ProfileId::try_from("desk-agent".to_owned()).expect("valid"),
                ProfileSpec {
                    kind: ProfileKind::ServiceApi,
                    auth_mode: ProfileAuthMode::ServiceBearerToken,
                    bound_project: ProjectId::try_from("desk".to_owned()).expect("valid"),
                    service_subject: "desk-agent".into(),
                    permission_ceiling: ["ticketing.agent.read"]
                        .map(str::to_owned)
                        .into_iter()
                        .collect(),
                    allowed_origins: ["https://desk.example.test"]
                        .map(str::to_owned)
                        .into_iter()
                        .collect(),
                    resource_types: BTreeSet::new(),
                    secret_reference: Some("DESK_AGENT_TOKEN".into()),
                },
            ))
            .collect(),
            grants: std::iter::once((
                "agent@example.test".to_owned(),
                std::iter::once(ProjectId::try_from("desk".to_owned()).expect("valid"))
                    .collect::<BTreeSet<_>>(),
            ))
            .collect(),
        }
    }

    #[tokio::test]
    async fn migrations_use_their_own_ledger_and_never_touch_the_sqlx_default() {
        let (_directory, store) = store().await;
        let ledger: Vec<(String,)> = sqlx::query_as(
            "SELECT name FROM sqlite_master WHERE type = 'table' AND name LIKE '%migrations%'",
        )
        .fetch_all(store.pool())
        .await
        .expect("table list");
        let names: Vec<String> = ledger.into_iter().map(|row| row.0).collect();
        assert!(names.contains(&WORKSPACE_MIGRATION_LEDGER.to_owned()));
        assert!(
            !names.contains(&"_sqlx_migrations".to_owned()),
            "the workspace stream must never run under the sqlx default ledger: {names:?}"
        );
        // Repeated migration is idempotent and checksum validation stays on.
        store.migrate().await.expect("idempotent migration");
    }

    #[tokio::test]
    async fn provisioning_binds_once_persists_everything_and_repeats_idempotently() {
        let (_directory, store) = store().await;
        let service = WorkspaceService::new(WorkspaceStoreService::new(store.clone()), config())
            .expect("valid configuration");
        let first = service.provision().await.expect("first provisioning");
        assert!(first.created);
        let rows: Vec<(String,)> =
            sqlx::query_as("SELECT project_id FROM workspace_projects ORDER BY project_id")
                .fetch_all(store.pool())
                .await
                .expect("projects");
        assert_eq!(rows.len(), 2);
        let again = service.provision().await.expect("repeat provisioning");
        assert!(!again.created);
        assert_eq!(again.workspace, first.workspace);
        // Restarting over the same file preserves the bound identity.
        let restarted = WorkspaceService::new(WorkspaceStoreService::new(store), config())
            .expect("valid configuration");
        let report = restarted.provision().await.expect("restart");
        assert_eq!(report.workspace, first.workspace);
    }

    #[tokio::test]
    async fn concurrent_conflicting_provisioning_fails_closed_for_one_side() {
        let (_directory, store) = store().await;
        let first = WorkspaceService::new(WorkspaceStoreService::new(store.clone()), config())
            .expect("valid configuration");
        let mut conflicting = config();
        conflicting.workspace_id =
            Some(WorkspaceId::try_from("ws-other".to_owned()).expect("valid"));
        let second = WorkspaceService::new(WorkspaceStoreService::new(store.clone()), conflicting)
            .expect("valid configuration");
        let (a, b) = tokio::join!(first.provision(), second.provision());
        let successes = [&a, &b]
            .into_iter()
            .filter(|outcome| outcome.is_ok())
            .count();
        assert_eq!(
            successes, 1,
            "exactly one of two conflicting provisioning passes may land"
        );
        let binding = sqlx::query_as::<_, BindingRow>(
            "SELECT workspace_id, display_name, provisioned_at
             FROM workspace_deployment_binding WHERE singleton = 1",
        )
        .fetch_one(store.pool())
        .await
        .expect("exactly one binding row");
        assert_eq!(
            ["ws-test", "ws-other"]
                .iter()
                .filter(|id| **id == binding.workspace_id)
                .count(),
            1
        );
    }

    #[tokio::test]
    async fn composite_foreign_keys_reject_unregistered_ownership() {
        let (_directory, store) = store().await;
        let now = Utc::now();
        let plan = ProvisionPlan {
            binding: Some(Workspace {
                id: WorkspaceId::try_from("ws-fk".to_owned()).expect("valid"),
                display_name: "FK".into(),
                provisioned_at: now,
            }),
            projects: vec![ProjectRegistration {
                workspace: WorkspaceId::try_from("ws-fk".to_owned()).expect("valid"),
                project: ProjectId::try_from("desk".to_owned()).expect("valid"),
                registered_at: now,
            }],
            profiles: vec![],
            // A grant for a project that was never registered under this
            // workspace must be rejected by the composite foreign key.
            grants: vec![PrincipalGrant {
                workspace: WorkspaceId::try_from("ws-fk".to_owned()).expect("valid"),
                subject: "someone@example.test".into(),
                project: ProjectId::try_from("ghost".to_owned()).expect("valid"),
                granted_at: now,
            }],
        };
        let error = store
            .commit_provision(&plan)
            .await
            .expect_err("FK must fire");
        assert!(
            matches!(error, WorkspaceStoreError::Other(_)),
            "foreign-key violations surface as store failures: {error:?}"
        );
        assert!(
            store.binding().await.expect("binding").is_none(),
            "the failed transaction must roll the binding back too"
        );
    }

    #[tokio::test]
    async fn field_level_policy_drift_under_an_untouched_digest_fails_reconciliation() {
        // Round 1 finding 2: tampering a persisted policy field while
        // leaving the sealed digest unchanged must fail startup
        // reconciliation, and resolution must never use the drifted
        // values.
        let (_directory, store) = store().await;
        let service = WorkspaceService::new(WorkspaceStoreService::new(store.clone()), config())
            .expect("valid configuration");
        service.provision().await.expect("first provisioning");
        sqlx::query(
            "UPDATE workspace_profiles
                SET permission_ceiling_json = '[\"ticketing.manage\"]'
              WHERE profile_id = 'desk-agent'",
        )
        .execute(store.pool())
        .await
        .expect("tamper the persisted ceiling");
        let error = service
            .provision()
            .await
            .expect_err("drift must fail closed");
        assert!(matches!(error, WorkspaceError::ReconciliationRequired(_)));
        // Resolution is also denied for the drifted profile.
        let resolution = service
            .resolve_service_principal_scope(
                &ProfileId::try_from("desk-agent".to_owned()).expect("valid"),
            )
            .await;
        assert!(resolution.is_err());
    }

    #[tokio::test]
    async fn a_removed_profile_is_never_silently_recreated() {
        // Round 1 finding 2: deleting a seeded profile row and restarting
        // with the same configuration demands explicit reconciliation;
        // the retained provision history distinguishes this from a
        // genuinely new configuration seed.
        let (_directory, store) = store().await;
        let service = WorkspaceService::new(WorkspaceStoreService::new(store.clone()), config())
            .expect("valid configuration");
        service.provision().await.expect("first provisioning");
        sqlx::query("DELETE FROM workspace_profiles WHERE profile_id = 'desk-agent'")
            .execute(store.pool())
            .await
            .expect("remove the seeded profile");
        let error = service
            .provision()
            .await
            .expect_err("recreation must be refused");
        let message = format!("{error}");
        assert!(
            matches!(error, WorkspaceError::ReconciliationRequired(_)),
            "unexpected error: {message}"
        );
        assert!(message.contains("removed"), "message: {message}");
        // A genuinely NEW configured profile still seeds cleanly.
        let mut extended = config();
        extended.profiles.insert(
            ProfileId::try_from("desk-bff".to_owned()).expect("valid"),
            ProfileSpec {
                kind: ProfileKind::Portal,
                auth_mode: ProfileAuthMode::PortalSession,
                bound_project: ProjectId::try_from("desk".to_owned()).expect("valid"),
                service_subject: "desk-portal".into(),
                permission_ceiling: BTreeSet::new(),
                allowed_origins: BTreeSet::new(),
                resource_types: BTreeSet::new(),
                secret_reference: None,
            },
        );
        let grown = WorkspaceService::new(WorkspaceStoreService::new(store), extended)
            .expect("valid configuration");
        // Still refuses: the removed desk-agent remains configured.
        assert!(grown.provision().await.is_err());
    }

    #[tokio::test]
    async fn an_unsupported_persisted_kind_fails_closed_on_load() {
        // A persisted kind outside the supported taxonomy cannot resolve:
        // tampering the kind also breaks the sealed digest, so
        // reconciliation is demanded — fail closed either way.
        let (_directory, store) = store().await;
        let service = WorkspaceService::new(WorkspaceStoreService::new(store.clone()), config())
            .expect("valid configuration");
        service.provision().await.expect("first provisioning");
        sqlx::query(
            "UPDATE workspace_profiles SET kind = 'native_oidc' WHERE profile_id = 'desk-agent'",
        )
        .execute(store.pool())
        .await
        .expect("persist an unsupported kind");
        let error = service
            .resolve_service_principal_scope(
                &ProfileId::try_from("desk-agent".to_owned()).expect("valid"),
            )
            .await
            .expect_err("unsupported kind must fail closed");
        assert!(
            matches!(error, WorkspaceError::ReconciliationRequired(_)),
            "{error}"
        );
        // A persisted kind with an UNKNOWN vocabulary word fails at the
        // store parse itself.
        sqlx::query(
            "UPDATE workspace_profiles SET kind = 'not-a-kind' WHERE profile_id = 'desk-agent'",
        )
        .execute(store.pool())
        .await
        .expect("persist an unknown kind");
        let error = service
            .profile(&ProfileId::try_from("desk-agent".to_owned()).expect("valid"))
            .await
            .expect_err("unknown kind must fail the load");
        assert!(matches!(error, WorkspaceError::Store(_)), "{error}");
    }
}
