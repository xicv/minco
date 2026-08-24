//! S3 `ObjectCreated` → ticketing inbound wake translation (ADR-0060).
//!
//! One bounded notification record becomes exactly one
//! [`TicketingService::wake_inbound_email`] call. The queue message is
//! delivery, never truth: classified failures return stable worker codes
//! and SQS redelivery decides retry.

use crate::{MessageHandler, WorkerFailure, WorkerMessage};
use chrono::{DateTime, Utc};
use minco_plugin_ticketing::TicketingService;
use serde::Deserialize;
use std::sync::Arc;
use uuid::Uuid;

/// Maximum accepted notification body; S3 records are small by design.
pub const MAX_WAKE_EVENT_BYTES: usize = 64 * 1024;
/// Maximum records per message body before failing closed.
pub const MAX_WAKE_RECORDS: usize = 10;

/// Bounded parse of one S3 notification record (ADR-0060). Unknown
/// fields are rejected; nothing beyond these bounded identifiers is
/// ever accepted from the queue.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TicketingMailWakeEvent {
    pub bucket: String,
    pub key: String,
    pub event_time: String,
    pub sequencer: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ses_receipt_id: Option<String>,
}

/// Explicit worker configuration: the ticketing service and the fixed
/// mailbox scope this worker serves. No credentials, no identity
/// assertion, no provider state.
#[derive(Clone)]
pub struct TicketingMailWakeHandler {
    service: Arc<TicketingService>,
    mailbox_scope: String,
}

impl TicketingMailWakeHandler {
    #[must_use]
    pub fn new(service: Arc<TicketingService>, mailbox_scope: impl Into<String>) -> Self {
        Self {
            service,
            mailbox_scope: mailbox_scope.into(),
        }
    }

    /// Bounded external identity for the durable dedupe key: the SES
    /// receipt id when the notification carries one, otherwise a digest
    /// of the bucket and key — never message content.
    fn external_id(event: &TicketingMailWakeEvent) -> String {
        use sha2::{Digest, Sha256};
        if let Some(receipt) = event
            .ses_receipt_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            return receipt.to_owned();
        }
        let mut hasher = Sha256::new();
        hasher.update(event.bucket.as_bytes());
        hasher.update(event.key.as_bytes());
        format!("s3-{}", hex::encode(hasher.finalize()))
    }
}

impl std::fmt::Debug for TicketingMailWakeHandler {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TicketingMailWakeHandler")
            .field("mailbox_scope", &self.mailbox_scope)
            .finish_non_exhaustive()
    }
}

#[async_trait::async_trait]
impl MessageHandler for TicketingMailWakeHandler {
    async fn handle(&self, message: WorkerMessage) -> Result<(), WorkerFailure> {
        if message.body.len() > MAX_WAKE_EVENT_BYTES {
            return Err(WorkerFailure::new("ticketing.wake_body_too_large"));
        }
        let events: Vec<TicketingMailWakeEvent> = serde_json::from_str(&message.body)
            .map_err(|_| WorkerFailure::new("ticketing.wake_body_invalid"))?;
        if events.len() != 1 {
            return Err(WorkerFailure::new("ticketing.wake_record_count_invalid"));
        }
        // One queue message carries one notification record in practice;
        // extra records would silently drop, so only the first is
        // accepted and any excess fails the count check above.
        let event = &events[0];
        if !is_bounded(&event.bucket, 128) || !is_bounded(&event.key, 1024) {
            return Err(WorkerFailure::new("ticketing.wake_field_invalid"));
        }
        let arrived_at: DateTime<Utc> = event
            .event_time
            .parse()
            .map_err(|_| WorkerFailure::new("ticketing.wake_time_invalid"))?;
        let failure_code = match self
            .service
            .wake_inbound_email(
                "ses",
                &self.mailbox_scope,
                &Self::external_id(event),
                &event.key,
                Uuid::new_v4(),
                arrived_at,
            )
            .await
        {
            Ok(_) => return Ok(()),
            Err(minco_plugin_ticketing::TicketingServiceError::InboundObjectMissing) => {
                "ticketing.inbound_object_missing"
            }
            Err(minco_plugin_ticketing::TicketingServiceError::InboundMimeInvalid) => {
                "ticketing.inbound_mime_invalid"
            }
            Err(minco_plugin_ticketing::TicketingServiceError::InboundThreadUnresolved) => {
                "ticketing.inbound_thread_unresolved"
            }
            Err(minco_plugin_ticketing::TicketingServiceError::ObjectsUnavailable) => {
                "ticketing.objects_unavailable"
            }
            Err(minco_plugin_ticketing::TicketingServiceError::JobsUnavailable) => {
                "ticketing.jobs_unavailable"
            }
            Err(_) => "ticketing.wake_failed",
        };
        Err(WorkerFailure::new(failure_code))
    }
}

fn is_bounded(value: &str, maximum: usize) -> bool {
    !value.is_empty() && value.chars().count() <= maximum && !value.chars().any(char::is_control)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FakeMessageHandler;
    use minco_plugin_ticketing::{
        CreateTicketInput, ExternalMessageIdentity, MemoryTicketingStore, TicketChannel,
        TicketPriority, TicketRequester, TicketingConfig, TicketingPortalServices,
        TicketingService, TicketingStoreService,
    };
    use std::collections::{BTreeMap, BTreeSet};

    fn message(body: &str) -> WorkerMessage {
        WorkerMessage {
            message_id: "m-1".into(),
            body: body.into(),
            attributes: BTreeMap::new(),
            message_group_id: None,
        }
    }

    #[allow(clippy::unused_async)]
    async fn seeded_service() -> (
        Arc<TicketingService>,
        Arc<minco_plugin_jobs::MemoryJobStore>,
        Arc<minco_plugin_object_storage::MemoryObjectStore>,
        minco_plugin_identity::Identity,
    ) {
        let memory = Arc::new(MemoryTicketingStore::default());
        let objects = Arc::new(minco_plugin_object_storage::MemoryObjectStore::default());
        let registry = Arc::new(minco_plugin_jobs::JobHandlerRegistry::new());
        let (jobs, store, _dispatcher) = minco_plugin_jobs::JobsServices::memory(registry);
        let service = Arc::new(
            TicketingService::new(
                TicketingStoreService::new(memory),
                TicketingConfig {
                    project_id: "project-a".into(),
                    ..TicketingConfig::default()
                },
            )
            .unwrap()
            .with_portal_services(TicketingPortalServices {
                jobs: Some(Arc::new(jobs)),
                objects: Some(Arc::new(
                    minco_plugin_object_storage::ObjectStoreService::new(objects.clone()),
                )),
                ..TicketingPortalServices::default()
            }),
        );
        let identity = minco_plugin_identity::Identity {
            subject: "ingress".into(),
            permissions: BTreeSet::from([
                "ticketing.ingest".into(),
                "ticketing.create".into(),
                "ticketing.manage".into(),
            ]),
            scopes: BTreeSet::new(),
            claims: BTreeMap::new(),
        };
        (service, store, objects, identity)
    }

    fn worker_message_body(key: &str) -> String {
        serde_json::json!([{
            "bucket": "minco-mail",
            "key": key,
            "event_time": "2026-08-25T10:00:00Z",
            "sequencer": "0062",
        }])
        .to_string()
    }

    #[tokio::test]
    async fn invalid_bodies_fail_closed_with_stable_codes() {
        let (service, _store, _objects, _identity) = seeded_service().await;
        let handler = TicketingMailWakeHandler::new(service, "support@example.test");
        assert_eq!(
            handler
                .handle(message("not json"))
                .await
                .unwrap_err()
                .code(),
            "ticketing.wake_body_invalid"
        );
        assert_eq!(
            handler.handle(message("[]")).await.unwrap_err().code(),
            "ticketing.wake_record_count_invalid"
        );
        let too_many = serde_json::to_string(
            &(0..=MAX_WAKE_RECORDS)
                .map(|index| {
                    serde_json::json!({
                        "bucket": "b", "key": format!("k{index}"),
                        "event_time": "2026-08-25T10:00:00Z", "sequencer": "1",
                    })
                })
                .collect::<Vec<_>>(),
        )
        .unwrap();
        assert_eq!(
            handler.handle(message(&too_many)).await.unwrap_err().code(),
            "ticketing.wake_record_count_invalid"
        );
        let unknown_field = serde_json::json!([{
            "bucket": "b", "key": "k",
            "event_time": "2026-08-25T10:00:00Z", "sequencer": "1",
            "injected": "value",
        }])
        .to_string();
        assert_eq!(
            handler
                .handle(message(&unknown_field))
                .await
                .unwrap_err()
                .code(),
            "ticketing.wake_body_invalid"
        );
        let bad_time = serde_json::json!([{
            "bucket": "b", "key": "k", "event_time": "not-a-time", "sequencer": "1",
        }])
        .to_string();
        assert_eq!(
            handler.handle(message(&bad_time)).await.unwrap_err().code(),
            "ticketing.wake_time_invalid"
        );
    }

    #[tokio::test]
    async fn missing_object_maps_to_the_classified_worker_code() {
        let (service, _store, objects, _identity) = seeded_service().await;
        assert!(objects.is_empty().await);
        let handler = TicketingMailWakeHandler::new(service, "support@example.test");
        // The object store is empty: the wake fails closed at the object
        // read, and the translation preserves the classification.
        assert_eq!(
            handler
                .handle(message(&worker_message_body("mail/project-a/missing")))
                .await
                .unwrap_err()
                .code(),
            "ticketing.inbound_object_missing"
        );
    }

    #[tokio::test]
    async fn a_valid_threaded_notification_submits_exactly_one_durable_job() {
        use minco_plugin_object_storage::{ObjectStore as _, PutObject};
        const REPLY_EMAIL: &str = "From: user-1@example.test\r\n\
            To: support@example.test\r\n\
            Message-ID: <reply-w@example.test>\r\n\
            In-Reply-To: <original-1@example.test>\r\n\
            MIME-Version: 1.0\r\n\
            Content-Type: text/plain; charset=utf-8\r\n\
            \r\n\
            A wake-delivered reply.\r\n";
        let (service, jobs_store, objects, identity) = seeded_service().await;

        // Seed: one ticket plus one previously ingested external message
        // carrying the threading anchor.
        let created = TicketingService::clone(&service)
            .create_ticket(
                &identity,
                CreateTicketInput {
                    project_id: "project-a".into(),
                    subject: "Help".into(),
                    description: "It broke".into(),
                    requester: TicketRequester {
                        subject: "user-1".into(),
                        display_name: None,
                        email: None,
                    },
                    channel: TicketChannel::Email,
                    priority: TicketPriority::Normal,
                    resource_references: Vec::new(),
                },
                Uuid::new_v4(),
                Utc::now(),
            )
            .await
            .unwrap()
            .ticket;
        TicketingService::clone(&service)
            .ingest_external_message(
                &identity,
                ExternalMessageIdentity {
                    project_id: "project-a".into(),
                    provider: "ses".into(),
                    mailbox_scope: "support@example.test".into(),
                    external_id: "original-1".into(),
                    content_sha256: "a".repeat(64),
                    raw_message_object_key: None,
                    internet_message_id: Some("<original-1@example.test>".into()),
                    in_reply_to: None,
                    references: Vec::new(),
                },
                created.id,
                "Original external reply".into(),
                created.revision,
                Uuid::new_v4(),
                Utc::now(),
            )
            .await
            .unwrap();

        objects
            .put(PutObject {
                key: minco_plugin_object_storage::ObjectKey::parse("mail/project-a/reply-w")
                    .unwrap(),
                bytes: REPLY_EMAIL.as_bytes().to_vec(),
                content_type: "message/rfc822".into(),
                attributes: BTreeMap::new(),
            })
            .await
            .unwrap();

        let handler = TicketingMailWakeHandler::new(Arc::clone(&service), "support@example.test");
        handler
            .handle(message(&worker_message_body("mail/project-a/reply-w")))
            .await
            .unwrap();
        let records = jobs_store.records();
        assert_eq!(records.len(), 1, "exactly one durable job");
        assert_eq!(
            records[0].envelope.job_name,
            "ticketing.process-inbound-email"
        );

        // Redelivery of the same record dedupes to the same job.
        handler
            .handle(message(&worker_message_body("mail/project-a/reply-w")))
            .await
            .unwrap();
        assert_eq!(jobs_store.records().len(), 1);
        let _ = FakeMessageHandler::default();
    }

    #[test]
    fn external_id_prefers_the_ses_receipt_and_digests_otherwise() {
        let with_receipt = TicketingMailWakeEvent {
            bucket: "b".into(),
            key: "k".into(),
            event_time: "2026-08-25T10:00:00Z".into(),
            sequencer: "1".into(),
            ses_receipt_id: Some("receipt-1".into()),
        };
        assert_eq!(
            TicketingMailWakeHandler::external_id(&with_receipt),
            "receipt-1"
        );
        let without = TicketingMailWakeEvent {
            ses_receipt_id: None,
            ..with_receipt
        };
        let derived = TicketingMailWakeHandler::external_id(&without);
        assert!(derived.starts_with("s3-") && derived.len() == "s3-".len() + 64);
    }
}
