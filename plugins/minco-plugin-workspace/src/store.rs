//! The workspace store port and its deterministic in-memory profile.
//!
//! The port is provisioning-shaped rather than a generic repository: the one
//! compound write is [`WorkspaceStore::commit_provision`], executed
//! atomically, so provisioning either lands completely or not at all.

use crate::model::{
    IntegrationProfile, PrincipalGrant, ProfileId, ProjectId, ProjectRegistration, Workspace,
    WorkspaceId,
};
use async_trait::async_trait;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex},
};

/// A store-level failure. `Conflict` reports a provisioning attempt that
/// collides with persisted ownership; the service maps it to explicit
/// reconciliation, never to a silent overwrite.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WorkspaceStoreError {
    #[error("workspace store conflict: {0}")]
    Conflict(String),
    #[error("workspace store failure: {0}")]
    Other(String),
}

impl WorkspaceStoreError {
    #[cfg(feature = "sqlite")]
    pub(crate) fn other(error: impl std::fmt::Display) -> Self {
        Self::Other(error.to_string())
    }
}

/// The one deployment binding plus every registry insert one provisioning
/// pass applies, atomically.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProvisionPlan {
    /// `Some` only for first provisioning; inserting over an existing
    /// binding conflicts.
    pub binding: Option<Workspace>,
    pub projects: Vec<ProjectRegistration>,
    pub profiles: Vec<IntegrationProfile>,
    pub grants: Vec<PrincipalGrant>,
}

/// Persistence port for the workspace registry.
#[async_trait]
pub trait WorkspaceStore: Send + Sync + std::fmt::Debug {
    /// The deployment binding, if this database was ever provisioned.
    async fn binding(&self) -> Result<Option<Workspace>, WorkspaceStoreError>;

    /// Apply one provisioning plan atomically: either every insert lands or
    /// none does. Inserting over existing ownership is a conflict.
    async fn commit_provision(&self, plan: &ProvisionPlan) -> Result<(), WorkspaceStoreError>;

    /// Projects registered under one workspace.
    async fn projects(
        &self,
        workspace: &WorkspaceId,
    ) -> Result<Vec<ProjectId>, WorkspaceStoreError>;

    /// One profile by identifier.
    async fn profile(
        &self,
        id: &ProfileId,
    ) -> Result<Option<IntegrationProfile>, WorkspaceStoreError>;

    /// Every profile under one workspace.
    async fn profiles(
        &self,
        workspace: &WorkspaceId,
    ) -> Result<Vec<IntegrationProfile>, WorkspaceStoreError>;

    /// Every profile id ever provisioned under one workspace (round 1
    /// finding 2): retained history distinguishes a genuinely new
    /// configuration seed from a previously provisioned profile that was
    /// removed.
    async fn profile_history(
        &self,
        workspace: &WorkspaceId,
    ) -> Result<std::collections::BTreeSet<String>, WorkspaceStoreError>;

    /// Every grant one subject holds under a workspace.
    async fn grants_for_subject(
        &self,
        workspace: &WorkspaceId,
        subject: &str,
    ) -> Result<Vec<PrincipalGrant>, WorkspaceStoreError>;

    /// Every grant registered under one workspace.
    async fn grants(
        &self,
        workspace: &WorkspaceId,
    ) -> Result<Vec<PrincipalGrant>, WorkspaceStoreError>;

    /// Connectivity/readiness probe.
    async fn ready(&self) -> Result<(), WorkspaceStoreError>;
}

/// Thin shared handle over the port, mirroring the ticketing store service
/// convention.
#[derive(Clone)]
pub struct WorkspaceStoreService(pub Arc<dyn WorkspaceStore>);

impl std::fmt::Debug for WorkspaceStoreService {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_tuple("WorkspaceStoreService").finish()
    }
}

impl WorkspaceStoreService {
    #[must_use]
    pub fn new(store: Arc<dyn WorkspaceStore>) -> Self {
        Self(store)
    }
}

#[derive(Debug, Default)]
struct MemoryState {
    binding: Option<Workspace>,
    projects: BTreeSet<(String, String)>,
    profiles: BTreeMap<String, IntegrationProfile>,
    grants: BTreeSet<(String, String, String)>,
    profile_history: BTreeSet<(String, String)>,
}

/// Deterministic in-memory profile for application tests and the plugin's
/// memory composition. One mutex makes `commit_provision` atomic.
#[derive(Debug, Default)]
pub struct MemoryWorkspaceStore(Mutex<MemoryState>);

impl MemoryWorkspaceStore {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl WorkspaceStore for MemoryWorkspaceStore {
    async fn binding(&self) -> Result<Option<Workspace>, WorkspaceStoreError> {
        Ok(self
            .0
            .lock()
            .expect("workspace store mutex")
            .binding
            .clone())
    }

    async fn commit_provision(&self, plan: &ProvisionPlan) -> Result<(), WorkspaceStoreError> {
        let mut state = self.0.lock().expect("workspace store mutex");
        if let Some(binding) = &plan.binding
            && state.binding.is_some()
        {
            return Err(WorkspaceStoreError::Conflict(format!(
                "database already bound to a workspace; refusing to bind {}",
                binding.id
            )));
        }
        let workspace = state
            .binding
            .as_ref()
            .or(plan.binding.as_ref())
            .map(|workspace: &Workspace| workspace.id.clone());
        let Some(workspace) = workspace else {
            return Err(WorkspaceStoreError::Other(
                "provision plan carries no binding for an unbound store".into(),
            ));
        };
        // Validate the complete plan before mutating anything so a conflict
        // rolls the whole pass back.
        for registration in &plan.projects {
            if registration.workspace != workspace {
                return Err(WorkspaceStoreError::Conflict(format!(
                    "project {} belongs to a foreign workspace",
                    registration.project
                )));
            }
            if state.projects.contains(&(
                workspace.as_str().to_owned(),
                registration.project.as_str().to_owned(),
            )) {
                return Err(WorkspaceStoreError::Conflict(format!(
                    "project {} is already registered",
                    registration.project
                )));
            }
        }
        for profile in &plan.profiles {
            if profile.workspace != workspace {
                return Err(WorkspaceStoreError::Conflict(format!(
                    "profile {} belongs to a foreign workspace",
                    profile.id
                )));
            }
            if state.profiles.contains_key(profile.id.as_str()) {
                return Err(WorkspaceStoreError::Conflict(format!(
                    "profile {} already exists",
                    profile.id
                )));
            }
        }
        for grant in &plan.grants {
            if grant.workspace != workspace {
                return Err(WorkspaceStoreError::Conflict(format!(
                    "grant for {} belongs to a foreign workspace",
                    grant.subject
                )));
            }
            let key = (
                workspace.as_str().to_owned(),
                grant.subject.clone(),
                grant.project.as_str().to_owned(),
            );
            if state.grants.contains(&key) {
                return Err(WorkspaceStoreError::Conflict(format!(
                    "grant for {} on {} already exists",
                    grant.subject, grant.project
                )));
            }
        }
        if let Some(binding) = &plan.binding {
            state.binding = Some(binding.clone());
        }
        for registration in &plan.projects {
            state.projects.insert((
                workspace.as_str().to_owned(),
                registration.project.as_str().to_owned(),
            ));
        }
        for profile in &plan.profiles {
            state
                .profiles
                .insert(profile.id.as_str().to_owned(), profile.clone());
            state.profile_history.insert((
                workspace.as_str().to_owned(),
                profile.id.as_str().to_owned(),
            ));
        }
        for grant in &plan.grants {
            state.grants.insert((
                workspace.as_str().to_owned(),
                grant.subject.clone(),
                grant.project.as_str().to_owned(),
            ));
        }
        drop(state);
        Ok(())
    }

    async fn projects(
        &self,
        workspace: &WorkspaceId,
    ) -> Result<Vec<ProjectId>, WorkspaceStoreError> {
        let state = self.0.lock().expect("workspace store mutex");
        Ok(state
            .projects
            .iter()
            .filter(|(bound, _)| bound == workspace.as_str())
            .map(|(_, project)| {
                ProjectId::try_from(project.clone()).expect("registered project stays valid")
            })
            .collect())
    }

    async fn profile(
        &self,
        id: &ProfileId,
    ) -> Result<Option<IntegrationProfile>, WorkspaceStoreError> {
        Ok(self
            .0
            .lock()
            .expect("workspace store mutex")
            .profiles
            .get(id.as_str())
            .cloned())
    }

    async fn profiles(
        &self,
        workspace: &WorkspaceId,
    ) -> Result<Vec<IntegrationProfile>, WorkspaceStoreError> {
        let state = self.0.lock().expect("workspace store mutex");
        Ok(state
            .profiles
            .values()
            .filter(|profile| &profile.workspace == workspace)
            .cloned()
            .collect())
    }

    async fn profile_history(
        &self,
        workspace: &WorkspaceId,
    ) -> Result<std::collections::BTreeSet<String>, WorkspaceStoreError> {
        let state = self.0.lock().expect("workspace store mutex");
        Ok(state
            .profile_history
            .iter()
            .filter(|(bound, _)| bound == workspace.as_str())
            .map(|(_, profile)| profile.clone())
            .collect())
    }

    async fn grants_for_subject(
        &self,
        workspace: &WorkspaceId,
        subject: &str,
    ) -> Result<Vec<PrincipalGrant>, WorkspaceStoreError> {
        let state = self.0.lock().expect("workspace store mutex");
        Ok(state
            .grants
            .iter()
            .filter(|(bound, grant_subject, _)| {
                bound == workspace.as_str() && grant_subject == subject
            })
            .map(|(bound, grant_subject, project)| PrincipalGrant {
                workspace: WorkspaceId::try_from(bound.clone())
                    .expect("bound workspace stays valid"),
                subject: grant_subject.clone(),
                project: ProjectId::try_from(project.clone()).expect("granted project stays valid"),
                granted_at: chrono::Utc::now(),
            })
            .collect())
    }

    async fn grants(
        &self,
        workspace: &WorkspaceId,
    ) -> Result<Vec<PrincipalGrant>, WorkspaceStoreError> {
        let state = self.0.lock().expect("workspace store mutex");
        Ok(state
            .grants
            .iter()
            .filter(|(bound, _, _)| bound == workspace.as_str())
            .map(|(bound, grant_subject, project)| PrincipalGrant {
                workspace: WorkspaceId::try_from(bound.clone())
                    .expect("bound workspace stays valid"),
                subject: grant_subject.clone(),
                project: ProjectId::try_from(project.clone()).expect("granted project stays valid"),
                granted_at: chrono::Utc::now(),
            })
            .collect())
    }

    async fn ready(&self) -> Result<(), WorkspaceStoreError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ProfileAuthMode;

    fn workspace() -> Workspace {
        Workspace {
            id: WorkspaceId::try_from("ws-1".to_owned()).expect("valid"),
            display_name: "Default workspace".into(),
            provisioned_at: chrono::Utc::now(),
        }
    }

    fn registration(workspace: &Workspace) -> ProjectRegistration {
        ProjectRegistration {
            workspace: workspace.id.clone(),
            project: ProjectId::try_from("desk".to_owned()).expect("valid"),
            registered_at: chrono::Utc::now(),
        }
    }

    fn plan(workspace: &Workspace) -> ProvisionPlan {
        ProvisionPlan {
            binding: Some(workspace.clone()),
            projects: vec![registration(workspace)],
            profiles: vec![],
            grants: vec![],
        }
    }

    #[tokio::test]
    async fn provisioning_binds_once_and_conflicts_on_second_workspace() {
        let store = MemoryWorkspaceStore::new();
        let first = workspace();
        store.commit_provision(&plan(&first)).await.expect("first");
        let second = plan(&Workspace {
            id: WorkspaceId::try_from("ws-2".to_owned()).expect("valid"),
            display_name: "Second".into(),
            provisioned_at: chrono::Utc::now(),
        });
        // A second workspace plan fails closed before any row-level check.
        let error = store
            .commit_provision(&second)
            .await
            .expect_err("second workspace must fail closed");
        assert!(matches!(error, WorkspaceStoreError::Conflict(_)));
        assert_eq!(
            store.binding().await.expect("binding").expect("bound").id,
            first.id
        );
    }

    #[tokio::test]
    async fn provisioning_conflict_rolls_the_whole_plan_back() {
        let store = MemoryWorkspaceStore::new();
        let workspace = workspace();
        store
            .commit_provision(&plan(&workspace))
            .await
            .expect("first");
        // Re-registering an existing project inside a larger plan rolls
        // everything back, including the new profile.
        let profile = IntegrationProfile {
            id: crate::model::ProfileId::try_from("desk-agent".to_owned()).expect("valid"),
            workspace: workspace.id.clone(),
            kind: crate::model::ProfileKind::ServiceApi,
            auth_mode: ProfileAuthMode::ServiceBearerToken,
            status: crate::model::ProfileStatus::Enabled,
            bound_project: ProjectId::try_from("desk".to_owned()).expect("valid"),
            service_subject: "desk-agent".into(),
            permission_ceiling: BTreeSet::new(),
            allowed_origins: BTreeSet::new(),
            resource_types: BTreeSet::new(),
            secret_reference: None,
            policy_digest: "0".repeat(64),
        };
        let conflicting = ProvisionPlan {
            binding: None,
            projects: vec![registration(&workspace)],
            profiles: vec![profile],
            grants: vec![],
        };
        assert!(store.commit_provision(&conflicting).await.is_err());
        assert!(
            store
                .profile(
                    &crate::model::ProfileId::try_from("desk-agent".to_owned()).expect("valid")
                )
                .await
                .expect("profile read")
                .is_none(),
            "conflicting provisioning must not leave partial state"
        );
    }

    #[tokio::test]
    async fn plan_without_binding_on_unbound_store_fails() {
        let store = MemoryWorkspaceStore::new();
        let workspace = workspace();
        let mut unbound = plan(&workspace);
        unbound.binding = None;
        let error = store
            .commit_provision(&unbound)
            .await
            .expect_err("no binding");
        assert!(matches!(error, WorkspaceStoreError::Other(_)));
    }
}
