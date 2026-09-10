//! Workspace provisioning and scope resolution (ADR-0076).
//!
//! The service owns three fail-closed rules: provisioning binds exactly one
//! stable workspace identity per database and never silently rebinds,
//! broadens, or recreates; effective authority is resolved as an
//! intersection (explicit grants, profile ceilings, registered projects) and
//! never a union; and no resolution path treats a request-supplied project
//! as anything more than an untrusted selector validated against the
//! authoritative registry.

use crate::{
    model::{
        IntegrationProfile, InvalidIdentifier, PrincipalGrant, ProfileAuthMode, ProfileId,
        ProfileKind, ProfileStatus, ProjectId, ProjectRegistration, ProjectScope,
        ServicePrincipalScope, Workspace, WorkspaceId, validate_exact_origin,
    },
    store::{ProvisionPlan, WorkspaceStoreError, WorkspaceStoreService},
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Configuration-seeded provisioning input. Everything here is trusted
/// deployment configuration; nothing here arrives from a request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceConfig {
    /// Display name for the provisioned workspace (a label, never an
    /// authority identifier).
    #[serde(default = "default_display_name")]
    pub display_name: String,
    /// Pin the workspace identity from deployment configuration. When set,
    /// it must equal the already-bound identity on every later startup.
    #[serde(default)]
    pub workspace_id: Option<WorkspaceId>,
    /// Projects registered under the workspace. Existing project
    /// identifiers register verbatim.
    #[serde(default)]
    pub projects: Vec<ProjectId>,
    /// Integration profiles seeded by configuration, keyed by profile id.
    #[serde(default)]
    pub profiles: BTreeMap<ProfileId, ProfileSpec>,
    /// Explicit principal-to-project grants seeded by configuration,
    /// keyed by subject.
    #[serde(default)]
    pub grants: BTreeMap<String, BTreeSet<ProjectId>>,
}

fn default_display_name() -> String {
    "Default workspace".into()
}

impl Default for WorkspaceConfig {
    fn default() -> Self {
        Self {
            display_name: default_display_name(),
            workspace_id: None,
            projects: Vec::new(),
            profiles: BTreeMap::new(),
            grants: BTreeMap::new(),
        }
    }
}

/// One integration profile as declared by deployment configuration. The
/// profile identifier is the map key under `WorkspaceConfig::profiles`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileSpec {
    pub kind: ProfileKind,
    pub auth_mode: ProfileAuthMode,
    /// The single project this profile is bound to in this release.
    pub bound_project: ProjectId,
    /// Principal subject the profile acts as.
    pub service_subject: String,
    #[serde(default)]
    pub permission_ceiling: BTreeSet<String>,
    #[serde(default)]
    pub allowed_origins: BTreeSet<String>,
    #[serde(default)]
    pub resource_types: BTreeSet<String>,
    /// Secret **name** resolved by the composition; never a value.
    #[serde(default)]
    pub secret_reference: Option<String>,
}

/// The outcome of one provisioning pass.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProvisionReport {
    pub workspace: WorkspaceId,
    /// True when this pass created the deployment binding.
    pub created: bool,
    /// Profiles present in the store but absent from configuration.
    /// Provisioning never deletes or disables them.
    pub orphaned_profiles: Vec<ProfileId>,
    /// Explicit grants present in the store but absent from configuration.
    /// Provisioning never narrows them.
    pub retained_grants: usize,
}

/// Workspace service failures. Every ambiguous or conflicting state fails
/// closed with an explicit error rather than a default scope.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WorkspaceError {
    #[error("workspace configuration is invalid: {0}")]
    Configuration(String),
    #[error("this deployment is not bound to a workspace yet; provisioning must run first")]
    NotProvisioned,
    #[error(
        "workspace {existing} is already bound to this database; refusing to rebind to {requested}"
    )]
    BindingConflict {
        existing: WorkspaceId,
        requested: WorkspaceId,
    },
    #[error("explicit reconciliation required: {0}")]
    ReconciliationRequired(String),
    #[error("profile {0} is unknown")]
    UnknownProfile(ProfileId),
    #[error("profile {0} is disabled and cannot resolve scope")]
    ProfileDisabled(ProfileId),
    #[error("subject {0} holds no grant for project {1} in this workspace")]
    ScopeDenied(String, ProjectId),
    #[error("project {0} is not registered under the bound workspace")]
    UnregisteredProject(ProjectId),
    #[error("workspace store failure: {0}")]
    Store(String),
}

impl From<WorkspaceStoreError> for WorkspaceError {
    fn from(error: WorkspaceStoreError) -> Self {
        match error {
            WorkspaceStoreError::Conflict(detail) => Self::ReconciliationRequired(detail),
            WorkspaceStoreError::Other(detail) => Self::Store(detail),
        }
    }
}

impl From<InvalidIdentifier> for WorkspaceError {
    fn from(error: InvalidIdentifier) -> Self {
        Self::Configuration(error.to_string())
    }
}

/// The workspace scope-resolution service.
#[derive(Clone)]
pub struct WorkspaceService {
    store: WorkspaceStoreService,
    config: WorkspaceConfig,
}

impl std::fmt::Debug for WorkspaceService {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("WorkspaceService")
            .field("workspace_id", &self.config.workspace_id)
            .field("profiles", &self.config.profiles.len())
            .finish_non_exhaustive()
    }
}

impl WorkspaceService {
    /// Build the service after validating its configuration: supported
    /// kind/mode combinations, exact origins, known projects, and bounded
    /// subjects. Invalid configuration fails here, not at first use.
    pub fn new(
        store: WorkspaceStoreService,
        config: WorkspaceConfig,
    ) -> Result<Self, WorkspaceError> {
        Self::validate(&config)?;
        Ok(Self { store, config })
    }

    /// The configuration this service was built with.
    #[must_use]
    pub const fn config(&self) -> &WorkspaceConfig {
        &self.config
    }

    /// Store readiness probe for the plugin's critical health check.
    pub async fn store_ready(&self) -> Result<(), WorkspaceError> {
        self.store.0.ready().await.map_err(WorkspaceError::from)
    }

    fn validate(config: &WorkspaceConfig) -> Result<(), WorkspaceError> {
        validate_text("display_name", &config.display_name, 100)?;
        let mut registered: BTreeSet<String> = BTreeSet::new();
        for project in &config.projects {
            if !registered.insert(project.as_str().to_owned()) {
                return Err(WorkspaceError::Configuration(format!(
                    "project {project} is declared twice"
                )));
            }
        }
        for (id, spec) in &config.profiles {
            if !spec.kind.supports(spec.auth_mode) {
                return Err(WorkspaceError::Configuration(format!(
                    "profile {id} uses reserved taxonomy (kind {}, mode {}); only supported combinations may be enabled and unsupported kinds fail closed",
                    spec.kind.as_str(),
                    spec.auth_mode.as_str()
                )));
            }
            if !registered.contains(spec.bound_project.as_str()) {
                return Err(WorkspaceError::Configuration(format!(
                    "profile {id} binds unregistered project {}",
                    spec.bound_project
                )));
            }
            validate_text("service_subject", &spec.service_subject, 100)?;
            if let Some(secret) = &spec.secret_reference {
                validate_text("secret_reference", secret, 200)?;
            }
            for origin in &spec.allowed_origins {
                validate_exact_origin(origin).map_err(|error| {
                    WorkspaceError::Configuration(format!(
                        "profile {id} declares invalid origin {origin}: {error}"
                    ))
                })?;
            }
            for permission in &spec.permission_ceiling {
                validate_text("permission_ceiling", permission, 100)?;
            }
            for resource_type in &spec.resource_types {
                validate_text("resource_types", resource_type, 100)?;
            }
        }
        for (subject, projects) in &config.grants {
            validate_text("grant subject", subject, 100)?;
            for project in projects {
                if !registered.contains(project.as_str()) {
                    return Err(WorkspaceError::Configuration(format!(
                        "grant for {subject} references unregistered project {project}"
                    )));
                }
            }
        }
        Ok(())
    }

    /// Provision (or re-converge) this deployment's workspace registry.
    ///
    /// First run: binds one stable workspace identity (the configured pin or
    /// a minted identifier) and inserts every project, profile, and grant
    /// atomically. Later runs are idempotent: they insert only configuration
    /// the store lacks, report profiles and grants the store carries beyond
    /// the configuration, and never delete, disable, re-enable, or rewrite.
    /// A configured identity that differs from the bound one, or a profile
    /// whose persisted policy drifted, requires explicit reconciliation.
    pub async fn provision(&self) -> Result<ProvisionReport, WorkspaceError> {
        let now = Utc::now();
        let existing = self.store.0.binding().await?;
        let Some(binding) = existing else {
            let workspace = Workspace {
                id: self
                    .config
                    .workspace_id
                    .clone()
                    .unwrap_or_else(WorkspaceId::mint),
                display_name: self.config.display_name.clone(),
                provisioned_at: now,
            };
            let plan = ProvisionPlan {
                binding: Some(workspace.clone()),
                projects: self
                    .config
                    .projects
                    .iter()
                    .map(|project| ProjectRegistration {
                        workspace: workspace.id.clone(),
                        project: project.clone(),
                        registered_at: now,
                    })
                    .collect(),
                profiles: self
                    .config
                    .profiles
                    .iter()
                    .map(|(id, spec)| Self::profile_from_spec(id, &workspace.id, spec))
                    .collect(),
                grants: self
                    .config
                    .grants
                    .iter()
                    .flat_map(|(subject, projects)| {
                        let workspace_id = workspace.id.clone();
                        let subject = subject.clone();
                        projects.iter().map(move |project| PrincipalGrant {
                            workspace: workspace_id.clone(),
                            subject: subject.clone(),
                            project: project.clone(),
                            granted_at: now,
                        })
                    })
                    .collect(),
            };
            let workspace_id = plan
                .binding
                .as_ref()
                .map(|workspace: &Workspace| workspace.id.clone())
                .expect("binding");
            self.store.0.commit_provision(&plan).await?;
            return Ok(ProvisionReport {
                workspace: workspace_id,
                created: true,
                orphaned_profiles: Vec::new(),
                retained_grants: 0,
            });
        };

        if let Some(requested) = &self.config.workspace_id
            && requested != &binding.id
        {
            return Err(WorkspaceError::BindingConflict {
                existing: binding.id.clone(),
                requested: requested.clone(),
            });
        }
        let workspace_id = binding.id.clone();
        let registered: BTreeSet<String> = self
            .store
            .0
            .projects(&workspace_id)
            .await?
            .iter()
            .map(|project| project.as_str().to_owned())
            .collect();
        let persisted_profiles: BTreeMap<String, IntegrationProfile> = self
            .store
            .0
            .profiles(&workspace_id)
            .await?
            .into_iter()
            .map(|profile| (profile.id.as_str().to_owned(), profile))
            .collect();
        // Retained provisioning history (round 1 finding 2): a configured
        // profile absent from the live registry but present in history
        // was removed after provisioning and demands explicit
        // reconciliation instead of silent recreation.
        let profile_history = self.store.0.profile_history(&workspace_id).await?;
        // Field-level integrity: the persisted policy fields must still
        // match their own sealed digest. Drifted fields under an
        // untouched digest tamper with authority and fail closed.
        for (id, persisted) in &persisted_profiles {
            if persisted.current_policy_digest() != persisted.policy_digest {
                return Err(WorkspaceError::ReconciliationRequired(format!(
                    "profile {id} persisted policy fields do not match its sealed digest \
                     (field-level drift); restore the persisted policy or re-provision \
                     explicitly"
                )));
            }
        }
        let mut projects = Vec::new();
        for project in &self.config.projects {
            if !registered.contains(project.as_str()) {
                projects.push(ProjectRegistration {
                    workspace: workspace_id.clone(),
                    project: project.clone(),
                    registered_at: now,
                });
            }
        }
        let mut profiles = Vec::new();
        for (id, spec) in &self.config.profiles {
            match persisted_profiles.get(id.as_str()) {
                None => {
                    if profile_history.contains(id.as_str()) {
                        return Err(WorkspaceError::ReconciliationRequired(format!(
                            "profile {id} was previously provisioned and has been removed; \
                             restore the profile row or remove it from configuration \
                             explicitly"
                        )));
                    }
                    profiles.push(Self::profile_from_spec(id, &workspace_id, spec));
                }
                Some(persisted) => Self::reconcile_profile(persisted, spec, id, &workspace_id)?,
            }
        }
        let persisted_grants = self.store.0.grants(&workspace_id).await?;
        let mut grants = Vec::new();
        for (subject, projects) in &self.config.grants {
            for project in projects {
                if persisted_grants
                    .iter()
                    .any(|existing| &existing.subject == subject && &existing.project == project)
                {
                    continue;
                }
                grants.push(PrincipalGrant {
                    workspace: workspace_id.clone(),
                    subject: subject.clone(),
                    project: project.clone(),
                    granted_at: now,
                });
            }
        }
        let retained_grants = persisted_grants
            .iter()
            .filter(|existing| {
                self.config
                    .grants
                    .get(&existing.subject)
                    .is_none_or(|projects| !projects.contains(&existing.project))
            })
            .count();
        let orphaned_profiles = persisted_profiles
            .keys()
            .filter(|id| !self.config.profiles.keys().any(|key| key.as_str() == *id))
            .map(|id| ProfileId::try_from(id.clone()).expect("persisted profile stays valid"))
            .collect();
        self.store
            .0
            .commit_provision(&ProvisionPlan {
                binding: None,
                projects,
                profiles,
                grants,
            })
            .await?;
        Ok(ProvisionReport {
            workspace: workspace_id,
            created: false,
            orphaned_profiles,
            retained_grants,
        })
    }

    fn reconcile_profile(
        persisted: &IntegrationProfile,
        spec: &ProfileSpec,
        id: &ProfileId,
        workspace: &WorkspaceId,
    ) -> Result<(), WorkspaceError> {
        if &persisted.workspace != workspace {
            return Err(WorkspaceError::ReconciliationRequired(format!(
                "profile {id} is owned by workspace {}",
                persisted.workspace
            )));
        }
        if persisted.status != ProfileStatus::Enabled {
            return Err(WorkspaceError::ReconciliationRequired(format!(
                "profile {id} is persisted as {}; configuration cannot re-enable it",
                persisted.status.as_str()
            )));
        }
        let expected = Self::profile_from_spec(id, workspace, spec).current_policy_digest();
        if persisted.policy_digest != expected {
            return Err(WorkspaceError::ReconciliationRequired(format!(
                "profile {id} persisted policy digest {} does not match configured policy digest {expected}",
                persisted.policy_digest
            )));
        }
        Ok(())
    }

    fn profile_from_spec(
        id: &ProfileId,
        workspace: &WorkspaceId,
        spec: &ProfileSpec,
    ) -> IntegrationProfile {
        IntegrationProfile {
            id: id.clone(),
            workspace: workspace.clone(),
            kind: spec.kind,
            auth_mode: spec.auth_mode,
            status: ProfileStatus::Enabled,
            bound_project: spec.bound_project.clone(),
            service_subject: spec.service_subject.clone(),
            permission_ceiling: spec.permission_ceiling.clone(),
            allowed_origins: spec.allowed_origins.clone(),
            resource_types: spec.resource_types.clone(),
            secret_reference: spec.secret_reference.clone(),
            policy_digest: String::new(),
        }
        .with_recomputed_digest()
    }

    /// The deployment binding. Fails closed when provisioning has not run.
    pub async fn binding(&self) -> Result<Workspace, WorkspaceError> {
        self.store
            .0
            .binding()
            .await?
            .ok_or(WorkspaceError::NotProvisioned)
    }

    /// Projects registered under the bound workspace.
    pub async fn registered_projects(&self) -> Result<Vec<ProjectId>, WorkspaceError> {
        Ok(self.store.0.projects(&self.binding().await?.id).await?)
    }

    /// One persisted profile, checked against the bound workspace. The
    /// binding is checked first so an unprovisioned deployment reports
    /// `NotProvisioned` rather than a misleading unknown profile.
    pub async fn profile(&self, id: &ProfileId) -> Result<IntegrationProfile, WorkspaceError> {
        let workspace = self.binding().await?.id;
        let profile = self
            .store
            .0
            .profile(id)
            .await?
            .ok_or_else(|| WorkspaceError::UnknownProfile(id.clone()))?;
        if profile.workspace != workspace {
            return Err(WorkspaceError::UnknownProfile(id.clone()));
        }
        Ok(profile)
    }

    /// Resolve the checked service-principal context for one profile. The
    /// composition calls this **after** verifying the profile's concrete
    /// credential; this method fails closed for unknown, foreign, or
    /// disabled profiles, for persisted policy whose fields no longer
    /// match their sealed digest, and for persisted kind/mode
    /// combinations outside the supported taxonomy — it never widens
    /// the persisted ceiling (round 1 finding 2).
    pub async fn resolve_service_principal_scope(
        &self,
        id: &ProfileId,
    ) -> Result<ServicePrincipalScope, WorkspaceError> {
        let profile = self.profile(id).await?;
        if profile.current_policy_digest() != profile.policy_digest {
            return Err(WorkspaceError::ReconciliationRequired(format!(
                "profile {id} persisted policy fields do not match its sealed digest; \
                 refusing to resolve drifted policy"
            )));
        }
        if !profile.kind.supports(profile.auth_mode) {
            return Err(WorkspaceError::ReconciliationRequired(format!(
                "profile {id} persists an unsupported kind/mode combination \
                 ({} / {}); reserved taxonomy fails closed",
                profile.kind.as_str(),
                profile.auth_mode.as_str()
            )));
        }
        if profile.status != ProfileStatus::Enabled {
            return Err(WorkspaceError::ProfileDisabled(id.clone()));
        }
        self.require_registered(&profile.bound_project).await?;
        Ok(ServicePrincipalScope {
            scope: ProjectScope {
                workspace: profile.workspace.clone(),
                project: profile.bound_project.clone(),
            },
            service_subject: profile.service_subject.clone(),
            permission_ceiling: profile.permission_ceiling.clone(),
            allowed_origins: profile.allowed_origins.clone(),
            resource_types: profile.resource_types.clone(),
        })
    }

    /// Resolve scope for a named principal: an explicit grant is required,
    /// and a request-supplied project is only ever validated against it.
    pub async fn resolve_granted_scope(
        &self,
        subject: &str,
        project: &ProjectId,
    ) -> Result<ProjectScope, WorkspaceError> {
        let workspace = self.binding().await?.id;
        let granted = self.store.0.grants_for_subject(&workspace, subject).await?;
        if !granted.iter().any(|grant| &grant.project == project) {
            return Err(WorkspaceError::ScopeDenied(
                subject.to_owned(),
                project.clone(),
            ));
        }
        self.require_registered(project).await
    }

    /// Validate a credential-carried project selector (for example a
    /// requester session attribute) against the authoritative registry. No
    /// authority is minted: the caller's grant was validated where the
    /// credential was issued.
    pub async fn validate_session_scope(
        &self,
        project: &ProjectId,
    ) -> Result<ProjectScope, WorkspaceError> {
        self.require_registered(project).await
    }

    async fn require_registered(
        &self,
        project: &ProjectId,
    ) -> Result<ProjectScope, WorkspaceError> {
        let workspace = self.binding().await?.id;
        let registered = self.store.0.projects(&workspace).await?;
        if !registered.contains(project) {
            return Err(WorkspaceError::UnregisteredProject(project.clone()));
        }
        Ok(ProjectScope {
            workspace,
            project: project.clone(),
        })
    }
}

fn validate_text(field: &'static str, value: &str, maximum: usize) -> Result<(), WorkspaceError> {
    if value.trim().is_empty()
        || value.chars().count() > maximum
        || value.chars().any(char::is_control)
    {
        return Err(WorkspaceError::Configuration(format!(
            "{field} must contain 1-{maximum} visible characters"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::MemoryWorkspaceStore;
    use std::sync::Arc;

    fn project(name: &str) -> ProjectId {
        ProjectId::try_from(name.to_owned()).expect("valid project")
    }

    fn profile_id(name: &str) -> ProfileId {
        ProfileId::try_from(name.to_owned()).expect("valid profile")
    }

    fn agent_profile(project: &ProjectId) -> ProfileSpec {
        ProfileSpec {
            kind: ProfileKind::ServiceApi,
            auth_mode: ProfileAuthMode::ServiceBearerToken,
            bound_project: project.clone(),
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
        }
    }

    fn service(config: WorkspaceConfig) -> WorkspaceService {
        WorkspaceService::new(
            WorkspaceStoreService::new(Arc::new(MemoryWorkspaceStore::new())),
            config,
        )
        .expect("valid configuration")
    }

    fn shared_store() -> WorkspaceStoreService {
        WorkspaceStoreService::new(Arc::new(MemoryWorkspaceStore::new()))
    }

    fn over(store: &WorkspaceStoreService, config: WorkspaceConfig) -> WorkspaceService {
        WorkspaceService::new(store.clone(), config).expect("valid configuration")
    }

    fn base_config() -> WorkspaceConfig {
        WorkspaceConfig {
            display_name: "Default workspace".into(),
            workspace_id: None,
            projects: vec![project("desk"), project("legacy Prj")],
            profiles: std::iter::once((profile_id("desk-agent"), agent_profile(&project("desk"))))
                .collect(),
            grants: std::iter::once((
                "agent@example.test".to_owned(),
                std::iter::once(project("desk")).collect::<BTreeSet<_>>(),
            ))
            .collect(),
        }
    }

    #[tokio::test]
    async fn first_provisioning_binds_and_repeats_are_idempotent() {
        let workspace_service = service(base_config());
        let first = workspace_service
            .provision()
            .await
            .expect("first provision");
        assert!(first.created);
        let again = workspace_service
            .provision()
            .await
            .expect("repeat provision");
        assert!(!again.created);
        assert_eq!(again.workspace, first.workspace);
        assert!(again.orphaned_profiles.is_empty());
        assert_eq!(again.retained_grants, 0);
        let projects = workspace_service
            .registered_projects()
            .await
            .expect("projects");
        assert_eq!(
            projects,
            vec![project("desk"), project("legacy Prj")],
            "historical spelling registers verbatim"
        );
    }

    #[tokio::test]
    async fn a_different_pinned_identity_cannot_rebind_the_database() {
        let mut config = base_config();
        let store = shared_store();
        let bound = over(&store, config.clone())
            .provision()
            .await
            .expect("first")
            .workspace;
        config.workspace_id = Some(WorkspaceId::try_from("ws-other".to_owned()).expect("valid"));
        let error = over(&store, config)
            .provision()
            .await
            .expect_err("rebind must fail");
        assert_eq!(
            error,
            WorkspaceError::BindingConflict {
                existing: bound,
                requested: WorkspaceId::try_from("ws-other".to_owned()).expect("valid")
            }
        );
    }

    #[tokio::test]
    async fn the_same_pinned_identity_repeats_identically_and_survives_restart() {
        let mut config = base_config();
        config.workspace_id = Some(WorkspaceId::try_from("ws-pinned".to_owned()).expect("valid"));
        let store = WorkspaceStoreService::new(Arc::new(MemoryWorkspaceStore::new()));
        let first = WorkspaceService::new(store.clone(), config.clone())
            .expect("valid")
            .provision()
            .await
            .expect("first");
        // A "restart" is a new service over the same store.
        let restart = WorkspaceService::new(store, config).expect("valid");
        let report = restart.provision().await.expect("restart");
        assert!(!report.created);
        assert_eq!(report.workspace, first.workspace);
        assert_eq!(
            report.workspace,
            WorkspaceId::try_from("ws-pinned".to_owned()).expect("valid")
        );
    }

    #[tokio::test]
    async fn drifted_profile_policy_requires_explicit_reconciliation() {
        let mut config = base_config();
        let store = shared_store();
        over(&store, config.clone())
            .provision()
            .await
            .expect("first");
        config
            .profiles
            .get_mut(&profile_id("desk-agent"))
            .expect("configured profile")
            .permission_ceiling
            .insert("ticketing.manage".into());
        let error = over(&store, config)
            .provision()
            .await
            .expect_err("drift must fail");
        assert!(matches!(error, WorkspaceError::ReconciliationRequired(_)));
    }

    #[tokio::test]
    async fn configuration_only_profiles_are_reported_not_deleted() {
        let config = base_config();
        let store = shared_store();
        over(&store, config.clone())
            .provision()
            .await
            .expect("first");
        // A startup with the profile removed from configuration retains it.
        let mut reduced = config.clone();
        reduced.profiles.clear();
        let restarted = over(&store, reduced);
        let report = restarted.provision().await.expect("converge");
        assert_eq!(
            report.orphaned_profiles,
            vec![ProfileId::try_from("desk-agent".to_owned()).expect("valid")]
        );
        assert!(
            restarted
                .profile(&ProfileId::try_from("desk-agent".to_owned()).expect("valid"))
                .await
                .is_ok(),
            "provisioning never deletes profiles"
        );
    }

    #[tokio::test]
    async fn reserved_taxonomy_fails_at_configuration_time() {
        let mut config = base_config();
        config
            .profiles
            .get_mut(&profile_id("desk-agent"))
            .expect("configured profile")
            .kind = ProfileKind::NativeOidc;
        let error = WorkspaceService::new(
            WorkspaceStoreService::new(Arc::new(MemoryWorkspaceStore::new())),
            config,
        )
        .expect_err("reserved kind must fail closed");
        assert!(matches!(error, WorkspaceError::Configuration(_)));
    }

    #[tokio::test]
    async fn resolution_fails_closed_without_provisioning() {
        let workspace_service = service(base_config());
        let error = workspace_service
            .resolve_service_principal_scope(
                &ProfileId::try_from("desk-agent".to_owned()).expect("valid"),
            )
            .await
            .expect_err("unprovisioned");
        assert_eq!(error, WorkspaceError::NotProvisioned);
    }

    #[tokio::test]
    async fn granted_scope_requires_an_explicit_grant() {
        let workspace_service = service(base_config());
        workspace_service.provision().await.expect("provision");
        let granted = workspace_service
            .resolve_granted_scope("agent@example.test", &project("desk"))
            .await
            .expect("explicit grant resolves");
        assert_eq!(granted.project, project("desk"));
        // A subject without a grant is denied even for a registered project.
        let denied = workspace_service
            .resolve_granted_scope("stranger@example.test", &project("desk"))
            .await
            .expect_err("no grant");
        assert_eq!(
            denied,
            WorkspaceError::ScopeDenied("stranger@example.test".into(), project("desk"))
        );
        // A grant never extends to a project the subject was not granted.
        let foreign = workspace_service
            .resolve_granted_scope("agent@example.test", &project("legacy Prj"))
            .await
            .expect_err("grant is per project");
        assert_eq!(
            foreign,
            WorkspaceError::ScopeDenied("agent@example.test".into(), project("legacy Prj"))
        );
    }

    #[tokio::test]
    async fn service_principal_scope_never_widens_the_persisted_ceiling() {
        let mut config = base_config();
        config
            .profiles
            .get_mut(&profile_id("desk-agent"))
            .expect("configured profile")
            .permission_ceiling
            .insert("ticketing.manage".into());
        let workspace_service = service(config);
        workspace_service.provision().await.expect("provision");
        let resolved = workspace_service
            .resolve_service_principal_scope(
                &ProfileId::try_from("desk-agent".to_owned()).expect("valid"),
            )
            .await
            .expect("enabled profile resolves");
        assert_eq!(resolved.scope.project, project("desk"));
        assert_eq!(resolved.service_subject, "desk-agent");
        assert!(resolved.permission_ceiling.contains("ticketing.agent.read"));
        assert!(resolved.permission_ceiling.contains("ticketing.manage"));
        let unknown = workspace_service
            .resolve_service_principal_scope(
                &ProfileId::try_from("ghost".to_owned()).expect("valid"),
            )
            .await
            .expect_err("unknown profile");
        assert!(matches!(unknown, WorkspaceError::UnknownProfile(_)));
    }

    #[tokio::test]
    async fn session_and_unknown_project_selectors_fail_closed() {
        let workspace_service = service(base_config());
        workspace_service.provision().await.expect("provision");
        let session = workspace_service
            .validate_session_scope(&project("legacy Prj"))
            .await
            .expect("registered project validates");
        assert_eq!(session.project, project("legacy Prj"));
        let unregistered = workspace_service
            .validate_session_scope(&project("other"))
            .await
            .expect_err("unregistered project");
        assert_eq!(
            unregistered,
            WorkspaceError::UnregisteredProject(project("other"))
        );
    }
}
