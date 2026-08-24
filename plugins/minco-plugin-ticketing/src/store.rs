use crate::{
    CreateTicketInput, MAX_TICKET_LIST_FETCH_LIMIT, Ticket, TicketFromHandoffInput, TicketId,
    TicketRequester, TicketStatus, TicketSummary, TicketValidationError,
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use minco_interaction::{
    SupportHandoff, SupportHandoffDigest, SupportHandoffResult, SupportHandoffToken,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    sync::Arc,
};
use subtle::ConstantTimeEq;
use tokio::sync::Mutex;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TicketActivityIntent {
    pub id: Uuid,
    pub project_id: String,
    pub ticket_id: TicketId,
    pub kind: String,
    pub correlation_id: Uuid,
    pub payload: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

impl TicketActivityIntent {
    #[must_use]
    pub fn new(
        project_id: impl Into<String>,
        ticket_id: TicketId,
        kind: impl Into<String>,
        correlation_id: Uuid,
        payload: serde_json::Value,
        created_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id: Uuid::now_v7(),
            project_id: project_id.into(),
            ticket_id,
            kind: kind.into(),
            correlation_id,
            payload,
            created_at,
        }
    }
}

/// Newest-first compact summary filter. `before_*` is the exclusive pagination
/// cursor: only tickets strictly after (older than) the cursor pair match.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TicketSummaryFilter {
    pub project_id: String,
    pub statuses: BTreeSet<TicketStatus>,
    pub queue_id: Option<String>,
    pub assignee_subject: Option<String>,
    pub requester_subject: Option<String>,
    pub before_updated_at: Option<DateTime<Utc>>,
    pub before_id: Option<TicketId>,
    pub limit: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TicketListFilter {
    pub project_id: String,
    pub statuses: BTreeSet<TicketStatus>,
    pub queue_id: Option<String>,
    pub assignee_subject: Option<String>,
    pub requester_subject: Option<String>,
    pub after_updated_at: Option<DateTime<Utc>>,
    pub after_id: Option<TicketId>,
    pub limit: usize,
}

#[derive(Clone, PartialEq, Eq)]
pub struct ConsumeHandoffRequest {
    pub token: SupportHandoffToken,
    pub project_id: String,
    pub portal_origin: String,
    pub input: TicketFromHandoffInput,
    pub request_fingerprint: String,
    pub now: DateTime<Utc>,
}

impl fmt::Debug for ConsumeHandoffRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ConsumeHandoffRequest")
            .field("token", &self.token)
            .field("project_id", &self.project_id)
            .field("portal_origin", &self.portal_origin)
            .field("input", &"[BOUNDED]")
            .field("request_fingerprint", &"[REDACTED]")
            .field("now", &self.now)
            .finish()
    }
}

impl ConsumeHandoffRequest {
    pub fn new(
        token: SupportHandoffToken,
        project_id: impl Into<String>,
        portal_origin: impl Into<String>,
        input: TicketFromHandoffInput,
        now: DateTime<Utc>,
    ) -> Result<Self, TicketStoreError> {
        let project_id = project_id.into();
        let portal_origin = portal_origin.into();
        let serialized = serde_json::to_vec(&serde_json::json!({
            "project_id": project_id,
            "portal_origin": portal_origin,
            "subject": input.subject,
            "description": input.description,
            "channel": input.channel,
            "priority": input.priority,
        }))
        .map_err(|error| TicketStoreError::Infrastructure(error.to_string()))?;
        Ok(Self {
            token,
            project_id,
            portal_origin,
            input,
            request_fingerprint: hex::encode(Sha256::digest(serialized)),
            now,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsumedHandoff {
    pub ticket: Ticket,
    pub result: SupportHandoffResult,
    pub repeated: bool,
}

/// One-time handoff consumption that establishes a requester identity
/// without creating a ticket. Each handoff can be consumed for a session
/// exactly once; an identical replay returns the same identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsumeSessionRequest {
    pub token: SupportHandoffToken,
    pub project_id: String,
    pub portal_origin: String,
    pub request_fingerprint: String,
    pub now: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsumedSessionIdentity {
    pub requester_subject: String,
    pub requester_permissions: Vec<String>,
    pub correlation_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalMessageIdentity {
    pub project_id: String,
    pub provider: String,
    pub mailbox_scope: String,
    pub external_id: String,
    pub content_sha256: String,
    pub raw_message_object_key: Option<String>,
    pub internet_message_id: Option<String>,
    pub in_reply_to: Option<String>,
    pub references: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IngestExternalMessageRequest {
    pub identity: ExternalMessageIdentity,
    pub ticket_id: TicketId,
    pub body: String,
    pub expected_revision: u64,
    pub correlation_id: Uuid,
    pub now: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalMessageIngestResult {
    pub ticket: Ticket,
    pub repeated: bool,
}

/// One message append committed without rewriting the conversation
/// (ADR-0052, ADR-0054).
///
/// The projection snapshot is the complete post-append ticket row state;
/// only the listed columns are updated. Under the optional `jobs` feature,
/// `job_records` are enqueued in the same transaction; they are bounded
/// and carry identifiers only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppendTicketMessageRequest {
    pub project_id: String,
    pub ticket_id: TicketId,
    pub message: crate::TicketMessage,
    pub status: crate::TicketStatus,
    pub first_public_response_at: Option<DateTime<Utc>>,
    pub waiting_since: Option<DateTime<Utc>>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
    pub expected_revision: u64,
    pub intent: TicketActivityIntent,
    #[cfg(feature = "jobs")]
    pub job_records: Vec<minco_plugin_jobs::JobRecord>,
}

/// Newest-first bounded message pagination over one ticket's conversation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MessageListFilter {
    pub project_id: String,
    pub ticket_id: TicketId,
    pub include_internal: bool,
    pub before_created_at: Option<DateTime<Utc>>,
    pub before_id: Option<crate::TicketMessageId>,
    pub limit: usize,
}

#[async_trait]
pub trait TicketingStore: Send + Sync + fmt::Debug {
    async fn create(
        &self,
        ticket: Ticket,
        intent: TicketActivityIntent,
    ) -> Result<(), TicketStoreError>;

    async fn get(&self, project_id: &str, id: TicketId)
    -> Result<Option<Ticket>, TicketStoreError>;

    async fn list(&self, filter: TicketListFilter) -> Result<Vec<Ticket>, TicketStoreError>;

    async fn list_summaries(
        &self,
        filter: TicketSummaryFilter,
    ) -> Result<Vec<TicketSummary>, TicketStoreError>;

    async fn append_ticket_message(
        &self,
        request: AppendTicketMessageRequest,
    ) -> Result<(), TicketStoreError>;

    async fn list_ticket_messages(
        &self,
        filter: MessageListFilter,
    ) -> Result<Vec<crate::TicketMessage>, TicketStoreError>;

    async fn save(
        &self,
        ticket: Ticket,
        expected_revision: u64,
        intent: TicketActivityIntent,
    ) -> Result<(), TicketStoreError>;

    async fn insert_handoff(&self, handoff: SupportHandoff) -> Result<(), TicketStoreError>;

    async fn consume_and_create_ticket(
        &self,
        request: ConsumeHandoffRequest,
    ) -> Result<ConsumedHandoff, TicketStoreError>;

    async fn consume_handoff_identity(
        &self,
        request: ConsumeSessionRequest,
    ) -> Result<(ConsumedSessionIdentity, bool), TicketStoreError>;

    async fn ingest_external_message(
        &self,
        request: IngestExternalMessageRequest,
    ) -> Result<ExternalMessageIngestResult, TicketStoreError>;

    async fn ready(&self) -> Result<(), TicketStoreError> {
        Ok(())
    }
}

#[derive(Clone)]
pub struct TicketingStoreService(pub Arc<dyn TicketingStore>);

impl fmt::Debug for TicketingStoreService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_tuple("TicketingStoreService").finish()
    }
}

impl TicketingStoreService {
    pub fn new(store: Arc<dyn TicketingStore>) -> Self {
        Self(store)
    }

    pub async fn create(
        &self,
        ticket: Ticket,
        intent: TicketActivityIntent,
    ) -> Result<(), TicketStoreError> {
        self.0.create(ticket, intent).await
    }

    pub async fn get(
        &self,
        project_id: &str,
        id: TicketId,
    ) -> Result<Option<Ticket>, TicketStoreError> {
        self.0.get(project_id, id).await
    }

    pub async fn list(&self, filter: TicketListFilter) -> Result<Vec<Ticket>, TicketStoreError> {
        self.0.list(filter).await
    }

    pub async fn list_summaries(
        &self,
        filter: TicketSummaryFilter,
    ) -> Result<Vec<TicketSummary>, TicketStoreError> {
        self.0.list_summaries(filter).await
    }

    pub async fn append_ticket_message(
        &self,
        request: AppendTicketMessageRequest,
    ) -> Result<(), TicketStoreError> {
        self.0.append_ticket_message(request).await
    }

    pub async fn list_ticket_messages(
        &self,
        filter: MessageListFilter,
    ) -> Result<Vec<crate::TicketMessage>, TicketStoreError> {
        self.0.list_ticket_messages(filter).await
    }

    pub async fn save(
        &self,
        ticket: Ticket,
        expected_revision: u64,
        intent: TicketActivityIntent,
    ) -> Result<(), TicketStoreError> {
        self.0.save(ticket, expected_revision, intent).await
    }

    pub async fn insert_handoff(&self, handoff: SupportHandoff) -> Result<(), TicketStoreError> {
        self.0.insert_handoff(handoff).await
    }

    pub async fn consume_and_create_ticket(
        &self,
        request: ConsumeHandoffRequest,
    ) -> Result<ConsumedHandoff, TicketStoreError> {
        self.0.consume_and_create_ticket(request).await
    }

    pub async fn consume_handoff_identity(
        &self,
        request: ConsumeSessionRequest,
    ) -> Result<(ConsumedSessionIdentity, bool), TicketStoreError> {
        self.0.consume_handoff_identity(request).await
    }

    pub async fn ingest_external_message(
        &self,
        request: IngestExternalMessageRequest,
    ) -> Result<ExternalMessageIngestResult, TicketStoreError> {
        self.0.ingest_external_message(request).await
    }

    pub async fn ready(&self) -> Result<(), TicketStoreError> {
        self.0.ready().await
    }
}

#[derive(Debug, Clone)]
struct MemoryHandoff {
    handoff: SupportHandoff,
    completed_fingerprint: Option<String>,
    consumed_identity: Option<(String, ConsumedSessionIdentity)>,
}

#[derive(Debug, Clone, Default)]
struct MemoryState {
    tickets: BTreeMap<(String, TicketId), Ticket>,
    display_counters: BTreeMap<String, u64>,
    handoffs: BTreeMap<SupportHandoffDigest, MemoryHandoff>,
    external_messages:
        BTreeMap<(String, String, String, String), (ExternalMessageIdentity, TicketId)>,
    activity_intents: Vec<TicketActivityIntent>,
    #[cfg(feature = "jobs")]
    enqueued_job_records: Vec<minco_plugin_jobs::JobRecord>,
    fail_next_handoff_commit: bool,
}

#[derive(Debug, Default)]
pub struct MemoryTicketingStore {
    state: Mutex<MemoryState>,
}

impl MemoryTicketingStore {
    pub async fn activity_intents(&self) -> Vec<TicketActivityIntent> {
        self.state.lock().await.activity_intents.clone()
    }

    /// Job records committed transactionally with ticket mutations
    /// (ADR-0054); the memory profile records them for inspection.
    #[cfg(feature = "jobs")]
    pub async fn enqueued_job_records(&self) -> Vec<minco_plugin_jobs::JobRecord> {
        self.state.lock().await.enqueued_job_records.clone()
    }

    pub async fn fail_next_handoff_commit(&self) {
        self.state.lock().await.fail_next_handoff_commit = true;
    }

    pub async fn ticket_count(&self) -> usize {
        self.state.lock().await.tickets.len()
    }

    pub async fn handoff_consumed(&self, digest: &SupportHandoffDigest) -> bool {
        self.state
            .lock()
            .await
            .handoffs
            .get(digest)
            .is_some_and(|entry| entry.handoff.consumed_result.is_some())
    }
}

#[async_trait]
impl TicketingStore for MemoryTicketingStore {
    async fn create(
        &self,
        ticket: Ticket,
        intent: TicketActivityIntent,
    ) -> Result<(), TicketStoreError> {
        let mut state = self.state.lock().await;
        let key = (ticket.project_id.clone(), ticket.id);
        if state.tickets.contains_key(&key) {
            return Err(TicketStoreError::DuplicateTicket(ticket.id));
        }
        if state.tickets.values().any(|existing| {
            existing.project_id == ticket.project_id
                && existing.display_reference == ticket.display_reference
        }) {
            return Err(TicketStoreError::DuplicateDisplayReference);
        }
        state.tickets.insert(key, ticket);
        state.activity_intents.push(intent);
        drop(state);
        Ok(())
    }

    async fn get(
        &self,
        project_id: &str,
        id: TicketId,
    ) -> Result<Option<Ticket>, TicketStoreError> {
        Ok(self
            .state
            .lock()
            .await
            .tickets
            .get(&(project_id.to_owned(), id))
            .cloned())
    }

    async fn list(&self, filter: TicketListFilter) -> Result<Vec<Ticket>, TicketStoreError> {
        if !(1..=MAX_TICKET_LIST_FETCH_LIMIT).contains(&filter.limit) {
            return Err(TicketStoreError::InvalidListLimit);
        }
        if filter.after_updated_at.is_some() != filter.after_id.is_some() {
            return Err(TicketStoreError::InvalidListCursor);
        }
        let mut values = self
            .state
            .lock()
            .await
            .tickets
            .values()
            .filter(|ticket| ticket.project_id == filter.project_id)
            .filter(|ticket| filter.statuses.is_empty() || filter.statuses.contains(&ticket.status))
            .filter(|ticket| {
                filter
                    .queue_id
                    .as_ref()
                    .is_none_or(|value| ticket.queue_id.as_ref() == Some(value))
            })
            .filter(|ticket| {
                filter
                    .assignee_subject
                    .as_ref()
                    .is_none_or(|value| ticket.assignee_subject.as_ref() == Some(value))
            })
            .filter(|ticket| {
                filter
                    .requester_subject
                    .as_ref()
                    .is_none_or(|value| &ticket.requester.subject == value)
            })
            .filter(|ticket| match (filter.after_updated_at, filter.after_id) {
                (Some(updated), Some(id)) => (ticket.updated_at, ticket.id) > (updated, id),
                _ => true,
            })
            .cloned()
            .collect::<Vec<_>>();
        values.sort_by_key(|ticket| (ticket.updated_at, ticket.id));
        values.truncate(filter.limit);
        Ok(values)
    }

    async fn list_summaries(
        &self,
        filter: TicketSummaryFilter,
    ) -> Result<Vec<TicketSummary>, TicketStoreError> {
        if !(1..=MAX_TICKET_LIST_FETCH_LIMIT).contains(&filter.limit) {
            return Err(TicketStoreError::InvalidListLimit);
        }
        if filter.before_updated_at.is_some() != filter.before_id.is_some() {
            return Err(TicketStoreError::InvalidListCursor);
        }
        let mut summaries = self
            .state
            .lock()
            .await
            .tickets
            .values()
            .filter(|ticket| ticket.project_id == filter.project_id)
            .filter(|ticket| filter.statuses.is_empty() || filter.statuses.contains(&ticket.status))
            .filter(|ticket| {
                filter
                    .queue_id
                    .as_ref()
                    .is_none_or(|value| ticket.queue_id.as_ref() == Some(value))
            })
            .filter(|ticket| {
                filter
                    .assignee_subject
                    .as_ref()
                    .is_none_or(|value| ticket.assignee_subject.as_ref() == Some(value))
            })
            .filter(|ticket| {
                filter
                    .requester_subject
                    .as_ref()
                    .is_none_or(|value| &ticket.requester.subject == value)
            })
            .filter(
                |ticket| match (filter.before_updated_at, filter.before_id) {
                    (Some(updated), Some(id)) => (ticket.updated_at, ticket.id) < (updated, id),
                    _ => true,
                },
            )
            .map(Ticket::agent_summary)
            .collect::<Vec<_>>();
        summaries.sort_by_key(|summary| std::cmp::Reverse((summary.updated_at, summary.id)));
        summaries.truncate(filter.limit);
        Ok(summaries)
    }

    async fn append_ticket_message(
        &self,
        request: AppendTicketMessageRequest,
    ) -> Result<(), TicketStoreError> {
        let mut state = self.state.lock().await;
        let key = (request.project_id.clone(), request.ticket_id);
        let ticket = state
            .tickets
            .get_mut(&key)
            .ok_or(TicketStoreError::NotFound(request.ticket_id))?;
        if ticket.revision != request.expected_revision {
            return Err(TicketStoreError::StaleRevision {
                expected: request.expected_revision,
                actual: ticket.revision,
            });
        }
        #[cfg(feature = "jobs")]
        if request.job_records.len() > crate::MAX_JOB_RECORDS_PER_MUTATION {
            return Err(TicketStoreError::InvalidJobRecords);
        }
        ticket.messages.push(request.message);
        ticket.status = request.status;
        ticket.first_public_response_at = request.first_public_response_at;
        ticket.waiting_since = request.waiting_since;
        ticket.resolved_at = request.resolved_at;
        ticket.updated_at = request.updated_at;
        ticket.revision = request.expected_revision + 1;
        state.activity_intents.push(request.intent);
        #[cfg(feature = "jobs")]
        state
            .enqueued_job_records
            .extend(request.job_records.iter().cloned());
        drop(state);
        Ok(())
    }

    // The guard is already scoped to the snapshot block; the collect makes
    // the lint over-approximate its live range.
    #[allow(clippy::significant_drop_tightening)]
    async fn list_ticket_messages(
        &self,
        filter: MessageListFilter,
    ) -> Result<Vec<crate::TicketMessage>, TicketStoreError> {
        if !(1..=MAX_TICKET_LIST_FETCH_LIMIT).contains(&filter.limit) {
            return Err(TicketStoreError::InvalidListLimit);
        }
        if filter.before_created_at.is_some() != filter.before_id.is_some() {
            return Err(TicketStoreError::InvalidListCursor);
        }
        let mut messages;
        {
            let state = self.state.lock().await;
            let ticket = state
                .tickets
                .get(&(filter.project_id.clone(), filter.ticket_id))
                .ok_or(TicketStoreError::NotFound(filter.ticket_id))?;
            messages = ticket
                .messages
                .iter()
                .filter(|message| {
                    filter.include_internal
                        || message.kind != crate::TicketMessageKind::InternalNote
                })
                .filter(
                    |message| match (filter.before_created_at, filter.before_id) {
                        (Some(created), Some(id)) => {
                            (message.created_at, message.id) < (created, id)
                        }
                        _ => true,
                    },
                )
                .cloned()
                .collect::<Vec<_>>();
        }
        messages.sort_by_key(|message| std::cmp::Reverse((message.created_at, message.id)));
        messages.truncate(filter.limit);
        Ok(messages)
    }

    async fn save(
        &self,
        ticket: Ticket,
        expected_revision: u64,
        intent: TicketActivityIntent,
    ) -> Result<(), TicketStoreError> {
        let mut state = self.state.lock().await;
        let key = (ticket.project_id.clone(), ticket.id);
        let existing = state
            .tickets
            .get(&key)
            .ok_or(TicketStoreError::NotFound(ticket.id))?;
        if existing.revision != expected_revision || ticket.revision <= expected_revision {
            return Err(TicketStoreError::StaleRevision {
                expected: expected_revision,
                actual: existing.revision,
            });
        }
        state.tickets.insert(key, ticket);
        state.activity_intents.push(intent);
        drop(state);
        Ok(())
    }

    async fn insert_handoff(&self, handoff: SupportHandoff) -> Result<(), TicketStoreError> {
        let mut state = self.state.lock().await;
        if state.handoffs.contains_key(&handoff.digest) {
            return Err(TicketStoreError::DuplicateHandoff);
        }
        state.handoffs.insert(
            handoff.digest.clone(),
            MemoryHandoff {
                handoff,
                completed_fingerprint: None,
                consumed_identity: None,
            },
        );
        drop(state);
        Ok(())
    }

    async fn consume_and_create_ticket(
        &self,
        request: ConsumeHandoffRequest,
    ) -> Result<ConsumedHandoff, TicketStoreError> {
        let mut state = self.state.lock().await;
        let mut staged = state.clone();
        let digest = request.token.digest();
        let matched_digest = staged
            .handoffs
            .keys()
            .find(|candidate| {
                candidate
                    .as_str()
                    .as_bytes()
                    .ct_eq(digest.as_str().as_bytes())
                    .into()
            })
            .cloned()
            .ok_or(TicketStoreError::UnknownHandoff)?;
        let entry = staged
            .handoffs
            .get_mut(&matched_digest)
            .expect("matched key exists");
        if entry.handoff.project_id != request.project_id {
            return Err(TicketStoreError::WrongHandoffProject);
        }
        if entry.handoff.portal_origin != request.portal_origin {
            return Err(TicketStoreError::WrongHandoffPortal);
        }
        if let Some(result) = entry.handoff.consumed_result.clone() {
            if entry.completed_fingerprint.as_deref() != Some(&request.request_fingerprint) {
                return Err(TicketStoreError::HandoffAlreadyConsumed);
            }
            let ticket = staged
                .tickets
                .get(&(request.project_id.clone(), TicketId(result.ticket_id)))
                .cloned()
                .ok_or_else(|| {
                    TicketStoreError::Infrastructure("completed handoff ticket is missing".into())
                })?;
            return Ok(ConsumedHandoff {
                ticket,
                result,
                repeated: true,
            });
        }
        if entry.handoff.expires_at <= request.now {
            return Err(TicketStoreError::ExpiredHandoff);
        }
        let project_id = entry.handoff.project_id.clone();
        let requester_subject = entry.handoff.requester_subject.clone();
        let resources = entry.handoff.context.resource_references.clone();
        let correlation_id = entry.handoff.correlation_id;
        let counter = staged
            .display_counters
            .get(&project_id)
            .copied()
            .unwrap_or(0)
            .saturating_add(1);
        let ticket = Ticket::create(
            CreateTicketInput {
                project_id: project_id.clone(),
                subject: request.input.subject,
                description: request.input.description,
                requester: TicketRequester {
                    subject: requester_subject,
                    display_name: None,
                    email: None,
                },
                channel: request.input.channel,
                priority: request.input.priority,
                resource_references: resources,
            },
            format!("TKT-{counter:06}"),
            request.now,
        )?;
        let result = SupportHandoffResult {
            ticket_id: ticket.id.0,
            requester_session_id: Uuid::now_v7(),
        };
        let intent = TicketActivityIntent::new(
            project_id.clone(),
            ticket.id,
            "ticketing.created_from_handoff",
            correlation_id,
            serde_json::json!({ "ticket_id": ticket.id, "handoff_id": entry.handoff.id }),
            request.now,
        );
        entry.handoff.consumed_result = Some(result.clone());
        entry.completed_fingerprint = Some(request.request_fingerprint);
        staged.display_counters.insert(project_id.clone(), counter);
        staged
            .tickets
            .insert((project_id, ticket.id), ticket.clone());
        staged.activity_intents.push(intent);
        if staged.fail_next_handoff_commit {
            state.fail_next_handoff_commit = false;
            return Err(TicketStoreError::Infrastructure(
                "injected handoff transaction failure".into(),
            ));
        }
        *state = staged;
        drop(state);
        Ok(ConsumedHandoff {
            ticket,
            result,
            repeated: false,
        })
    }

    async fn consume_handoff_identity(
        &self,
        request: ConsumeSessionRequest,
    ) -> Result<(ConsumedSessionIdentity, bool), TicketStoreError> {
        let mut state = self.state.lock().await;
        let digest = request.token.digest();
        let matched_digest = state
            .handoffs
            .keys()
            .find(|candidate| {
                candidate
                    .as_str()
                    .as_bytes()
                    .ct_eq(digest.as_str().as_bytes())
                    .into()
            })
            .cloned()
            .ok_or(TicketStoreError::UnknownHandoff)?;
        let entry = state
            .handoffs
            .get(&matched_digest)
            .expect("matched key exists");
        if !entry.handoff.digest.matches_token(&request.token) {
            return Err(TicketStoreError::UnknownHandoff);
        }
        if entry.handoff.project_id != request.project_id {
            return Err(TicketStoreError::WrongHandoffProject);
        }
        if entry.handoff.portal_origin != request.portal_origin {
            return Err(TicketStoreError::WrongHandoffPortal);
        }
        let identity = ConsumedSessionIdentity {
            requester_subject: entry.handoff.requester_subject.clone(),
            requester_permissions: entry.handoff.requester_permissions.clone(),
            correlation_id: entry.handoff.correlation_id,
        };
        if let Some((fingerprint, existing)) = entry.consumed_identity.clone() {
            if fingerprint != request.request_fingerprint {
                return Err(TicketStoreError::HandoffAlreadyConsumed);
            }
            return Ok((existing, true));
        }
        if entry.handoff.expires_at <= request.now {
            return Err(TicketStoreError::ExpiredHandoff);
        }
        let entry = state
            .handoffs
            .get_mut(&matched_digest)
            .expect("matched key exists");
        entry.consumed_identity = Some((request.request_fingerprint, identity.clone()));
        drop(state);
        Ok((identity, false))
    }

    async fn ingest_external_message(
        &self,
        request: IngestExternalMessageRequest,
    ) -> Result<ExternalMessageIngestResult, TicketStoreError> {
        let mut state = self.state.lock().await;
        let external_key = (
            request.identity.project_id.clone(),
            request.identity.provider.clone(),
            request.identity.mailbox_scope.clone(),
            request.identity.external_id.clone(),
        );
        if let Some((existing, ticket_id)) = state.external_messages.get(&external_key) {
            return if existing.content_sha256 == request.identity.content_sha256 {
                let ticket = state
                    .tickets
                    .get(&(existing.project_id.clone(), *ticket_id))
                    .cloned()
                    .ok_or_else(|| {
                        TicketStoreError::Infrastructure(
                            "external message authoritative ticket is missing".into(),
                        )
                    })?;
                Ok(ExternalMessageIngestResult {
                    ticket,
                    repeated: true,
                })
            } else {
                Err(TicketStoreError::ExternalIdentityConflict)
            };
        }
        let key = (request.identity.project_id.clone(), request.ticket_id);
        let mut ticket = state
            .tickets
            .get(&key)
            .cloned()
            .ok_or(TicketStoreError::NotFound(request.ticket_id))?;
        if ticket.revision != request.expected_revision {
            return Err(TicketStoreError::StaleRevision {
                expected: request.expected_revision,
                actual: ticket.revision,
            });
        }
        ticket.reply_as_requester(request.body, request.now)?;
        let intent = TicketActivityIntent::new(
            ticket.project_id.clone(),
            ticket.id,
            "ticketing.external_message_ingested",
            request.correlation_id,
            serde_json::json!({ "ticket_id": ticket.id }),
            request.now,
        );
        state
            .external_messages
            .insert(external_key, (request.identity, request.ticket_id));
        state.tickets.insert(key, ticket.clone());
        state.activity_intents.push(intent);
        drop(state);
        Ok(ExternalMessageIngestResult {
            ticket,
            repeated: false,
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum TicketStoreError {
    #[error(transparent)]
    Validation(#[from] TicketValidationError),
    #[error("ticket was not found: {0}")]
    NotFound(TicketId),
    #[error("ticket already exists: {0}")]
    DuplicateTicket(TicketId),
    #[error("ticket display reference already exists in the project")]
    DuplicateDisplayReference,
    #[error("ticket revision is stale: expected {expected}, actual {actual}")]
    StaleRevision { expected: u64, actual: u64 },
    #[error("ticket list fetch limit must be between 1 and 201")]
    InvalidListLimit,
    #[error("ticket list cursor must contain both timestamp and ticket ID")]
    InvalidListCursor,
    #[error("support handoff already exists")]
    DuplicateHandoff,
    #[error("support handoff is unknown")]
    UnknownHandoff,
    #[error("support handoff expired")]
    ExpiredHandoff,
    #[error("support handoff belongs to a different project")]
    WrongHandoffProject,
    #[error("support handoff belongs to a different portal")]
    WrongHandoffPortal,
    #[error("support handoff was already consumed")]
    HandoffAlreadyConsumed,
    #[error("external provider identity was reused with different content")]
    ExternalIdentityConflict,
    #[error("ticketing storage failed: {0}")]
    Infrastructure(String),
    #[error("a ticketing mutation may carry at most 8 job records")]
    InvalidJobRecords,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{TicketChannel, TicketPriority};
    use chrono::TimeDelta;
    use minco_interaction::{
        SupportContext, SupportLocationPolicy, SupportSurface, issue_support_handoff,
    };

    fn issue(now: DateTime<Utc>) -> (SupportHandoff, SupportHandoffToken) {
        let policy = SupportLocationPolicy {
            portal_origin: "https://support.example.test".into(),
            allowed_return_paths: BTreeMap::from([(
                "https://app.example.test".into(),
                vec!["/orders".into()],
            )]),
        };
        let (handoff, grant) = issue_support_handoff(
            "project-a",
            "user-1",
            vec!["ticketing.create".into()],
            SupportSurface::Widget,
            SupportContext {
                page_url: "https://app.example.test/orders/1".into(),
                ..SupportContext::default()
            },
            "https://app.example.test/orders/1",
            Uuid::now_v7(),
            &policy,
            now,
            TimeDelta::minutes(5),
        )
        .unwrap();
        (handoff, grant.token)
    }

    fn consume(token: SupportHandoffToken, now: DateTime<Utc>) -> ConsumeHandoffRequest {
        ConsumeHandoffRequest::new(
            token,
            "project-a",
            "https://support.example.test",
            TicketFromHandoffInput {
                subject: "Help".into(),
                description: "Broken".into(),
                channel: TicketChannel::Portal,
                priority: TicketPriority::Normal,
            },
            now,
        )
        .unwrap()
    }

    fn ingress(
        ticket_id: TicketId,
        content_sha256: &str,
        now: DateTime<Utc>,
    ) -> IngestExternalMessageRequest {
        IngestExternalMessageRequest {
            identity: ExternalMessageIdentity {
                project_id: "project-a".into(),
                provider: "example-mail".into(),
                mailbox_scope: "support@example.test".into(),
                external_id: "message-1".into(),
                content_sha256: content_sha256.into(),
                raw_message_object_key: Some("mail/project-a/message-1".into()),
                internet_message_id: Some("<message-1@example.test>".into()),
                in_reply_to: None,
                references: Vec::new(),
            },
            ticket_id,
            body: "External reply".into(),
            expected_revision: 0,
            correlation_id: Uuid::now_v7(),
            now,
        }
    }

    #[tokio::test]
    async fn handoff_is_atomic_idempotent_project_scoped_and_rolls_back() {
        let store = Arc::new(MemoryTicketingStore::default());
        let now = Utc::now();
        let (handoff, token) = issue(now);
        let digest = handoff.digest.clone();
        store.insert_handoff(handoff).await.unwrap();
        store.fail_next_handoff_commit().await;
        assert!(matches!(
            store
                .consume_and_create_ticket(consume(token.clone(), now))
                .await,
            Err(TicketStoreError::Infrastructure(_))
        ));
        assert_eq!(store.ticket_count().await, 0);
        assert!(!store.handoff_consumed(&digest).await);

        let first = store
            .consume_and_create_ticket(consume(token.clone(), now))
            .await
            .unwrap();
        let repeated = store
            .consume_and_create_ticket(consume(token, now + TimeDelta::minutes(10)))
            .await
            .unwrap();
        assert!(!first.repeated);
        assert!(repeated.repeated);
        assert_eq!(first.result, repeated.result);
        assert_eq!(store.ticket_count().await, 1);
        assert_eq!(store.activity_intents().await.len(), 1);
    }

    #[tokio::test]
    async fn concurrent_double_consume_creates_one_authoritative_ticket() {
        let store = Arc::new(MemoryTicketingStore::default());
        let now = Utc::now();
        let (handoff, token) = issue(now);
        store.insert_handoff(handoff).await.unwrap();
        let left = {
            let store = store.clone();
            let token = token.clone();
            tokio::spawn(async move { store.consume_and_create_ticket(consume(token, now)).await })
        };
        let right = {
            let store = store.clone();
            tokio::spawn(async move { store.consume_and_create_ticket(consume(token, now)).await })
        };
        let (left, right) = tokio::join!(left, right);
        assert!(left.unwrap().is_ok());
        assert!(right.unwrap().is_ok());
        assert_eq!(store.ticket_count().await, 1);
    }

    #[tokio::test]
    async fn invalid_handoffs_fail_closed_and_different_replay_is_rejected() {
        let now = Utc::now();

        let store = MemoryTicketingStore::default();
        let (handoff, token) = issue(now);
        store.insert_handoff(handoff).await.unwrap();
        assert!(matches!(
            store
                .consume_and_create_ticket(consume(token, now + TimeDelta::minutes(6)))
                .await,
            Err(TicketStoreError::ExpiredHandoff)
        ));

        let store = MemoryTicketingStore::default();
        let (handoff, token) = issue(now);
        store.insert_handoff(handoff).await.unwrap();
        let mut wrong_project = consume(token, now);
        wrong_project.project_id = "project-b".into();
        assert!(matches!(
            store.consume_and_create_ticket(wrong_project).await,
            Err(TicketStoreError::WrongHandoffProject)
        ));

        let store = MemoryTicketingStore::default();
        let (handoff, token) = issue(now);
        store.insert_handoff(handoff).await.unwrap();
        let mut wrong_portal = consume(token, now);
        wrong_portal.portal_origin = "https://other.example.test".into();
        assert!(matches!(
            store.consume_and_create_ticket(wrong_portal).await,
            Err(TicketStoreError::WrongHandoffPortal)
        ));

        let store = MemoryTicketingStore::default();
        assert!(matches!(
            store
                .consume_and_create_ticket(consume(SupportHandoffToken::generate(), now))
                .await,
            Err(TicketStoreError::UnknownHandoff)
        ));

        let store = MemoryTicketingStore::default();
        let (handoff, token) = issue(now);
        store.insert_handoff(handoff).await.unwrap();
        store
            .consume_and_create_ticket(consume(token.clone(), now))
            .await
            .unwrap();
        let mut different = consume(token, now);
        different.input.description = "A different request".into();
        different.request_fingerprint = "different-fingerprint".into();
        assert!(matches!(
            store.consume_and_create_ticket(different).await,
            Err(TicketStoreError::HandoffAlreadyConsumed)
        ));
    }

    #[tokio::test]
    async fn external_identity_replay_returns_authoritative_result_and_conflict_fails() {
        let store = MemoryTicketingStore::default();
        let now = Utc::now();
        let ticket = Ticket::create(
            CreateTicketInput {
                project_id: "project-a".into(),
                subject: "Help".into(),
                description: "Broken".into(),
                requester: TicketRequester {
                    subject: "user".into(),
                    display_name: None,
                    email: None,
                },
                channel: TicketChannel::Api,
                priority: TicketPriority::Normal,
                resource_references: Vec::new(),
            },
            "TKT-EXTERNAL",
            now,
        )
        .unwrap();
        let created = TicketActivityIntent::new(
            "project-a",
            ticket.id,
            "created",
            Uuid::now_v7(),
            serde_json::json!({}),
            now,
        );
        store.create(ticket.clone(), created).await.unwrap();

        let digest = "a".repeat(64);
        let first = store
            .ingest_external_message(ingress(ticket.id, &digest, now))
            .await
            .unwrap();
        let repeated = store
            .ingest_external_message(ingress(ticket.id, &digest, now))
            .await
            .unwrap();
        assert!(!first.repeated);
        assert!(repeated.repeated);
        assert_eq!(first.ticket, repeated.ticket);
        assert_eq!(first.ticket.revision, 1);
        assert_eq!(store.activity_intents().await.len(), 2);

        assert!(matches!(
            store
                .ingest_external_message(ingress(ticket.id, &"b".repeat(64), now))
                .await,
            Err(TicketStoreError::ExternalIdentityConflict)
        ));
        assert_eq!(
            store.get("project-a", ticket.id).await.unwrap(),
            Some(first.ticket)
        );
    }

    #[tokio::test]
    async fn stale_revision_and_project_isolation_fail_closed() {
        let store = MemoryTicketingStore::default();
        let now = Utc::now();
        let ticket = Ticket::create(
            CreateTicketInput {
                project_id: "project-a".into(),
                subject: "Help".into(),
                description: "Broken".into(),
                requester: TicketRequester {
                    subject: "user".into(),
                    display_name: None,
                    email: None,
                },
                channel: TicketChannel::Api,
                priority: TicketPriority::Normal,
                resource_references: Vec::new(),
            },
            "TKT-1",
            now,
        )
        .unwrap();
        let intent = TicketActivityIntent::new(
            "project-a",
            ticket.id,
            "created",
            Uuid::now_v7(),
            serde_json::json!({}),
            now,
        );
        store.create(ticket.clone(), intent).await.unwrap();
        assert!(store.get("project-b", ticket.id).await.unwrap().is_none());
        let mut changed = ticket.clone();
        changed.change_priority(TicketPriority::High, now);
        let intent = TicketActivityIntent::new(
            "project-a",
            changed.id,
            "changed",
            Uuid::now_v7(),
            serde_json::json!({}),
            now,
        );
        assert!(matches!(
            store.save(changed, 1, intent).await,
            Err(TicketStoreError::StaleRevision { .. })
        ));
    }

    fn ticket_at(instant: DateTime<Utc>, reference: &str) -> Ticket {
        let mut ticket = Ticket::create(
            CreateTicketInput {
                project_id: "project-a".into(),
                subject: format!("Help {reference}"),
                description: "Broken".into(),
                requester: TicketRequester {
                    subject: "user-1".into(),
                    display_name: None,
                    email: None,
                },
                channel: TicketChannel::Api,
                priority: TicketPriority::Normal,
                resource_references: Vec::new(),
            },
            reference,
            instant,
        )
        .unwrap();
        ticket.updated_at = instant;
        ticket
    }

    async fn seeded_summary_store() -> MemoryTicketingStore {
        let store = MemoryTicketingStore::default();
        let base = DateTime::from_timestamp(1_777_000_000, 0).unwrap();
        let tied = base + chrono::TimeDelta::seconds(10);
        let mut tickets: Vec<Ticket> = Vec::new();
        for (index, (instant, reference)) in [
            (base, "TKT-OLD"),
            (base + chrono::TimeDelta::seconds(5), "TKT-MID"),
            (tied, "TKT-TIE-A"),
            (tied, "TKT-TIE-B"),
        ]
        .into_iter()
        .enumerate()
        {
            let mut ticket = ticket_at(instant, reference);
            ticket.id = TicketId(
                Uuid::parse_str(&format!("00000000-0000-0000-0000-00000000000{index}")).unwrap(),
            );
            tickets.push(ticket);
        }
        for ticket in tickets {
            let intent = TicketActivityIntent::new(
                "project-a",
                ticket.id,
                "created",
                Uuid::now_v7(),
                serde_json::json!({}),
                ticket.created_at,
            );
            store.create(ticket, intent).await.unwrap();
        }
        store
    }

    fn summary_filter(limit: usize) -> TicketSummaryFilter {
        TicketSummaryFilter {
            project_id: "project-a".into(),
            limit,
            ..TicketSummaryFilter::default()
        }
    }

    #[tokio::test]
    async fn summaries_are_newest_first_with_id_tiebreak_and_cursor_excludes_seen() {
        let store = seeded_summary_store().await;
        let first_page = store.list_summaries(summary_filter(2)).await.unwrap();
        assert_eq!(
            first_page
                .iter()
                .map(|summary| summary.display_reference.clone())
                .collect::<Vec<_>>(),
            vec!["TKT-TIE-B", "TKT-TIE-A"]
        );
        let cursor = (first_page[1].updated_at, first_page[1].id);
        let mut filter = summary_filter(10);
        filter.before_updated_at = Some(cursor.0);
        filter.before_id = Some(cursor.1);
        let rest = store.list_summaries(filter).await.unwrap();
        assert_eq!(
            rest.iter()
                .map(|summary| summary.display_reference.clone())
                .collect::<Vec<_>>(),
            vec!["TKT-MID", "TKT-OLD"]
        );
    }

    #[tokio::test]
    async fn summary_store_rejects_invalid_limit_and_half_cursor() {
        let store = seeded_summary_store().await;
        assert!(matches!(
            store.list_summaries(summary_filter(0)).await,
            Err(TicketStoreError::InvalidListLimit)
        ));
        assert!(matches!(
            store.list_summaries(summary_filter(202)).await,
            Err(TicketStoreError::InvalidListLimit)
        ));
        let mut filter = summary_filter(10);
        filter.before_id = Some(TicketId::new());
        assert!(matches!(
            store.list_summaries(filter).await,
            Err(TicketStoreError::InvalidListCursor)
        ));
    }

    #[tokio::test]
    async fn summary_excludes_private_payload_and_project_is_isolated() {
        let store = seeded_summary_store().await;
        let mut ticket = ticket_at(Utc::now(), "TKT-PRIVATE");
        ticket
            .add_internal_note("agent", "private note body", Utc::now())
            .unwrap();
        ticket.description = "very long private description".into();
        let intent = TicketActivityIntent::new(
            "project-a",
            ticket.id,
            "created",
            Uuid::now_v7(),
            serde_json::json!({}),
            ticket.created_at,
        );
        store.create(ticket, intent).await.unwrap();
        let summaries = store.list_summaries(summary_filter(10)).await.unwrap();
        let encoded = serde_json::to_string(&summaries).unwrap();
        assert!(!encoded.contains("private note body"));
        assert!(!encoded.contains("very long private description"));

        let mut other = summary_filter(10);
        other.project_id = "project-b".into();
        assert!(store.list_summaries(other).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn session_handoff_identity_is_one_time_replayable_and_fail_closed() {
        let store = MemoryTicketingStore::default();
        let now = Utc::now();
        let (handoff, token) = issue(now);
        store.insert_handoff(handoff).await.unwrap();

        let request = |fingerprint: &str, portal: &str| ConsumeSessionRequest {
            token: token.clone(),
            project_id: "project-a".into(),
            portal_origin: portal.into(),
            request_fingerprint: fingerprint.into(),
            now,
        };

        let (first, repeated_flag) = store
            .consume_handoff_identity(request("fingerprint-1", "https://support.example.test"))
            .await
            .unwrap();
        assert!(!repeated_flag);
        assert_eq!(first.requester_subject, "user-1");
        assert_eq!(first.requester_permissions, vec!["ticketing.create"]);

        let (repeated, replayed) = store
            .consume_handoff_identity(request("fingerprint-1", "https://support.example.test"))
            .await
            .unwrap();
        assert!(replayed);
        assert_eq!(repeated, first);

        assert!(matches!(
            store
                .consume_handoff_identity(request("fingerprint-2", "https://support.example.test"))
                .await,
            Err(TicketStoreError::HandoffAlreadyConsumed)
        ));
        assert!(matches!(
            store
                .consume_handoff_identity(request("fingerprint-1", "https://other.example.test"))
                .await,
            Err(TicketStoreError::WrongHandoffPortal)
        ));

        // Ticket creation consumption is independent of session consumption:
        // after a session exchange the handoff can still create its ticket
        // exactly once, and the identical ticket exchange replays.
        let created = store
            .consume_and_create_ticket(consume(token.clone(), now))
            .await
            .unwrap();
        assert!(!created.repeated);
        let replayed = store
            .consume_and_create_ticket(consume(token, now))
            .await
            .unwrap();
        assert!(replayed.repeated);
    }
}
