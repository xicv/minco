//! Optional Ticketing-to-Jobs bridge (ADR-0054).
//!
//! One typed command ships in this stage: `ticketing.deliver-public-notification`
//! v1, with a real handler that delivers through the notifications plugin's
//! `NotificationService` port. The bridge owns no queue, lease, retry or
//! scheduling machinery — all of that is the released jobs plugin (ADR-0048).

#[cfg(feature = "sqlite")]
use crate::TicketStoreError;
use crate::{TicketId, TicketMessageId, TicketingStoreService};
#[cfg(feature = "sqlite")]
use async_trait::async_trait;
use chrono::{TimeDelta, Utc};
use minco_plugin_jobs::{
    Job, JobEnvelope, JobError, JobExecutionFailure, JobHandlerRegistry, JobOptions, RetryPolicy,
    pending_record,
};
use minco_plugin_notifications::{Notification, NotificationChannel, NotificationService};
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

pub const TICKETING_MAIL_PROFILE: &str = "ticketing-mail";
pub const NOTIFICATION_DEADLINE_SECONDS: i64 = 3600;

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

/// Register the ticketing handlers on the composition's registry. Static
/// and explicit: the composition root calls this before building
/// `JobsServices`; no runtime scanning, no plugin retro-fit.
pub fn register_ticketing_jobs(
    registry: &JobHandlerRegistry,
    store: TicketingStoreService,
    notifications: Arc<NotificationService>,
) -> Result<(), JobError> {
    registry.register_typed::<DeliverPublicNotification, _, _>(move |command, _context| {
        let store = store.clone();
        let notifications = notifications.clone();
        async move { deliver_public_notification(&store, &notifications, &command).await }
    })
}

async fn deliver_public_notification(
    store: &TicketingStoreService,
    notifications: &NotificationService,
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
    let recipient = ticket
        .requester
        .email
        .clone()
        .unwrap_or_else(|| ticket.requester.subject.clone());
    notifications
        .send(Notification {
            id: Uuid::now_v7(),
            topic: "ticketing.public-notification".into(),
            channel: if ticket.requester.email.is_some() {
                NotificationChannel::Email
            } else {
                NotificationChannel::InApp
            },
            recipient,
            title: format!("{} — {}", ticket.display_reference, ticket.subject),
            body: message.body.clone(),
            link: None,
            metadata: std::collections::BTreeMap::default(),
            created_at: Utc::now(),
        })
        .await
        .map_err(|_| JobExecutionFailure::retryable("ticketing.notification_send_failed"))
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

    fn identity() -> minco_plugin_identity::Identity {
        minco_plugin_identity::Identity {
            subject: "agent-1".into(),
            permissions: BTreeSet::from(["ticketing.reply".into()]),
            scopes: BTreeSet::new(),
            claims: BTreeMap::new(),
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
    async fn handler_delivers_the_public_message_through_the_notifications_port() {
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
        let notifications = Arc::new(NotificationService::new(sink.clone()));
        let registry = Arc::new(JobHandlerRegistry::new());
        register_ticketing_jobs(&registry, TicketingStoreService::new(store), notifications)
            .unwrap();

        // Execute through the released inline path: a real handler run,
        // not a mocked assertion.
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
        assert_eq!(sent[0].topic, "ticketing.public-notification");
        assert_eq!(sent[0].recipient, "user-1@example.test");
        assert_eq!(sent[0].body, "Your fix is live.");
        assert!(sent[0].title.contains("TKT-JOB"));
        let _ = identity();
    }

    #[tokio::test]
    async fn missing_target_is_permanent_and_nothing_is_sent() {
        let store = Arc::new(MemoryTicketingStore::default());
        let sink = Arc::new(MemoryNotificationSink::default());
        let notifications = Arc::new(NotificationService::new(sink.clone()));
        let registry = Arc::new(JobHandlerRegistry::new());
        register_ticketing_jobs(&registry, TicketingStoreService::new(store), notifications)
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
}
