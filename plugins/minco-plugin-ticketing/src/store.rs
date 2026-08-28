use crate::{
    AgentMacro, AutomationProposal, Clarification, CreateTicketInput, MAX_TICKET_LIST_FETCH_LIMIT,
    Ticket, TicketFromHandoffInput, TicketId, TicketMessageId, TicketRequester, TicketStatus,
    TicketSummary, TicketValidationError,
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

/// Fact kinds recorded for one outbound public reply (ADR-0063).
///
/// `Accepted` is provider acceptance only — it is never a delivery claim;
/// delivery is disproven or indicated by `Feedback` rows, never asserted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutboundEvidenceKind {
    Accepted,
    Ambiguous,
    PermanentFailure,
    Feedback,
}

/// Provider feedback about a previously accepted outbound message
/// (bounce, complaint or delay evidence).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryFeedbackKind {
    Bounce,
    Complaint,
    Delay,
}

/// One append-only outbound delivery-evidence row. Reconciliation reads
/// these rows existentially (an `Accepted` row suppresses any resend);
/// rows are never rewritten.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutboundDeliveryEvidence {
    pub project_id: String,
    pub ticket_id: TicketId,
    pub message_id: TicketMessageId,
    pub kind: OutboundEvidenceKind,
    /// Bounded transport/provider identifier (at most 100 characters).
    pub provider: String,
    /// Provider's message identifier when known; empty otherwise
    /// (at most 500 characters).
    pub provider_message_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feedback: Option<DeliveryFeedbackKind>,
    /// Mail error kind name recorded for `ambiguous` and
    /// `permanent_failure` rows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_kind: Option<String>,
    pub recorded_at: DateTime<Utc>,
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
    /// Only tickets with no assignee (curated `new-unassigned` view).
    pub unassigned: bool,
    /// Bounded substring search over subject, display reference and
    /// description (ADR-0069); None disables the search filter.
    pub query: Option<String>,
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
    /// External ingress is append-only and idempotent by external identity:
    /// the store reloads the authoritative ticket inside the transaction, so
    /// no caller-supplied revision is required or honored (review finding 7 —
    /// a frozen `expected_revision` in an immutable job payload can never
    /// converge on retry).
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
/// A recoverable idempotency receipt committed in the same transaction
/// as the mutation it describes (exact-head review R2): the serialized
/// authoritative response is rebuilt from this row when a lost response
/// is retried after the shared idempotency lease has gone stale.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationReceipt {
    pub idempotency_key: String,
    pub fingerprint: String,
    pub response_json: String,
    pub created_at: DateTime<Utc>,
}

/// One session-exchange replay grant (exact-head review R3): holds only
/// non-secret rotation material — the active session ID and the
/// attributes needed to mint a replacement.
///
/// A replay mints a fresh session from these attributes, revokes the
/// recorded session and updates this row; no bearer token is ever
/// persisted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionExchangeGrant {
    pub exchange_key: String,
    pub session_id: minco_plugin_sessions::SessionId,
    pub subject: String,
    pub project_id: String,
    pub permissions: Vec<String>,
    pub portal_origin: String,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

/// One atomic pool-mode assignment request (exact-head review R7).
///
/// The store verifies the revision, selects the assignee (advancing the
/// round-robin cursor or evaluating the workload under the same lock),
/// updates the ticket and appends the activity intent in one
/// transaction — a stale revision can never consume a cursor slot and
/// concurrent least-workload requests can never observe the same counts.
#[derive(Debug, Clone)]
pub struct AtomicAssignmentRequest {
    pub project_id: String,
    pub ticket_id: TicketId,
    pub mode: crate::AssignmentMode,
    pub pool: Vec<String>,
    pub expected_revision: u64,
    pub correlation_id: Uuid,
    pub now: DateTime<Utc>,
}

/// One durable outbound send intent (exact-head review R4): committed
/// before provider contact and resolved by stable logical identity, so
/// ambiguous transport outcomes recover without duplicate sends.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SendIntent {
    pub logical_send_id: String,
    pub project_id: String,
    pub ticket_id: TicketId,
    pub message_id: TicketMessageId,
    pub state: SendIntentState,
    pub provider_message_id: Option<String>,
    pub updated_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SendIntentState {
    /// Reconciled authoritative no-send: one identity-stable resend may
    /// proceed.
    PendingSend,
    /// Committed immediately before provider contact; an outcome is not
    /// yet known.
    Sending,
    /// Provider acceptance recorded with the provider's message identity.
    Sent,
    /// Ambiguous outcome: resolve by reconciliation, never a blind resend.
    RecoveryRequired,
    /// Reconciled permanent failure; the intent is terminal.
    FailedNoSend,
}

#[derive(Debug, Clone)]
pub struct AppendTicketMessageRequest {
    pub project_id: String,
    pub ticket_id: TicketId,
    /// Optional idempotency receipt committed atomically with this
    /// append (exact-head review R2).
    pub receipt: Option<OperationReceipt>,
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

    /// Oldest-first unpublished activity intents for one project
    /// (ADR-0056), bounded by `limit`.
    async fn pending_activity_intents(
        &self,
        project_id: &str,
        limit: usize,
    ) -> Result<Vec<TicketActivityIntent>, TicketStoreError>;

    /// Marks one intent published; `false` when it was already published
    /// or is unknown.
    async fn mark_activity_published(
        &self,
        intent_id: Uuid,
        at: DateTime<Utc>,
    ) -> Result<bool, TicketStoreError>;

    /// Oldest-first audit-undelivered activity intents for one project
    /// (exact-head review R5), bounded by `limit`.
    async fn pending_audit_intents(
        &self,
        project_id: &str,
        limit: usize,
    ) -> Result<Vec<TicketActivityIntent>, TicketStoreError>;

    /// Marks one intent's audit record delivered; `false` when it was
    /// already delivered or is unknown.
    async fn mark_audit_published(
        &self,
        intent_id: Uuid,
        at: DateTime<Utc>,
    ) -> Result<bool, TicketStoreError>;

    /// Resolves a ticket (and its current revision) from a previously
    /// ingested external message's `internet_message_id` (ADR-0058).
    /// Subject heuristics are never used.
    async fn find_ticket_by_message_identity(
        &self,
        project_id: &str,
        provider: &str,
        internet_message_id: &str,
    ) -> Result<Option<(TicketId, u64)>, TicketStoreError>;

    /// Registers an outbound message's threading identity so email replies
    /// that reference it resolve to the originating ticket (review
    /// finding 8). Idempotent: re-registering the same identity is a no-op.
    async fn register_outbound_identity(
        &self,
        project_id: &str,
        identity: ExternalMessageIdentity,
        ticket_id: TicketId,
    ) -> Result<(), TicketStoreError>;

    /// Reads one operation receipt by idempotency key (exact-head
    /// review R2): the recovery path replays the authoritative response
    /// instead of re-executing the mutation.
    async fn operation_receipt(
        &self,
        idempotency_key: &str,
    ) -> Result<Option<OperationReceipt>, TicketStoreError>;

    /// Records or replaces the replay grant for one session exchange
    /// (exact-head review R3); removing it kills future replays.
    async fn put_session_exchange_grant(
        &self,
        grant: SessionExchangeGrant,
    ) -> Result<(), TicketStoreError>;

    /// Atomically assigns one ticket in a pool mode (exact-head review
    /// R7): revision check, selection, update and intent append commit
    /// together; a stale revision rejects without consuming a slot.
    async fn assign_ticket_atomically(
        &self,
        request: AtomicAssignmentRequest,
    ) -> Result<Ticket, TicketStoreError>;

    /// Claims or advances one send intent atomically (exact-head review
    /// R4). `Ok(None)` means the intent was newly claimed for sending;
    /// `Ok(Some(current))` returns the existing state so the caller can
    /// decide (sent -> done, recovery -> fail closed, pending -> resend).
    async fn claim_send_intent(
        &self,
        intent: SendIntent,
    ) -> Result<Option<SendIntent>, TicketStoreError>;

    /// Resolves one send intent by logical identity.
    async fn send_intent(
        &self,
        logical_send_id: &str,
    ) -> Result<Option<SendIntent>, TicketStoreError>;

    /// Records the outcome for one send intent: provider acceptance with
    /// its message identity, `recovery_required`, or a reconciled
    /// no-send.
    async fn resolve_send_intent(
        &self,
        logical_send_id: &str,
        state: SendIntentState,
        provider_message_id: Option<String>,
        now: DateTime<Utc>,
    ) -> Result<bool, TicketStoreError>;

    async fn session_exchange_grant(
        &self,
        exchange_key: &str,
    ) -> Result<Option<SessionExchangeGrant>, TicketStoreError>;

    async fn remove_session_exchange_grant(
        &self,
        exchange_key: &str,
    ) -> Result<bool, TicketStoreError>;

    /// Verified first-contact intake (review finding 6): create one
    /// ticket from an inbound email and register its external identity
    /// in the same transaction. Replaying the same identity returns the
    /// originally created ticket; a different digest for a known
    /// identity conflicts.
    async fn create_ticket_from_external(
        &self,
        ticket: Ticket,
        intent: TicketActivityIntent,
        identity: ExternalMessageIdentity,
    ) -> Result<ExternalMessageIngestResult, TicketStoreError>;

    /// Appends one append-only outbound delivery-evidence row
    /// (ADR-0063).
    async fn append_outbound_evidence(
        &self,
        evidence: OutboundDeliveryEvidence,
    ) -> Result<(), TicketStoreError>;

    /// Retention erasure (ADR-0073): deletes resolved-or-closed tickets
    /// whose last update precedes the cutoff, cascading every child row;
    /// returns how many tickets were erased. Bounded by `limit`.
    async fn erase_tickets_resolved_before(
        &self,
        project_id: &str,
        cutoff: DateTime<Utc>,
        limit: usize,
    ) -> Result<usize, TicketStoreError>;

    /// Stores one clarification (ADR-0071).
    async fn insert_clarification(
        &self,
        project_id: &str,
        clarification: Clarification,
    ) -> Result<(), TicketStoreError>;

    /// Clarifications for one ticket, oldest first.
    async fn list_clarifications(
        &self,
        project_id: &str,
        ticket_id: TicketId,
    ) -> Result<Vec<Clarification>, TicketStoreError>;

    /// Loads one clarification for a decision or reply.
    async fn get_clarification(
        &self,
        project_id: &str,
        id: Uuid,
    ) -> Result<Option<Clarification>, TicketStoreError>;

    /// Persists a state transition (send/reply/withdraw).
    async fn update_clarification(
        &self,
        project_id: &str,
        clarification: Clarification,
    ) -> Result<(), TicketStoreError>;

    /// Stores one automation proposal (ADR-0070).
    async fn insert_automation_proposal(
        &self,
        project_id: &str,
        proposal: AutomationProposal,
    ) -> Result<(), TicketStoreError>;

    /// Automation proposals for one ticket, oldest first.
    async fn list_automation_proposals(
        &self,
        project_id: &str,
        ticket_id: TicketId,
    ) -> Result<Vec<AutomationProposal>, TicketStoreError>;

    /// Loads one proposal for a decide decision.
    async fn get_automation_proposal(
        &self,
        project_id: &str,
        id: Uuid,
    ) -> Result<Option<AutomationProposal>, TicketStoreError>;

    /// Persists a decided proposal (state and `decided_at`).
    async fn update_automation_proposal(
        &self,
        project_id: &str,
        proposal: AutomationProposal,
    ) -> Result<(), TicketStoreError>;

    /// Atomically advances the project's round-robin cursor and returns
    /// the index to use for a pool of `pool_len` members (ADR-0068).
    async fn advance_assignment_cursor(
        &self,
        project_id: &str,
        pool_len: usize,
    ) -> Result<usize, TicketStoreError>;

    /// Open (not resolved, not closed) ticket counts per requested
    /// subject (ADR-0068); missing subjects report zero.
    async fn assignee_workload(
        &self,
        project_id: &str,
        subjects: &[String],
    ) -> Result<BTreeMap<String, u64>, TicketStoreError>;

    /// Records that one agent viewed one ticket (advisory collision
    /// indication, ADR-0067); upserts the viewer's timestamp.
    async fn record_ticket_view(
        &self,
        project_id: &str,
        ticket_id: TicketId,
        subject: &str,
        at: DateTime<Utc>,
    ) -> Result<(), TicketStoreError>;

    /// Other agents who viewed the ticket inside the window, newest
    /// first, bounded by `limit`, excluding `subject`.
    async fn recent_ticket_viewers(
        &self,
        project_id: &str,
        ticket_id: TicketId,
        excluding: &str,
        within: chrono::TimeDelta,
        now: DateTime<Utc>,
        limit: usize,
    ) -> Result<Vec<String>, TicketStoreError>;

    /// All shared saved replies, ordered by title.
    async fn list_macros(&self, project_id: &str) -> Result<Vec<AgentMacro>, TicketStoreError>;

    /// Inserts a new macro into the project's shared library; a
    /// duplicate title in the project is refused.
    async fn insert_macro(
        &self,
        project_id: &str,
        macro_: AgentMacro,
    ) -> Result<(), TicketStoreError>;

    /// Replaces one macro under the expected revision; stale revisions
    /// fail with [`TicketStoreError::StaleRevision`].
    #[allow(clippy::significant_drop_tightening)]
    async fn update_macro(
        &self,
        project_id: &str,
        id: Uuid,
        expected_revision: u64,
        title: &str,
        body: &str,
        now: DateTime<Utc>,
    ) -> Result<AgentMacro, TicketStoreError>;

    /// Chronological outbound delivery evidence for one ticket message.
    async fn outbound_evidence(
        &self,
        project_id: &str,
        ticket_id: TicketId,
        message_id: TicketMessageId,
    ) -> Result<Vec<OutboundDeliveryEvidence>, TicketStoreError>;

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

    pub async fn pending_activity_intents(
        &self,
        project_id: &str,
        limit: usize,
    ) -> Result<Vec<TicketActivityIntent>, TicketStoreError> {
        self.0.pending_activity_intents(project_id, limit).await
    }

    pub async fn pending_audit_intents(
        &self,
        project_id: &str,
        limit: usize,
    ) -> Result<Vec<TicketActivityIntent>, TicketStoreError> {
        self.0.pending_audit_intents(project_id, limit).await
    }

    pub async fn mark_audit_published(
        &self,
        intent_id: Uuid,
        at: DateTime<Utc>,
    ) -> Result<bool, TicketStoreError> {
        self.0.mark_audit_published(intent_id, at).await
    }

    pub async fn mark_activity_published(
        &self,
        intent_id: Uuid,
        at: DateTime<Utc>,
    ) -> Result<bool, TicketStoreError> {
        self.0.mark_activity_published(intent_id, at).await
    }

    pub async fn find_ticket_by_message_identity(
        &self,
        project_id: &str,
        provider: &str,
        internet_message_id: &str,
    ) -> Result<Option<(TicketId, u64)>, TicketStoreError> {
        self.0
            .find_ticket_by_message_identity(project_id, provider, internet_message_id)
            .await
    }

    pub async fn register_outbound_identity(
        &self,
        project_id: &str,
        identity: ExternalMessageIdentity,
        ticket_id: TicketId,
    ) -> Result<(), TicketStoreError> {
        self.0
            .register_outbound_identity(project_id, identity, ticket_id)
            .await
    }

    pub async fn create_ticket_from_external(
        &self,
        ticket: Ticket,
        intent: TicketActivityIntent,
        identity: ExternalMessageIdentity,
    ) -> Result<ExternalMessageIngestResult, TicketStoreError> {
        self.0
            .create_ticket_from_external(ticket, intent, identity)
            .await
    }

    pub async fn operation_receipt(
        &self,
        idempotency_key: &str,
    ) -> Result<Option<OperationReceipt>, TicketStoreError> {
        self.0.operation_receipt(idempotency_key).await
    }

    pub async fn assign_ticket_atomically(
        &self,
        request: AtomicAssignmentRequest,
    ) -> Result<Ticket, TicketStoreError> {
        self.0.assign_ticket_atomically(request).await
    }

    pub async fn claim_send_intent(
        &self,
        intent: SendIntent,
    ) -> Result<Option<SendIntent>, TicketStoreError> {
        self.0.claim_send_intent(intent).await
    }

    pub async fn send_intent(
        &self,
        logical_send_id: &str,
    ) -> Result<Option<SendIntent>, TicketStoreError> {
        self.0.send_intent(logical_send_id).await
    }

    pub async fn resolve_send_intent(
        &self,
        logical_send_id: &str,
        state: SendIntentState,
        provider_message_id: Option<String>,
        now: DateTime<Utc>,
    ) -> Result<bool, TicketStoreError> {
        self.0
            .resolve_send_intent(logical_send_id, state, provider_message_id, now)
            .await
    }

    pub async fn put_session_exchange_grant(
        &self,
        grant: SessionExchangeGrant,
    ) -> Result<(), TicketStoreError> {
        self.0.put_session_exchange_grant(grant).await
    }

    pub async fn session_exchange_grant(
        &self,
        exchange_key: &str,
    ) -> Result<Option<SessionExchangeGrant>, TicketStoreError> {
        self.0.session_exchange_grant(exchange_key).await
    }

    pub async fn remove_session_exchange_grant(
        &self,
        exchange_key: &str,
    ) -> Result<bool, TicketStoreError> {
        self.0.remove_session_exchange_grant(exchange_key).await
    }

    pub async fn erase_tickets_resolved_before(
        &self,
        project_id: &str,
        cutoff: DateTime<Utc>,
        limit: usize,
    ) -> Result<usize, TicketStoreError> {
        self.0
            .erase_tickets_resolved_before(project_id, cutoff, limit)
            .await
    }

    pub async fn insert_clarification(
        &self,
        project_id: &str,
        clarification: Clarification,
    ) -> Result<(), TicketStoreError> {
        self.0.insert_clarification(project_id, clarification).await
    }

    pub async fn list_clarifications(
        &self,
        project_id: &str,
        ticket_id: TicketId,
    ) -> Result<Vec<Clarification>, TicketStoreError> {
        self.0.list_clarifications(project_id, ticket_id).await
    }

    pub async fn get_clarification(
        &self,
        project_id: &str,
        id: Uuid,
    ) -> Result<Option<Clarification>, TicketStoreError> {
        self.0.get_clarification(project_id, id).await
    }

    pub async fn update_clarification(
        &self,
        project_id: &str,
        clarification: Clarification,
    ) -> Result<(), TicketStoreError> {
        self.0.update_clarification(project_id, clarification).await
    }

    pub async fn insert_automation_proposal(
        &self,
        project_id: &str,
        proposal: AutomationProposal,
    ) -> Result<(), TicketStoreError> {
        self.0
            .insert_automation_proposal(project_id, proposal)
            .await
    }

    pub async fn list_automation_proposals(
        &self,
        project_id: &str,
        ticket_id: TicketId,
    ) -> Result<Vec<AutomationProposal>, TicketStoreError> {
        self.0
            .list_automation_proposals(project_id, ticket_id)
            .await
    }

    pub async fn get_automation_proposal(
        &self,
        project_id: &str,
        id: Uuid,
    ) -> Result<Option<AutomationProposal>, TicketStoreError> {
        self.0.get_automation_proposal(project_id, id).await
    }

    pub async fn update_automation_proposal(
        &self,
        project_id: &str,
        proposal: AutomationProposal,
    ) -> Result<(), TicketStoreError> {
        self.0
            .update_automation_proposal(project_id, proposal)
            .await
    }

    pub async fn advance_assignment_cursor(
        &self,
        project_id: &str,
        pool_len: usize,
    ) -> Result<usize, TicketStoreError> {
        self.0.advance_assignment_cursor(project_id, pool_len).await
    }

    pub async fn assignee_workload(
        &self,
        project_id: &str,
        subjects: &[String],
    ) -> Result<BTreeMap<String, u64>, TicketStoreError> {
        self.0.assignee_workload(project_id, subjects).await
    }

    pub async fn record_ticket_view(
        &self,
        project_id: &str,
        ticket_id: TicketId,
        subject: &str,
        at: DateTime<Utc>,
    ) -> Result<(), TicketStoreError> {
        self.0
            .record_ticket_view(project_id, ticket_id, subject, at)
            .await
    }

    pub async fn recent_ticket_viewers(
        &self,
        project_id: &str,
        ticket_id: TicketId,
        excluding: &str,
        within: chrono::TimeDelta,
        now: DateTime<Utc>,
        limit: usize,
    ) -> Result<Vec<String>, TicketStoreError> {
        self.0
            .recent_ticket_viewers(project_id, ticket_id, excluding, within, now, limit)
            .await
    }

    pub async fn list_macros(&self, project_id: &str) -> Result<Vec<AgentMacro>, TicketStoreError> {
        self.0.list_macros(project_id).await
    }

    pub async fn insert_macro(
        &self,
        project_id: &str,
        macro_: AgentMacro,
    ) -> Result<(), TicketStoreError> {
        self.0.insert_macro(project_id, macro_).await
    }

    pub async fn update_macro(
        &self,
        project_id: &str,
        id: Uuid,
        expected_revision: u64,
        title: &str,
        body: &str,
        now: DateTime<Utc>,
    ) -> Result<AgentMacro, TicketStoreError> {
        self.0
            .update_macro(project_id, id, expected_revision, title, body, now)
            .await
    }

    pub async fn append_outbound_evidence(
        &self,
        evidence: OutboundDeliveryEvidence,
    ) -> Result<(), TicketStoreError> {
        self.0.append_outbound_evidence(evidence).await
    }

    pub async fn outbound_evidence(
        &self,
        project_id: &str,
        ticket_id: TicketId,
        message_id: TicketMessageId,
    ) -> Result<Vec<OutboundDeliveryEvidence>, TicketStoreError> {
        self.0
            .outbound_evidence(project_id, ticket_id, message_id)
            .await
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
    published_intents: BTreeSet<Uuid>,
    audit_published_intents: BTreeSet<Uuid>,
    outbound_evidence: Vec<OutboundDeliveryEvidence>,
    ticket_views: BTreeMap<(String, TicketId), BTreeMap<String, DateTime<Utc>>>,
    macros: BTreeMap<(String, Uuid), AgentMacro>,
    assignment_cursor: BTreeMap<String, u64>,
    automation_proposals: BTreeMap<(String, Uuid), AutomationProposal>,
    clarifications: BTreeMap<(String, Uuid), Clarification>,
    operation_receipts: BTreeMap<String, OperationReceipt>,
    session_exchange_grants: BTreeMap<String, SessionExchangeGrant>,
    send_intents: BTreeMap<String, SendIntent>,
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

    /// Test seeding for the outbound send-intent state machine.
    pub async fn put_send_intent_for_tests(
        &self,
        logical_send_id: &str,
        ticket_id: TicketId,
        message_id: TicketMessageId,
        state: SendIntentState,
    ) {
        let mut guard = self.state.lock().await;
        guard.send_intents.insert(
            logical_send_id.to_owned(),
            SendIntent {
                logical_send_id: logical_send_id.to_owned(),
                project_id: "project-a".into(),
                ticket_id,
                message_id,
                state,
                provider_message_id: None,
                updated_at: Utc::now(),
                created_at: Utc::now(),
            },
        );
    }

    pub async fn published_intent_ids(&self) -> BTreeSet<Uuid> {
        self.state.lock().await.published_intents.clone()
    }

    /// Every recorded outbound delivery-evidence row (memory inspection
    /// for tests and local runs).
    pub async fn all_outbound_evidence(&self) -> Vec<OutboundDeliveryEvidence> {
        self.state.lock().await.outbound_evidence.clone()
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
    #[allow(clippy::significant_drop_tightening)]
    async fn erase_tickets_resolved_before(
        &self,
        project_id: &str,
        cutoff: DateTime<Utc>,
        limit: usize,
    ) -> Result<usize, TicketStoreError> {
        let mut state = self.state.lock().await;
        let doomed: Vec<(String, TicketId)> = state
            .tickets
            .values()
            .filter(|ticket| {
                ticket.project_id == project_id
                    && matches!(ticket.status, TicketStatus::Resolved | TicketStatus::Closed)
                    && ticket.updated_at < cutoff
            })
            .take(limit)
            .map(|ticket| (ticket.project_id.clone(), ticket.id))
            .collect();
        let erased = doomed.len();
        for key in doomed {
            state.tickets.remove(&key);
            state.ticket_views.remove(&key);
            state
                .automation_proposals
                .retain(|(p, _), proposal| !(p == &key.0 && proposal.ticket_id == key.1));
            state
                .clarifications
                .retain(|(p, _), item| !(p == &key.0 && item.ticket_id == key.1));
        }
        Ok(erased)
    }

    #[allow(clippy::significant_drop_tightening)]
    async fn insert_clarification(
        &self,
        project_id: &str,
        clarification: Clarification,
    ) -> Result<(), TicketStoreError> {
        let mut state = self.state.lock().await;
        state
            .clarifications
            .insert((project_id.to_owned(), clarification.id), clarification);
        Ok(())
    }

    #[allow(clippy::significant_drop_tightening)]
    async fn list_clarifications(
        &self,
        project_id: &str,
        ticket_id: TicketId,
    ) -> Result<Vec<Clarification>, TicketStoreError> {
        let state = self.state.lock().await;
        let mut items = state
            .clarifications
            .iter()
            .filter(|((item_project, _), item)| {
                item_project == project_id && item.ticket_id == ticket_id
            })
            .map(|(_, item)| item.clone())
            .collect::<Vec<_>>();
        items.sort_by_key(|item| item.created_at);
        Ok(items)
    }

    async fn get_clarification(
        &self,
        project_id: &str,
        id: Uuid,
    ) -> Result<Option<Clarification>, TicketStoreError> {
        Ok(self
            .state
            .lock()
            .await
            .clarifications
            .get(&(project_id.to_owned(), id))
            .cloned())
    }

    #[allow(clippy::significant_drop_tightening)]
    async fn update_clarification(
        &self,
        project_id: &str,
        clarification: Clarification,
    ) -> Result<(), TicketStoreError> {
        let mut state = self.state.lock().await;
        let key = (project_id.to_owned(), clarification.id);
        if state.clarifications.contains_key(&key) {
            state.clarifications.insert(key, clarification);
        }
        Ok(())
    }

    #[allow(clippy::significant_drop_tightening)]
    async fn insert_automation_proposal(
        &self,
        project_id: &str,
        proposal: AutomationProposal,
    ) -> Result<(), TicketStoreError> {
        let mut state = self.state.lock().await;
        state
            .automation_proposals
            .insert((project_id.to_owned(), proposal.id), proposal);
        Ok(())
    }

    #[allow(clippy::significant_drop_tightening)]
    async fn list_automation_proposals(
        &self,
        project_id: &str,
        ticket_id: TicketId,
    ) -> Result<Vec<AutomationProposal>, TicketStoreError> {
        let state = self.state.lock().await;
        let mut proposals = state
            .automation_proposals
            .iter()
            .filter(|((proposal_project, _), proposal)| {
                proposal_project == project_id && proposal.ticket_id == ticket_id
            })
            .map(|(_, proposal)| proposal.clone())
            .collect::<Vec<_>>();
        proposals.sort_by_key(|proposal| proposal.created_at);
        Ok(proposals)
    }

    #[allow(clippy::significant_drop_tightening)]
    async fn get_automation_proposal(
        &self,
        project_id: &str,
        id: Uuid,
    ) -> Result<Option<AutomationProposal>, TicketStoreError> {
        Ok(self
            .state
            .lock()
            .await
            .automation_proposals
            .get(&(project_id.to_owned(), id))
            .cloned())
    }

    #[allow(clippy::significant_drop_tightening)]
    async fn update_automation_proposal(
        &self,
        project_id: &str,
        proposal: AutomationProposal,
    ) -> Result<(), TicketStoreError> {
        let mut state = self.state.lock().await;
        let key = (project_id.to_owned(), proposal.id);
        if state.automation_proposals.contains_key(&key) {
            state.automation_proposals.insert(key, proposal);
        }
        Ok(())
    }

    #[allow(clippy::significant_drop_tightening)]
    async fn advance_assignment_cursor(
        &self,
        project_id: &str,
        pool_len: usize,
    ) -> Result<usize, TicketStoreError> {
        if pool_len == 0 {
            return Err(TicketStoreError::Infrastructure(
                "assignment pool is empty".into(),
            ));
        }
        let mut state = self.state.lock().await;
        let next = state
            .assignment_cursor
            .entry(project_id.to_owned())
            .or_insert(0);
        let divisor = u64::try_from(pool_len)
            .map_err(|_| TicketStoreError::Infrastructure("pool too large".into()))?;
        let index = usize::try_from(*next % divisor)
            .map_err(|_| TicketStoreError::Infrastructure("cursor index overflow".into()))?;
        *next = (*next + 1) % divisor;
        Ok(index)
    }

    #[allow(clippy::significant_drop_tightening)]
    async fn assignee_workload(
        &self,
        project_id: &str,
        subjects: &[String],
    ) -> Result<BTreeMap<String, u64>, TicketStoreError> {
        let state = self.state.lock().await;
        let mut workload = subjects
            .iter()
            .map(|subject| (subject.clone(), 0u64))
            .collect::<BTreeMap<_, _>>();
        for ticket in state.tickets.values() {
            if ticket.project_id != project_id
                || !matches!(ticket.status, TicketStatus::Resolved | TicketStatus::Closed)
            {
                continue;
            }
            if let Some(assignee) = &ticket.assignee_subject
                && let Some(count) = workload.get_mut(assignee)
            {
                *count += 1;
            }
        }
        Ok(workload)
    }

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
            .filter(|ticket| match filter.query.as_deref() {
                None => true,
                Some(query) => {
                    let needle = query.to_ascii_lowercase();
                    [
                        ticket.subject.as_str(),
                        ticket.display_reference.as_str(),
                        ticket.description.as_str(),
                    ]
                    .iter()
                    .any(|haystack| haystack.to_ascii_lowercase().contains(&needle))
                }
            })
            .filter(|ticket| filter.statuses.is_empty() || filter.statuses.contains(&ticket.status))
            .filter(|ticket| match filter.query.as_deref() {
                None => true,
                Some(query) => {
                    let needle = query.to_ascii_lowercase();
                    [
                        ticket.subject.as_str(),
                        ticket.display_reference.as_str(),
                        ticket.description.as_str(),
                    ]
                    .iter()
                    .any(|haystack| haystack.to_ascii_lowercase().contains(&needle))
                }
            })
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
        // The idempotency receipt commits with the append itself
        // (exact-head review R2).
        if let Some(receipt) = &request.receipt {
            state
                .operation_receipts
                .insert(receipt.idempotency_key.clone(), receipt.clone());
        }
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
                ticket_type: request.input.ticket_type,
                form_answers: request.input.form_answers.clone(),
                resource_references: resources,
            },
            format!("TKT-{counter:06}"),
            request.now,
        )?
        .with_deadlines(
            request.input.first_response_deadline,
            request.input.resolution_deadline,
        );
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

    async fn pending_activity_intents(
        &self,
        project_id: &str,
        limit: usize,
    ) -> Result<Vec<TicketActivityIntent>, TicketStoreError> {
        let state = self.state.lock().await;
        Ok(state
            .activity_intents
            .iter()
            .filter(|intent| intent.project_id == project_id)
            .filter(|intent| !state.published_intents.contains(&intent.id))
            .take(limit)
            .cloned()
            .collect())
    }

    async fn pending_audit_intents(
        &self,
        project_id: &str,
        limit: usize,
    ) -> Result<Vec<TicketActivityIntent>, TicketStoreError> {
        let state = self.state.lock().await;
        Ok(state
            .activity_intents
            .iter()
            .filter(|intent| intent.project_id == project_id)
            .filter(|intent| !state.audit_published_intents.contains(&intent.id))
            .take(limit)
            .cloned()
            .collect())
    }

    async fn mark_audit_published(
        &self,
        intent_id: Uuid,
        _at: DateTime<Utc>,
    ) -> Result<bool, TicketStoreError> {
        let mut state = self.state.lock().await;
        Ok(state.audit_published_intents.insert(intent_id))
    }

    async fn mark_activity_published(
        &self,
        intent_id: Uuid,
        _at: DateTime<Utc>,
    ) -> Result<bool, TicketStoreError> {
        Ok(self.state.lock().await.published_intents.insert(intent_id))
    }

    async fn find_ticket_by_message_identity(
        &self,
        project_id: &str,
        provider: &str,
        internet_message_id: &str,
    ) -> Result<Option<(TicketId, u64)>, TicketStoreError> {
        let state = self.state.lock().await;
        let Some((_, ticket_id)) = state.external_messages.values().find(|(identity, _)| {
            identity.project_id == project_id
                && identity.provider == provider
                && identity
                    .internet_message_id
                    .as_deref()
                    .is_some_and(|registered| {
                        message_identity_matches(registered, internet_message_id)
                    })
        }) else {
            return Ok(None);
        };
        Ok(state
            .tickets
            .get(&(project_id.to_owned(), *ticket_id))
            .map(|ticket| (*ticket_id, ticket.revision)))
    }

    async fn register_outbound_identity(
        &self,
        project_id: &str,
        identity: ExternalMessageIdentity,
        ticket_id: TicketId,
    ) -> Result<(), TicketStoreError> {
        let mut state = self.state.lock().await;
        let key = (
            project_id.to_owned(),
            identity.provider.clone(),
            identity.mailbox_scope.clone(),
            identity.external_id.clone(),
        );
        state
            .external_messages
            .entry(key)
            .or_insert((identity, ticket_id));
        drop(state);
        Ok(())
    }

    async fn operation_receipt(
        &self,
        idempotency_key: &str,
    ) -> Result<Option<OperationReceipt>, TicketStoreError> {
        let state = self.state.lock().await;
        Ok(state.operation_receipts.get(idempotency_key).cloned())
    }

    async fn put_session_exchange_grant(
        &self,
        grant: SessionExchangeGrant,
    ) -> Result<(), TicketStoreError> {
        let mut state = self.state.lock().await;
        state
            .session_exchange_grants
            .insert(grant.exchange_key.clone(), grant);
        drop(state);
        Ok(())
    }

    async fn session_exchange_grant(
        &self,
        exchange_key: &str,
    ) -> Result<Option<SessionExchangeGrant>, TicketStoreError> {
        let state = self.state.lock().await;
        Ok(state.session_exchange_grants.get(exchange_key).cloned())
    }

    async fn remove_session_exchange_grant(
        &self,
        exchange_key: &str,
    ) -> Result<bool, TicketStoreError> {
        let mut state = self.state.lock().await;
        Ok(state.session_exchange_grants.remove(exchange_key).is_some())
    }

    async fn assign_ticket_atomically(
        &self,
        request: AtomicAssignmentRequest,
    ) -> Result<Ticket, TicketStoreError> {
        if request.pool.is_empty() {
            return Err(TicketStoreError::Infrastructure(
                "assignment_pool is not configured".into(),
            ));
        }
        let mut state = self.state.lock().await;
        let key = (request.project_id.clone(), request.ticket_id);
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
        let assignee = match request.mode {
            crate::AssignmentMode::Manual => {
                return Err(TicketStoreError::Infrastructure(
                    "manual assignment is not a pool mode".into(),
                ));
            }
            crate::AssignmentMode::RoundRobin => {
                let next = state
                    .assignment_cursor
                    .get(&request.project_id)
                    .copied()
                    .unwrap_or(0);
                let pool_len = request.pool.len() as u64;
                let index = usize::try_from(next % pool_len).unwrap_or(0);
                state
                    .assignment_cursor
                    .insert(request.project_id.clone(), (next + 1) % pool_len);
                request.pool.get(index).cloned().unwrap_or_default()
            }
            crate::AssignmentMode::LeastWorkload => {
                let workload: std::collections::BTreeMap<String, u64> = state
                    .tickets
                    .values()
                    .filter(|candidate| candidate.project_id == request.project_id)
                    .filter(|candidate| candidate.assignee_subject.is_some())
                    .filter(|candidate| {
                        !matches!(
                            candidate.status,
                            crate::TicketStatus::Resolved | crate::TicketStatus::Closed
                        )
                    })
                    .fold(std::collections::BTreeMap::new(), |mut acc, candidate| {
                        let subject = candidate.assignee_subject.clone().unwrap_or_default();
                        *acc.entry(subject).or_insert(0u64) += 1;
                        acc
                    });
                request
                    .pool
                    .iter()
                    .min_by_key(|subject| (workload.get(*subject).copied().unwrap_or(0), *subject))
                    .cloned()
                    .unwrap_or_default()
            }
        };
        ticket
            .assign(Some(assignee), request.now)
            .map_err(TicketStoreError::Validation)?;
        let intent = TicketActivityIntent::new(
            ticket.project_id.clone(),
            ticket.id,
            "ticketing.assignment_changed",
            request.correlation_id,
            serde_json::json!({ "ticket_id": ticket.id }),
            request.now,
        );
        state.tickets.insert(key, ticket.clone());
        state.activity_intents.push(intent);
        drop(state);
        Ok(ticket)
    }

    async fn claim_send_intent(
        &self,
        intent: SendIntent,
    ) -> Result<Option<SendIntent>, TicketStoreError> {
        let mut state = self.state.lock().await;
        if let Some(existing) = state.send_intents.get(&intent.logical_send_id) {
            return Ok(Some(existing.clone()));
        }
        state
            .send_intents
            .insert(intent.logical_send_id.clone(), intent.clone());
        drop(state);
        Ok(None)
    }

    async fn send_intent(
        &self,
        logical_send_id: &str,
    ) -> Result<Option<SendIntent>, TicketStoreError> {
        let state = self.state.lock().await;
        Ok(state.send_intents.get(logical_send_id).cloned())
    }

    async fn resolve_send_intent(
        &self,
        logical_send_id: &str,
        state: SendIntentState,
        provider_message_id: Option<String>,
        now: DateTime<Utc>,
    ) -> Result<bool, TicketStoreError> {
        let mut guard = self.state.lock().await;
        match guard.send_intents.get_mut(logical_send_id) {
            Some(intent) => {
                intent.state = state;
                intent.provider_message_id = provider_message_id;
                intent.updated_at = now;
                drop(guard);
                Ok(true)
            }
            None => Ok(false),
        }
    }

    async fn create_ticket_from_external(
        &self,
        ticket: Ticket,
        intent: TicketActivityIntent,
        identity: ExternalMessageIdentity,
    ) -> Result<ExternalMessageIngestResult, TicketStoreError> {
        let mut state = self.state.lock().await;
        let external_key = (
            identity.project_id.clone(),
            identity.provider.clone(),
            identity.mailbox_scope.clone(),
            identity.external_id.clone(),
        );
        if let Some((existing, ticket_id)) = state.external_messages.get(&external_key) {
            return if existing.content_sha256 == identity.content_sha256 {
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
        if state
            .tickets
            .contains_key(&(ticket.project_id.clone(), ticket.id))
        {
            return Err(TicketStoreError::DuplicateTicket(ticket.id));
        }
        let key = (ticket.project_id.clone(), ticket.id);
        state.tickets.insert(key, ticket.clone());
        state.activity_intents.push(intent);
        state
            .external_messages
            .insert(external_key, (identity, ticket.id));
        drop(state);
        Ok(ExternalMessageIngestResult {
            ticket,
            repeated: false,
        })
    }

    #[allow(clippy::significant_drop_tightening)]
    async fn record_ticket_view(
        &self,
        project_id: &str,
        ticket_id: TicketId,
        subject: &str,
        at: DateTime<Utc>,
    ) -> Result<(), TicketStoreError> {
        let mut state = self.state.lock().await;
        if !state
            .tickets
            .contains_key(&(project_id.to_owned(), ticket_id))
        {
            return Err(TicketStoreError::NotFound(ticket_id));
        }
        state
            .ticket_views
            .entry((project_id.to_owned(), ticket_id))
            .or_default()
            .insert(subject.to_owned(), at);
        Ok(())
    }

    #[allow(clippy::significant_drop_tightening)]
    async fn recent_ticket_viewers(
        &self,
        project_id: &str,
        ticket_id: TicketId,
        excluding: &str,
        within: chrono::TimeDelta,
        now: DateTime<Utc>,
        limit: usize,
    ) -> Result<Vec<String>, TicketStoreError> {
        let state = self.state.lock().await;
        let Some(viewers) = state.ticket_views.get(&(project_id.to_owned(), ticket_id)) else {
            return Ok(Vec::new());
        };
        let mut recent: Vec<(DateTime<Utc>, String)> = viewers
            .iter()
            .filter(|(subject, viewed_at)| {
                subject.as_str() != excluding && now - *viewed_at <= within
            })
            .map(|(subject, viewed_at)| (*viewed_at, subject.clone()))
            .collect();
        recent.sort();
        recent.reverse();
        Ok(recent
            .into_iter()
            .take(limit)
            .map(|(_, subject)| subject)
            .collect())
    }

    #[allow(clippy::significant_drop_tightening)]
    async fn list_macros(&self, project_id: &str) -> Result<Vec<AgentMacro>, TicketStoreError> {
        let state = self.state.lock().await;
        let mut macros = state
            .macros
            .iter()
            .filter(|((macro_project, _), _)| macro_project == project_id)
            .map(|(_, macro_)| macro_.clone())
            .collect::<Vec<_>>();
        macros.sort_by(|a, b| a.title.cmp(&b.title).then(a.id.cmp(&b.id)));
        Ok(macros)
    }

    #[allow(clippy::significant_drop_tightening)]
    async fn insert_macro(
        &self,
        project_id: &str,
        macro_: AgentMacro,
    ) -> Result<(), TicketStoreError> {
        let mut state = self.state.lock().await;
        let project = project_id.to_owned();
        if state.macros.iter().any(|((macro_project, _), existing)| {
            macro_project == &project && existing.title == macro_.title
        }) {
            return Err(TicketStoreError::DuplicateMacroTitle);
        }
        state.macros.insert((project, macro_.id), macro_);
        Ok(())
    }

    #[allow(clippy::significant_drop_tightening)]
    async fn update_macro(
        &self,
        project_id: &str,
        id: Uuid,
        expected_revision: u64,
        title: &str,
        body: &str,
        now: DateTime<Utc>,
    ) -> Result<AgentMacro, TicketStoreError> {
        let mut state = self.state.lock().await;
        let key = (project_id.to_owned(), id);
        let Some(existing) = state.macros.get(&key).cloned() else {
            return Err(TicketStoreError::MacroNotFound(id));
        };
        if existing.revision != expected_revision {
            return Err(TicketStoreError::StaleRevision {
                expected: expected_revision,
                actual: existing.revision,
            });
        }
        let updated = existing.with_next_revision(title, body, now);
        state.macros.insert(key, updated.clone());
        Ok(updated)
    }

    async fn append_outbound_evidence(
        &self,
        evidence: OutboundDeliveryEvidence,
    ) -> Result<(), TicketStoreError> {
        self.state.lock().await.outbound_evidence.push(evidence);
        Ok(())
    }

    async fn outbound_evidence(
        &self,
        project_id: &str,
        ticket_id: TicketId,
        message_id: TicketMessageId,
    ) -> Result<Vec<OutboundDeliveryEvidence>, TicketStoreError> {
        Ok(self
            .state
            .lock()
            .await
            .outbound_evidence
            .iter()
            .filter(|row| {
                row.project_id == project_id
                    && row.ticket_id == ticket_id
                    && row.message_id == message_id
            })
            .cloned()
            .collect())
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
    #[error("a saved reply with this title already exists in the project")]
    DuplicateMacroTitle,
    #[error("saved reply was not found: {0}")]
    MacroNotFound(Uuid),
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

/// Outbound registrations pin the mail identity's local part (the
/// deterministic message id); the rendered domain belongs to the sending
/// transport, so reply resolution compares the angle-bracket local part
/// scoped to project and provider (review finding 8).
fn message_id_local_part(value: &str) -> &str {
    value
        .trim()
        .trim_start_matches('<')
        .split('@')
        .next()
        .unwrap_or_default()
}

fn message_identity_matches(registered: &str, candidate: &str) -> bool {
    if registered == candidate {
        return true;
    }
    let (registered_local, candidate_local) = (
        message_id_local_part(registered),
        message_id_local_part(candidate),
    );
    !registered_local.is_empty() && registered_local == candidate_local
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
                ticket_type: crate::TicketType::default(),
                form_answers: Vec::new(),

                first_response_deadline: None,
                resolution_deadline: None,
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
                ticket_type: crate::TicketType::default(),
                form_answers: Vec::new(),
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
                ticket_type: crate::TicketType::default(),
                form_answers: Vec::new(),
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
                ticket_type: crate::TicketType::default(),
                form_answers: Vec::new(),
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
