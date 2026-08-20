use crate::{
    ConsumeHandoffRequest, ConsumedHandoff, CreateTicketInput, ExternalMessageIdentity,
    IngestExternalMessageRequest, RequesterTicket, Ticket, TicketActivityIntent, TicketAiContext,
    TicketAttachment, TicketFromHandoffInput, TicketId, TicketListFilter, TicketPriority,
    TicketStatus, TicketStoreError, TicketingStoreService,
};
use chrono::{DateTime, TimeDelta, Utc};
use minco_interaction::{
    AttachmentMetadata, SupportContext, SupportHandoffGrant, SupportLocationPolicy, SupportSurface,
    issue_support_handoff,
};
use minco_plugin_identity::Identity;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TicketingConfig {
    pub project_id: String,
    pub portal_origin: String,
    #[serde(default)]
    pub allowed_return_paths: BTreeMap<String, Vec<String>>,
    #[serde(default = "default_handoff_ttl_seconds")]
    pub handoff_ttl_seconds: i64,
    #[serde(default = "default_support_label")]
    pub support_label: String,
    #[serde(default = "default_support_brand")]
    pub support_brand: String,
    #[serde(default = "default_privacy_notice")]
    pub privacy_notice: String,
}

impl Default for TicketingConfig {
    fn default() -> Self {
        Self {
            project_id: "default".into(),
            portal_origin: "https://support.example.invalid".into(),
            allowed_return_paths: BTreeMap::new(),
            handoff_ttl_seconds: default_handoff_ttl_seconds(),
            support_label: default_support_label(),
            support_brand: default_support_brand(),
            privacy_notice: default_privacy_notice(),
        }
    }
}

impl TicketingConfig {
    pub fn validate(&self) -> Result<(), TicketingServiceError> {
        validate_text("project_id", &self.project_id, 100)?;
        validate_text("support_label", &self.support_label, 80)?;
        validate_text("support_brand", &self.support_brand, 80)?;
        validate_text("privacy_notice", &self.privacy_notice, 2_000)?;
        if !(1..=900).contains(&self.handoff_ttl_seconds) {
            return Err(TicketingServiceError::Configuration(
                "handoff_ttl_seconds must be between 1 and 900".into(),
            ));
        }
        self.location_policy().validate()?;
        Ok(())
    }

    #[must_use]
    pub fn location_policy(&self) -> SupportLocationPolicy {
        SupportLocationPolicy {
            portal_origin: self.portal_origin.clone(),
            allowed_return_paths: self.allowed_return_paths.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssueTicketingHandoffInput {
    pub project_id: String,
    pub requester_subject: String,
    pub requester_permissions: Vec<String>,
    pub surface: SupportSurface,
    pub context: SupportContext,
    pub return_location: String,
    pub correlation_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TicketingWarning {
    pub code: String,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TicketingMutationResult {
    pub ticket: Ticket,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<TicketingWarning>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequesterTicketResult {
    pub ticket: RequesterTicket,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<TicketingWarning>,
}

#[derive(Debug, Clone)]
pub struct TicketingService {
    store: TicketingStoreService,
    config: TicketingConfig,
}

impl TicketingService {
    pub fn new(
        store: TicketingStoreService,
        config: TicketingConfig,
    ) -> Result<Self, TicketingServiceError> {
        config.validate()?;
        Ok(Self { store, config })
    }

    #[must_use]
    pub const fn config(&self) -> &TicketingConfig {
        &self.config
    }

    pub async fn issue_ticketing_handoff(
        &self,
        principal: &Identity,
        input: IssueTicketingHandoffInput,
        now: DateTime<Utc>,
    ) -> Result<SupportHandoffGrant, TicketingServiceError> {
        authorize(principal, "ticketing.integrate")?;
        self.require_project(&input.project_id)?;
        let (handoff, grant) = issue_support_handoff(
            input.project_id,
            input.requester_subject,
            input.requester_permissions,
            input.surface,
            input.context,
            &input.return_location,
            input.correlation_id,
            &self.config.location_policy(),
            now,
            TimeDelta::seconds(self.config.handoff_ttl_seconds),
        )?;
        self.store.insert_handoff(handoff).await?;
        Ok(grant)
    }

    pub async fn create_ticket_from_handoff(
        &self,
        token: minco_interaction::SupportHandoffToken,
        project_id: &str,
        portal_origin: &str,
        input: TicketFromHandoffInput,
        now: DateTime<Utc>,
    ) -> Result<ConsumedHandoff, TicketingServiceError> {
        self.require_project(project_id)?;
        Ok(self
            .store
            .consume_and_create_ticket(ConsumeHandoffRequest::new(
                token,
                project_id,
                portal_origin,
                input,
                now,
            )?)
            .await?)
    }

    pub async fn create_ticket(
        &self,
        principal: &Identity,
        input: CreateTicketInput,
        correlation_id: Uuid,
        now: DateTime<Utc>,
    ) -> Result<TicketingMutationResult, TicketingServiceError> {
        authorize(principal, "ticketing.create")?;
        self.require_project(&input.project_id)?;
        if input.requester.subject != principal.subject
            && !principal.has_permission("ticketing.manage")
        {
            return Err(TicketingServiceError::RequesterMismatch);
        }
        let id = Uuid::now_v7();
        let display_reference = format!("TKT-{}", &id.simple().to_string()[..12]);
        let ticket = Ticket::create(input, display_reference, now)?;
        let intent = activity(&ticket, "ticketing.created", correlation_id, now);
        self.store.create(ticket.clone(), intent).await?;
        Ok(result(ticket))
    }

    pub async fn get_ticket_for_requester(
        &self,
        principal: &Identity,
        project_id: &str,
        id: TicketId,
    ) -> Result<RequesterTicket, TicketingServiceError> {
        authorize(principal, "ticketing.read")?;
        let ticket = self.load(project_id, id).await?;
        if ticket.requester.subject != principal.subject
            && !principal.has_permission("ticketing.manage")
        {
            return Err(TicketingServiceError::RequesterMismatch);
        }
        Ok(ticket.requester_projection())
    }

    pub async fn get_ticket_for_agent(
        &self,
        principal: &Identity,
        project_id: &str,
        id: TicketId,
    ) -> Result<Ticket, TicketingServiceError> {
        authorize(principal, "ticketing.read")?;
        self.load(project_id, id).await
    }

    pub async fn list_tickets(
        &self,
        principal: &Identity,
        filter: TicketListFilter,
    ) -> Result<Vec<Ticket>, TicketingServiceError> {
        authorize(principal, "ticketing.read")?;
        self.require_project(&filter.project_id)?;
        Ok(self.store.list(filter).await?)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn reply_as_requester(
        &self,
        principal: &Identity,
        project_id: &str,
        id: TicketId,
        body: String,
        expected_revision: u64,
        correlation_id: Uuid,
        now: DateTime<Utc>,
    ) -> Result<RequesterTicketResult, TicketingServiceError> {
        authorize(principal, "ticketing.reply")?;
        let mut ticket = self.load(project_id, id).await?;
        if ticket.requester.subject != principal.subject {
            return Err(TicketingServiceError::RequesterMismatch);
        }
        require_revision(&ticket, expected_revision)?;
        ticket.reply_as_requester(body, now)?;
        self.save(
            ticket.clone(),
            expected_revision,
            "ticketing.requester_replied",
            correlation_id,
            now,
        )
        .await?;
        Ok(RequesterTicketResult {
            ticket: ticket.requester_projection(),
            warnings: Vec::new(),
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn reply_as_agent(
        &self,
        principal: &Identity,
        project_id: &str,
        id: TicketId,
        body: String,
        expected_revision: u64,
        correlation_id: Uuid,
        now: DateTime<Utc>,
    ) -> Result<TicketingMutationResult, TicketingServiceError> {
        authorize(principal, "ticketing.reply")?;
        let mut ticket = self.load(project_id, id).await?;
        require_revision(&ticket, expected_revision)?;
        ticket.reply_as_agent(principal.subject.clone(), body, now)?;
        self.save(
            ticket.clone(),
            expected_revision,
            "ticketing.agent_replied",
            correlation_id,
            now,
        )
        .await?;
        Ok(result(ticket))
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn add_internal_note(
        &self,
        principal: &Identity,
        project_id: &str,
        id: TicketId,
        body: String,
        expected_revision: u64,
        correlation_id: Uuid,
        now: DateTime<Utc>,
    ) -> Result<TicketingMutationResult, TicketingServiceError> {
        authorize(principal, "ticketing.manage")?;
        let mut ticket = self.load(project_id, id).await?;
        require_revision(&ticket, expected_revision)?;
        ticket.add_internal_note(principal.subject.clone(), body, now)?;
        self.save(
            ticket.clone(),
            expected_revision,
            "ticketing.internal_note_added",
            correlation_id,
            now,
        )
        .await?;
        Ok(result(ticket))
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn assign_ticket(
        &self,
        principal: &Identity,
        project_id: &str,
        id: TicketId,
        assignee: String,
        expected_revision: u64,
        correlation_id: Uuid,
        now: DateTime<Utc>,
    ) -> Result<TicketingMutationResult, TicketingServiceError> {
        self.change_assignment(
            principal,
            project_id,
            id,
            Some(assignee),
            expected_revision,
            correlation_id,
            now,
        )
        .await
    }

    pub async fn unassign_ticket(
        &self,
        principal: &Identity,
        project_id: &str,
        id: TicketId,
        expected_revision: u64,
        correlation_id: Uuid,
        now: DateTime<Utc>,
    ) -> Result<TicketingMutationResult, TicketingServiceError> {
        self.change_assignment(
            principal,
            project_id,
            id,
            None,
            expected_revision,
            correlation_id,
            now,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn change_assignment(
        &self,
        principal: &Identity,
        project_id: &str,
        id: TicketId,
        assignee: Option<String>,
        expected_revision: u64,
        correlation_id: Uuid,
        now: DateTime<Utc>,
    ) -> Result<TicketingMutationResult, TicketingServiceError> {
        authorize(principal, "ticketing.manage")?;
        let mut ticket = self.load(project_id, id).await?;
        require_revision(&ticket, expected_revision)?;
        ticket.assign(assignee, now)?;
        self.save(
            ticket.clone(),
            expected_revision,
            "ticketing.assignment_changed",
            correlation_id,
            now,
        )
        .await?;
        Ok(result(ticket))
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn transfer_queue(
        &self,
        principal: &Identity,
        project_id: &str,
        id: TicketId,
        queue_id: String,
        expected_revision: u64,
        correlation_id: Uuid,
        now: DateTime<Utc>,
    ) -> Result<TicketingMutationResult, TicketingServiceError> {
        authorize(principal, "ticketing.manage")?;
        let mut ticket = self.load(project_id, id).await?;
        require_revision(&ticket, expected_revision)?;
        ticket.transfer_queue(queue_id, now)?;
        self.save(
            ticket.clone(),
            expected_revision,
            "ticketing.queue_transferred",
            correlation_id,
            now,
        )
        .await?;
        Ok(result(ticket))
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn change_priority(
        &self,
        principal: &Identity,
        project_id: &str,
        id: TicketId,
        priority: TicketPriority,
        expected_revision: u64,
        correlation_id: Uuid,
        now: DateTime<Utc>,
    ) -> Result<TicketingMutationResult, TicketingServiceError> {
        authorize(principal, "ticketing.manage")?;
        let mut ticket = self.load(project_id, id).await?;
        require_revision(&ticket, expected_revision)?;
        ticket.change_priority(priority, now);
        self.save(
            ticket.clone(),
            expected_revision,
            "ticketing.priority_changed",
            correlation_id,
            now,
        )
        .await?;
        Ok(result(ticket))
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn change_status(
        &self,
        principal: &Identity,
        project_id: &str,
        id: TicketId,
        status: TicketStatus,
        resolution: Option<String>,
        close_reason: Option<String>,
        expected_revision: u64,
        correlation_id: Uuid,
        now: DateTime<Utc>,
    ) -> Result<TicketingMutationResult, TicketingServiceError> {
        authorize(principal, "ticketing.manage")?;
        let mut ticket = self.load(project_id, id).await?;
        require_revision(&ticket, expected_revision)?;
        ticket.change_status(status, resolution, close_reason, now)?;
        self.save(
            ticket.clone(),
            expected_revision,
            "ticketing.status_changed",
            correlation_id,
            now,
        )
        .await?;
        Ok(result(ticket))
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn add_verified_attachment(
        &self,
        principal: &Identity,
        project_id: &str,
        id: TicketId,
        attachment: AttachmentMetadata,
        expected_revision: u64,
        correlation_id: Uuid,
        now: DateTime<Utc>,
    ) -> Result<TicketingMutationResult, TicketingServiceError> {
        authorize(principal, "ticketing.manage")?;
        let mut ticket = self.load(project_id, id).await?;
        require_revision(&ticket, expected_revision)?;
        ticket.add_attachment(TicketAttachment::from(attachment), now);
        self.save(
            ticket.clone(),
            expected_revision,
            "ticketing.attachment_added",
            correlation_id,
            now,
        )
        .await?;
        Ok(result(ticket))
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn ingest_external_message(
        &self,
        principal: &Identity,
        mut identity: ExternalMessageIdentity,
        id: TicketId,
        body: String,
        expected_revision: u64,
        correlation_id: Uuid,
        now: DateTime<Utc>,
    ) -> Result<TicketingMutationResult, TicketingServiceError> {
        authorize(principal, "ticketing.ingest")?;
        self.require_project(&identity.project_id)?;
        validate_sha256(&identity.content_sha256)?;
        identity.content_sha256.make_ascii_lowercase();
        if !valid_external_text(&identity.provider, 100)
            || !valid_external_text(&identity.mailbox_scope, 300)
            || !valid_external_text(&identity.external_id, 500)
            || identity.references.len() > 64
        {
            return Err(TicketingServiceError::InvalidExternalIdentity);
        }
        for value in [
            identity.raw_message_object_key.as_deref(),
            identity.internet_message_id.as_deref(),
            identity.in_reply_to.as_deref(),
        ]
        .into_iter()
        .flatten()
        .chain(identity.references.iter().map(String::as_str))
        {
            if value.trim().is_empty()
                || value.chars().count() > 1_000
                || value.chars().any(char::is_control)
            {
                return Err(TicketingServiceError::InvalidExternalIdentity);
            }
        }
        let outcome = self
            .store
            .ingest_external_message(IngestExternalMessageRequest {
                identity,
                ticket_id: id,
                body,
                expected_revision,
                correlation_id,
                now,
            })
            .await?;
        Ok(result(outcome.ticket))
    }

    pub async fn export_ai_context(
        &self,
        principal: &Identity,
        project_id: &str,
        id: TicketId,
    ) -> Result<TicketAiContext, TicketingServiceError> {
        authorize(principal, "ticketing.ai-context")?;
        Ok(self.load(project_id, id).await?.export_ai_context())
    }

    pub async fn ready(&self) -> Result<(), TicketingServiceError> {
        Ok(self.store.ready().await?)
    }

    async fn load(&self, project_id: &str, id: TicketId) -> Result<Ticket, TicketingServiceError> {
        self.require_project(project_id)?;
        self.store
            .get(project_id, id)
            .await?
            .ok_or(TicketingServiceError::NotFound(id))
    }

    async fn save(
        &self,
        ticket: Ticket,
        expected_revision: u64,
        kind: &str,
        correlation_id: Uuid,
        now: DateTime<Utc>,
    ) -> Result<(), TicketingServiceError> {
        let intent = activity(&ticket, kind, correlation_id, now);
        Ok(self.store.save(ticket, expected_revision, intent).await?)
    }

    fn require_project(&self, project_id: &str) -> Result<(), TicketingServiceError> {
        if project_id == self.config.project_id {
            Ok(())
        } else {
            Err(TicketingServiceError::ProjectDenied)
        }
    }
}

fn activity(
    ticket: &Ticket,
    kind: &str,
    correlation_id: Uuid,
    now: DateTime<Utc>,
) -> TicketActivityIntent {
    TicketActivityIntent::new(
        ticket.project_id.clone(),
        ticket.id,
        kind,
        correlation_id,
        serde_json::json!({ "ticket_id": ticket.id, "revision": ticket.revision, "status": ticket.status }),
        now,
    )
}

fn authorize(principal: &Identity, permission: &str) -> Result<(), TicketingServiceError> {
    principal
        .has_permission(permission)
        .then_some(())
        .ok_or_else(|| TicketingServiceError::PermissionDenied(permission.into()))
}

const fn require_revision(ticket: &Ticket, expected: u64) -> Result<(), TicketingServiceError> {
    if ticket.revision == expected {
        Ok(())
    } else {
        Err(TicketingServiceError::StaleRevision {
            expected,
            actual: ticket.revision,
        })
    }
}

fn validate_text(
    field: &'static str,
    value: &str,
    maximum: usize,
) -> Result<(), TicketingServiceError> {
    if value.trim().is_empty()
        || value.chars().count() > maximum
        || value.chars().any(char::is_control)
    {
        Err(TicketingServiceError::Configuration(format!(
            "{field} must contain 1-{maximum} visible characters"
        )))
    } else {
        Ok(())
    }
}

fn validate_sha256(value: &str) -> Result<(), TicketingServiceError> {
    if value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(TicketingServiceError::InvalidContentDigest)
    }
}

fn valid_external_text(value: &str, maximum: usize) -> bool {
    !value.trim().is_empty()
        && value.chars().count() <= maximum
        && !value.chars().any(char::is_control)
}

const fn result(ticket: Ticket) -> TicketingMutationResult {
    TicketingMutationResult {
        ticket,
        warnings: Vec::new(),
    }
}

const fn default_handoff_ttl_seconds() -> i64 {
    120
}
fn default_support_label() -> String {
    "Get support".into()
}
fn default_support_brand() -> String {
    "Support".into()
}
fn default_privacy_notice() -> String {
    "Share only information needed to resolve this request.".into()
}

#[derive(Debug, thiserror::Error)]
pub enum TicketingServiceError {
    #[error("permission denied: {0}")]
    PermissionDenied(String),
    #[error("ticketing project is not available to this operation")]
    ProjectDenied,
    #[error("the requester does not match the authenticated subject")]
    RequesterMismatch,
    #[error("ticket was not found: {0}")]
    NotFound(TicketId),
    #[error("ticket revision is stale: expected {expected}, actual {actual}")]
    StaleRevision { expected: u64, actual: u64 },
    #[error("external message content digest must be hexadecimal SHA-256")]
    InvalidContentDigest,
    #[error("external message identity is invalid")]
    InvalidExternalIdentity,
    #[error("invalid ticketing configuration: {0}")]
    Configuration(String),
    #[error(transparent)]
    SupportEntry(#[from] minco_interaction::SupportEntryError),
    #[error(transparent)]
    Validation(#[from] crate::TicketValidationError),
    #[error(transparent)]
    Store(#[from] TicketStoreError),
}

#[must_use]
pub fn external_content_sha256(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MemoryTicketingStore, TicketChannel, TicketRequester};
    use std::{collections::BTreeSet, sync::Arc};

    fn identity(subject: &str, permissions: &[&str]) -> Identity {
        Identity {
            subject: subject.into(),
            permissions: permissions
                .iter()
                .map(|value| (*value).into())
                .collect::<BTreeSet<_>>(),
            scopes: BTreeSet::new(),
            claims: BTreeMap::new(),
        }
    }

    fn service() -> TicketingService {
        TicketingService::new(
            TicketingStoreService::new(Arc::new(MemoryTicketingStore::default())),
            TicketingConfig {
                project_id: "project-a".into(),
                portal_origin: "https://support.example.test".into(),
                allowed_return_paths: BTreeMap::from([(
                    "https://app.example.test".into(),
                    vec!["/orders".into()],
                )]),
                ..TicketingConfig::default()
            },
        )
        .unwrap()
    }

    #[tokio::test]
    async fn authorization_precedes_persistence_and_requesters_are_isolated() {
        let service = service();
        let input = CreateTicketInput {
            project_id: "project-a".into(),
            subject: "Help".into(),
            description: "Broken".into(),
            requester: TicketRequester {
                subject: "user-a".into(),
                display_name: None,
                email: None,
            },
            channel: TicketChannel::Api,
            priority: TicketPriority::Normal,
            resource_references: Vec::new(),
        };
        assert!(matches!(
            service
                .create_ticket(
                    &identity("user-a", &[]),
                    input.clone(),
                    Uuid::now_v7(),
                    Utc::now()
                )
                .await,
            Err(TicketingServiceError::PermissionDenied(_))
        ));
        let created = service
            .create_ticket(
                &identity("user-a", &["ticketing.create"]),
                input,
                Uuid::now_v7(),
                Utc::now(),
            )
            .await
            .unwrap();
        assert!(matches!(
            service
                .get_ticket_for_requester(
                    &identity("user-b", &["ticketing.read"]),
                    "project-a",
                    created.ticket.id
                )
                .await,
            Err(TicketingServiceError::RequesterMismatch)
        ));
    }

    #[tokio::test]
    async fn external_content_digest_is_canonical_before_idempotency() {
        let service = service();
        let now = Utc::now();
        let created = service
            .create_ticket(
                &identity("user-a", &["ticketing.create"]),
                CreateTicketInput {
                    project_id: "project-a".into(),
                    subject: "Help".into(),
                    description: "Broken".into(),
                    requester: TicketRequester {
                        subject: "user-a".into(),
                        display_name: None,
                        email: None,
                    },
                    channel: TicketChannel::Api,
                    priority: TicketPriority::Normal,
                    resource_references: Vec::new(),
                },
                Uuid::now_v7(),
                now,
            )
            .await
            .unwrap();
        let digest = external_content_sha256(b"same message");
        let external = |content_sha256: String| ExternalMessageIdentity {
            project_id: "project-a".into(),
            provider: "mail".into(),
            mailbox_scope: "support@example.test".into(),
            external_id: "message-1".into(),
            content_sha256,
            raw_message_object_key: None,
            internet_message_id: None,
            in_reply_to: None,
            references: Vec::new(),
        };
        let first = service
            .ingest_external_message(
                &identity("ingress", &["ticketing.ingest"]),
                external(digest.to_ascii_uppercase()),
                created.ticket.id,
                "Reply".into(),
                0,
                Uuid::now_v7(),
                now,
            )
            .await
            .unwrap();
        let replay = service
            .ingest_external_message(
                &identity("ingress", &["ticketing.ingest"]),
                external(digest),
                created.ticket.id,
                "Reply".into(),
                0,
                Uuid::now_v7(),
                now,
            )
            .await
            .unwrap();

        assert_eq!(replay.ticket, first.ticket);
    }
}
