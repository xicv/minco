//! Pre-isolation consumer fixture (round 1 finding 8): an external store
//! implementation written against the ORIGINAL `TicketingStore` surface —
//! plain delegation over a store object, implementing exactly the
//! required methods that existed before workspace isolation and none of
//! the newer provided ones — still compiles unchanged and serves a full
//! ticketing lifecycle with isolation off, while the legacy receipt
//! entry fails closed the moment isolation is enabled.

use chrono::{DateTime, Utc};
use minco_interaction::SupportHandoff;
use minco_plugin_ticketing::{
    AgentMacro, AppendTicketMessageRequest, AtomicAssignmentRequest, AutomationProposal,
    Clarification, CompleteSendOutcome, ConsumeHandoffRequest, ConsumeSessionRequest,
    ConsumedHandoff, ConsumedSessionIdentity, CreateTicketInput, ExternalMessageIdentity,
    ExternalMessageIngestResult, IngestExternalMessageRequest, MemoryTicketingStore,
    MessageListFilter, OperationReceipt, OutboundDeliveryEvidence, SendIntent, SendIntentState,
    SessionExchangeGrant, Ticket, TicketActivityIntent, TicketChannel, TicketId, TicketListFilter,
    TicketMessageId, TicketPriority, TicketRequester, TicketStoreError, TicketSummary,
    TicketSummaryFilter, TicketingConfig, TicketingService, TicketingStore, TicketingStoreService,
};
use std::collections::BTreeMap;
use std::sync::Arc;
use uuid::Uuid;

/// The consumer's wrapper: an external `Arc<dyn TicketingStore>` fronted
/// by its own type, exactly as a downstream integration would own one.
/// It implements ONLY the pre-isolation required surface; the scoped
/// marks added by ADR-0076 arrive through their provided defaults.
struct PreIsolationStore(pub Arc<dyn TicketingStore>);

impl std::fmt::Debug for PreIsolationStore {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_tuple("PreIsolationStore").finish()
    }
}

#[async_trait::async_trait]
impl TicketingStore for PreIsolationStore {
    async fn create(
        &self,
        ticket: Ticket,
        intent: TicketActivityIntent,
    ) -> Result<(), TicketStoreError> {
        (self.0.as_ref()).create(ticket, intent).await
    }

    async fn get(
        &self,
        project_id: &str,
        id: TicketId,
    ) -> Result<Option<Ticket>, TicketStoreError> {
        (self.0.as_ref()).get(project_id, id).await
    }

    async fn list(&self, filter: TicketListFilter) -> Result<Vec<Ticket>, TicketStoreError> {
        (self.0.as_ref()).list(filter).await
    }

    async fn list_summaries(
        &self,
        filter: TicketSummaryFilter,
    ) -> Result<Vec<TicketSummary>, TicketStoreError> {
        (self.0.as_ref()).list_summaries(filter).await
    }

    async fn append_ticket_message(
        &self,
        request: AppendTicketMessageRequest,
    ) -> Result<(), TicketStoreError> {
        (self.0.as_ref()).append_ticket_message(request).await
    }

    async fn list_ticket_messages(
        &self,
        filter: MessageListFilter,
    ) -> Result<Vec<minco_plugin_ticketing::TicketMessage>, TicketStoreError> {
        (self.0.as_ref()).list_ticket_messages(filter).await
    }

    async fn save(
        &self,
        ticket: Ticket,
        expected_revision: u64,
        intent: TicketActivityIntent,
    ) -> Result<(), TicketStoreError> {
        (self.0.as_ref())
            .save(ticket, expected_revision, intent)
            .await
    }

    async fn insert_handoff(&self, handoff: SupportHandoff) -> Result<(), TicketStoreError> {
        (self.0.as_ref()).insert_handoff(handoff).await
    }

    async fn consume_and_create_ticket(
        &self,
        request: ConsumeHandoffRequest,
    ) -> Result<ConsumedHandoff, TicketStoreError> {
        (self.0.as_ref()).consume_and_create_ticket(request).await
    }

    async fn consume_handoff_identity(
        &self,
        request: ConsumeSessionRequest,
    ) -> Result<(ConsumedSessionIdentity, bool), TicketStoreError> {
        (self.0.as_ref()).consume_handoff_identity(request).await
    }

    async fn ingest_external_message(
        &self,
        request: IngestExternalMessageRequest,
    ) -> Result<ExternalMessageIngestResult, TicketStoreError> {
        (self.0.as_ref()).ingest_external_message(request).await
    }

    async fn pending_activity_intents(
        &self,
        project_id: &str,
        limit: usize,
    ) -> Result<Vec<TicketActivityIntent>, TicketStoreError> {
        (self.0.as_ref())
            .pending_activity_intents(project_id, limit)
            .await
    }

    async fn mark_activity_published(
        &self,
        intent_id: Uuid,
        at: DateTime<Utc>,
    ) -> Result<bool, TicketStoreError> {
        (self.0.as_ref())
            .mark_activity_published(intent_id, at)
            .await
    }

    async fn pending_audit_intents(
        &self,
        project_id: &str,
        limit: usize,
    ) -> Result<Vec<TicketActivityIntent>, TicketStoreError> {
        (self.0.as_ref())
            .pending_audit_intents(project_id, limit)
            .await
    }

    async fn mark_audit_published(
        &self,
        intent_id: Uuid,
        at: DateTime<Utc>,
    ) -> Result<bool, TicketStoreError> {
        (self.0.as_ref()).mark_audit_published(intent_id, at).await
    }

    async fn find_ticket_by_message_identity(
        &self,
        project_id: &str,
        provider: &str,
        internet_message_id: &str,
    ) -> Result<Option<(TicketId, u64)>, TicketStoreError> {
        (self.0.as_ref())
            .find_ticket_by_message_identity(project_id, provider, internet_message_id)
            .await
    }

    async fn register_outbound_identity(
        &self,
        project_id: &str,
        identity: ExternalMessageIdentity,
        ticket_id: TicketId,
    ) -> Result<(), TicketStoreError> {
        (self.0.as_ref())
            .register_outbound_identity(project_id, identity, ticket_id)
            .await
    }

    async fn operation_receipt(
        &self,
        idempotency_key: &str,
    ) -> Result<Option<OperationReceipt>, TicketStoreError> {
        (self.0.as_ref()).operation_receipt(idempotency_key).await
    }

    async fn record_session_exchange_fenced(
        &self,
        grant: SessionExchangeGrant,
        expected_generation: Option<u64>,
    ) -> Result<SessionExchangeGrant, TicketStoreError> {
        (self.0.as_ref())
            .record_session_exchange_fenced(grant, expected_generation)
            .await
    }

    async fn revoke_session_exchange(
        &self,
        exchange_key: &str,
        now: DateTime<Utc>,
    ) -> Result<bool, TicketStoreError> {
        (self.0.as_ref())
            .revoke_session_exchange(exchange_key, now)
            .await
    }

    async fn stage_rotation_fenced(
        &self,
        exchange_key: &str,
        expected_session_id: minco_plugin_sessions::SessionId,
        staged_session_id: minco_plugin_sessions::SessionId,
    ) -> Result<bool, TicketStoreError> {
        (self.0.as_ref())
            .stage_rotation_fenced(exchange_key, expected_session_id, staged_session_id)
            .await
    }

    async fn complete_rotation_fenced(
        &self,
        exchange_key: &str,
        expected_session_id: minco_plugin_sessions::SessionId,
        staged_session_id: minco_plugin_sessions::SessionId,
    ) -> Result<bool, TicketStoreError> {
        (self.0.as_ref())
            .complete_rotation_fenced(exchange_key, expected_session_id, staged_session_id)
            .await
    }

    async fn clear_rotation_staged_fenced(
        &self,
        exchange_key: &str,
        expected_session_id: minco_plugin_sessions::SessionId,
        staged_session_id: minco_plugin_sessions::SessionId,
    ) -> Result<bool, TicketStoreError> {
        (self.0.as_ref())
            .clear_rotation_staged_fenced(exchange_key, expected_session_id, staged_session_id)
            .await
    }

    async fn remove_session_exchange_grant_fenced(
        &self,
        exchange_key: &str,
        expected_session_id: minco_plugin_sessions::SessionId,
    ) -> Result<bool, TicketStoreError> {
        (self.0.as_ref())
            .remove_session_exchange_grant_fenced(exchange_key, expected_session_id)
            .await
    }

    async fn put_session_exchange_grant(
        &self,
        grant: SessionExchangeGrant,
    ) -> Result<(), TicketStoreError> {
        (self.0.as_ref()).put_session_exchange_grant(grant).await
    }

    async fn assign_ticket_atomically(
        &self,
        request: AtomicAssignmentRequest,
    ) -> Result<Ticket, TicketStoreError> {
        (self.0.as_ref()).assign_ticket_atomically(request).await
    }

    async fn claim_send_attempt(
        &self,
        logical_send_id: &str,
        expected_state: SendIntentState,
        now: DateTime<Utc>,
    ) -> Result<Option<Uuid>, TicketStoreError> {
        (self.0.as_ref())
            .claim_send_attempt(logical_send_id, expected_state, now)
            .await
    }

    async fn resolve_send_intent_fenced(
        &self,
        logical_send_id: &str,
        attempt_id: Uuid,
        to: SendIntentState,
        provider_message_id: Option<String>,
        now: DateTime<Utc>,
    ) -> Result<bool, TicketStoreError> {
        (self.0.as_ref())
            .resolve_send_intent_fenced(logical_send_id, attempt_id, to, provider_message_id, now)
            .await
    }

    async fn complete_send_attempt(
        &self,
        logical_send_id: &str,
        attempt_id: Uuid,
        provider_message_id: &str,
        evidence: OutboundDeliveryEvidence,
        threading: ExternalMessageIdentity,
        now: DateTime<Utc>,
    ) -> Result<CompleteSendOutcome, TicketStoreError> {
        (self.0.as_ref())
            .complete_send_attempt(
                logical_send_id,
                attempt_id,
                provider_message_id,
                evidence,
                threading,
                now,
            )
            .await
    }

    async fn append_outbound_evidence_fenced(
        &self,
        logical_send_id: &str,
        attempt_id: Uuid,
        evidence: OutboundDeliveryEvidence,
    ) -> Result<bool, TicketStoreError> {
        (self.0.as_ref())
            .append_outbound_evidence_fenced(logical_send_id, attempt_id, evidence)
            .await
    }

    async fn claim_send_intent(
        &self,
        intent: SendIntent,
    ) -> Result<Option<SendIntent>, TicketStoreError> {
        (self.0.as_ref()).claim_send_intent(intent).await
    }

    async fn send_intent(
        &self,
        logical_send_id: &str,
    ) -> Result<Option<SendIntent>, TicketStoreError> {
        (self.0.as_ref()).send_intent(logical_send_id).await
    }

    async fn resolve_send_intent(
        &self,
        logical_send_id: &str,
        state: SendIntentState,
        provider_message_id: Option<String>,
        now: DateTime<Utc>,
    ) -> Result<bool, TicketStoreError> {
        (self.0.as_ref())
            .resolve_send_intent(logical_send_id, state, provider_message_id, now)
            .await
    }

    async fn session_exchange_grant(
        &self,
        exchange_key: &str,
    ) -> Result<Option<SessionExchangeGrant>, TicketStoreError> {
        (self.0.as_ref()).session_exchange_grant(exchange_key).await
    }

    async fn remove_session_exchange_grant(
        &self,
        exchange_key: &str,
    ) -> Result<bool, TicketStoreError> {
        (self.0.as_ref())
            .remove_session_exchange_grant(exchange_key)
            .await
    }

    async fn create_ticket_from_external(
        &self,
        ticket: Ticket,
        intent: TicketActivityIntent,
        identity: ExternalMessageIdentity,
    ) -> Result<ExternalMessageIngestResult, TicketStoreError> {
        (self.0.as_ref())
            .create_ticket_from_external(ticket, intent, identity)
            .await
    }

    async fn append_outbound_evidence(
        &self,
        evidence: OutboundDeliveryEvidence,
    ) -> Result<(), TicketStoreError> {
        (self.0.as_ref()).append_outbound_evidence(evidence).await
    }

    async fn erase_tickets_resolved_before(
        &self,
        project_id: &str,
        cutoff: DateTime<Utc>,
        limit: usize,
    ) -> Result<usize, TicketStoreError> {
        (self.0.as_ref())
            .erase_tickets_resolved_before(project_id, cutoff, limit)
            .await
    }

    async fn insert_clarification(
        &self,
        project_id: &str,
        clarification: Clarification,
    ) -> Result<(), TicketStoreError> {
        (self.0.as_ref())
            .insert_clarification(project_id, clarification)
            .await
    }

    async fn list_clarifications(
        &self,
        project_id: &str,
        ticket_id: TicketId,
    ) -> Result<Vec<Clarification>, TicketStoreError> {
        (self.0.as_ref())
            .list_clarifications(project_id, ticket_id)
            .await
    }

    async fn get_clarification(
        &self,
        project_id: &str,
        id: Uuid,
    ) -> Result<Option<Clarification>, TicketStoreError> {
        (self.0.as_ref()).get_clarification(project_id, id).await
    }

    async fn update_clarification(
        &self,
        project_id: &str,
        clarification: Clarification,
    ) -> Result<(), TicketStoreError> {
        (self.0.as_ref())
            .update_clarification(project_id, clarification)
            .await
    }

    async fn insert_automation_proposal(
        &self,
        project_id: &str,
        proposal: AutomationProposal,
    ) -> Result<(), TicketStoreError> {
        (self.0.as_ref())
            .insert_automation_proposal(project_id, proposal)
            .await
    }

    async fn list_automation_proposals(
        &self,
        project_id: &str,
        ticket_id: TicketId,
    ) -> Result<Vec<AutomationProposal>, TicketStoreError> {
        (self.0.as_ref())
            .list_automation_proposals(project_id, ticket_id)
            .await
    }

    async fn get_automation_proposal(
        &self,
        project_id: &str,
        id: Uuid,
    ) -> Result<Option<AutomationProposal>, TicketStoreError> {
        (self.0.as_ref())
            .get_automation_proposal(project_id, id)
            .await
    }

    async fn update_automation_proposal(
        &self,
        project_id: &str,
        proposal: AutomationProposal,
    ) -> Result<(), TicketStoreError> {
        (self.0.as_ref())
            .update_automation_proposal(project_id, proposal)
            .await
    }

    async fn advance_assignment_cursor(
        &self,
        project_id: &str,
        pool_len: usize,
    ) -> Result<usize, TicketStoreError> {
        (self.0.as_ref())
            .advance_assignment_cursor(project_id, pool_len)
            .await
    }

    async fn assignee_workload(
        &self,
        project_id: &str,
        subjects: &[String],
    ) -> Result<BTreeMap<String, u64>, TicketStoreError> {
        (self.0.as_ref())
            .assignee_workload(project_id, subjects)
            .await
    }

    async fn record_ticket_view(
        &self,
        project_id: &str,
        ticket_id: TicketId,
        subject: &str,
        at: DateTime<Utc>,
    ) -> Result<(), TicketStoreError> {
        (self.0.as_ref())
            .record_ticket_view(project_id, ticket_id, subject, at)
            .await
    }

    async fn recent_ticket_viewers(
        &self,
        project_id: &str,
        ticket_id: TicketId,
        excluding: &str,
        within: chrono::TimeDelta,
        now: DateTime<Utc>,
        limit: usize,
    ) -> Result<Vec<String>, TicketStoreError> {
        (self.0.as_ref())
            .recent_ticket_viewers(project_id, ticket_id, excluding, within, now, limit)
            .await
    }

    async fn list_macros(&self, project_id: &str) -> Result<Vec<AgentMacro>, TicketStoreError> {
        (self.0.as_ref()).list_macros(project_id).await
    }

    async fn insert_macro(
        &self,
        project_id: &str,
        macro_: AgentMacro,
    ) -> Result<(), TicketStoreError> {
        (self.0.as_ref()).insert_macro(project_id, macro_).await
    }

    async fn update_macro(
        &self,
        project_id: &str,
        id: Uuid,
        expected_revision: u64,
        title: &str,
        body: &str,
        now: DateTime<Utc>,
    ) -> Result<AgentMacro, TicketStoreError> {
        (self.0.as_ref())
            .update_macro(project_id, id, expected_revision, title, body, now)
            .await
    }

    async fn outbound_evidence(
        &self,
        project_id: &str,
        ticket_id: TicketId,
        message_id: TicketMessageId,
    ) -> Result<Vec<OutboundDeliveryEvidence>, TicketStoreError> {
        (self.0.as_ref())
            .outbound_evidence(project_id, ticket_id, message_id)
            .await
    }
}

#[tokio::test]
async fn a_pre_isolation_consumer_keeps_compiling_and_working() {
    // The consumer composes its own store wrapper through the unchanged
    // public surface, with isolation off (their world).
    let store = PreIsolationStore(Arc::new(MemoryTicketingStore::default()));
    let service = TicketingService::new(
        TicketingStoreService::new(Arc::new(store)),
        TicketingConfig {
            project_id: "consumer".into(),
            portal_origin: "https://support.example.test".into(),
            ..TicketingConfig::default()
        },
    )
    .unwrap();
    let agent = minco_plugin_identity::Identity {
        subject: "consumer-agent".into(),
        permissions: [
            "ticketing.create",
            "ticketing.agent.read",
            "ticketing.manage",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect(),
        scopes: std::collections::BTreeSet::default(),
        claims: std::collections::BTreeMap::default(),
    };
    let created = service
        .create_ticket(
            &agent,
            CreateTicketInput {
                project_id: "consumer".into(),
                subject: "Consumer ticket".into(),
                description: "Through the consumer's own store.".into(),
                requester: TicketRequester {
                    subject: "consumer-requester".into(),
                    display_name: None,
                    email: None,
                },
                channel: TicketChannel::Api,
                priority: TicketPriority::Normal,
                ticket_type: minco_plugin_ticketing::TicketType::default(),
                form_answers: Vec::new(),
                resource_references: Vec::new(),
            },
            Uuid::now_v7(),
            Utc::now(),
        )
        .await
        .expect("the consumer's store serves the lifecycle");
    assert_eq!(created.ticket.project_id, "consumer");
    // The original receipt entry still works in their world.
    assert!(
        service
            .operation_receipt("any-key")
            .await
            .expect("legacy receipt entry keeps its pre-isolation behavior")
            .is_none()
    );

    // The moment isolation is on, the SAME legacy entry fails closed —
    // it can never bypass isolated mode.
    let isolated_store = PreIsolationStore(Arc::new(MemoryTicketingStore::default()));
    let isolated = TicketingService::new(
        TicketingStoreService::new(Arc::new(isolated_store)),
        TicketingConfig {
            project_id: "consumer".into(),
            portal_origin: "https://support.example.test".into(),
            ..TicketingConfig::default()
        },
    )
    .unwrap()
    .with_isolation(minco_plugin_ticketing::TicketingIsolationConfig {
        workspace_id: Some("ws-consumer".into()),
        ..minco_plugin_ticketing::TicketingIsolationConfig::default()
    })
    .unwrap();
    assert!(isolated.operation_receipt("any-key").await.is_err());
}

/// Round 1 finding 8 (round 2 reopening): the pre-isolation public
/// shapes must remain constructible exactly as the merged base allowed.
/// A field added to any of these public structs would break every
/// downstream exhaustive struct literal — this witness compiles the
/// base-style constructions verbatim.
#[test]
fn pre_isolation_public_shapes_stay_constructible_verbatim() {
    use minco_plugin_ticketing::{SessionExchangeGrant, TicketingConfig};

    // TicketingConfig: exactly the base field set, no isolation fields.
    let _config = TicketingConfig {
        project_id: "consumer".into(),
        portal_origin: "https://support.example.test".into(),
        allowed_return_paths: BTreeMap::new(),
        handoff_ttl_seconds: 900,
        support_label: "Support".into(),
        support_brand: "Brand".into(),
        privacy_notice: "Privacy".into(),
        requester_session_ttl_seconds: 3600,
        assignment_pool: Vec::new(),
        sla: None,
        notify_requester_on_public_reply: false,
        inbound_email_first_contact: false,
        inbound_auth_policy: minco_plugin_ticketing::InboundAuthPolicy::default(),
        inbound_authserv_id: String::default(),
        inbound_scan_verdicts: minco_plugin_ticketing::ScanVerdictPolicy::default(),
        automation: minco_plugin_ticketing::AutomationConfig::default(),
    };

    // SessionExchangeGrant: exactly the base field set — the workspace
    // binding lives in durable storage beside the grant, never as an
    // added struct field.
    let _grant = SessionExchangeGrant {
        exchange_key: "exchange-key".into(),
        session_id: minco_plugin_sessions::SessionId(uuid::Uuid::new_v4()),
        generation: 0,
        subject: "user-1".into(),
        project_id: "consumer".into(),
        permissions: vec!["ticketing.requester.read".into()],
        portal_origin: "https://support.example.test".into(),
        expires_at: Utc::now(),
        created_at: Utc::now(),
        revoked_at: None,
        rotation_staged_session_id: None,
    };
}

#[tokio::test]
async fn an_isolated_service_over_a_legacy_adapter_never_writes_unbound_grants() {
    // Round 2c review (P1-1): over a store that implements only the
    // pre-isolation surface, an isolated service can neither commit a
    // NEW grant (the scoped write's provided default fails without
    // effect) nor act on an EXISTING unbound grant (ownership reads the
    // persisted binding and denies). Nothing is stored, and the legacy
    // grant survives untouched — generation, liveness and binding.
    let inner = Arc::new(MemoryTicketingStore::default());
    let seeded_session = minco_plugin_sessions::SessionId(uuid::Uuid::new_v4());
    inner
        .record_session_exchange_fenced(
            SessionExchangeGrant {
                exchange_key: "seeded-key".into(),
                session_id: seeded_session,
                generation: 0,
                subject: "user-1".into(),
                project_id: "consumer".into(),
                permissions: vec!["ticketing.requester.read".into()],
                portal_origin: "https://support.example.test".into(),
                expires_at: Utc::now() + chrono::TimeDelta::minutes(10),
                created_at: Utc::now(),
                revoked_at: None,
                rotation_staged_session_id: None,
            },
            None,
        )
        .await
        .expect("the consumer's original write path keeps working");

    let isolated = TicketingService::new(
        TicketingStoreService::new(Arc::new(PreIsolationStore(inner.clone()))),
        TicketingConfig {
            project_id: "consumer".into(),
            portal_origin: "https://support.example.test".into(),
            ..TicketingConfig::default()
        },
    )
    .unwrap()
    .with_isolation(minco_plugin_ticketing::TicketingIsolationConfig {
        workspace_id: Some("ws-consumer".into()),
        ..minco_plugin_ticketing::TicketingIsolationConfig::default()
    })
    .unwrap()
    .with_portal_services(minco_plugin_ticketing::TicketingPortalServices {
        sessions: Some(Arc::new(minco_plugin_sessions::SessionService::new(
            Arc::new(minco_plugin_sessions::MemorySessionStore::default()),
        ))),
        csrf: Some(Arc::new(
            minco_plugin_sessions::CsrfService::new(
                "legacy-adapter-csrf-secret-of-sufficient-length",
            )
            .expect("csrf"),
        )),
        ..Default::default()
    });

    // A new exchange cannot commit its grant: the legacy adapter cannot
    // establish the requested binding, so the scoped write fails without
    // writing — no unbound grant to reject only at a later read.
    let denied = isolated
        .record_session_exchange_grant(
            "fresh-key",
            minco_plugin_sessions::SessionId(uuid::Uuid::new_v4()),
            "user-2",
            "https://support.example.test",
            vec!["ticketing.requester.read".into()],
            Utc::now() + chrono::TimeDelta::minutes(10),
        )
        .await
        .expect_err("a legacy adapter cannot serve an isolated scoped write");
    assert!(matches!(
        denied,
        minco_plugin_ticketing::TicketingServiceError::Store(_)
    ));
    assert!(
        inner
            .session_exchange_grant("fresh-key")
            .await
            .unwrap()
            .is_none(),
        "nothing was committed for the denied exchange"
    );

    // The pre-existing unbound grant is untouchable: rotation is denied
    // on ownership before any effect, and the grant survives verbatim.
    let denied = isolated
        .rotate_session_exchange("seeded-key")
        .await
        .expect_err("rotation of a legacy-unbound grant must deny");
    assert!(matches!(
        denied,
        minco_plugin_ticketing::TicketingServiceError::ScopeDenied
    ));
    let surviving = inner
        .session_exchange_grant("seeded-key")
        .await
        .unwrap()
        .expect("the legacy grant survives the denial untouched");
    assert_eq!(surviving.session_id, seeded_session);
    assert_eq!(surviving.generation, 0);
    assert!(surviving.revoked_at.is_none());
    assert!(surviving.rotation_staged_session_id.is_none());
}
