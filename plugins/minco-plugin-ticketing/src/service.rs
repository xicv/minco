use crate::{
    AppendTicketMessageRequest, ConsumeHandoffRequest, ConsumeSessionRequest, ConsumedHandoff,
    CreateTicketInput, DeliveryFeedbackKind, ExternalMessageIdentity, IngestExternalMessageRequest,
    MessageListFilter, OutboundDeliveryEvidence, OutboundEvidenceKind, PublicTicketMessage,
    PublicTicketSummary, RequesterTicket, Ticket, TicketActivityIntent, TicketAiContext,
    TicketAttachment, TicketFromHandoffInput, TicketId, TicketListFilter, TicketMessage,
    TicketMessageId, TicketMessageKind, TicketPriority, TicketStatus, TicketStoreError,
    TicketSummary, TicketSummaryFilter, TicketingStoreService,
};
use chrono::{DateTime, TimeDelta, Utc};
use minco_interaction::{
    AttachmentMetadata, SupportContext, SupportHandoffGrant, SupportHandoffToken,
    SupportLocationPolicy, SupportSurface, issue_support_handoff,
};
use minco_plugin_identity::Identity;
use minco_plugin_sessions::{
    CsrfService, CsrfToken, SessionId, SessionRecord, SessionService, SessionToken,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    sync::Arc,
};
use uuid::Uuid;

/// Optional portal services resolved from the service registry at install;
/// the base plugin works without all of them.
#[derive(Clone, Default)]
pub struct TicketingPortalServices {
    pub sessions: Option<Arc<SessionService>>,
    pub csrf: Option<Arc<CsrfService>>,
    pub idempotency: Option<Arc<minco_plugin_idempotency::IdempotencyService>>,
    pub events: Option<Arc<minco_plugin_events::EventServices>>,
    /// Durable job submission handle (ADR-0058); present when the jobs
    /// feature is enabled and the application registered the jobs plugin.
    #[cfg(feature = "jobs")]
    pub jobs: Option<Arc<minco_plugin_jobs::JobsServices>>,
    /// Object-storage handle (ADR-0059); a required install dependency
    /// that the inbound wake use case consumes.
    pub objects: Option<Arc<minco_plugin_object_storage::ObjectStoreService>>,
}

impl fmt::Debug for TicketingPortalServices {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Redacted: these services hold session and idempotency state.
        let _ = self;
        formatter
            .debug_struct("TicketingPortalServices")
            .field("sessions", &self.sessions.is_some())
            .field("csrf", &self.csrf.is_some())
            .field("idempotency", &self.idempotency.is_some())
            .field("events", &self.events.is_some())
            .finish_non_exhaustive()
    }
}

/// Explicit SLA bounds (ADR-0068): hours from creation; `0` disables
/// that deadline while keeping the other.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TicketSlaConfig {
    pub first_response_hours: u32,
    pub resolution_hours: u32,
}

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
    #[serde(default = "default_requester_session_ttl_seconds")]
    pub requester_session_ttl_seconds: i64,
    /// Explicit assignment pool for the round-robin and least-workload
    /// modes (ADR-0068): bounded, unique agent subjects.
    #[serde(default)]
    pub assignment_pool: Vec<String>,
    /// Optional SLA: when configured, ticket creation snapshots
    /// first-response and resolution deadlines (ADR-0068).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sla: Option<TicketSlaConfig>,
    /// When true (and the `jobs` feature is enabled and an enqueue adapter
    /// is configured), a public agent reply also enqueues a
    /// `ticketing.deliver-public-notification` job in the same transaction
    /// (ADR-0054). Default false: no application gets a queue by surprise.
    #[serde(default)]
    pub notify_requester_on_public_reply: bool,
    /// Private development automation (ADR-0070). Off by default: no
    /// application gets automation by surprise.
    #[serde(default)]
    pub automation: crate::AutomationConfig,
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
            requester_session_ttl_seconds: default_requester_session_ttl_seconds(),
            assignment_pool: Vec::new(),
            sla: None,
            notify_requester_on_public_reply: false,
            automation: crate::AutomationConfig::default(),
        }
    }
}

impl TicketingConfig {
    pub fn validate(&self) -> Result<(), TicketingServiceError> {
        validate_text("project_id", &self.project_id, 100)?;
        if self.assignment_pool.len() > 32
            || self
                .assignment_pool
                .iter()
                .any(|subject| subject.trim().is_empty() || subject.chars().count() > 300)
            || self.assignment_pool.iter().collect::<BTreeSet<_>>().len()
                != self.assignment_pool.len()
        {
            return Err(TicketingServiceError::Configuration(
                "assignment_pool must contain at most 32 unique non-empty subjects".into(),
            ));
        }
        if let Some(sla) = &self.sla
            && !(1..=8_760).contains(&sla.first_response_hours)
            && sla.first_response_hours != 0
        {
            return Err(TicketingServiceError::Configuration(
                "sla.first_response_hours must be 0..=8760".into(),
            ));
        }
        if let Some(sla) = &self.sla
            && !(1..=87_600).contains(&sla.resolution_hours)
            && sla.resolution_hours != 0
        {
            return Err(TicketingServiceError::Configuration(
                "sla.resolution_hours must be 0..=87600".into(),
            ));
        }
        validate_text("support_label", &self.support_label, 80)?;
        validate_text("support_brand", &self.support_brand, 80)?;
        validate_text("privacy_notice", &self.privacy_notice, 2_000)?;
        if !(1..=900).contains(&self.handoff_ttl_seconds) {
            return Err(TicketingServiceError::Configuration(
                "handoff_ttl_seconds must be between 1 and 900".into(),
            ));
        }
        if !(1..=86_400).contains(&self.requester_session_ttl_seconds) {
            return Err(TicketingServiceError::Configuration(
                "requester_session_ttl_seconds must be between 1 and 86400".into(),
            ));
        }
        if self.automation.profile != crate::AutomationProfile::Off
            && self.automation.review == crate::AutomationReview::Disabled
        {
            // Trusted deterministic verification does not ship in this
            // slice, so review-disabled automation fails closed (ADR-0070).
            return Err(TicketingServiceError::Configuration(
                "automation review may not be disabled: trusted verification is not available"
                    .into(),
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

/// Per-feature implemented-and-enabled truth for the support bootstrap.
/// Every field defaults to `false`; the service sets exactly what is
/// implemented and registered (ADR-0053).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
// A capability set is one boolean per real feature.
#[allow(clippy::struct_excessive_bools)]
pub struct SupportCapabilities {
    pub portal_sessions: bool,
    pub history: bool,
    pub files: bool,
    pub screenshots: bool,
    pub voice: bool,
    pub knowledge: bool,
    pub email: bool,
    pub automation: bool,
}

/// Truthful, permission-derived agent console capabilities. Every field maps
/// to a real operation the console calls; nothing is claimed that the
/// authenticated principal cannot do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
// A capability set is one boolean per real operation; grouping them in a
// nested struct would not improve honesty or readability.
#[allow(clippy::struct_excessive_bools)]
pub struct AgentConsoleCapabilities {
    pub create: bool,
    pub reply: bool,
    pub internal_note: bool,
    pub manage: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentConsoleBootstrap {
    pub schema_version: u16,
    pub project_id: String,
    pub brand: String,
    pub label: String,
    pub subject: String,
    pub capabilities: AgentConsoleCapabilities,
}

/// One-time requester session grant. The bearer token is serialized exactly
/// once, in the exchange response; the sessions crate redacts it from Debug.
pub struct RequesterSessionGrant {
    pub token: SessionToken,
    pub expires_at: DateTime<Utc>,
    pub csrf_token: CsrfToken,
}

impl fmt::Debug for RequesterSessionGrant {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        // The bearer and CSRF token are redacted from Debug output.
        formatter
            .debug_struct("RequesterSessionGrant")
            .field("token", &"[REDACTED]")
            .field("expires_at", &self.expires_at)
            .field("csrf_token", &"[REDACTED]")
            .finish()
    }
}

/// One atomic agent management decision. Absent fields stay unchanged;
/// `clear_assignee` unassigns. Validation covers the complete change set
/// before any persistence happens.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AgentManagementInput {
    pub priority: Option<TicketPriority>,
    pub assignee_subject: Option<String>,
    pub clear_assignee: bool,
    pub queue_id: Option<String>,
    pub status: Option<TicketStatus>,
    pub resolution: Option<String>,
    pub close_reason: Option<String>,
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

#[derive(Clone)]
pub struct TicketingService {
    store: TicketingStoreService,
    config: TicketingConfig,
    pub(crate) portal: TicketingPortalServices,
}

impl fmt::Debug for TicketingService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TicketingService")
            .field("config", &self.config)
            .field("portal", &self.portal)
            .finish_non_exhaustive()
    }
}

/// One clarification draft decision (ADR-0071).
#[derive(Debug, Clone)]
pub struct ClarificationDraftInput {
    pub reason: crate::ClarificationReason,
    pub questions: Vec<crate::ClarificationQuestion>,
    pub checkpoint: String,
}

impl TicketingService {
    pub fn new(
        store: TicketingStoreService,
        config: TicketingConfig,
    ) -> Result<Self, TicketingServiceError> {
        config.validate()?;
        Ok(Self {
            store,
            config,
            portal: TicketingPortalServices::default(),
        })
    }

    #[must_use]
    pub fn with_portal_services(mut self, portal: TicketingPortalServices) -> Self {
        self.portal = portal;
        self
    }

    #[must_use]
    pub const fn portal_services(&self) -> &TicketingPortalServices {
        &self.portal
    }

    #[must_use]
    pub const fn config(&self) -> &TicketingConfig {
        &self.config
    }

    /// Per-feature implemented-and-enabled truth (ADR-0053): computed from
    /// registered services and implemented operations, never hard-coded.
    #[must_use]
    pub const fn support_capabilities(&self) -> crate::SupportCapabilities {
        crate::SupportCapabilities {
            portal_sessions: self.portal.sessions.is_some() && self.portal.csrf.is_some(),
            history: true,
            files: false,
            screenshots: false,
            voice: false,
            knowledge: false,
            email: false,
            automation: false,
        }
    }

    pub async fn issue_ticketing_handoff(
        &self,
        principal: &Identity,
        mut input: IssueTicketingHandoffInput,
        now: DateTime<Utc>,
    ) -> Result<SupportHandoffGrant, TicketingServiceError> {
        authorize(principal, "ticketing.integrate")?;
        self.require_project(&input.project_id)?;
        // Handoffs mint requester portal sessions; the granted set is
        // constrained to the requester portal capabilities and can never
        // carry agent or integration permissions into a session (review
        // finding 10). An omitted set means the full requester set.
        if input.requester_permissions.is_empty() {
            input.requester_permissions = REQUESTER_PORTAL_CAPABILITIES
                .iter()
                .map(|value| (*value).to_owned())
                .collect();
        }
        for permission in &input.requester_permissions {
            if !REQUESTER_PORTAL_CAPABILITIES.contains(&permission.as_str()) {
                return Err(TicketingServiceError::Configuration(format!(
                    "requester permissions must be a subset of {REQUESTER_PORTAL_CAPABILITIES:?}; \
                     got {permission:?}"
                )));
            }
        }
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
        let mut input = input;
        if let Some(sla) = self.config.sla {
            if sla.first_response_hours > 0 {
                input.first_response_deadline =
                    Some(now + TimeDelta::hours(i64::from(sla.first_response_hours)));
            }
            if sla.resolution_hours > 0 {
                input.resolution_deadline =
                    Some(now + TimeDelta::hours(i64::from(sla.resolution_hours)));
            }
        }
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
        // The full v7 suffix is required: the leading 12 hex characters are
        // only the millisecond timestamp, so two tickets created within the
        // same millisecond would collide on the display reference.
        let display_reference = format!("TKT-{}", id.simple());
        let ticket = Ticket::create(input, display_reference, now)?;
        let ticket = self.apply_sla(ticket);
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
        authorize(principal, "ticketing.requester.read")?;
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
        authorize(principal, "ticketing.agent.read")?;
        self.load(project_id, id).await
    }

    pub async fn list_tickets(
        &self,
        principal: &Identity,
        filter: TicketListFilter,
    ) -> Result<Vec<Ticket>, TicketingServiceError> {
        authorize(principal, "ticketing.agent.read")?;
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
        authorize(principal, "ticketing.requester.write")?;
        let mut ticket = self.load(project_id, id).await?;
        if ticket.requester.subject != principal.subject {
            return Err(TicketingServiceError::RequesterMismatch);
        }
        require_revision(&ticket, expected_revision)?;
        let message = ticket.reply_as_requester_message(body, now)?;
        self.append_message(
            &ticket,
            message,
            "ticketing.requester_replied",
            expected_revision,
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
        let message = ticket.reply_as_agent_message(&principal.subject, body, now)?;
        #[cfg(feature = "jobs")]
        let job_records = self.notification_records(&ticket, &message, correlation_id, now)?;
        #[cfg(not(feature = "jobs"))]
        if self.config.notify_requester_on_public_reply {
            // The configuration requests notifications but this build has no
            // jobs bridge; fail closed instead of silently skipping.
            return Err(TicketingServiceError::Configuration(
                "notify_requester_on_public_reply requires the jobs feature".into(),
            ));
        }
        #[cfg(feature = "jobs")]
        return self
            .append_message_with_jobs(
                &ticket,
                message,
                "ticketing.agent_replied",
                expected_revision,
                correlation_id,
                now,
                job_records,
            )
            .await
            .map(|()| result(ticket));
        #[cfg(not(feature = "jobs"))]
        {
            self.append_message(
                &ticket,
                message,
                "ticketing.agent_replied",
                expected_revision,
                correlation_id,
                now,
            )
            .await?;
            Ok(result(ticket))
        }
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
        let message = ticket.internal_note_message(&principal.subject, body, now)?;
        self.append_message(
            &ticket,
            message,
            "ticketing.internal_note_added",
            expected_revision,
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

    #[must_use]
    fn apply_sla(&self, ticket: Ticket) -> Ticket {
        match self.config.sla {
            Some(sla) => ticket.with_sla_snapshots(sla),
            None => ticket,
        }
    }

    /// One assignment decision (ADR-0068): manual carries (or clears)
    /// the subject; the pool modes pick from the configured assignment
    /// pool — round-robin through a durable cursor, least-workload by
    /// the fewest open tickets with a lexicographic tie-break.
    #[allow(clippy::too_many_arguments)]
    pub async fn assign_ticket_by_mode(
        &self,
        principal: &Identity,
        project_id: &str,
        id: TicketId,
        mode: crate::AssignmentMode,
        manual: Option<String>,
        expected_revision: u64,
        correlation_id: Uuid,
        now: DateTime<Utc>,
    ) -> Result<TicketingMutationResult, TicketingServiceError> {
        authorize(principal, "ticketing.manage")?;
        self.require_project(project_id)?;
        let assignee = match mode {
            crate::AssignmentMode::Manual => manual,
            crate::AssignmentMode::RoundRobin => {
                let pool = self.config.assignment_pool.clone();
                if pool.is_empty() {
                    return Err(TicketingServiceError::Configuration(
                        "assignment_pool is not configured".into(),
                    ));
                }
                let index = self
                    .store
                    .advance_assignment_cursor(project_id, pool.len())
                    .await?;
                // The cursor index is bounded by the pool length by
                // construction; unwrap_or_else keeps this total.
                Some(pool.get(index).cloned().unwrap_or_default())
            }
            crate::AssignmentMode::LeastWorkload => {
                let pool = self.config.assignment_pool.clone();
                if pool.is_empty() {
                    return Err(TicketingServiceError::Configuration(
                        "assignment_pool is not configured".into(),
                    ));
                }
                let workload = self.store.assignee_workload(project_id, &pool).await?;
                Some(
                    pool.iter()
                        .min_by_key(|subject| {
                            (workload.get(*subject).copied().unwrap_or(0), *subject)
                        })
                        .cloned()
                        // A non-empty pool always has a minimum.
                        .unwrap_or_default(),
                )
            }
        };
        self.change_assignment(
            principal,
            project_id,
            id,
            assignee,
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
                correlation_id,
                now,
            })
            .await?;
        Ok(result(outcome.ticket))
    }

    /// Records provider delivery feedback (bounce, complaint or delay) as
    /// append-only evidence for one outbound public reply (ADR-0063).
    /// Feedback must reference an existing public reply; orphan or
    /// misattributed feedback fails closed without persisting anything.
    #[allow(clippy::too_many_arguments)]
    pub async fn record_delivery_feedback(
        &self,
        principal: &Identity,
        project_id: &str,
        ticket_id: TicketId,
        message_id: TicketMessageId,
        feedback: DeliveryFeedbackKind,
        provider: &str,
        provider_message_id: &str,
        now: DateTime<Utc>,
    ) -> Result<(), TicketingServiceError> {
        authorize(principal, "ticketing.ingest")?;
        self.require_project(project_id)?;
        if !valid_external_text(provider, 100)
            || !valid_external_text(provider_message_id, 500)
            || provider_message_id.trim().is_empty()
        {
            return Err(TicketingServiceError::InvalidDeliveryFeedback);
        }
        let ticket = self.load(project_id, ticket_id).await?;
        if !ticket.messages.iter().any(|message| {
            message.id == message_id && message.kind == TicketMessageKind::PublicReply
        }) {
            return Err(TicketingServiceError::InvalidDeliveryFeedback);
        }
        self.store
            .append_outbound_evidence(OutboundDeliveryEvidence {
                project_id: project_id.to_owned(),
                ticket_id,
                message_id,
                kind: OutboundEvidenceKind::Feedback,
                provider: provider.to_owned(),
                provider_message_id: provider_message_id.to_owned(),
                feedback: Some(feedback),
                failure_kind: None,
                recorded_at: now,
            })
            .await?;
        Ok(())
    }

    /// Authoritative reconciliation of one ambiguous outbound delivery
    /// (review finding 8): the caller has verified the provider outcome
    /// out of band. `accepted` records provider acceptance — which
    /// suppresses every future resend; a reconciled non-send is recorded
    /// as evidence so a redrive may resend under the same stable message
    /// identity. Reconciliation must reference an existing public reply.
    #[allow(clippy::too_many_arguments)]
    pub async fn reconcile_outbound_delivery(
        &self,
        principal: &Identity,
        project_id: &str,
        ticket_id: TicketId,
        message_id: TicketMessageId,
        accepted: bool,
        provider: &str,
        provider_message_id: &str,
        now: DateTime<Utc>,
    ) -> Result<(), TicketingServiceError> {
        authorize(principal, "ticketing.ingest")?;
        self.require_project(project_id)?;
        if !valid_external_text(provider, 100)
            || !valid_external_text(provider_message_id, 500)
            || provider_message_id.trim().is_empty()
        {
            return Err(TicketingServiceError::InvalidDeliveryFeedback);
        }
        let ticket = self.load(project_id, ticket_id).await?;
        if !ticket.messages.iter().any(|message| {
            message.id == message_id && message.kind == TicketMessageKind::PublicReply
        }) {
            return Err(TicketingServiceError::InvalidDeliveryFeedback);
        }
        self.store
            .append_outbound_evidence(OutboundDeliveryEvidence {
                project_id: project_id.to_owned(),
                ticket_id,
                message_id,
                kind: if accepted {
                    OutboundEvidenceKind::Accepted
                } else {
                    OutboundEvidenceKind::PermanentFailure
                },
                provider: provider.to_owned(),
                provider_message_id: provider_message_id.to_owned(),
                feedback: None,
                failure_kind: (!accepted).then(|| "reconciled_not_sent".to_owned()),
                recorded_at: now,
            })
            .await?;
        Ok(())
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

    pub fn agent_bootstrap(
        &self,
        principal: &Identity,
    ) -> Result<AgentConsoleBootstrap, TicketingServiceError> {
        authorize(principal, "ticketing.agent-console")?;
        Ok(AgentConsoleBootstrap {
            schema_version: 1,
            project_id: self.config.project_id.clone(),
            brand: self.config.support_brand.clone(),
            label: self.config.support_label.clone(),
            subject: principal.subject.clone(),
            capabilities: AgentConsoleCapabilities {
                create: principal.has_permission("ticketing.create"),
                reply: principal.has_permission("ticketing.reply"),
                internal_note: principal.has_permission("ticketing.manage"),
                manage: principal.has_permission("ticketing.agent.manage"),
            },
        })
    }

    pub async fn list_ticket_summaries(
        &self,
        principal: &Identity,
        filter: TicketSummaryFilter,
    ) -> Result<Vec<TicketSummary>, TicketingServiceError> {
        authorize(principal, "ticketing.agent.read")?;
        self.require_project(&filter.project_id)?;
        Ok(self.store.list_summaries(filter).await?)
    }

    /// Drafts a clarification (ADR-0071): bounded questions plus the
    /// resume checkpoint; nothing reaches the requester until sent.
    pub async fn create_clarification_draft(
        &self,
        principal: &Identity,
        project_id: &str,
        ticket_id: TicketId,
        draft: ClarificationDraftInput,
        now: DateTime<Utc>,
    ) -> Result<crate::Clarification, TicketingServiceError> {
        authorize(principal, "ticketing.manage")?;
        self.require_project(project_id)?;
        // The ticket must exist: clarifications anchor to a real ticket.
        let _ = self.load(project_id, ticket_id).await?;
        let clarification = crate::Clarification::new_draft(
            ticket_id,
            draft.reason,
            draft.questions,
            &draft.checkpoint,
            &principal.subject,
            now,
        )?;
        self.store
            .insert_clarification(project_id, clarification.clone())
            .await?;
        Ok(clarification)
    }

    /// The human send decision (ADR-0071): transitions a draft to sent.
    /// Automation may draft; only this decision asks the requester.
    pub async fn send_clarification(
        &self,
        principal: &Identity,
        project_id: &str,
        id: Uuid,
        now: DateTime<Utc>,
    ) -> Result<crate::Clarification, TicketingServiceError> {
        authorize(principal, "ticketing.manage")?;
        self.require_project(project_id)?;
        let mut clarification = self
            .store
            .get_clarification(project_id, id)
            .await?
            .ok_or(TicketingServiceError::NotFound(TicketId(id)))?;
        clarification.send(now)?;
        self.store
            .update_clarification(project_id, clarification.clone())
            .await?;
        Ok(clarification)
    }

    /// Agent-only listing with full state including checkpoints.
    pub async fn list_clarifications(
        &self,
        principal: &Identity,
        project_id: &str,
        ticket_id: TicketId,
    ) -> Result<Vec<crate::Clarification>, TicketingServiceError> {
        authorize(principal, "ticketing.agent.read")?;
        self.require_project(project_id)?;
        Ok(self
            .store
            .list_clarifications(project_id, ticket_id)
            .await?)
    }

    /// The requester's own sent clarifications, projected safely.
    pub async fn list_requester_clarifications(
        &self,
        principal: &Identity,
    ) -> Result<Vec<crate::RequesterClarification>, TicketingServiceError> {
        authorize(principal, "ticketing.requester.read")?;
        let project_id = self.config().project_id.clone();
        let tickets = self
            .store
            .list_summaries(TicketSummaryFilter {
                project_id: project_id.clone(),
                requester_subject: Some(principal.subject.clone()),
                limit: crate::MAX_TICKET_LIST_FETCH_LIMIT,
                ..TicketSummaryFilter {
                    project_id: project_id.clone(),
                    statuses: BTreeSet::new(),
                    queue_id: None,
                    assignee_subject: None,
                    requester_subject: None,
                    unassigned: false,
                    query: None,
                    before_updated_at: None,
                    before_id: None,
                    limit: 0,
                }
            })
            .await?;
        let mut projected = Vec::new();
        for summary in tickets {
            let items = self
                .store
                .list_clarifications(&project_id, summary.id)
                .await?;
            for item in items {
                if matches!(
                    item.state,
                    crate::ClarificationState::Sent | crate::ClarificationState::Answered
                ) {
                    projected.push(item.requester_projection());
                }
            }
        }
        Ok(projected)
    }

    /// The requester answers exactly once (ADR-0071); answering exposes
    /// the checkpoint to agents for resuming, never to the requester.
    pub async fn reply_to_clarification(
        &self,
        principal: &Identity,
        project_id: &str,
        id: Uuid,
        answers: Vec<String>,
        now: DateTime<Utc>,
    ) -> Result<crate::RequesterClarification, TicketingServiceError> {
        authorize(principal, "ticketing.requester.write")?;
        self.require_project(project_id)?;
        let mut clarification = self
            .store
            .get_clarification(project_id, id)
            .await?
            .ok_or(TicketingServiceError::NotFound(TicketId(id)))?;
        let ticket = self.load(project_id, clarification.ticket_id).await?;
        if ticket.requester.subject != principal.subject {
            return Err(TicketingServiceError::RequesterMismatch);
        }
        if clarification.state == crate::ClarificationState::Draft {
            // A draft is invisible to requesters: answering one is a
            // not-found, not a state leak.
            return Err(TicketingServiceError::NotFound(TicketId(id)));
        }
        clarification.reply(answers, now)?;
        self.store
            .update_clarification(project_id, clarification.clone())
            .await?;
        Ok(clarification.requester_projection())
    }

    /// Requests one development-automation run (ADR-0070) by submitting    /// Requests one development-automation run (ADR-0070) by submitting
    /// the durable command. Profile-off fails closed here too: the
    /// command never queues for an application that has not opted in.
    pub async fn request_development_automation(
        &self,
        principal: &Identity,
        project_id: &str,
        id: TicketId,
        now: DateTime<Utc>,
    ) -> Result<Uuid, TicketingServiceError> {
        authorize(principal, "ticketing.manage")?;
        self.require_project(project_id)?;
        if self.config().automation.profile == crate::AutomationProfile::Off {
            return Err(TicketingServiceError::Configuration(
                "automation is not enabled for this application".into(),
            ));
        }
        #[cfg(feature = "jobs")]
        {
            let jobs = self
                .portal
                .jobs
                .as_ref()
                .ok_or(TicketingServiceError::JobsUnavailable)?;
            let correlation = Uuid::now_v7();
            let envelope = crate::development_automation_envelope(
                &crate::RunDevelopmentAutomation {
                    project_id: project_id.to_owned(),
                    ticket_id: id,
                    requested_by: principal.subject.clone(),
                },
                correlation,
                now,
            )
            .map_err(|_| TicketingServiceError::JobsUnavailable)?;
            jobs.submit_durable(envelope)
                .await
                .map_err(|_| TicketingServiceError::JobsUnavailable)?;
            Ok(correlation)
        }
        #[cfg(not(feature = "jobs"))]
        {
            let _ = (id, now);
            Err(TicketingServiceError::JobsUnavailable)
        }
    }

    /// Agent-only listing of a ticket's automation proposals (ADR-0070);
    /// automation state never crosses into requester projections.
    pub async fn list_automation_proposals(
        &self,
        principal: &Identity,
        project_id: &str,
        id: TicketId,
    ) -> Result<Vec<crate::AutomationProposal>, TicketingServiceError> {
        authorize(principal, "ticketing.agent.read")?;
        self.require_project(project_id)?;
        Ok(self.store.list_automation_proposals(project_id, id).await?)
    }

    /// Records the human decision on one automation proposal
    /// (ADR-0070): accepting authorizes the *proposal record only* —
    /// executing accepted proposals is later, explicit work.
    pub async fn decide_automation_proposal(
        &self,
        principal: &Identity,
        project_id: &str,
        id: Uuid,
        accept: bool,
        now: DateTime<Utc>,
    ) -> Result<crate::AutomationProposal, TicketingServiceError> {
        authorize(principal, "ticketing.manage")?;
        self.require_project(project_id)?;
        let mut proposal = self
            .store
            .get_automation_proposal(project_id, id)
            .await?
            .ok_or(TicketingServiceError::NotFound(TicketId(id)))?;
        proposal.decide(accept, now)?;
        self.store
            .update_automation_proposal(project_id, proposal.clone())
            .await?;
        Ok(proposal)
    }

    /// Bounded agent search (ADR-0069):    /// Bounded agent search (ADR-0069): substring over subject, display
    /// reference and description — never message bodies.
    pub async fn search_tickets(
        &self,
        principal: &Identity,
        query: &str,
        limit: usize,
        before: Option<(DateTime<Utc>, TicketId)>,
    ) -> Result<Vec<TicketSummary>, TicketingServiceError> {
        authorize(principal, "ticketing.agent.read")?;
        let trimmed = query.trim();
        if trimmed.chars().count() < 2
            || trimmed.chars().count() > 200
            || trimmed.chars().any(char::is_control)
        {
            return Err(TicketingServiceError::InvalidManagementRequest);
        }
        Ok(self
            .store
            .list_summaries(TicketSummaryFilter {
                project_id: self.config().project_id.clone(),
                statuses: BTreeSet::new(),
                queue_id: None,
                assignee_subject: None,
                requester_subject: None,
                unassigned: false,
                query: Some(trimmed.to_owned()),
                before_updated_at: before.map(|value| value.0),
                before_id: before.map(|value| value.1),
                limit,
            })
            .await?)
    }

    /// Replaces the ticket's bounded knowledge links (ADR-0069) as one
    /// revision-gated decision.
    #[allow(clippy::too_many_arguments)]
    pub async fn replace_knowledge_links(
        &self,
        principal: &Identity,
        project_id: &str,
        id: TicketId,
        links: Vec<crate::KnowledgeLink>,
        expected_revision: u64,
        correlation_id: Uuid,
        now: DateTime<Utc>,
    ) -> Result<TicketingMutationResult, TicketingServiceError> {
        authorize(principal, "ticketing.manage")?;
        self.require_project(project_id)?;
        let mut ticket = self.load(project_id, id).await?;
        require_revision(&ticket, expected_revision)?;
        ticket.replace_knowledge_links(links, now)?;
        self.save(
            ticket.clone(),
            expected_revision,
            "ticketing.knowledge_links_replaced",
            correlation_id,
            now,
        )
        .await?;
        Ok(result(ticket))
    }

    /// The requester's one-shot CSAT submission (ADR-0069): only for a
    /// resolved or closed ticket the requester owns, exactly once.
    pub async fn submit_csat(
        &self,
        principal: &Identity,
        project_id: &str,
        id: TicketId,
        score: u8,
        comment: Option<String>,
        now: DateTime<Utc>,
    ) -> Result<TicketingMutationResult, TicketingServiceError> {
        authorize(principal, "ticketing.requester.write")?;
        self.require_project(project_id)?;
        let mut ticket = self.load(project_id, id).await?;
        if ticket.requester.subject != principal.subject {
            return Err(TicketingServiceError::RequesterMismatch);
        }
        ticket.submit_csat(score, comment, now)?;
        self.save(
            ticket.clone(),
            ticket.revision - 1,
            "ticketing.csat_submitted",
            Uuid::new_v4(),
            now,
        )
        .await?;
        Ok(result(ticket))
    }

    /// One curated agent view (ADR-0067): a closed server-defined
    /// predicate over ticket summaries — never an ad-hoc query surface.
    pub async fn list_agent_view(
        &self,
        principal: &Identity,
        view: crate::AgentCuratedView,
        limit: usize,
        before: Option<(DateTime<Utc>, TicketId)>,
    ) -> Result<Vec<TicketSummary>, TicketingServiceError> {
        authorize(principal, "ticketing.agent.read")?;
        let mut statuses = BTreeSet::new();
        let mut assignee_subject = None;
        let mut unassigned = false;
        match view {
            crate::AgentCuratedView::NewUnassigned => {
                statuses.insert(crate::TicketStatus::New);
                unassigned = true;
            }
            crate::AgentCuratedView::PendingRequester => {
                statuses.insert(crate::TicketStatus::PendingRequester);
            }
            crate::AgentCuratedView::PendingInternal => {
                statuses.insert(crate::TicketStatus::PendingInternal);
            }
            crate::AgentCuratedView::Mine => {
                assignee_subject = Some(principal.subject.clone());
            }
            crate::AgentCuratedView::RecentlyResolved => {
                statuses.insert(crate::TicketStatus::Resolved);
            }
        }
        Ok(self
            .store
            .list_summaries(TicketSummaryFilter {
                project_id: self.config().project_id.clone(),
                statuses,
                queue_id: None,
                assignee_subject,
                unassigned,
                query: None,
                requester_subject: None,
                before_updated_at: before.map(|value| value.0),
                before_id: before.map(|value| value.1),
                limit,
            })
            .await?)
    }

    /// Agent detail with advisory collision indication (ADR-0067):
    /// records this view and reports the other recent viewers. If-Match
    /// on mutations remains the collision authority.
    pub async fn agent_ticket_with_viewers(
        &self,
        principal: &Identity,
        project_id: &str,
        id: TicketId,
    ) -> Result<(Ticket, Vec<String>), TicketingServiceError> {
        authorize(principal, "ticketing.agent.read")?;
        self.require_project(project_id)?;
        let ticket = self.load(project_id, id).await?;
        let now = Utc::now();
        self.store
            .record_ticket_view(project_id, id, &principal.subject, now)
            .await?;
        let viewers = self
            .store
            .recent_ticket_viewers(
                project_id,
                id,
                &principal.subject,
                crate::TICKET_VIEW_WINDOW,
                now,
                crate::MAX_OTHER_VIEWERS,
            )
            .await?;
        Ok((ticket, viewers))
    }

    /// The project's shared saved replies, ordered by title (ADR-0067).
    pub async fn list_agent_macros(
        &self,
        principal: &Identity,
    ) -> Result<Vec<crate::AgentMacro>, TicketingServiceError> {
        authorize(principal, "ticketing.agent.read")?;
        Ok(self
            .store
            .list_macros(&self.config().project_id.clone())
            .await?)
    }

    /// Creates one shared saved reply (ADR-0067). Managers only: a
    /// shared library is team vocabulary, not personal scratch.
    pub async fn create_agent_macro(
        &self,
        principal: &Identity,
        title: &str,
        body: &str,
        now: DateTime<Utc>,
    ) -> Result<crate::AgentMacro, TicketingServiceError> {
        authorize(principal, "ticketing.manage")?;
        let macro_ = crate::AgentMacro::new_decision(Uuid::now_v7(), title, body, now)?;
        self.store
            .insert_macro(&self.config().project_id.clone(), macro_.clone())
            .await?;
        Ok(macro_)
    }

    /// Replaces one shared saved reply under the expected revision
    /// (ADR-0067). Macros stay editable text; revision awareness keeps
    /// concurrent editors from overwriting each other silently.
    pub async fn update_agent_macro(
        &self,
        principal: &Identity,
        id: Uuid,
        expected_revision: u64,
        title: &str,
        body: &str,
        now: DateTime<Utc>,
    ) -> Result<crate::AgentMacro, TicketingServiceError> {
        authorize(principal, "ticketing.manage")?;
        crate::AgentMacro::validate_decision(title, body)?;
        Ok(self
            .store
            .update_macro(
                &self.config().project_id.clone(),
                id,
                expected_revision,
                title,
                body,
                now,
            )
            .await?)
    }

    /// Requester-scoped own-ticket list. The subject filter is forcibly the
    /// authenticated subject; client-supplied requester filters are ignored,
    /// so a requester can never enumerate another requester's tickets.
    pub async fn list_requester_summaries(
        &self,
        principal: &Identity,
        filter: TicketSummaryFilter,
    ) -> Result<Vec<PublicTicketSummary>, TicketingServiceError> {
        authorize(principal, "ticketing.requester.read")?;
        let own = TicketSummaryFilter {
            requester_subject: Some(principal.subject.clone()),
            assignee_subject: None,
            ..filter
        };
        self.require_project(&own.project_id)?;
        Ok(self
            .store
            .list_summaries(own)
            .await?
            .iter()
            .map(PublicTicketSummary::from)
            .collect())
    }

    pub async fn get_agent_ticket(
        &self,
        principal: &Identity,
        project_id: &str,
        id: TicketId,
    ) -> Result<Ticket, TicketingServiceError> {
        authorize(principal, "ticketing.agent.read")?;
        self.load(project_id, id).await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn manage_ticket(
        &self,
        principal: &Identity,
        project_id: &str,
        id: TicketId,
        input: AgentManagementInput,
        expected_revision: u64,
        correlation_id: Uuid,
        now: DateTime<Utc>,
    ) -> Result<TicketingMutationResult, TicketingServiceError> {
        authorize(principal, "ticketing.agent.manage")?;
        let mut ticket = self.load(project_id, id).await?;
        require_revision(&ticket, expected_revision)?;
        if input.assignee_subject.is_some() && input.clear_assignee {
            return Err(TicketingServiceError::InvalidManagementRequest);
        }
        if let Some(priority) = input.priority {
            ticket.change_priority(priority, now);
        }
        if input.clear_assignee {
            ticket.assign(None, now)?;
        } else if let Some(assignee) = input.assignee_subject {
            ticket.assign(Some(assignee), now)?;
        }
        if let Some(queue_id) = input.queue_id {
            ticket.transfer_queue(queue_id, now)?;
        }
        if let Some(status) = input.status {
            ticket.change_status(status, input.resolution, input.close_reason, now)?;
        }
        self.save(
            ticket.clone(),
            expected_revision,
            "ticketing.agent_managed",
            correlation_id,
            now,
        )
        .await?;
        Ok(result(ticket))
    }

    pub async fn ready(&self) -> Result<(), TicketingServiceError> {
        Ok(self.store.ready().await?)
    }

    /// Engine-neutral inbound wake (ADR-0059): read the raw object through
    /// the object-storage port, extract routing facts (digest,
    /// `Message-ID`, `In-Reply-To`, bounded `References`) from the
    /// authoritative bytes, and submit through the routing use case. The
    /// durable job remains the verification and ingestion authority.
    #[cfg(feature = "jobs")]
    pub async fn wake_inbound_email(
        &self,
        provider: &str,
        mailbox_scope: &str,
        external_id: &str,
        object_key: &str,
        correlation_id: Uuid,
        arrived_at: DateTime<Utc>,
    ) -> Result<Uuid, TicketingServiceError> {
        let objects = self
            .portal
            .objects
            .as_ref()
            .ok_or(TicketingServiceError::ObjectsUnavailable)?;
        let key = minco_plugin_object_storage::ObjectKey::parse(object_key.to_owned())
            .map_err(|_| TicketingServiceError::InboundObjectMissing)?;
        let stored = objects
            .0
            .get(&key)
            .await
            .map_err(|_| TicketingServiceError::InboundObjectMissing)?
            .ok_or(TicketingServiceError::InboundObjectMissing)?;
        let digest = external_content_sha256(&stored.bytes);
        let message = mail_parser::MessageParser::default()
            .parse(&stored.bytes)
            .ok_or(TicketingServiceError::InboundMimeInvalid)?;
        let header_text = |name: mail_parser::HeaderName<'_>| -> Option<String> {
            message
                .header_raw(name)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
        };
        let internet_message_id = header_text(mail_parser::HeaderName::MessageId);
        let in_reply_to = header_text(mail_parser::HeaderName::InReplyTo);
        let references = header_text(mail_parser::HeaderName::References)
            .map(|value| {
                value
                    .split_whitespace()
                    .map(str::to_owned)
                    .take(64)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let in_reply_ref = in_reply_to.as_deref();
        self.submit_inbound_email(
            provider,
            mailbox_scope,
            external_id,
            &digest,
            object_key,
            internet_message_id.as_deref(),
            in_reply_ref,
            &references,
            correlation_id,
            arrived_at,
        )
        .await
    }

    /// Verified-reference inbound submission (ADR-0058): resolve the
    /// target ticket strictly by `In-Reply-To`/`References` against
    /// previously ingested external identities, then durably submit the
    /// `ticketing.process-inbound-email` job. The append is revision-free:
    /// the store reloads authoritative state inside its transaction, so a
    /// delayed or retried command can never wedge on a stale revision.
    /// Unresolved threading fails closed; no ticket is guessed.
    #[cfg(feature = "jobs")]
    #[allow(clippy::too_many_arguments)]
    pub async fn submit_inbound_email(
        &self,
        provider: &str,
        mailbox_scope: &str,
        external_id: &str,
        content_sha256: &str,
        raw_object_key: &str,
        internet_message_id: Option<&str>,
        in_reply_to: Option<&str>,
        references: &[String],
        correlation_id: Uuid,
        arrived_at: DateTime<Utc>,
    ) -> Result<Uuid, TicketingServiceError> {
        let jobs = self
            .portal
            .jobs
            .as_ref()
            .ok_or(TicketingServiceError::JobsUnavailable)?;
        let project_id = self.config.project_id.clone();
        let mut candidates: Vec<String> = Vec::with_capacity(1 + references.len());
        if let Some(value) = in_reply_to {
            candidates.push(value.to_owned());
        }
        candidates.extend(references.iter().rev().cloned());
        if candidates.len() > 65 {
            return Err(TicketingServiceError::Configuration(
                "inbound threading candidates exceed the bounded set".into(),
            ));
        }
        let mut resolved = None;
        for candidate in candidates {
            if let Some(found) = self
                .store
                .find_ticket_by_message_identity(&project_id, provider, &candidate)
                .await?
            {
                resolved = Some(found);
                break;
            }
        }
        let (ticket_id, _resolved_revision) =
            resolved.ok_or(TicketingServiceError::InboundThreadUnresolved)?;
        let envelope = crate::inbound_email_envelope(
            &crate::ProcessInboundEmail {
                project_id: project_id.clone(),
                provider: provider.to_owned(),
                mailbox_scope: mailbox_scope.to_owned(),
                external_id: external_id.to_owned(),
                content_sha256: content_sha256.to_ascii_lowercase(),
                raw_object_key: raw_object_key.to_owned(),
                ticket_id,
                internet_message_id: internet_message_id.map(str::to_owned),
                in_reply_to: in_reply_to.map(str::to_owned),
                references: references.to_vec(),
            },
            correlation_id,
            arrived_at,
        )
        .map_err(|error| {
            TicketingServiceError::Configuration(format!(
                "inbound email envelope could not be built: {error}"
            ))
        })?;
        let submission = jobs.submit_durable(envelope).await.map_err(|error| {
            TicketingServiceError::Store(TicketStoreError::Infrastructure(error.to_string()))
        })?;
        Ok(match submission {
            minco_plugin_jobs::DurableSubmission::Inserted(job_id)
            | minco_plugin_jobs::DurableSubmission::Duplicate(job_id) => job_id,
        })
    }

    /// One bounded, explicit dispatch pass (ADR-0056): publishes
    /// transactionally-committed activity intents as domain events through
    /// the events outbox claim/lease lifecycle, marking each published only
    /// after its publication succeeded. One intent carries one event
    /// identity forever — the event id IS the intent id — so a crash between
    /// publish and mark replays the same identity and consumers can dedupe;
    /// concurrent passes cannot double-publish because only one worker holds
    /// the outbox claim (review finding 9). Never scheduled implicitly.
    /// Returns the number of intents published in this pass.
    pub async fn dispatch_pending_activity(
        &self,
        project_id: &str,
        limit: usize,
    ) -> Result<usize, TicketingServiceError> {
        let events = self
            .portal
            .events
            .as_ref()
            .ok_or(TicketingServiceError::EventsUnavailable)?;
        self.require_project(project_id)?;
        if !(1..=100).contains(&limit) {
            return Err(TicketingServiceError::Configuration(
                "activity dispatch limit must be between 1 and 100".into(),
            ));
        }
        let pending = self
            .store
            .pending_activity_intents(project_id, limit)
            .await?;
        let worker = format!("ticketing-activity-{}", Uuid::now_v7());
        let claim_expires_at = Utc::now() + TimeDelta::seconds(300);
        let infrastructure = |error: minco_plugin_events::EventError| {
            TicketingServiceError::Store(TicketStoreError::Infrastructure(error.to_string()))
        };
        let mut published = 0;
        for intent in pending {
            let event = minco_plugin_events::DomainEvent {
                id: intent.id,
                event_type: intent.kind.clone(),
                aggregate_type: "ticketing.ticket".into(),
                aggregate_id: intent.ticket_id.to_string(),
                correlation_id: intent.correlation_id,
                occurred_at: intent.created_at,
                payload: intent.payload.clone(),
                metadata: BTreeMap::new(),
            };
            match events
                .outbox
                .enqueue(minco_plugin_events::OutboxRecord::pending(event))
                .await
            {
                // A fresh enqueue and a duplicate from an earlier pass are
                // both fine: the claim below decides who publishes it.
                Ok(()) | Err(minco_plugin_events::EventError::DuplicateEvent(_)) => {}
                Err(error) => return Err(infrastructure(error)),
            }
            match events
                .outbox
                .claim_event(intent.id, &worker, claim_expires_at)
                .await
            {
                Ok(Some(record)) => {
                    events
                        .publisher
                        .publish(&record.event)
                        .await
                        .map_err(infrastructure)?;
                    events
                        .outbox
                        .mark_published(record.event.id, &worker)
                        .await
                        .map_err(infrastructure)?;
                    let _ = self
                        .store
                        .mark_activity_published(intent.id, Utc::now())
                        .await?;
                    published += 1;
                }
                // Claimed by another worker, or already published; skip.
                Ok(None) => {}
                Err(error) => return Err(infrastructure(error)),
            }
        }
        Ok(published)
    }

    /// Resolve a session token to the bound requester identity. Permissions
    /// are exactly the handoff-granted set recorded in session attributes.
    pub async fn resolve_requester_session(
        &self,
        token: &SessionToken,
    ) -> Result<(SessionRecord, Identity), TicketingServiceError> {
        let sessions = self
            .portal
            .sessions
            .as_ref()
            .ok_or(TicketingServiceError::SessionsUnavailable)?;
        let record = sessions
            .resolve(token)
            .await
            .map_err(|_| TicketingServiceError::SessionUnauthenticated)?;
        let bound_project = record
            .attributes
            .get("ticketing.project")
            .ok_or(TicketingServiceError::SessionUnauthenticated)?;
        if bound_project != &self.config.project_id {
            return Err(TicketingServiceError::SessionUnauthenticated);
        }
        let permissions = record
            .attributes
            .get("ticketing.permissions")
            .map(|value| {
                value
                    .split(',')
                    .filter(|part| !part.is_empty())
                    .map(String::from)
                    .collect()
            })
            .unwrap_or_default();
        let identity = Identity {
            subject: record.subject.clone(),
            permissions,
            scopes: BTreeSet::default(),
            claims: BTreeMap::new(),
        };
        Ok((record, identity))
    }

    pub fn verify_session_csrf(
        &self,
        session_id: SessionId,
        token: &CsrfToken,
    ) -> Result<(), TicketingServiceError> {
        let csrf = self
            .portal
            .csrf
            .as_ref()
            .ok_or(TicketingServiceError::SessionsUnavailable)?;
        csrf.verify(session_id, token)
            .map_err(|_| TicketingServiceError::CsrfRejected)
    }

    pub async fn revoke_requester_session(
        &self,
        session_id: SessionId,
    ) -> Result<bool, TicketingServiceError> {
        let sessions = self
            .portal
            .sessions
            .as_ref()
            .ok_or(TicketingServiceError::SessionsUnavailable)?;
        sessions.revoke(session_id).await.map_err(|error| {
            TicketingServiceError::Store(TicketStoreError::Infrastructure(error.to_string()))
        })
    }

    /// One-time handoff consumption that mints a durable requester portal
    /// session. Requires the sessions and CSRF services; identical replay is
    /// answered by the shared idempotency layer before this is reached.
    pub async fn exchange_requester_session(
        &self,
        token: SupportHandoffToken,
        portal_origin: &str,
        request_fingerprint: &str,
        now: DateTime<Utc>,
    ) -> Result<RequesterSessionGrant, TicketingServiceError> {
        let sessions = self
            .portal
            .sessions
            .as_ref()
            .ok_or(TicketingServiceError::SessionsUnavailable)?;
        let csrf = self
            .portal
            .csrf
            .as_ref()
            .ok_or(TicketingServiceError::SessionsUnavailable)?;
        let project_id = self.config.project_id.clone();
        let (identity, _repeated) = self
            .store
            .consume_handoff_identity(ConsumeSessionRequest {
                token,
                project_id: project_id.clone(),
                portal_origin: portal_origin.to_owned(),
                request_fingerprint: request_fingerprint.to_owned(),
                now,
            })
            .await?;
        let issued = sessions
            .issue(minco_plugin_sessions::CreateSession {
                subject: identity.requester_subject.clone(),
                ttl: TimeDelta::seconds(self.config.requester_session_ttl_seconds),
                attributes: BTreeMap::from([
                    ("ticketing.project".into(), project_id),
                    ("ticketing.portal_origin".into(), portal_origin.to_owned()),
                    (
                        "ticketing.permissions".into(),
                        identity.requester_permissions.join(","),
                    ),
                ]),
            })
            .await
            .map_err(|error| {
                TicketingServiceError::Store(TicketStoreError::Infrastructure(error.to_string()))
            })?;
        let csrf_token = csrf.issue(issued.session.id);
        Ok(RequesterSessionGrant {
            token: issued.token,
            expires_at: issued.session.expires_at,
            csrf_token,
        })
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

    /// Commits one message append atomically without rewriting the
    /// conversation (ADR-0052). `ticket` is the post-mutation in-memory
    /// aggregate; the store updates only the projection columns it lists.
    async fn append_message(
        &self,
        ticket: &Ticket,
        message: TicketMessage,
        kind: &str,
        expected_revision: u64,
        correlation_id: Uuid,
        now: DateTime<Utc>,
    ) -> Result<(), TicketingServiceError> {
        self.append_message_with_jobs(
            ticket,
            message,
            kind,
            expected_revision,
            correlation_id,
            now,
            #[cfg(feature = "jobs")]
            Vec::new(),
        )
        .await
    }

    /// Notification job records for a public agent reply when the
    /// configuration enables them (ADR-0054); identifiers only.
    #[cfg(feature = "jobs")]
    fn notification_records(
        &self,
        ticket: &Ticket,
        message: &TicketMessage,
        correlation_id: Uuid,
        now: DateTime<Utc>,
    ) -> Result<Vec<minco_plugin_jobs::JobRecord>, TicketingServiceError> {
        if !self.config.notify_requester_on_public_reply
            || message.kind != crate::TicketMessageKind::PublicReply
        {
            return Ok(Vec::new());
        }
        crate::notification_record_for_reply(
            &ticket.project_id,
            ticket.id,
            message.id,
            correlation_id,
            now,
        )
        .map(Option::into_iter)
        .map(Iterator::collect)
        .map_err(|error| {
            TicketingServiceError::Configuration(format!(
                "notification job could not be built: {error}"
            ))
        })
    }

    /// Like `append_message`, committing bounded job records in the same
    /// transaction (ADR-0054).
    #[allow(clippy::too_many_arguments)]
    async fn append_message_with_jobs(
        &self,
        ticket: &Ticket,
        message: TicketMessage,
        kind: &str,
        expected_revision: u64,
        correlation_id: Uuid,
        now: DateTime<Utc>,
        #[cfg(feature = "jobs")] job_records: Vec<minco_plugin_jobs::JobRecord>,
    ) -> Result<(), TicketingServiceError> {
        let intent = activity(ticket, kind, correlation_id, now);
        Ok(self
            .store
            .append_ticket_message(AppendTicketMessageRequest {
                project_id: ticket.project_id.clone(),
                ticket_id: ticket.id,
                message,
                status: ticket.status,
                first_public_response_at: ticket.first_public_response_at,
                waiting_since: ticket.waiting_since,
                resolved_at: ticket.resolved_at,
                updated_at: ticket.updated_at,
                expected_revision,
                intent,
                #[cfg(feature = "jobs")]
                job_records,
            })
            .await?)
    }

    /// Paginated public conversation of one ticket for its own requester.
    /// Memory and `SQLite` both read through the message port, never the
    /// whole aggregate.
    pub async fn list_requester_messages(
        &self,
        principal: &Identity,
        project_id: &str,
        ticket_id: TicketId,
        before: Option<(DateTime<Utc>, TicketMessageId)>,
        limit: usize,
    ) -> Result<Vec<PublicTicketMessage>, TicketingServiceError> {
        authorize(principal, "ticketing.requester.read")?;
        let ticket = self.load(project_id, ticket_id).await?;
        if ticket.requester.subject != principal.subject
            && !principal.has_permission("ticketing.manage")
        {
            return Err(TicketingServiceError::RequesterMismatch);
        }
        let requester_subject = ticket.requester.subject.clone();
        Ok(self
            .store
            .list_ticket_messages(MessageListFilter {
                project_id: project_id.to_owned(),
                ticket_id,
                include_internal: false,
                before_created_at: before.map(|value| value.0),
                before_id: before.map(|value| value.1),
                limit,
            })
            .await?
            .iter()
            .map(|message| Ticket::public_message(message, &requester_subject))
            .collect())
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

/// The complete capability set a requester portal session may carry
/// (review finding 10). Requester-facing use cases authorize these exact
/// names; handoff grants are constrained to this set.
pub const REQUESTER_PORTAL_CAPABILITIES: &[&str] =
    &["ticketing.requester.read", "ticketing.requester.write"];

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
const fn default_requester_session_ttl_seconds() -> i64 {
    3600
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
    #[error("agent management request fields are mutually exclusive")]
    InvalidManagementRequest,
    #[error("requester portal sessions are not configured for this application")]
    SessionsUnavailable,
    #[error("the requester session is unknown, expired or revoked")]
    SessionUnauthenticated,
    #[error("the request did not carry a valid session CSRF token")]
    CsrfRejected,
    #[error("the events service is not registered for this application")]
    EventsUnavailable,
    #[error("the durable jobs service is not registered for this application")]
    JobsUnavailable,
    #[error("inbound threading does not reference a known ticket")]
    InboundThreadUnresolved,
    #[error("delivery feedback fields are invalid or reference an unknown outbound message")]
    InvalidDeliveryFeedback,
    #[error("the inbound raw object is missing or unreadable")]
    InboundObjectMissing,
    #[error("the inbound raw object is not parseable MIME")]
    InboundMimeInvalid,
    #[error("the object-storage service is not registered for this application")]
    ObjectsUnavailable,
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
            test_config(),
        )
        .unwrap()
    }

    fn test_config() -> TicketingConfig {
        TicketingConfig {
            project_id: "project-a".into(),
            portal_origin: "https://support.example.test".into(),
            allowed_return_paths: BTreeMap::from([(
                "https://app.example.test".into(),
                vec!["/orders".into()],
            )]),
            ..TicketingConfig::default()
        }
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
            ticket_type: crate::TicketType::default(),
            form_answers: Vec::new(),
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
                    &identity("user-b", &["ticketing.requester.read"]),
                    "project-a",
                    created.ticket.id
                )
                .await,
            Err(TicketingServiceError::RequesterMismatch)
        ));
    }

    #[tokio::test]
    async fn requester_and_agent_capabilities_are_separated() {
        let service = service();
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
                    ticket_type: crate::TicketType::default(),
                    form_answers: Vec::new(),
                    priority: TicketPriority::Normal,
                    resource_references: Vec::new(),
                },
                Uuid::now_v7(),
                Utc::now(),
            )
            .await
            .unwrap();
        // Broad agent capabilities must not open the requester portal
        // surface (review finding 10).
        for agent_only in [
            &["ticketing.read"][..],
            &["ticketing.reply"][..],
            &["ticketing.agent.read", "ticketing.agent.manage"][..],
        ] {
            assert!(matches!(
                service
                    .get_ticket_for_requester(
                        &identity("user-a", agent_only),
                        "project-a",
                        created.ticket.id
                    )
                    .await,
                Err(TicketingServiceError::PermissionDenied(_))
            ));
            assert!(matches!(
                service
                    .list_requester_summaries(
                        &identity("user-a", agent_only),
                        crate::TicketSummaryFilter {
                            project_id: "project-a".into(),
                            statuses: BTreeSet::default(),
                            queue_id: None,
                            assignee_subject: None,
                            unassigned: false,
                            query: None,
                            requester_subject: None,
                            before_updated_at: None,
                            before_id: None,
                            limit: 10,
                        },
                    )
                    .await,
                Err(TicketingServiceError::PermissionDenied(_))
            ));
        }
        // Requester capabilities must not open the agent surface.
        let requester = identity(
            "user-a",
            &["ticketing.requester.read", "ticketing.requester.write"],
        );
        assert!(matches!(
            service
                .list_tickets(
                    &requester,
                    crate::TicketListFilter {
                        project_id: "project-a".into(),
                        statuses: BTreeSet::default(),
                        queue_id: None,
                        assignee_subject: None,
                        requester_subject: None,
                        after_updated_at: None,
                        after_id: None,
                        limit: 10,
                    }
                )
                .await,
            Err(TicketingServiceError::PermissionDenied(_))
        ));
        assert!(matches!(
            service
                .get_ticket_for_agent(&requester, "project-a", created.ticket.id)
                .await,
            Err(TicketingServiceError::PermissionDenied(_))
        ));
    }

    #[tokio::test]
    async fn handoff_grants_are_constrained_to_requester_capabilities() {
        let service = service();
        let now = Utc::now();
        let integration = identity("integration", &["ticketing.integrate"]);
        // Broad agent permissions are refused at issuance.
        for forbidden in [
            vec!["ticketing.read".to_owned()],
            vec!["ticketing.manage".to_owned()],
            vec!["ticketing.integrate".to_owned()],
            vec![
                "ticketing.requester.read".to_owned(),
                "ticketing.agent.manage".to_owned(),
            ],
        ] {
            assert!(matches!(
                service
                    .issue_ticketing_handoff(
                        &integration,
                        IssueTicketingHandoffInput {
                            project_id: "project-a".into(),
                            requester_subject: "user-1".into(),
                            requester_permissions: forbidden,
                            surface: minco_interaction::SupportSurface::Portal,
                            context: minco_interaction::SupportContext {
                                page_url: "https://app.example.test/orders/1".into(),
                                ..minco_interaction::SupportContext::default()
                            },
                            return_location: "https://app.example.test/orders/1".into(),
                            correlation_id: Uuid::now_v7(),
                        },
                        now,
                    )
                    .await,
                Err(TicketingServiceError::Configuration(_))
            ));
        }
        // The requester set itself issues, and an omitted set defaults to it.
        for allowed in [
            vec!["ticketing.requester.read".to_owned()],
            REQUESTER_PORTAL_CAPABILITIES
                .iter()
                .map(|value| (*value).to_owned())
                .collect(),
            Vec::new(),
        ] {
            assert!(
                service
                    .issue_ticketing_handoff(
                        &integration,
                        IssueTicketingHandoffInput {
                            project_id: "project-a".into(),
                            requester_subject: "user-1".into(),
                            requester_permissions: allowed,
                            surface: minco_interaction::SupportSurface::Portal,
                            context: minco_interaction::SupportContext {
                                page_url: "https://app.example.test/orders/1".into(),
                                ..minco_interaction::SupportContext::default()
                            },
                            return_location: "https://app.example.test/orders/1".into(),
                            correlation_id: Uuid::now_v7(),
                        },
                        now,
                    )
                    .await
                    .is_ok(),
                "handoff issuance must accept the constrained requester set"
            );
        }
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
                    ticket_type: crate::TicketType::default(),
                    form_answers: Vec::new(),
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
                Uuid::now_v7(),
                now,
            )
            .await
            .unwrap();

        assert_eq!(replay.ticket, first.ticket);
    }

    async fn feedback_fixture() -> (
        TicketingService,
        Arc<MemoryTicketingStore>,
        crate::Ticket,
        crate::TicketMessage,
    ) {
        let memory = Arc::new(MemoryTicketingStore::default());
        let service =
            TicketingService::new(TicketingStoreService::new(memory.clone()), test_config())
                .unwrap();
        let now = Utc::now();
        let mut ticket = Ticket::create(create_input("Help"), "TKT-FB", now).unwrap();
        let message = ticket
            .reply_as_agent_message("agent-1", "Shipped.", now)
            .unwrap();
        TicketingStoreService::new(memory.clone())
            .create(
                ticket.clone(),
                TicketActivityIntent::new(
                    "project-a",
                    ticket.id,
                    "created",
                    Uuid::now_v7(),
                    serde_json::json!({}),
                    now,
                ),
            )
            .await
            .unwrap();
        (service, memory, ticket, message)
    }

    fn manager_identity() -> Identity {
        identity(
            "manager",
            &[
                "ticketing.create",
                "ticketing.manage",
                "ticketing.agent.read",
                "ticketing.agent.manage",
            ],
        )
    }

    fn sla_pool_service() -> TicketingService {
        let memory = Arc::new(MemoryTicketingStore::default());
        TicketingService::new(
            TicketingStoreService::new(memory),
            TicketingConfig {
                project_id: "project-a".into(),
                assignment_pool: vec!["agent-a".into(), "agent-b".into()],
                sla: Some(TicketSlaConfig {
                    first_response_hours: 4,
                    resolution_hours: 48,
                }),
                ..test_config()
            },
        )
        .unwrap()
    }

    fn automation_service(profile: crate::AutomationProfile) -> TicketingService {
        let memory = Arc::new(MemoryTicketingStore::default());
        let registry = Arc::new(minco_plugin_jobs::JobHandlerRegistry::new());
        let (jobs, _store, _dispatcher) = minco_plugin_jobs::JobsServices::memory(registry);
        TicketingService::new(
            TicketingStoreService::new(memory),
            TicketingConfig {
                project_id: "project-a".into(),
                automation: crate::AutomationConfig {
                    profile,
                    review: crate::AutomationReview::Always,
                },
                ..test_config()
            },
        )
        .unwrap()
        .with_portal_services(TicketingPortalServices {
            jobs: Some(Arc::new(jobs)),
            ..TicketingPortalServices::default()
        })
    }

    #[tokio::test]
    async fn clarification_lifecycle_is_gated_sent_once_and_answered_once() {
        let service = service();
        let manager = manager_identity();
        let requester = identity(
            "user-a",
            &[
                "ticketing.create",
                "ticketing.requester.read",
                "ticketing.requester.write",
            ],
        );
        let ticket = service
            .create_ticket(
                &requester,
                create_input("Ambiguous"),
                Uuid::new_v4(),
                Utc::now(),
            )
            .await
            .unwrap()
            .ticket;
        let now = Utc::now();

        // A draft is invisible to the requester.
        let draft = service
            .create_clarification_draft(
                &manager,
                "project-a",
                ticket.id,
                ClarificationDraftInput {
                    reason: crate::ClarificationReason::MissingRequirement,
                    questions: vec![
                        crate::ClarificationQuestion {
                            id: "q1".into(),
                            text: "Which checkout step fails?".into(),
                        },
                        crate::ClarificationQuestion {
                            id: "q2".into(),
                            text: "What error do you see?".into(),
                        },
                    ],
                    checkpoint: "automation:step-3".into(),
                },
                now,
            )
            .await
            .unwrap();
        assert_eq!(draft.state, crate::ClarificationState::Draft);
        let requester_view = service
            .list_requester_clarifications(&requester)
            .await
            .unwrap();
        assert!(requester_view.is_empty(), "drafts never reach requesters");

        // Drafts cannot be answered; the requester sees not-found.
        assert!(matches!(
            service
                .reply_to_clarification(&requester, "project-a", draft.id, vec!["x".into()], now)
                .await
                .unwrap_err(),
            TicketingServiceError::NotFound(_)
        ));

        // The human send decision makes it visible.
        let sent = service
            .send_clarification(&manager, "project-a", draft.id, now)
            .await
            .unwrap();
        assert_eq!(sent.state, crate::ClarificationState::Sent);
        let requester_view = service
            .list_requester_clarifications(&requester)
            .await
            .unwrap();
        assert_eq!(requester_view.len(), 1);
        // The requester projection never carries the checkpoint.
        let encoded = serde_json::to_string(&requester_view[0]).unwrap();
        assert!(!encoded.contains("checkpoint"));
        assert!(!encoded.contains("automation:step-3"));

        // Sending twice fails; a stranger cannot answer.
        assert!(
            service
                .send_clarification(&manager, "project-a", draft.id, now)
                .await
                .is_err()
        );
        let stranger = identity(
            "user-b",
            &["ticketing.requester.read", "ticketing.requester.write"],
        );
        assert!(
            service
                .reply_to_clarification(
                    &stranger,
                    "project-a",
                    draft.id,
                    vec!["a".into(), "b".into()],
                    now
                )
                .await
                .is_err()
        );

        // Answers must cover every question exactly once.
        assert!(
            service
                .reply_to_clarification(
                    &requester,
                    "project-a",
                    draft.id,
                    vec!["only one".into()],
                    now
                )
                .await
                .is_err()
        );
        let answered = service
            .reply_to_clarification(
                &requester,
                "project-a",
                draft.id,
                vec!["Payment step".into(), "Card declined".into()],
                now,
            )
            .await
            .unwrap();
        assert_eq!(answered.state, crate::ClarificationState::Answered);
        assert_eq!(answered.answers.as_ref().map(Vec::len), Some(2));

        // Answering twice fails; the agent side now sees the checkpoint
        // to resume from.
        assert!(
            service
                .reply_to_clarification(
                    &requester,
                    "project-a",
                    draft.id,
                    vec!["a".into(), "b".into()],
                    now
                )
                .await
                .is_err()
        );
        let agent_view = service
            .list_clarifications(&manager, "project-a", ticket.id)
            .await
            .unwrap();
        assert_eq!(agent_view[0].checkpoint, "automation:step-3");
        assert_eq!(agent_view[0].state, crate::ClarificationState::Answered);
    }

    #[tokio::test]
    async fn automation_fails_closed_when_off_and_review_disabled_is_unconfigurable() {
        let identity = manager_identity();
        // Off: the command never queues.
        let off = automation_service(crate::AutomationProfile::Off);
        let ticket = off
            .create_ticket(&identity, create_input("Help"), Uuid::new_v4(), Utc::now())
            .await
            .unwrap()
            .ticket;
        assert!(matches!(
            off.request_development_automation(&identity, "project-a", ticket.id, Utc::now())
                .await
                .unwrap_err(),
            TicketingServiceError::Configuration(_)
        ));
        // Review disabled with automation on fails config validation.
        assert!(
            TicketingService::new(
                TicketingStoreService::new(Arc::new(MemoryTicketingStore::default())),
                TicketingConfig {
                    project_id: "project-a".into(),
                    automation: crate::AutomationConfig {
                        profile: crate::AutomationProfile::Assist,
                        review: crate::AutomationReview::Disabled,
                    },
                    ..test_config()
                },
            )
            .is_err()
        );
    }

    #[tokio::test]
    async fn exclusion_list_refuses_production_authority() {
        for excluded in [
            "merge",
            "release",
            "publish",
            "deploy",
            "production.mutation",
            "secret.management",
            "workflow.dispatch",
        ] {
            assert!(
                crate::validate_automation_actions(&[excluded.to_owned()]).is_err(),
                "`{excluded}` must be excluded"
            );
        }
        assert!(crate::validate_automation_actions(&["summarize".to_owned()]).is_ok());
    }

    #[tokio::test]
    async fn proposals_are_decided_once_and_never_reach_requesters() {
        let service = automation_service(crate::AutomationProfile::Assist);
        let identity = manager_identity();
        let ticket = service
            .create_ticket(
                &identity,
                create_input("Automate me"),
                Uuid::new_v4(),
                Utc::now(),
            )
            .await
            .unwrap()
            .ticket;
        // A proposal inserted directly stands in for the executed job.
        let proposal = crate::AutomationProposal::new(
            ticket.id,
            "Automation proposal".into(),
            vec!["summarize".into()],
            "agent-1",
            Utc::now(),
        );
        service
            .store
            .insert_automation_proposal("project-a", proposal.clone())
            .await
            .unwrap();
        let listed = service
            .list_automation_proposals(&identity, "project-a", ticket.id)
            .await
            .unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(
            listed[0].state,
            crate::AutomationProposalState::AwaitingReview
        );

        let decided = service
            .decide_automation_proposal(&identity, "project-a", proposal.id, true, Utc::now())
            .await
            .unwrap();
        assert_eq!(decided.state, crate::AutomationProposalState::Accepted);
        assert!(decided.decided_at.is_some());
        // Exactly once: the second decision fails closed.
        assert!(
            service
                .decide_automation_proposal(&identity, "project-a", proposal.id, false, Utc::now())
                .await
                .is_err()
        );
        // Privacy: the requester projection carries no automation state.
        let decided_json = serde_json::to_string(&decided).unwrap();
        assert!(!decided_json.contains("awaiting"));
        let requester_projection = ticket.requester_projection();
        let encoded = serde_json::to_string(&requester_projection).unwrap();
        assert!(!encoded.contains("automation"));
    }

    #[tokio::test]
    async fn bounded_search_matches_subject_reference_and_description() {
        let service = service();
        let identity = manager_identity();
        let mut input = create_input("Checkout fails");
        input.description = "Payment rejected after login".into();
        service
            .create_ticket(&identity, input, Uuid::new_v4(), Utc::now())
            .await
            .unwrap();
        service
            .create_ticket(
                &identity,
                create_input("Unrelated"),
                Uuid::new_v4(),
                Utc::now(),
            )
            .await
            .unwrap();
        for needle in ["checkout", "payment rejected"] {
            let hits = service
                .search_tickets(&identity, needle, 10, None)
                .await
                .unwrap();
            assert_eq!(hits.len(), 1, "needle {needle}");
        }
        // The display reference matches both tickets.
        assert_eq!(
            service
                .search_tickets(&identity, "TKT-", 10, None)
                .await
                .unwrap()
                .len(),
            2
        );
        // Too-short queries fail closed; message bodies are never searched.
        assert!(
            service
                .search_tickets(&identity, "x", 10, None)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn knowledge_links_replace_atomically() {
        let service = service();
        let identity = manager_identity();
        let ticket = service
            .create_ticket(&identity, create_input("Help"), Uuid::new_v4(), Utc::now())
            .await
            .unwrap()
            .ticket;
        let links = vec![crate::KnowledgeLink {
            article_id: "kb-1".into(),
            title: "Fix checkout".into(),
            url: "https://kb.example.test/checkout".into(),
        }];
        let updated = service
            .replace_knowledge_links(
                &identity,
                "project-a",
                ticket.id,
                links,
                ticket.revision,
                Uuid::new_v4(),
                Utc::now(),
            )
            .await
            .unwrap()
            .ticket;
        assert_eq!(updated.knowledge_links.len(), 1);
        // Duplicate article ids in one decision fail closed.
        let mut duplicate = updated.knowledge_links.clone();
        duplicate.push(crate::KnowledgeLink {
            article_id: "kb-1".into(),
            title: "Other".into(),
            url: "https://kb.example.test/other".into(),
        });
        assert!(
            service
                .replace_knowledge_links(
                    &identity,
                    "project-a",
                    ticket.id,
                    duplicate,
                    updated.revision,
                    Uuid::new_v4(),
                    Utc::now(),
                )
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn csat_is_one_shot_and_only_after_resolution() {
        let service = service();
        let requester = identity(
            "user-a",
            &[
                "ticketing.create",
                "ticketing.requester.read",
                "ticketing.requester.write",
            ],
        );
        let ticket = service
            .create_ticket(&requester, create_input("Help"), Uuid::new_v4(), Utc::now())
            .await
            .unwrap()
            .ticket;
        // Open tickets refuse a rating.
        assert!(
            service
                .submit_csat(&requester, "project-a", ticket.id, 5, None, Utc::now())
                .await
                .is_err()
        );
        // Resolve as the manager.
        let manager = manager_identity();
        let resolved = service
            .manage_ticket(
                &manager,
                "project-a",
                ticket.id,
                crate::AgentManagementInput {
                    status: Some(crate::TicketStatus::Resolved),
                    resolution: Some("Fixed".into()),
                    ..Default::default()
                },
                ticket.revision,
                Uuid::new_v4(),
                Utc::now(),
            )
            .await
            .unwrap()
            .ticket;
        let rated = service
            .submit_csat(
                &requester,
                "project-a",
                ticket.id,
                4,
                Some("Thanks".into()),
                Utc::now(),
            )
            .await
            .unwrap()
            .ticket;
        assert_eq!(rated.csat.as_ref().unwrap().score, 4);
        // A second submission is refused; another requester cannot rate.
        assert!(
            service
                .submit_csat(&requester, "project-a", ticket.id, 5, None, Utc::now())
                .await
                .is_err()
        );
        let stranger = identity(
            "user-b",
            &["ticketing.requester.read", "ticketing.requester.write"],
        );
        assert!(
            service
                .submit_csat(&stranger, "project-a", ticket.id, 5, None, Utc::now())
                .await
                .is_err()
        );
        let _ = resolved;
    }

    #[tokio::test]
    async fn creation_snapshots_sla_deadlines() {
        let service = sla_pool_service();
        let identity = manager_identity();
        let created = service
            .create_ticket(&identity, create_input("Help"), Uuid::new_v4(), Utc::now())
            .await
            .unwrap()
            .ticket;
        let deadline = created
            .first_response_deadline
            .expect("first response deadline snapshot");
        assert!(deadline > created.created_at);
        assert!(created.resolution_deadline.is_some());
    }

    #[tokio::test]
    async fn assignment_modes_pick_from_the_pool_deterministically() {
        let service = sla_pool_service();
        let identity = manager_identity();
        let first = service
            .create_ticket(&identity, create_input("One"), Uuid::new_v4(), Utc::now())
            .await
            .unwrap()
            .ticket;
        let second = service
            .create_ticket(&identity, create_input("Two"), Uuid::new_v4(), Utc::now())
            .await
            .unwrap()
            .ticket;
        let now = Utc::now();

        // Round-robin advances: first pick a, second pick b.
        let assigned = service
            .assign_ticket_by_mode(
                &identity,
                "project-a",
                first.id,
                crate::AssignmentMode::RoundRobin,
                None,
                first.revision,
                Uuid::new_v4(),
                now,
            )
            .await
            .unwrap()
            .ticket;
        assert_eq!(assigned.assignee_subject.as_deref(), Some("agent-a"));
        let assigned = service
            .assign_ticket_by_mode(
                &identity,
                "project-a",
                second.id,
                crate::AssignmentMode::RoundRobin,
                None,
                second.revision,
                Uuid::new_v4(),
                now,
            )
            .await
            .unwrap()
            .ticket;
        assert_eq!(assigned.assignee_subject.as_deref(), Some("agent-b"));

        // Least-workload picks the member with the fewest open tickets;
        // agent-a holds one, agent-b holds one, so the tie breaks
        // lexicographically to agent-a.
        let third = service
            .create_ticket(&identity, create_input("Three"), Uuid::new_v4(), Utc::now())
            .await
            .unwrap()
            .ticket;
        let assigned = service
            .assign_ticket_by_mode(
                &identity,
                "project-a",
                third.id,
                crate::AssignmentMode::LeastWorkload,
                None,
                third.revision,
                Uuid::new_v4(),
                now,
            )
            .await
            .unwrap()
            .ticket;
        assert_eq!(assigned.assignee_subject.as_deref(), Some("agent-a"));

        // Manual still sets and clears explicitly.
        let assigned = service
            .assign_ticket_by_mode(
                &identity,
                "project-a",
                third.id,
                crate::AssignmentMode::Manual,
                Some("agent-b".into()),
                assigned.revision,
                Uuid::new_v4(),
                now,
            )
            .await
            .unwrap()
            .ticket;
        assert_eq!(assigned.assignee_subject.as_deref(), Some("agent-b"));
    }

    #[tokio::test]
    async fn pool_modes_fail_closed_without_a_pool() {
        let service = sla_pool_service();
        let identity = manager_identity();
        let ticket = service
            .create_ticket(&identity, create_input("Help"), Uuid::new_v4(), Utc::now())
            .await
            .unwrap()
            .ticket;
        let bare = TicketingService::new(
            TicketingStoreService::new(Arc::new(MemoryTicketingStore::default())),
            test_config(),
        )
        .unwrap();
        let error = bare
            .assign_ticket_by_mode(
                &identity,
                "project-a",
                ticket.id,
                crate::AssignmentMode::RoundRobin,
                None,
                ticket.revision,
                Uuid::new_v4(),
                Utc::now(),
            )
            .await
            .unwrap_err();
        assert!(matches!(error, TicketingServiceError::Configuration(_)));
    }

    #[tokio::test]
    async fn delivery_feedback_requires_the_ingest_permission() {
        let (service, memory, ticket, message) = feedback_fixture().await;
        let error = service
            .record_delivery_feedback(
                &identity(
                    "web",
                    &["ticketing.requester.read", "ticketing.requester.write"],
                ),
                "project-a",
                ticket.id,
                message.id,
                DeliveryFeedbackKind::Bounce,
                "ses",
                "ses-1",
                Utc::now(),
            )
            .await
            .unwrap_err();
        assert!(matches!(error, TicketingServiceError::PermissionDenied(_)));
        // Fail before persistence: no evidence row exists.
        assert!(memory.all_outbound_evidence().await.is_empty());
    }

    #[tokio::test]
    async fn delivery_feedback_rejects_invalid_and_unknown_targets() {
        let (service, memory, ticket, message) = feedback_fixture().await;
        let now = Utc::now();
        let principal = identity("ingress", &["ticketing.ingest"]);
        // Unknown message id: orphan feedback fails closed.
        let error = service
            .record_delivery_feedback(
                &principal,
                "project-a",
                ticket.id,
                TicketMessageId::new(),
                DeliveryFeedbackKind::Complaint,
                "ses",
                "ses-1",
                now,
            )
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            TicketingServiceError::InvalidDeliveryFeedback
        ));
        // Empty provider message id: not attributable, refused.
        let error = service
            .record_delivery_feedback(
                &principal,
                "project-a",
                ticket.id,
                message.id,
                DeliveryFeedbackKind::Delay,
                "ses",
                "  ",
                now,
            )
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            TicketingServiceError::InvalidDeliveryFeedback
        ));
        assert!(memory.all_outbound_evidence().await.is_empty());
    }

    #[tokio::test]
    async fn delivery_feedback_records_bounce_and_complaint_evidence() {
        let (service, memory, ticket, message) = feedback_fixture().await;
        let now = Utc::now();
        let principal = identity("ingress", &["ticketing.ingest"]);
        service
            .record_delivery_feedback(
                &principal,
                "project-a",
                ticket.id,
                message.id,
                DeliveryFeedbackKind::Bounce,
                "ses",
                "ses-out-1",
                now,
            )
            .await
            .unwrap();
        service
            .record_delivery_feedback(
                &principal,
                "project-a",
                ticket.id,
                message.id,
                DeliveryFeedbackKind::Complaint,
                "ses",
                "ses-out-1",
                now,
            )
            .await
            .unwrap();
        let rows = memory.all_outbound_evidence().await;
        assert_eq!(rows.len(), 2);
        assert!(
            rows.iter()
                .all(|row| row.kind == OutboundEvidenceKind::Feedback
                    && row.provider == "ses"
                    && row.provider_message_id == "ses-out-1")
        );
        assert!(
            rows.iter()
                .any(|row| row.feedback == Some(DeliveryFeedbackKind::Bounce))
        );
        assert!(
            rows.iter()
                .any(|row| row.feedback == Some(DeliveryFeedbackKind::Complaint))
        );
    }

    fn create_input(subject: &str) -> CreateTicketInput {
        CreateTicketInput {
            project_id: "project-a".into(),
            subject: subject.into(),
            description: "It broke".into(),
            requester: TicketRequester {
                subject: "user-a".into(),
                display_name: None,
                email: None,
            },
            channel: TicketChannel::Api,
            ticket_type: crate::TicketType::default(),
            form_answers: Vec::new(),
            priority: TicketPriority::Normal,
            resource_references: Vec::new(),
        }
    }

    async fn created_ticket(service: &TicketingService) -> Ticket {
        service
            .create_ticket(
                &identity("user-a", &["ticketing.create"]),
                create_input("Help"),
                Uuid::now_v7(),
                Utc::now(),
            )
            .await
            .unwrap()
            .ticket
    }

    #[tokio::test]
    async fn agent_use_cases_require_agent_capabilities_and_isolate_projects() {
        let service = service();
        let ticket = created_ticket(&service).await;

        assert!(matches!(
            service.agent_bootstrap(&identity("agent", &["ticketing.read"])),
            Err(TicketingServiceError::PermissionDenied(_))
        ));
        assert!(service.agent_bootstrap(&identity("agent", &[])).is_err());
        let mut filter = TicketSummaryFilter {
            project_id: "project-a".into(),
            limit: 10,
            ..TicketSummaryFilter::default()
        };
        assert!(
            service
                .list_ticket_summaries(
                    &identity("agent", &["ticketing.agent.read"]),
                    filter.clone()
                )
                .await
                .is_ok()
        );
        assert!(
            service
                .list_ticket_summaries(&identity("agent", &["ticketing.read"]), filter.clone())
                .await
                .is_err()
        );
        filter.project_id = "project-b".into();
        assert!(matches!(
            service
                .list_ticket_summaries(&identity("agent", &["ticketing.agent.read"]), filter)
                .await,
            Err(TicketingServiceError::ProjectDenied)
        ));
        assert!(
            service
                .get_agent_ticket(
                    &identity("agent", &["ticketing.agent.read"]),
                    "project-a",
                    ticket.id
                )
                .await
                .is_ok()
        );
        assert!(
            service
                .manage_ticket(
                    &identity("agent", &["ticketing.agent.read"]),
                    "project-a",
                    ticket.id,
                    AgentManagementInput::default(),
                    ticket.revision,
                    Uuid::now_v7(),
                    Utc::now(),
                )
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn agent_bootstrap_reports_only_enforced_capabilities() {
        let service = service();
        let full = service
            .agent_bootstrap(&identity(
                "agent-1",
                &[
                    "ticketing.agent-console",
                    "ticketing.create",
                    "ticketing.reply",
                    "ticketing.manage",
                    "ticketing.agent.manage",
                ],
            ))
            .unwrap();
        assert_eq!(full.subject, "agent-1");
        assert_eq!(
            full.capabilities,
            AgentConsoleCapabilities {
                create: true,
                reply: true,
                internal_note: true,
                manage: true
            }
        );
        let minimal = service
            .agent_bootstrap(&identity("agent-2", &["ticketing.agent-console"]))
            .unwrap();
        assert_eq!(
            minimal.capabilities,
            AgentConsoleCapabilities {
                create: false,
                reply: false,
                internal_note: false,
                manage: false
            }
        );
    }

    #[tokio::test]
    async fn management_is_atomic_and_one_save() {
        let service = service();
        let ticket = created_ticket(&service).await;
        let now = Utc::now();

        // Complete valid change set applies together in one revision bump.
        let managed = service
            .manage_ticket(
                &identity(
                    "agent",
                    &["ticketing.agent-console", "ticketing.agent.manage"],
                ),
                "project-a",
                ticket.id,
                AgentManagementInput {
                    priority: Some(TicketPriority::High),
                    assignee_subject: Some("agent-2".into()),
                    queue_id: Some("tier-2".into()),
                    status: Some(TicketStatus::PendingRequester),
                    ..AgentManagementInput::default()
                },
                ticket.revision,
                Uuid::now_v7(),
                now,
            )
            .await
            .unwrap()
            .ticket;
        assert_eq!(managed.priority, TicketPriority::High);
        assert_eq!(managed.assignee_subject.as_deref(), Some("agent-2"));
        assert_eq!(managed.queue_id.as_deref(), Some("tier-2"));
        assert_eq!(managed.status, TicketStatus::PendingRequester);
        // Each applied field advances the domain revision; one save commits
        // the complete change set as a single atomic mutation.
        assert_eq!(managed.revision, ticket.revision + 4);

        // A late validation failure rejects the whole request: the valid
        // priority change must not commit when the status transition fails.
        let invalid = service
            .manage_ticket(
                &identity("agent", &["ticketing.agent.manage"]),
                "project-a",
                managed.id,
                AgentManagementInput {
                    priority: Some(TicketPriority::Urgent),
                    status: Some(TicketStatus::Closed),
                    ..AgentManagementInput::default()
                },
                managed.revision,
                Uuid::now_v7(),
                now,
            )
            .await;
        assert!(invalid.is_err());
        let unchanged = service
            .get_agent_ticket(
                &identity("agent", &["ticketing.agent.read"]),
                "project-a",
                managed.id,
            )
            .await
            .unwrap();
        assert_eq!(unchanged.priority, TicketPriority::High);
        assert_eq!(unchanged.revision, managed.revision);

        // Contradictory assignee instructions fail closed before any load.
        assert!(matches!(
            service
                .manage_ticket(
                    &identity("agent", &["ticketing.agent.manage"]),
                    "project-a",
                    managed.id,
                    AgentManagementInput {
                        assignee_subject: Some("agent-3".into()),
                        clear_assignee: true,
                        ..AgentManagementInput::default()
                    },
                    unchanged.revision,
                    Uuid::now_v7(),
                    now,
                )
                .await,
            Err(TicketingServiceError::InvalidManagementRequest)
        ));
    }

    #[tokio::test]
    async fn requester_list_is_forcibly_isolated_to_the_authenticated_subject() {
        let service = service();
        for subject in ["user-a", "user-b"] {
            service
                .create_ticket(
                    &identity(subject, &["ticketing.create"]),
                    CreateTicketInput {
                        requester: TicketRequester {
                            subject: subject.into(),
                            display_name: None,
                            email: None,
                        },
                        ..create_input(subject)
                    },
                    Uuid::now_v7(),
                    Utc::now(),
                )
                .await
                .unwrap();
        }
        // Even a caller-injected requester filter is overridden.
        let summaries = service
            .list_requester_summaries(
                &identity(
                    "user-a",
                    &["ticketing.requester.read", "ticketing.requester.write"],
                ),
                TicketSummaryFilter {
                    project_id: "project-a".into(),
                    requester_subject: Some("user-b".into()),
                    limit: 10,
                    ..TicketSummaryFilter::default()
                },
            )
            .await
            .unwrap();
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].subject, "user-a");
        assert_eq!(summaries[0].status, crate::PublicTicketStatus::Open);

        // A requester without ticketing.read is refused.
        assert!(matches!(
            service
                .list_requester_summaries(
                    &identity("user-a", &[]),
                    TicketSummaryFilter {
                        project_id: "project-a".into(),
                        limit: 10,
                        ..TicketSummaryFilter::default()
                    },
                )
                .await,
            Err(TicketingServiceError::PermissionDenied(_))
        ));
    }

    #[cfg(feature = "jobs")]
    #[tokio::test]
    async fn public_reply_couples_a_notification_job_only_when_configured() {
        let memory = Arc::new(MemoryTicketingStore::default());
        let mut config = test_config();
        config.notify_requester_on_public_reply = true;
        let service =
            TicketingService::new(TicketingStoreService::new(memory.clone()), config).unwrap();
        let created = service
            .create_ticket(
                &identity("user-a", &["ticketing.create"]),
                create_input("user-a"),
                Uuid::now_v7(),
                Utc::now(),
            )
            .await
            .unwrap()
            .ticket;
        service
            .reply_as_agent(
                &identity("agent-1", &["ticketing.reply"]),
                "project-a",
                created.id,
                "Public answer.".into(),
                created.revision,
                Uuid::now_v7(),
                Utc::now(),
            )
            .await
            .unwrap();
        let records = memory.enqueued_job_records().await;
        assert_eq!(records.len(), 1);
        assert_eq!(
            records[0].envelope.job_name,
            "ticketing.deliver-public-notification"
        );
        // Internal notes never notify.
        let after = service
            .get_agent_ticket(
                &identity("agent-1", &["ticketing.agent.read"]),
                "project-a",
                created.id,
            )
            .await
            .unwrap();
        service
            .add_internal_note(
                &identity("agent-1", &["ticketing.manage"]),
                "project-a",
                created.id,
                "private".into(),
                after.revision,
                Uuid::now_v7(),
                Utc::now(),
            )
            .await
            .unwrap();
        assert_eq!(memory.enqueued_job_records().await.len(), 1);

        // Default configuration attaches nothing.
        let quiet = Arc::new(MemoryTicketingStore::default());
        let service =
            TicketingService::new(TicketingStoreService::new(quiet.clone()), test_config())
                .unwrap();
        let created = service
            .create_ticket(
                &identity("user-a", &["ticketing.create"]),
                create_input("user-a"),
                Uuid::now_v7(),
                Utc::now(),
            )
            .await
            .unwrap()
            .ticket;
        service
            .reply_as_agent(
                &identity("agent-1", &["ticketing.reply"]),
                "project-a",
                created.id,
                "Public answer.".into(),
                created.revision,
                Uuid::now_v7(),
                Utc::now(),
            )
            .await
            .unwrap();
        assert!(quiet.enqueued_job_records().await.is_empty());
    }

    #[tokio::test]
    async fn concurrent_dispatch_publishes_each_intent_once_with_stable_identity() {
        let memory = Arc::new(MemoryTicketingStore::default());
        let (_plugin, bus) = minco_plugin_events::EventsPlugin::memory();
        let events = minco_plugin_events::EventServices {
            publisher: bus.clone(),
            outbox: bus.clone(),
        };
        let service =
            TicketingService::new(TicketingStoreService::new(memory.clone()), test_config())
                .unwrap()
                .with_portal_services(TicketingPortalServices {
                    events: Some(Arc::new(events)),
                    ..TicketingPortalServices::default()
                });
        let created = service
            .create_ticket(
                &identity("user-a", &["ticketing.create"]),
                create_input("user-a"),
                Uuid::now_v7(),
                Utc::now(),
            )
            .await
            .unwrap()
            .ticket;
        service
            .reply_as_agent(
                &identity("agent-1", &["ticketing.reply"]),
                "project-a",
                created.id,
                "Answer.".into(),
                created.revision,
                Uuid::now_v7(),
                Utc::now(),
            )
            .await
            .unwrap();
        // The published event identity must BE the intent identity.
        let intent_ids = TicketingStoreService::new(memory.clone())
            .pending_activity_intents("project-a", 100)
            .await
            .unwrap()
            .into_iter()
            .map(|intent| intent.id)
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(intent_ids.len(), 2);

        // Two racing passes must publish each intent exactly once; only
        // one worker can hold the outbox claim (review finding 9).
        let (first, second) = tokio::join!(
            service.dispatch_pending_activity("project-a", 10),
            service.dispatch_pending_activity("project-a", 10),
        );
        assert_eq!(first.unwrap() + second.unwrap(), 2);
        let published = bus.published().await;
        assert_eq!(published.len(), 2, "no intent may be published twice");
        let event_ids = published
            .iter()
            .map(|event| event.id)
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(event_ids, intent_ids);

        // A settled pass publishes nothing new.
        assert_eq!(
            service
                .dispatch_pending_activity("project-a", 10)
                .await
                .unwrap(),
            0
        );
        assert_eq!(bus.published().await.len(), 2);
    }

    #[tokio::test]
    async fn activity_intents_dispatch_once_as_domain_events() {
        let memory = Arc::new(MemoryTicketingStore::default());
        let (_plugin, bus) = minco_plugin_events::EventsPlugin::memory();
        let events = minco_plugin_events::EventServices {
            publisher: bus.clone(),
            outbox: bus.clone(),
        };
        let service =
            TicketingService::new(TicketingStoreService::new(memory.clone()), test_config())
                .unwrap()
                .with_portal_services(TicketingPortalServices {
                    events: Some(Arc::new(events)),
                    ..TicketingPortalServices::default()
                });
        let created = service
            .create_ticket(
                &identity("user-a", &["ticketing.create"]),
                create_input("user-a"),
                Uuid::now_v7(),
                Utc::now(),
            )
            .await
            .unwrap()
            .ticket;
        service
            .reply_as_agent(
                &identity("agent-1", &["ticketing.reply"]),
                "project-a",
                created.id,
                "Answer.".into(),
                created.revision,
                Uuid::now_v7(),
                Utc::now(),
            )
            .await
            .unwrap();

        let published = service
            .dispatch_pending_activity("project-a", 10)
            .await
            .unwrap();
        assert_eq!(published, 2);
        let events = bus.published().await;
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].event_type, "ticketing.created");
        assert_eq!(events[1].event_type, "ticketing.agent_replied");
        assert_eq!(events[0].aggregate_type, "ticketing.ticket");
        assert_eq!(events[0].aggregate_id, created.id.to_string());
        assert!(!events[0].correlation_id.is_nil());

        // Second pass publishes nothing: intents are marked published.
        assert_eq!(
            service
                .dispatch_pending_activity("project-a", 10)
                .await
                .unwrap(),
            0
        );
        assert_eq!(bus.published().await.len(), 2);
        assert_eq!(memory.published_intent_ids().await.len(), 2);

        // Without the events service the pass fails closed.
        let bare = TicketingService::new(
            TicketingStoreService::new(Arc::new(MemoryTicketingStore::default())),
            test_config(),
        )
        .unwrap();
        assert!(matches!(
            bare.dispatch_pending_activity("project-a", 10).await,
            Err(TicketingServiceError::EventsUnavailable)
        ));
    }

    #[cfg(feature = "jobs")]
    mod inbound_routing {
        use super::*;
        use crate::ExternalMessageIdentity;

        fn ingress_identity() -> Identity {
            Identity {
                subject: "ingress".into(),
                permissions: std::iter::once("ticketing.ingest".into()).collect(),
                scopes: BTreeSet::default(),
                claims: BTreeMap::new(),
            }
        }

        async fn routed_service() -> (
            TicketingService,
            Arc<minco_plugin_jobs::MemoryJobStore>,
            TicketId,
        ) {
            routed_service_with_objects(Arc::new(
                minco_plugin_object_storage::MemoryObjectStore::default(),
            ))
            .await
        }

        async fn routed_service_with_objects(
            objects: Arc<minco_plugin_object_storage::MemoryObjectStore>,
        ) -> (
            TicketingService,
            Arc<minco_plugin_jobs::MemoryJobStore>,
            TicketId,
        ) {
            let memory = Arc::new(MemoryTicketingStore::default());
            let registry = Arc::new(minco_plugin_jobs::JobHandlerRegistry::new());
            let (jobs, store, _dispatcher) = minco_plugin_jobs::JobsServices::memory(registry);
            let service =
                TicketingService::new(TicketingStoreService::new(memory.clone()), test_config())
                    .unwrap()
                    .with_portal_services(TicketingPortalServices {
                        jobs: Some(Arc::new(jobs)),
                        objects: Some(Arc::new(
                            minco_plugin_object_storage::ObjectStoreService::new(objects),
                        )),
                        ..TicketingPortalServices::default()
                    });
            let created = service
                .create_ticket(
                    &identity("user-a", &["ticketing.create"]),
                    create_input("user-a"),
                    Uuid::now_v7(),
                    Utc::now(),
                )
                .await
                .unwrap()
                .ticket;
            // One previously ingested external message carries the
            // internet message id that replies thread against.
            let identity_record = ExternalMessageIdentity {
                project_id: "project-a".into(),
                provider: "ses".into(),
                mailbox_scope: "support@example.test".into(),
                external_id: "original-1".into(),
                content_sha256: "a".repeat(64),
                raw_message_object_key: None,
                internet_message_id: Some("<original-1@example.test>".into()),
                in_reply_to: None,
                references: Vec::new(),
            };
            service
                .ingest_external_message(
                    &ingress_identity(),
                    identity_record,
                    created.id,
                    "Original external reply".into(),
                    Uuid::now_v7(),
                    Utc::now(),
                )
                .await
                .unwrap();
            (service, store, created.id)
        }

        #[tokio::test]
        async fn inbound_email_routes_by_threading_and_submits_durably() {
            let (service, store, ticket_id) = routed_service().await;
            let arrival = Utc::now();
            let digest = "b".repeat(64);
            let job_id = service
                .submit_inbound_email(
                    "ses",
                    "support@example.test",
                    "reply-1",
                    &digest,
                    "mail/project-a/reply-1",
                    None,
                    Some("<original-1@example.test>"),
                    &[],
                    Uuid::now_v7(),
                    arrival,
                )
                .await
                .unwrap();
            let record = store
                .records()
                .into_iter()
                .find(|record| record.envelope.job_id == job_id)
                .expect("job recorded");
            assert_eq!(record.envelope.job_name, "ticketing.process-inbound-email");
            {
                use sha2::Digest;
                let mut hasher = sha2::Sha256::new();
                for part in ["ses", "support@example.test", "reply-1"] {
                    hasher.update((part.len() as u64).to_le_bytes());
                    hasher.update(part.as_bytes());
                }
                let expected = format!("mail:{}", hex::encode(hasher.finalize()));
                assert_eq!(
                    record.envelope.dedupe_key.as_deref(),
                    Some(expected.as_str())
                );
            }
            let payload = serde_json::from_value::<crate::ProcessInboundEmail>(
                record.envelope.payload.clone(),
            )
            .unwrap();
            assert_eq!(payload.ticket_id, ticket_id);

            // References chain resolves when In-Reply-To is absent.
            let chained_job_id = service
                .submit_inbound_email(
                    "ses",
                    "support@example.test",
                    "reply-2",
                    &digest,
                    "mail/project-a/reply-2",
                    None,
                    None,
                    &[
                        "<unrelated@example.test>".into(),
                        "<original-1@example.test>".into(),
                    ],
                    Uuid::now_v7(),
                    arrival,
                )
                .await
                .unwrap();
            assert!(!chained_job_id.is_nil());

            // Same external identity resubmits to the same durable job.
            let again = service
                .submit_inbound_email(
                    "ses",
                    "support@example.test",
                    "reply-1",
                    &digest,
                    "mail/project-a/reply-1",
                    None,
                    Some("<original-1@example.test>"),
                    &[],
                    Uuid::now_v7(),
                    arrival,
                )
                .await
                .unwrap();
            assert_eq!(again, job_id);
        }

        #[tokio::test]
        async fn unresolved_threading_fails_closed_and_jobs_handle_is_required() {
            let (service, _store, _ticket_id) = routed_service().await;
            assert!(matches!(
                service
                    .submit_inbound_email(
                        "ses",
                        "support@example.test",
                        "orphan-1",
                        &"c".repeat(64),
                        "mail/project-a/orphan-1",
                        None,
                        Some("<unknown@example.test>"),
                        &[],
                        Uuid::now_v7(),
                        Utc::now(),
                    )
                    .await,
                Err(TicketingServiceError::InboundThreadUnresolved)
            ));

            // Without the jobs handle nothing is submitted.
            let bare = TicketingService::new(
                TicketingStoreService::new(Arc::new(MemoryTicketingStore::default())),
                test_config(),
            )
            .unwrap();
            assert!(matches!(
                bare.submit_inbound_email(
                    "ses",
                    "support@example.test",
                    "x",
                    &"c".repeat(64),
                    "k",
                    None,
                    None,
                    &[],
                    Uuid::now_v7(),
                    Utc::now(),
                )
                .await,
                Err(TicketingServiceError::JobsUnavailable)
            ));
        }

        const REPLY_EMAIL: &str = "From: user-1@example.test\r\n\
            To: support@example.test\r\n\
            Subject: Re: Help\r\n\
            Message-ID: <reply-9@example.test>\r\n\
            In-Reply-To: <original-1@example.test>\r\n\
            References: <older@example.test> <original-1@example.test>\r\n\
            MIME-Version: 1.0\r\n\
            Content-Type: text/plain; charset=utf-8\r\n\
            \r\n\
            A threaded reply.\r\n";

        #[tokio::test]
        async fn wake_extracts_routing_facts_and_submits_the_durable_job() {
            use minco_plugin_object_storage::{ObjectStore as _, PutObject};
            let objects = Arc::new(minco_plugin_object_storage::MemoryObjectStore::default());
            objects
                .put(PutObject {
                    key: minco_plugin_object_storage::ObjectKey::parse("mail/project-a/reply-9")
                        .unwrap(),
                    bytes: REPLY_EMAIL.as_bytes().to_vec(),
                    content_type: "message/rfc822".into(),
                    attributes: BTreeMap::new(),
                })
                .await
                .unwrap();
            let (service, store, ticket_id) = routed_service_with_objects(objects).await;
            let arrival = Utc::now();
            let job_id = service
                .wake_inbound_email(
                    "ses",
                    "support@example.test",
                    "wake-reply-9",
                    "mail/project-a/reply-9",
                    Uuid::now_v7(),
                    arrival,
                )
                .await
                .unwrap();
            let record = store
                .records()
                .into_iter()
                .find(|record| record.envelope.job_id == job_id)
                .expect("durable job recorded");
            let payload = serde_json::from_value::<crate::ProcessInboundEmail>(
                record.envelope.payload.clone(),
            )
            .unwrap();
            assert_eq!(payload.ticket_id, ticket_id);
            assert_eq!(
                payload.content_sha256,
                crate::external_content_sha256(REPLY_EMAIL.as_bytes())
            );
            assert_eq!(
                payload.internet_message_id.as_deref(),
                Some("<reply-9@example.test>")
            );
            assert_eq!(
                payload.in_reply_to.as_deref(),
                Some("<original-1@example.test>")
            );
            assert_eq!(payload.references.len(), 2);
            // Same wake replays to the same durable job (stable fingerprint).
            let again = service
                .wake_inbound_email(
                    "ses",
                    "support@example.test",
                    "wake-reply-9",
                    "mail/project-a/reply-9",
                    Uuid::now_v7(),
                    arrival,
                )
                .await
                .unwrap();
            assert_eq!(again, job_id);
        }

        #[tokio::test]
        async fn wake_fails_closed_for_missing_object_and_garbage_mime() {
            use minco_plugin_object_storage::{ObjectStore as _, PutObject};
            let objects = Arc::new(minco_plugin_object_storage::MemoryObjectStore::default());
            let (service, _store, _ticket_id) = routed_service_with_objects(objects.clone()).await;
            assert!(matches!(
                service
                    .wake_inbound_email(
                        "ses",
                        "support@example.test",
                        "wake-missing",
                        "mail/project-a/missing",
                        Uuid::now_v7(),
                        Utc::now(),
                    )
                    .await,
                Err(TicketingServiceError::InboundObjectMissing)
            ));
            objects
                .put(PutObject {
                    key: minco_plugin_object_storage::ObjectKey::parse("mail/project-a/garbage")
                        .unwrap(),
                    bytes: b"\x00\x01 garbage \xff".to_vec(),
                    content_type: "application/octet-stream".into(),
                    attributes: BTreeMap::new(),
                })
                .await
                .unwrap();
            // mail-parser is deliberately lenient: headerless garbage parses
            // as an empty single part, so the wake fails closed one step
            // later, at threading resolution — never at ingestion.
            assert!(matches!(
                service
                    .wake_inbound_email(
                        "ses",
                        "support@example.test",
                        "wake-garbage",
                        "mail/project-a/garbage",
                        Uuid::now_v7(),
                        Utc::now(),
                    )
                    .await,
                Err(TicketingServiceError::InboundMimeInvalid
                    | TicketingServiceError::InboundThreadUnresolved)
            ));

            // Without the object handle the wake fails closed.
            let bare = TicketingService::new(
                TicketingStoreService::new(Arc::new(MemoryTicketingStore::default())),
                test_config(),
            )
            .unwrap();
            assert!(matches!(
                bare.wake_inbound_email(
                    "ses",
                    "support@example.test",
                    "x",
                    "k",
                    Uuid::now_v7(),
                    Utc::now(),
                )
                .await,
                Err(TicketingServiceError::ObjectsUnavailable)
            ));
        }
    }
}
