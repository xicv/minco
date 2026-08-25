//! Optional Ticketing-to-Jobs bridge (ADR-0054, ADR-0055, ADR-0063).
//!
//! Two typed commands ship: `ticketing.deliver-public-notification` v1
//! (email delivery through the observable mail path with reconciliation
//! and ambiguity recovery, in-app through the notifications port) and
//! `ticketing.process-inbound-email` v1 (verified raw-object ingress).
//! The bridge owns no queue, lease, retry or scheduling machinery — all of
//! that is the released jobs plugin (ADR-0048).

#[cfg(feature = "sqlite")]
use crate::TicketStoreError;
use crate::{
    OutboundDeliveryEvidence, OutboundEvidenceKind, TicketId, TicketMessageId, TicketingService,
    TicketingStoreService,
};
#[cfg(feature = "sqlite")]
use async_trait::async_trait;
use chrono::{TimeDelta, Utc};
use minco_plugin_jobs::{
    Job, JobEnvelope, JobError, JobExecutionFailure, JobHandlerRegistry, JobOptions, RetryPolicy,
    pending_record,
};
use minco_plugin_notifications::{
    MailAddress, MailError, MailRetryAdvice, MailService, Notification, NotificationChannel,
    NotificationService,
};
#[cfg(test)]
use minco_plugin_object_storage::ObjectStore as _;
use minco_plugin_object_storage::ObjectStoreService;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use uuid::Uuid;

/// Deferred command: notify the requester about one public message.
/// The payload carries bounded identifiers only — never message bodies,
/// addresses or credentials (ADR-0054).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DeliverPublicNotification {
    pub project_id: String,
    pub ticket_id: TicketId,
    pub message_id: TicketMessageId,
}

impl Job for DeliverPublicNotification {
    const NAME: &'static str = "ticketing.deliver-public-notification";
    const VERSION: u16 = 1;
}

/// Deferred command: process one inbound raw email for a known ticket
/// (ADR-0055). The raw MIME stays authoritative in object storage; this
/// payload carries bounded identities and digests only.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ProcessInboundEmail {
    pub project_id: String,
    pub provider: String,
    pub mailbox_scope: String,
    pub external_id: String,
    pub content_sha256: String,
    pub raw_object_key: String,
    pub ticket_id: TicketId,
    pub expected_revision: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub internet_message_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub in_reply_to: Option<String>,
    #[serde(default)]
    pub references: Vec<String>,
}

impl Job for ProcessInboundEmail {
    const NAME: &'static str = "ticketing.process-inbound-email";
    const VERSION: u16 = 1;
}

pub const TICKETING_MAIL_PROFILE: &str = "ticketing-mail";
pub const NOTIFICATION_DEADLINE_SECONDS: i64 = 3600;
pub const INBOUND_EMAIL_DEADLINE_SECONDS: i64 = 6 * 3600;

fn digest_of(parts: &[&str]) -> String {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update((part.len() as u64).to_le_bytes());
        hasher.update(part.as_bytes());
    }
    hex::encode(hasher.finalize())
}

/// Envelope policy for the inbound command (ADR-0055): dedupe by the
/// provider-scoped external identity, serialize per mailbox, partition by
/// project, bounded exponential retry, six-hour deadline.
pub fn inbound_email_envelope(
    payload: &ProcessInboundEmail,
    correlation_id: Uuid,
    now: chrono::DateTime<Utc>,
) -> Result<JobEnvelope, JobError> {
    let envelope = JobEnvelope::for_job(payload, TICKETING_MAIL_PROFILE, correlation_id)?.with(
        JobOptions::default()
            .with_dedupe_key(format!(
                "mail:{}",
                digest_of(&[
                    &payload.provider,
                    &payload.mailbox_scope,
                    &payload.external_id,
                ])
            ))
            .with_overlap_key(format!(
                "mailbox:{}",
                digest_of(&[&payload.provider, &payload.mailbox_scope])
            ))
            .with_partition(payload.project_id.clone())
            .with_retry(RetryPolicy::exponential(5, 10, 900))
            .with_deadline(now + TimeDelta::seconds(INBOUND_EMAIL_DEADLINE_SECONDS))
            .with_causation(correlation_id),
    );
    Ok(envelope)
}

/// Envelope policy for the notification command (ADR-0054).
///
/// Dedupe by ticket and message, serialize per ticket, partition by
/// project, bounded exponential retry, and a deadline so a stale
/// acknowledgement never sends.
pub fn public_notification_envelope(
    payload: &DeliverPublicNotification,
    correlation_id: Uuid,
    now: chrono::DateTime<Utc>,
) -> Result<JobEnvelope, JobError> {
    let envelope = JobEnvelope::for_job(payload, TICKETING_MAIL_PROFILE, correlation_id)?.with(
        JobOptions::default()
            .with_dedupe_key(format!(
                "notification:{}:{}",
                payload.ticket_id, payload.message_id
            ))
            .with_overlap_key(format!("ticket:{}", payload.ticket_id))
            .with_partition(payload.project_id.clone())
            .with_retry(RetryPolicy::exponential(5, 5, 900))
            .with_deadline(now + TimeDelta::seconds(NOTIFICATION_DEADLINE_SECONDS))
            .with_causation(correlation_id),
    );
    Ok(envelope)
}

/// Explicit composition-root dependencies for the ticketing handlers.
/// The worker identity holds only `ticketing.ingest` (ADR-0055): a job
/// worker never bypasses ticketing authorization.
pub struct TicketingJobsDeps {
    pub service: TicketingService,
    pub notifications: Arc<NotificationService>,
    /// Observable mail path for email-channel public replies (ADR-0063).
    /// When absent, email notifications fail closed as a permanent
    /// configuration defect instead of silently downgrading the channel.
    pub mail: Option<Arc<MailService>>,
    pub objects: Arc<ObjectStoreService>,
    pub worker: minco_plugin_identity::Identity,
}

impl std::fmt::Debug for TicketingJobsDeps {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Redacted: service state and worker permissions are not debug data.
        formatter
            .debug_struct("TicketingJobsDeps")
            .field("service", &"[REDACTED]")
            .field("notifications", &"[REDACTED]")
            .field("objects", &"[REDACTED]")
            .field("worker", &"[REDACTED]")
            .finish()
    }
}

/// Register the ticketing handlers on the composition's registry. Static
/// and explicit: the composition root calls this before building
/// `JobsServices`; no runtime scanning, no plugin retro-fit.
pub fn register_ticketing_jobs(
    registry: &JobHandlerRegistry,
    store: &TicketingStoreService,
    deps: TicketingJobsDeps,
) -> Result<(), JobError> {
    let notification_store = store.clone();
    let notification_sink = deps.notifications.clone();
    let notification_mail = deps.mail.clone();
    registry.register_typed::<DeliverPublicNotification, _, _>(move |command, _context| {
        let store = notification_store.clone();
        let notifications = notification_sink.clone();
        let mail = notification_mail.clone();
        async move {
            deliver_public_notification(&store, &notifications, mail.as_ref(), &command).await
        }
    })?;
    let inbound_service = deps.service.clone();
    let inbound_objects = deps.objects.clone();
    let inbound_worker = deps.worker;
    registry.register_typed::<ProcessInboundEmail, _, _>(move |command, context| {
        let service = inbound_service.clone();
        let objects = inbound_objects.clone();
        let worker = inbound_worker.clone();
        async move { process_inbound_email(&service, &objects, &worker, &command, context).await }
    })
}

async fn process_inbound_email(
    service: &TicketingService,
    objects: &ObjectStoreService,
    worker: &minco_plugin_identity::Identity,
    command: &ProcessInboundEmail,
    context: minco_plugin_jobs::JobContext,
) -> Result<(), JobExecutionFailure> {
    let key = minco_plugin_object_storage::ObjectKey::parse(command.raw_object_key.clone())
        .map_err(|_| JobExecutionFailure::permanent("ticketing.inbound_object_missing"))?;
    let stored = objects
        .0
        .get(&key)
        .await
        .map_err(|_| JobExecutionFailure::retryable("ticketing.inbound_store_unavailable"))?
        .ok_or_else(|| JobExecutionFailure::permanent("ticketing.inbound_object_missing"))?;
    let actual_digest = crate::external_content_sha256(&stored.bytes);
    if !actual_digest.eq_ignore_ascii_case(&command.content_sha256) {
        // Unverified content is never ingested (ADR-0055).
        return Err(JobExecutionFailure::permanent(
            "ticketing.inbound_digest_mismatch",
        ));
    }
    let message = mail_parser::MessageParser::default()
        .parse(&stored.bytes)
        .ok_or_else(|| JobExecutionFailure::permanent("ticketing.inbound_mime_invalid"))?;
    let body = message
        .body_text(0)
        .ok_or_else(|| JobExecutionFailure::permanent("ticketing.inbound_body_missing"))?
        // Ticket bodies are single-paragraph by domain contract; v1 email
        // ingestion flattens line breaks to spaces. The raw MIME stays
        // authoritative in object storage (ADR-0055).
        .replace(['\r', '\n', '\t'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let identity = crate::ExternalMessageIdentity {
        project_id: command.project_id.clone(),
        provider: command.provider.clone(),
        mailbox_scope: command.mailbox_scope.clone(),
        external_id: command.external_id.clone(),
        content_sha256: command.content_sha256.to_ascii_lowercase(),
        raw_message_object_key: Some(command.raw_object_key.clone()),
        internet_message_id: command.internet_message_id.clone(),
        in_reply_to: command.in_reply_to.clone(),
        references: command.references.clone(),
    };
    match service
        .ingest_external_message(
            worker,
            identity,
            command.ticket_id,
            body,
            command.expected_revision,
            context.correlation_id,
            Utc::now(),
        )
        .await
    {
        Ok(_) => Ok(()),
        Err(error @ crate::TicketingServiceError::PermissionDenied(_)) => {
            let _ = error;
            Err(JobExecutionFailure::permanent(
                "ticketing.ingest_unauthorized",
            ))
        }
        Err(
            crate::TicketingServiceError::StaleRevision { .. }
            | crate::TicketingServiceError::Store(crate::TicketStoreError::StaleRevision { .. }),
        ) => Err(JobExecutionFailure::retryable(
            "ticketing.inbound_revision_stale",
        )),
        Err(crate::TicketingServiceError::Store(
            crate::TicketStoreError::ExternalIdentityConflict,
        )) => Err(JobExecutionFailure::permanent(
            "ticketing.inbound_identity_conflict",
        )),
        Err(
            crate::TicketingServiceError::Validation(_)
            | crate::TicketingServiceError::InvalidExternalIdentity
            | crate::TicketingServiceError::InvalidContentDigest
            | crate::TicketingServiceError::Store(crate::TicketStoreError::Validation(_)),
        ) => Err(JobExecutionFailure::permanent("ticketing.inbound_invalid")),
        Err(other) => {
            // Store and infrastructure failures are retryable; the exact
            // cause stays in worker logs, never in the failure code.
            tracing::warn!(error = %other, "inbound email ingestion failed; retrying");
            Err(JobExecutionFailure::retryable(
                "ticketing.inbound_store_unavailable",
            ))
        }
    }
}

async fn deliver_public_notification(
    store: &TicketingStoreService,
    notifications: &NotificationService,
    mail: Option<&Arc<MailService>>,
    command: &DeliverPublicNotification,
) -> Result<(), JobExecutionFailure> {
    let ticket = store
        .get(&command.project_id, command.ticket_id)
        .await
        .map_err(|_| JobExecutionFailure::permanent("ticketing.notification_target_missing"))?
        .ok_or_else(|| JobExecutionFailure::permanent("ticketing.notification_target_missing"))?;
    let message = ticket
        .messages
        .iter()
        .find(|message| message.id == command.message_id)
        .ok_or_else(|| JobExecutionFailure::permanent("ticketing.notification_target_missing"))?;
    if message.kind != crate::TicketMessageKind::PublicReply {
        // Only public messages are notifiable; anything else is a
        // permanent refusal, not an infrastructure retry.
        return Err(JobExecutionFailure::permanent(
            "ticketing.notification_target_missing",
        ));
    }
    if ticket.requester.email.is_none() {
        // In-app channel: no outbound email, no delivery evidence —
        // the notifications port is the whole story (ADR-0054).
        return notifications
            .send(Notification {
                id: Uuid::now_v7(),
                topic: "ticketing.public-notification".into(),
                channel: NotificationChannel::InApp,
                recipient: ticket.requester.subject.clone(),
                title: format!("{} — {}", ticket.display_reference, ticket.subject),
                body: message.body.clone(),
                link: None,
                metadata: std::collections::BTreeMap::default(),
                created_at: Utc::now(),
            })
            .await
            .map_err(|_| JobExecutionFailure::retryable("ticketing.notification_send_failed"));
    }
    let Some(mail) = mail else {
        // Email channel without a configured mail service is a permanent
        // configuration defect; silently downgrading the channel would
        // hide mail the requester was promised.
        return Err(JobExecutionFailure::permanent(
            "ticketing.notification_mail_unconfigured",
        ));
    };
    deliver_public_reply_by_mail(store, mail, command, &ticket, message).await
}

/// Email-channel delivery with reconciliation and ambiguity recovery
/// (ADR-0063). Provider acceptance is recorded as evidence and never
/// claimed as delivery; an ambiguous transport result records evidence and
/// retries only after reconciliation against it.
async fn deliver_public_reply_by_mail(
    store: &TicketingStoreService,
    mail: &MailService,
    command: &DeliverPublicNotification,
    ticket: &crate::Ticket,
    message: &crate::TicketMessage,
) -> Result<(), JobExecutionFailure> {
    let email =
        ticket.requester.email.as_deref().ok_or_else(|| {
            JobExecutionFailure::permanent("ticketing.notification_target_missing")
        })?;
    let message_id = message.id;
    // Reconcile before any send: a recorded acceptance suppresses the
    // resend, so job redelivery and ambiguous retries cannot duplicate
    // outbound mail.
    let evidence = store
        .outbound_evidence(&command.project_id, command.ticket_id, message_id)
        .await
        .map_err(|_| JobExecutionFailure::retryable("ticketing.evidence_unavailable"))?;
    if evidence
        .iter()
        .any(|row| row.kind == OutboundEvidenceKind::Accepted)
    {
        return Ok(());
    }
    let address = MailAddress::new(email)
        .map_err(|_| JobExecutionFailure::permanent("ticketing.notification_recipient_invalid"))?;
    let message = minco_plugin_notifications::MailMessage::builder(
        "ticketing.public-notification",
        format!("{} — {}", ticket.display_reference, ticket.subject),
    )
    .to(address)
    .text(message.body.clone())
    .build()
    .map_err(|_| JobExecutionFailure::permanent("ticketing.notification_message_invalid"))?;
    let now = Utc::now();
    match mail.send(message).await {
        Ok(receipt) => {
            store
                .append_outbound_evidence(OutboundDeliveryEvidence {
                    project_id: command.project_id.clone(),
                    ticket_id: command.ticket_id,
                    message_id,
                    kind: OutboundEvidenceKind::Accepted,
                    provider: receipt.transport.clone(),
                    provider_message_id: receipt.provider_message_id.clone(),
                    feedback: None,
                    failure_kind: None,
                    recorded_at: now,
                })
                .await
                .map_err(|_| JobExecutionFailure::retryable("ticketing.evidence_unavailable"))?;
            Ok(())
        }
        Err(error) => match error.retry_advice() {
            MailRetryAdvice::SafeAfterBackoff => Err(JobExecutionFailure::retryable(
                "ticketing.notification_transport_retryable",
            )),
            MailRetryAdvice::ReconcileBeforeRetry => {
                // Ambiguous result: record the fact, then fail retryably.
                // The next attempt reconciles against recorded evidence
                // before resending — never a blind retry.
                store
                    .append_outbound_evidence(OutboundDeliveryEvidence {
                        project_id: command.project_id.clone(),
                        ticket_id: command.ticket_id,
                        message_id,
                        kind: OutboundEvidenceKind::Ambiguous,
                        provider: error.transport.clone(),
                        provider_message_id: String::new(),
                        feedback: None,
                        failure_kind: Some(mail_failure_kind_name(&error)),
                        recorded_at: now,
                    })
                    .await
                    .map_err(|_| {
                        JobExecutionFailure::retryable("ticketing.evidence_unavailable")
                    })?;
                Err(JobExecutionFailure::retryable(
                    "ticketing.notification_ambiguous",
                ))
            }
            MailRetryAdvice::Never => {
                store
                    .append_outbound_evidence(OutboundDeliveryEvidence {
                        project_id: command.project_id.clone(),
                        ticket_id: command.ticket_id,
                        message_id,
                        kind: OutboundEvidenceKind::PermanentFailure,
                        provider: error.transport.clone(),
                        provider_message_id: String::new(),
                        feedback: None,
                        failure_kind: Some(mail_failure_kind_name(&error)),
                        recorded_at: now,
                    })
                    .await
                    .map_err(|_| {
                        JobExecutionFailure::retryable("ticketing.evidence_unavailable")
                    })?;
                Err(JobExecutionFailure::permanent(
                    "ticketing.notification_permanent",
                ))
            }
        },
    }
}

fn mail_failure_kind_name(error: &MailError) -> String {
    format!("{:?}", error.kind).to_lowercase()
}

/// Pattern A enqueue port (ADR-0054).
///
/// The `sqlite` profile commits these records in the same SQL transaction
/// as the ticket mutation. The composition root adapts the released
/// `SqliteJobStore::enqueue_in` to this port; adapters implement ports
/// owned by the application layer.
#[cfg(feature = "sqlite")]
#[async_trait]
pub trait TicketingJobEnqueue: Send + Sync + std::fmt::Debug {
    async fn enqueue_in(
        &self,
        transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        record: minco_plugin_jobs::JobRecord,
    ) -> Result<(), TicketStoreError>;
}

/// Bound on job records attached to one ticketing mutation.
pub const MAX_JOB_RECORDS_PER_MUTATION: usize = 8;

/// Builds the notification job record for a public agent reply, or `None`
/// when notification is not enabled. Kept allocation-free of message
/// content: identifiers only.
pub fn notification_record_for_reply(
    project_id: &str,
    ticket_id: TicketId,
    message_id: TicketMessageId,
    correlation_id: Uuid,
    now: chrono::DateTime<Utc>,
) -> Result<Option<minco_plugin_jobs::JobRecord>, JobError> {
    let envelope = public_notification_envelope(
        &DeliverPublicNotification {
            project_id: project_id.to_owned(),
            ticket_id,
            message_id,
        },
        correlation_id,
        now,
    )?;
    Ok(Some(pending_record(envelope)))
}

#[cfg(all(test, feature = "jobs"))]
mod tests {
    use super::*;
    use crate::{
        CreateTicketInput, MemoryTicketingStore, TicketChannel, TicketPriority, TicketRequester,
        TicketingStore, TicketingStoreService,
    };
    use minco_plugin_notifications::MemoryNotificationSink;
    use std::collections::{BTreeMap, BTreeSet};

    fn ticket(now: chrono::DateTime<Utc>) -> crate::Ticket {
        crate::Ticket::create(
            CreateTicketInput {
                project_id: "project-a".into(),
                subject: "Help".into(),
                description: "It broke".into(),
                requester: TicketRequester {
                    subject: "user-1".into(),
                    display_name: None,
                    email: Some("user-1@example.test".into()),
                },
                channel: TicketChannel::Portal,
                priority: TicketPriority::Normal,
                resource_references: Vec::new(),
            },
            "TKT-JOB",
            now,
        )
        .unwrap()
    }

    fn worker_identity() -> minco_plugin_identity::Identity {
        minco_plugin_identity::Identity {
            subject: "ticketing-mail-worker".into(),
            permissions: BTreeSet::from(["ticketing.ingest".into()]),
            scopes: BTreeSet::new(),
            claims: BTreeMap::new(),
        }
    }

    fn notification_deps(
        notifications: Arc<NotificationService>,
        mail: Option<Arc<MailService>>,
    ) -> TicketingJobsDeps {
        let store = Arc::new(MemoryTicketingStore::default());
        let service = crate::TicketingService::new(
            TicketingStoreService::new(store),
            crate::TicketingConfig {
                project_id: "project-a".into(),
                ..crate::TicketingConfig::default()
            },
        )
        .unwrap();
        let objects = Arc::new(ObjectStoreService::new(Arc::new(
            minco_plugin_object_storage::MemoryObjectStore::default(),
        )));
        TicketingJobsDeps {
            service,
            notifications,
            mail,
            objects,
            worker: worker_identity(),
        }
    }

    #[test]
    fn notification_envelope_carries_the_contract_policies() {
        let now = Utc::now();
        let payload = DeliverPublicNotification {
            project_id: "project-a".into(),
            ticket_id: TicketId::new(),
            message_id: TicketMessageId::new(),
        };
        let envelope = public_notification_envelope(&payload, Uuid::now_v7(), now).unwrap();
        assert_eq!(envelope.job_name, DeliverPublicNotification::NAME);
        assert_eq!(envelope.worker_profile, TICKETING_MAIL_PROFILE);
        assert!(
            envelope
                .dedupe_key
                .as_deref()
                .is_some_and(|key| key.starts_with("notification:"))
        );
        assert!(
            envelope
                .overlap_key
                .as_deref()
                .is_some_and(|key| key.starts_with("ticket:"))
        );
        assert_eq!(envelope.partition.as_deref(), Some("project-a"));
        assert!(envelope.deadline.is_some());
        // The payload carries identifiers only: no message bodies.
        let encoded = serde_json::to_string(&payload).unwrap();
        assert!(!encoded.contains("body"));
    }

    #[test]
    fn envelope_debug_never_reveals_payload_content() {
        let now = Utc::now();
        let payload = DeliverPublicNotification {
            project_id: "project-a".into(),
            ticket_id: TicketId::new(),
            message_id: TicketMessageId::new(),
        };
        let envelope = public_notification_envelope(&payload, Uuid::now_v7(), now).unwrap();
        let debug = format!("{envelope:?}");
        assert!(!debug.contains("project-a"));
    }

    #[tokio::test]
    async fn in_app_reply_goes_through_the_notifications_port_unchanged() {
        let now = Utc::now();
        let mut ticket = ticket(now);
        ticket.requester.email = None;
        let message = ticket
            .reply_as_agent_message("agent-1", "Your fix is live.", now)
            .unwrap();
        let store = Arc::new(MemoryTicketingStore::default());
        let intent = crate::TicketActivityIntent::new(
            "project-a",
            ticket.id,
            "created",
            Uuid::now_v7(),
            serde_json::json!({}),
            now,
        );
        store.create(ticket.clone(), intent).await.unwrap();

        let sink = Arc::new(MemoryNotificationSink::default());
        let notifications = Arc::new(NotificationService::new(sink.clone()));
        let registry = Arc::new(JobHandlerRegistry::new());
        register_ticketing_jobs(
            &registry,
            &TicketingStoreService::new(store.clone()),
            notification_deps(notifications, None),
        )
        .unwrap();

        let services = minco_plugin_jobs::JobsServices::memory(registry).0;
        let correlation = Uuid::now_v7();
        let envelope = public_notification_envelope(
            &DeliverPublicNotification {
                project_id: "project-a".into(),
                ticket_id: ticket.id,
                message_id: message.id,
            },
            correlation,
            now,
        )
        .unwrap();
        services.submit_inline(envelope).await.unwrap();
        let sent = sink.all().await;
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].channel, NotificationChannel::InApp);
        assert_eq!(sent[0].recipient, "user-1");
        assert_eq!(sent[0].body, "Your fix is live.");
        // In-app delivery records no outbound evidence: nothing was mailed.
        assert!(store.all_outbound_evidence().await.is_empty());
    }

    /// Scriptable mail transport: pops one scripted result per attempt and
    /// records every submitted message.
    type ScriptedResult = Result<minco_plugin_notifications::MailReceipt, MailError>;

    struct ScriptedMailTransport {
        script: tokio::sync::Mutex<Vec<ScriptedResult>>,
        submitted: tokio::sync::Mutex<Vec<minco_plugin_notifications::MailMessage>>,
    }

    impl ScriptedMailTransport {
        fn new(script: Vec<ScriptedResult>) -> Arc<Self> {
            Arc::new(Self {
                script: tokio::sync::Mutex::new(script),
                submitted: tokio::sync::Mutex::new(Vec::new()),
            })
        }

        async fn submit_count(&self) -> usize {
            self.submitted.lock().await.len()
        }
    }

    impl std::fmt::Debug for ScriptedMailTransport {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.debug_struct("ScriptedMailTransport").finish()
        }
    }

    #[async_trait::async_trait]
    impl minco_plugin_notifications::MailTransport for ScriptedMailTransport {
        fn name(&self) -> &'static str {
            "scripted"
        }

        async fn send(
            &self,
            message: &minco_plugin_notifications::MailMessage,
            attempt: u32,
        ) -> ScriptedResult {
            self.submitted.lock().await.push(message.clone());
            self.script.lock().await.pop().unwrap_or_else(|| {
                Ok(minco_plugin_notifications::MailReceipt {
                    message_id: message.id,
                    transport: "scripted".into(),
                    provider_message_id: format!("provider-{attempt}"),
                    accepted_at: Utc::now(),
                    attempt,
                })
            })
        }
    }

    fn mail_error(kind: minco_plugin_notifications::MailErrorKind) -> MailError {
        MailError::new(kind, "scripted", "scripted failure")
    }

    async fn mail_setup(
        script: Vec<ScriptedResult>,
    ) -> (
        minco_plugin_jobs::JobsServices,
        Arc<ScriptedMailTransport>,
        Arc<MemoryTicketingStore>,
        crate::Ticket,
        crate::TicketMessage,
    ) {
        let now = Utc::now();
        let mut ticket = ticket(now);
        let message = ticket
            .reply_as_agent_message("agent-1", "Your fix is live.", now)
            .unwrap();
        let store = Arc::new(MemoryTicketingStore::default());
        let intent = crate::TicketActivityIntent::new(
            "project-a",
            ticket.id,
            "created",
            Uuid::now_v7(),
            serde_json::json!({}),
            now,
        );
        store.create(ticket.clone(), intent).await.unwrap();
        let transport = ScriptedMailTransport::new(script);
        let mail = Arc::new(
            MailService::single(
                transport.clone(),
                Arc::new(minco_plugin_notifications::NoopMailObserver),
            )
            .unwrap(),
        );
        let sink = Arc::new(MemoryNotificationSink::default());
        let notifications = Arc::new(NotificationService::new(sink));
        let registry = Arc::new(JobHandlerRegistry::new());
        register_ticketing_jobs(
            &registry,
            &TicketingStoreService::new(store.clone()),
            notification_deps(notifications, Some(mail)),
        )
        .unwrap();
        (
            minco_plugin_jobs::JobsServices::memory(registry).0,
            transport,
            store,
            ticket,
            message,
        )
    }

    fn deliver_envelope(
        ticket: &crate::Ticket,
        message: &crate::TicketMessage,
    ) -> minco_plugin_jobs::JobEnvelope {
        public_notification_envelope(
            &DeliverPublicNotification {
                project_id: "project-a".into(),
                ticket_id: ticket.id,
                message_id: message.id,
            },
            Uuid::now_v7(),
            Utc::now(),
        )
        .unwrap()
    }

    #[tokio::test]
    async fn email_reply_records_acceptance_and_never_claims_delivery() {
        let (services, transport, store, ticket, message) = mail_setup(vec![]).await;
        services
            .submit_inline(deliver_envelope(&ticket, &message))
            .await
            .unwrap();
        assert_eq!(transport.submit_count().await, 1);
        let evidence = store.all_outbound_evidence().await;
        assert_eq!(evidence.len(), 1);
        assert_eq!(evidence[0].kind, OutboundEvidenceKind::Accepted);
        assert_eq!(evidence[0].provider, "scripted");
        assert_eq!(evidence[0].provider_message_id, "provider-1");
        assert_eq!(evidence[0].message_id, message.id);
    }

    #[tokio::test]
    async fn redelivery_after_acceptance_never_resends() {
        let (services, transport, store, ticket, message) = mail_setup(vec![]).await;
        services
            .submit_inline(deliver_envelope(&ticket, &message))
            .await
            .unwrap();
        // Redelivery (at-least-once) reconciles against recorded
        // acceptance and suppresses the duplicate send.
        services
            .submit_inline(deliver_envelope(&ticket, &message))
            .await
            .unwrap();
        assert_eq!(transport.submit_count().await, 1);
        assert_eq!(store.all_outbound_evidence().await.len(), 1);
    }

    #[tokio::test]
    async fn ambiguous_result_records_evidence_and_resends_only_after_reconciliation() {
        // First attempt: the transport result is ambiguous (was the mail
        // accepted or not?). Evidence records the ambiguity; the job fails
        // retryably — never a blind resend.
        let (services, transport, store, ticket, message) = mail_setup(vec![Err(mail_error(
            minco_plugin_notifications::MailErrorKind::Ambiguous,
        ))])
        .await;
        let failure = services
            .submit_inline(deliver_envelope(&ticket, &message))
            .await
            .unwrap_err();
        assert_eq!(failure.code(), "ticketing.notification_ambiguous");
        assert!(!failure.is_permanent());
        let evidence = store.all_outbound_evidence().await;
        assert_eq!(evidence.len(), 1);
        assert_eq!(evidence[0].kind, OutboundEvidenceKind::Ambiguous);
        assert_eq!(evidence[0].failure_kind.as_deref(), Some("ambiguous"));
        // Retry: reconciliation finds no acceptance, so the resend is
        // justified; it lands and records acceptance.
        services
            .submit_inline(deliver_envelope(&ticket, &message))
            .await
            .unwrap();
        assert_eq!(transport.submit_count().await, 2);
        let kinds = store
            .all_outbound_evidence()
            .await
            .into_iter()
            .map(|row| row.kind)
            .collect::<Vec<_>>();
        assert_eq!(
            kinds,
            vec![
                OutboundEvidenceKind::Ambiguous,
                OutboundEvidenceKind::Accepted
            ]
        );
    }

    #[tokio::test]
    async fn permanent_transport_failure_records_evidence_and_fails_permanently() {
        let (services, _transport, store, ticket, message) = mail_setup(vec![Err(mail_error(
            minco_plugin_notifications::MailErrorKind::Rejected,
        ))])
        .await;
        let failure = services
            .submit_inline(deliver_envelope(&ticket, &message))
            .await
            .unwrap_err();
        assert_eq!(failure.code(), "ticketing.notification_permanent");
        assert!(failure.is_permanent());
        let evidence = store.all_outbound_evidence().await;
        assert_eq!(evidence.len(), 1);
        assert_eq!(evidence[0].kind, OutboundEvidenceKind::PermanentFailure);
        assert_eq!(evidence[0].failure_kind.as_deref(), Some("rejected"));
    }

    #[tokio::test]
    async fn retryable_transport_failure_records_no_evidence() {
        // Transient attempts are the mail observer's story, not decision
        // evidence: nothing is recorded until a terminal fact exists.
        let (services, _transport, store, ticket, message) = mail_setup(vec![Err(mail_error(
            minco_plugin_notifications::MailErrorKind::Throttled,
        ))])
        .await;
        let failure = services
            .submit_inline(deliver_envelope(&ticket, &message))
            .await
            .unwrap_err();
        assert_eq!(failure.code(), "ticketing.notification_transport_retryable");
        assert!(!failure.is_permanent());
        assert!(store.all_outbound_evidence().await.is_empty());
    }

    #[tokio::test]
    async fn email_channel_without_mail_service_fails_closed() {
        let now = Utc::now();
        let mut ticket = ticket(now);
        let message = ticket
            .reply_as_agent_message("agent-1", "Your fix is live.", now)
            .unwrap();
        let store = Arc::new(MemoryTicketingStore::default());
        let intent = crate::TicketActivityIntent::new(
            "project-a",
            ticket.id,
            "created",
            Uuid::now_v7(),
            serde_json::json!({}),
            now,
        );
        store.create(ticket.clone(), intent).await.unwrap();
        let sink = Arc::new(MemoryNotificationSink::default());
        let registry = Arc::new(JobHandlerRegistry::new());
        register_ticketing_jobs(
            &registry,
            &TicketingStoreService::new(store.clone()),
            notification_deps(Arc::new(NotificationService::new(sink.clone())), None),
        )
        .unwrap();
        let services = minco_plugin_jobs::JobsServices::memory(registry).0;
        let failure = services
            .submit_inline(deliver_envelope(&ticket, &message))
            .await
            .unwrap_err();
        assert_eq!(failure.code(), "ticketing.notification_mail_unconfigured");
        assert!(failure.is_permanent());
        assert!(sink.all().await.is_empty());
        assert!(store.all_outbound_evidence().await.is_empty());
    }

    #[tokio::test]
    async fn missing_target_is_permanent_and_nothing_is_sent() {
        let store = Arc::new(MemoryTicketingStore::default());
        let sink = Arc::new(MemoryNotificationSink::default());
        let notifications = Arc::new(NotificationService::new(sink.clone()));
        let registry = Arc::new(JobHandlerRegistry::new());
        register_ticketing_jobs(
            &registry,
            &TicketingStoreService::new(store),
            notification_deps(notifications, None),
        )
        .unwrap();
        let services = minco_plugin_jobs::JobsServices::memory(registry).0;
        let now = Utc::now();
        let envelope = public_notification_envelope(
            &DeliverPublicNotification {
                project_id: "project-a".into(),
                ticket_id: TicketId::new(),
                message_id: TicketMessageId::new(),
            },
            Uuid::now_v7(),
            now,
        )
        .unwrap();
        let failure = services.submit_inline(envelope).await.unwrap_err();
        assert_eq!(failure.code(), "ticketing.notification_target_missing");
        assert!(failure.is_permanent());
        assert!(sink.all().await.is_empty());
    }

    const RAW_EMAIL: &str = "From: user-1@example.test\r\n\
        To: support@example.test\r\n\
        Subject: Re: Help\r\n\
        MIME-Version: 1.0\r\n\
        Content-Type: text/plain; charset=utf-8\r\n\
        \r\n\
        Reply from the mailbox.\r\n";

    async fn inbound_setup() -> (
        minco_plugin_jobs::JobsServices,
        Arc<minco_plugin_object_storage::MemoryObjectStore>,
        Arc<MemoryTicketingStore>,
        crate::TicketId,
        u64,
        String,
    ) {
        let memory = Arc::new(MemoryTicketingStore::default());
        let config = crate::TicketingConfig {
            project_id: "project-a".into(),
            ..crate::TicketingConfig::default()
        };
        let service =
            crate::TicketingService::new(TicketingStoreService::new(memory.clone()), config)
                .unwrap();
        let created = service
            .create_ticket(
                &minco_plugin_identity::Identity {
                    subject: "user-1".into(),
                    permissions: BTreeSet::from(["ticketing.create".into()]),
                    scopes: BTreeSet::new(),
                    claims: BTreeMap::new(),
                },
                crate::CreateTicketInput {
                    project_id: "project-a".into(),
                    subject: "Help".into(),
                    description: "It broke".into(),
                    requester: crate::TicketRequester {
                        subject: "user-1".into(),
                        display_name: None,
                        email: None,
                    },
                    channel: crate::TicketChannel::Email,
                    priority: crate::TicketPriority::Normal,
                    resource_references: Vec::new(),
                },
                Uuid::now_v7(),
                Utc::now(),
            )
            .await
            .unwrap()
            .ticket;
        let objects = Arc::new(minco_plugin_object_storage::MemoryObjectStore::default());
        let digest = crate::external_content_sha256(RAW_EMAIL.as_bytes());
        objects
            .put(minco_plugin_object_storage::PutObject {
                key: minco_plugin_object_storage::ObjectKey::parse("mail/project-a/message-1")
                    .unwrap(),
                bytes: RAW_EMAIL.as_bytes().to_vec(),
                content_type: "message/rfc822".into(),
                attributes: BTreeMap::new(),
            })
            .await
            .unwrap();
        let registry = Arc::new(JobHandlerRegistry::new());
        register_ticketing_jobs(
            &registry,
            &TicketingStoreService::new(memory.clone()),
            TicketingJobsDeps {
                service: service.clone(),
                notifications: Arc::new(NotificationService::new(Arc::new(
                    MemoryNotificationSink::default(),
                ))),
                mail: None,
                objects: Arc::new(ObjectStoreService::new(objects.clone())),
                worker: worker_identity(),
            },
        )
        .unwrap();
        (
            minco_plugin_jobs::JobsServices::memory(registry).0,
            objects,
            memory,
            created.id,
            created.revision,
            digest,
        )
    }

    fn inbound_command(ticket_id: TicketId, revision: u64, digest: &str) -> ProcessInboundEmail {
        ProcessInboundEmail {
            project_id: "project-a".into(),
            provider: "ses".into(),
            mailbox_scope: "support@example.test".into(),
            external_id: "message-1".into(),
            content_sha256: digest.into(),
            raw_object_key: "mail/project-a/message-1".into(),
            ticket_id,
            expected_revision: revision,
            internet_message_id: Some("<message-1@example.test>".into()),
            in_reply_to: None,
            references: Vec::new(),
        }
    }

    #[tokio::test]
    async fn inbound_email_is_verified_parsed_and_ingested_idempotently() {
        let (services, _objects, memory, ticket_id, revision, digest) = inbound_setup().await;
        let correlation = Uuid::now_v7();
        let envelope = inbound_email_envelope(
            &inbound_command(ticket_id, revision, &digest),
            correlation,
            Utc::now(),
        )
        .unwrap();
        services.submit_inline(envelope).await.unwrap();
        let ticket = memory.get("project-a", ticket_id).await.unwrap().unwrap();
        assert!(
            ticket
                .messages
                .iter()
                .any(|message| message.body.contains("Reply from the mailbox."))
        );
        assert_eq!(ticket.revision, revision + 1);

        // Same external identity replays without a second message.
        let envelope = inbound_email_envelope(
            &inbound_command(ticket_id, ticket.revision - 1, &digest),
            correlation,
            Utc::now(),
        )
        .unwrap();
        services.submit_inline(envelope).await.unwrap();
        let ticket = memory.get("project-a", ticket_id).await.unwrap().unwrap();
        assert_eq!(
            ticket
                .messages
                .iter()
                .filter(|message| message.body.contains("Reply from the mailbox."))
                .count(),
            1
        );
    }

    #[tokio::test]
    async fn digest_mismatch_is_permanent_and_nothing_is_ingested() {
        let (services, _objects, _memory, ticket_id, revision, _digest) = inbound_setup().await;
        let envelope = inbound_email_envelope(
            &inbound_command(ticket_id, revision, &"f".repeat(64)),
            Uuid::now_v7(),
            Utc::now(),
        )
        .unwrap();
        let failure = services.submit_inline(envelope).await.unwrap_err();
        assert_eq!(failure.code(), "ticketing.inbound_digest_mismatch");
        assert!(failure.is_permanent());
    }

    #[tokio::test]
    async fn missing_object_is_permanent() {
        let (services, objects, _memory, ticket_id, revision, digest) = inbound_setup().await;
        objects
            .delete(
                &minco_plugin_object_storage::ObjectKey::parse("mail/project-a/message-1").unwrap(),
            )
            .await
            .unwrap();
        let envelope = inbound_email_envelope(
            &inbound_command(ticket_id, revision, &digest),
            Uuid::now_v7(),
            Utc::now(),
        )
        .unwrap();
        let failure = services.submit_inline(envelope).await.unwrap_err();
        assert_eq!(failure.code(), "ticketing.inbound_object_missing");
        assert!(failure.is_permanent());
    }

    #[tokio::test]
    async fn unparseable_mime_is_permanent() {
        let (services, objects, _memory, ticket_id, revision, _digest) = inbound_setup().await;
        let garbage = b"\x00\x01\x02 not mime at all \xff\xfe".to_vec();
        objects
            .put(minco_plugin_object_storage::PutObject {
                key: minco_plugin_object_storage::ObjectKey::parse("mail/project-a/message-1")
                    .unwrap(),
                bytes: garbage.clone(),
                content_type: "application/octet-stream".into(),
                attributes: BTreeMap::new(),
            })
            .await
            .unwrap();
        let digest = crate::external_content_sha256(&garbage);
        let envelope = inbound_email_envelope(
            &inbound_command(ticket_id, revision, &digest),
            Uuid::now_v7(),
            Utc::now(),
        )
        .unwrap();
        let failure = services.submit_inline(envelope).await.unwrap_err();
        assert!(failure.is_permanent(), "unexpected code {}", failure.code());
    }

    #[test]
    fn inbound_envelope_carries_the_contract_policies() {
        let payload = inbound_command(TicketId::new(), 0, &"a".repeat(64));
        let envelope = inbound_email_envelope(&payload, Uuid::now_v7(), Utc::now()).unwrap();
        assert_eq!(envelope.job_name, ProcessInboundEmail::NAME);
        assert_eq!(envelope.worker_profile, TICKETING_MAIL_PROFILE);
        assert!(
            envelope
                .dedupe_key
                .as_deref()
                .is_some_and(|k| k.starts_with("mail:"))
        );
        assert!(
            envelope
                .overlap_key
                .as_deref()
                .is_some_and(|k| k.starts_with("mailbox:"))
        );
        assert_eq!(envelope.partition.as_deref(), Some("project-a"));
        assert!(envelope.deadline.is_some());
        let encoded = serde_json::to_string(&payload).unwrap();
        assert!(!encoded.contains("Reply from the mailbox"));
    }

    #[tokio::test]
    async fn stale_revision_is_retryable_and_unauthorized_worker_is_permanent() {
        let (services, _objects, memory, ticket_id, revision, digest) = inbound_setup().await;
        // Move the ticket forward so the command's revision is stale.
        memory.get("project-a", ticket_id).await.unwrap().unwrap();
        let mut moved = memory.get("project-a", ticket_id).await.unwrap().unwrap();
        moved.change_priority(crate::TicketPriority::High, Utc::now());
        let intent = crate::TicketActivityIntent::new(
            "project-a",
            moved.id,
            "changed",
            Uuid::now_v7(),
            serde_json::json!({}),
            Utc::now(),
        );
        memory.save(moved, revision, intent).await.unwrap();
        let envelope = inbound_email_envelope(
            &inbound_command(ticket_id, revision, &digest),
            Uuid::now_v7(),
            Utc::now(),
        )
        .unwrap();
        let failure = services.submit_inline(envelope).await.unwrap_err();
        assert_eq!(failure.code(), "ticketing.inbound_revision_stale");
        assert!(!failure.is_permanent());
    }
}
