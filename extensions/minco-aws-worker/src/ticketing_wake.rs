//! S3 `ObjectCreated` → ticketing inbound wake translation
//! (ADR-0060, ADR-0061).
//!
//! One bounded notification record from the real S3 `Records` envelope
//! becomes exactly one [`TicketingService::wake_inbound_email`] call.
//! The queue message is delivery, never truth: classified failures
//! return stable worker codes and SQS redelivery decides retry.

use crate::{MessageHandler, WorkerFailure, WorkerMessage};
use aws_lambda_events::event::s3::S3Event;
use chrono::Utc;
use minco_plugin_ticketing::TicketingService;
use std::sync::Arc;
use uuid::Uuid;

/// Maximum accepted notification body; S3 records are small by design.
pub const MAX_WAKE_EVENT_BYTES: usize = 64 * 1024;

/// The one wake-relevant view of a validated S3 notification record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TicketingMailWakeEvent {
    pub bucket: String,
    /// URL-decoded object key when S3 provided one, else the raw key.
    pub key: String,
    pub event_time: chrono::DateTime<Utc>,
    pub sequencer: String,
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

    /// Parse the real S3 `Records` envelope (ADR-0061) into exactly one
    /// wake event. Non-S3 sources, non-ObjectCreated events, zero or
    /// multiple records, and missing bounded fields fail closed with
    /// stable codes; nothing is guessed.
    fn parse_event(body: &str) -> Result<TicketingMailWakeEvent, WorkerFailure> {
        let event: S3Event = serde_json::from_str(body)
            .map_err(|_| WorkerFailure::new("ticketing.wake_body_invalid"))?;
        if event.records.len() != 1 {
            return Err(WorkerFailure::new("ticketing.wake_record_count_invalid"));
        }
        let record = &event.records[0];
        if record.event_source.as_deref() != Some("aws:s3") {
            return Err(WorkerFailure::new("ticketing.wake_source_invalid"));
        }
        if !record
            .event_name
            .as_deref()
            .is_some_and(|name| name.starts_with("ObjectCreated:"))
        {
            return Err(WorkerFailure::new("ticketing.wake_event_kind_invalid"));
        }
        let bucket = record
            .s3
            .bucket
            .name
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| WorkerFailure::new("ticketing.wake_field_invalid"))?;
        let raw_key = record
            .s3
            .object
            .key
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| WorkerFailure::new("ticketing.wake_field_invalid"))?;
        let key = record
            .s3
            .object
            .url_decoded_key
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(raw_key);
        let sequencer = record
            .s3
            .object
            .sequencer
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| WorkerFailure::new("ticketing.wake_field_invalid"))?;
        if !is_bounded(bucket, 128) || !is_bounded(key, 1024) || !is_bounded(sequencer, 64) {
            return Err(WorkerFailure::new("ticketing.wake_field_invalid"));
        }
        Ok(TicketingMailWakeEvent {
            bucket: bucket.to_owned(),
            key: key.to_owned(),
            event_time: record.event_time,
            sequencer: sequencer.to_owned(),
        })
    }

    /// Bounded external identity for the durable dedupe key: a digest of
    /// the bucket and key — never message content. SES receipt-id
    /// attribution through message attributes is a slice-3b concern.
    fn external_id(event: &TicketingMailWakeEvent) -> String {
        use sha2::{Digest, Sha256};
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
        let event = Self::parse_event(&message.body)?;
        let failure_code = match self
            .service
            .wake_inbound_email(
                "ses",
                &self.mailbox_scope,
                &Self::external_id(&event),
                &event.key,
                Uuid::new_v4(),
                event.event_time,
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

    /// Byte-accurate real S3 notification envelope for one `ObjectCreated`
    /// Put of the given key (percent-encoded as S3 delivers keys).
    /// One eventTime stamp per test process: the semantic fingerprint
    /// anchors on the arrival time, so a redelivery pair must share it,
    /// and it must move with the wall clock so the six-hour deadline
    /// window never expires under the test.
    fn envelope_event_time() -> String {
        use std::sync::OnceLock;
        static STAMP: OnceLock<String> = OnceLock::new();
        STAMP
            .get_or_init(|| chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
            .clone()
    }

    fn real_envelope(raw_key: &str, url_decoded_key: Option<&str>) -> String {
        let mut object = serde_json::json!({
            "key": raw_key,
            "sequencer": "0062FB4BD93640D5",
        });
        if let Some(decoded) = url_decoded_key {
            object["urlDecodedKey"] = serde_json::Value::String(decoded.into());
        }
        serde_json::json!({
            "Records": [{
                "eventVersion": "2.2",
                "eventSource": "aws:s3",
                "awsRegion": "us-east-1",
                "eventTime": envelope_event_time(),
                "eventName": "ObjectCreated:Put",
                "userIdentity": {"principalId": "AWS:SES"},
                "requestParameters": {"sourceIPAddress": "10.0.0.1"},
                "responseElements": {},
                "s3": {
                    "s3SchemaVersion": "1.0",
                    "configurationId": "ses-receiving-drop",
                    "bucket": {"name": "minco-mail"},
                    "object": object,
                },
            }],
        })
        .to_string()
    }

    fn full_record(key: &str) -> serde_json::Value {
        serde_json::json!({
            "eventSource": "aws:s3", "eventName": "ObjectCreated:Put",
            "eventTime": envelope_event_time(),
            "userIdentity": {"principalId": "AWS:SES"},
            "requestParameters": {"sourceIPAddress": "10.0.0.1"},
            "responseElements": {},
            "s3": {"bucket": {"name": "b"}, "object": {"key": key, "sequencer": "1"}},
        })
    }

    fn worker_message_body(key: &str) -> String {
        real_envelope(key, None)
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
        // Empty record list fails the count check.
        assert_eq!(
            handler
                .handle(message(r#"{"Records":[]}"#))
                .await
                .unwrap_err()
                .code(),
            "ticketing.wake_record_count_invalid"
        );
        // Two records would silently drop one; fail closed instead.
        let two = serde_json::json!({
            "Records": [full_record("k1"), full_record("k2")],
        })
        .to_string();
        assert_eq!(
            handler.handle(message(&two)).await.unwrap_err().code(),
            "ticketing.wake_record_count_invalid"
        );
        // Non-S3 source is rejected.
        let mut foreign_record = full_record("k");
        foreign_record["eventSource"] = serde_json::Value::String("aws:sns".into());
        let foreign = serde_json::json!({"Records": [foreign_record]}).to_string();
        assert_eq!(
            handler.handle(message(&foreign)).await.unwrap_err().code(),
            "ticketing.wake_source_invalid"
        );
        // Non-ObjectCreated events (e.g. ObjectRemoved) are rejected.
        let mut removed_record = full_record("k");
        removed_record["eventName"] = serde_json::Value::String("ObjectRemoved:Delete".into());
        let removed = serde_json::json!({"Records": [removed_record]}).to_string();
        assert_eq!(
            handler.handle(message(&removed)).await.unwrap_err().code(),
            "ticketing.wake_event_kind_invalid"
        );
        // Missing bucket / key / sequencer fail the field check.
        let mut missing_key_record = full_record("k");
        missing_key_record["s3"]["object"]
            .as_object_mut()
            .unwrap()
            .remove("key");
        let missing_key = serde_json::json!({"Records": [missing_key_record]}).to_string();
        assert_eq!(
            handler
                .handle(message(&missing_key))
                .await
                .unwrap_err()
                .code(),
            "ticketing.wake_field_invalid"
        );
        // Malformed eventTime fails envelope deserialization up front.
        let mut bad_time_record = full_record("k");
        bad_time_record["eventTime"] = serde_json::Value::String("not-a-time".into());
        let bad_time = serde_json::json!({"Records": [bad_time_record]}).to_string();
        assert_eq!(
            handler.handle(message(&bad_time)).await.unwrap_err().code(),
            "ticketing.wake_body_invalid"
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
                    ticket_type: minco_plugin_ticketing::TicketType::default(),
                    form_answers: Vec::new(),
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
    fn parse_event_is_url_decoded_key_aware() {
        // Percent-encoded key with a urlDecodedKey companion: the decoded
        // key wins.
        let decoded = TicketingMailWakeHandler::parse_event(&real_envelope(
            "mail/project-a/reply+w%40v1",
            Some("mail/project-a/reply+w@v1"),
        ))
        .unwrap();
        assert_eq!(decoded.key, "mail/project-a/reply+w@v1");
        assert_eq!(decoded.bucket, "minco-mail");
        assert_eq!(decoded.sequencer, "0062FB4BD93640D5");
        // Without urlDecodedKey the raw key is used, bounded as before.
        let raw =
            TicketingMailWakeHandler::parse_event(&real_envelope("mail/project-a/plain", None))
                .unwrap();
        assert_eq!(raw.key, "mail/project-a/plain");
    }

    #[test]
    fn external_id_digests_bucket_and_key() {
        let event = TicketingMailWakeEvent {
            bucket: "b".into(),
            key: "k".into(),
            event_time: Utc::now(),
            sequencer: "1".into(),
        };
        let derived = TicketingMailWakeHandler::external_id(&event);
        assert!(derived.starts_with("s3-") && derived.len() == "s3-".len() + 64);
    }
}
