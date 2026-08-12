//! Explicit SQS-triggered Lambda worker runtime with partial-batch responses.
#![forbid(unsafe_code)]

use async_trait::async_trait;
use aws_lambda_events::event::sqs::{BatchItemFailure, SqsBatchResponse, SqsEvent, SqsMessage};
use futures::{StreamExt, stream};
use lambda_runtime::{Error as LambdaError, service_fn};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fmt,
    sync::Arc,
};
use thiserror::Error;
use tokio::sync::Mutex;

const MAX_BATCH_SIZE: usize = 10_000;
const MAX_MESSAGE_BYTES: usize = 1024 * 1024;
const MESSAGE_GROUP_ID: &str = "MessageGroupId";

/// Per-invocation limits. Minco starts sequentially and never schedules work.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkerConfig {
    pub max_batch_size: usize,
    pub max_message_bytes: usize,
    pub max_concurrency: usize,
}

impl Default for WorkerConfig {
    fn default() -> Self {
        Self {
            max_batch_size: 10,
            max_message_bytes: 256 * 1024,
            max_concurrency: 1,
        }
    }
}

impl WorkerConfig {
    pub fn validate(self) -> Result<Self, WorkerError> {
        if !(1..=MAX_BATCH_SIZE).contains(&self.max_batch_size) {
            return Err(WorkerError::InvalidBatchLimit(self.max_batch_size));
        }
        if !(1..=MAX_MESSAGE_BYTES).contains(&self.max_message_bytes) {
            return Err(WorkerError::InvalidMessageLimit(self.max_message_bytes));
        }
        if !(1..=self.max_batch_size).contains(&self.max_concurrency) {
            return Err(WorkerError::InvalidConcurrency(self.max_concurrency));
        }
        Ok(self)
    }
}

/// Validated message passed to application code.
#[derive(Clone, PartialEq, Eq)]
pub struct WorkerMessage {
    pub message_id: String,
    pub body: String,
    pub attributes: BTreeMap<String, String>,
    pub message_group_id: Option<String>,
}

impl fmt::Debug for WorkerMessage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("WorkerMessage")
            .field("message_id", &self.message_id)
            .field("body_bytes", &self.body.len())
            .field(
                "attribute_names",
                &self.attributes.keys().collect::<Vec<_>>(),
            )
            .field("has_message_group_id", &self.message_group_id.is_some())
            .finish()
    }
}

/// Stable public-safe handler failure. Provider details stay in application logs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkerFailure {
    code: String,
}

impl WorkerFailure {
    #[must_use]
    pub fn new(code: impl Into<String>) -> Self {
        let code = code.into();
        let valid = !code.is_empty()
            && code.len() <= 64
            && code.bytes().all(|byte| {
                byte.is_ascii_lowercase()
                    || byte.is_ascii_digit()
                    || matches!(byte, b'-' | b'_' | b'.')
            });
        Self {
            code: if valid {
                code
            } else {
                "handler_failed".to_owned()
            },
        }
    }

    #[must_use]
    pub fn code(&self) -> &str {
        &self.code
    }
}

#[async_trait]
pub trait MessageHandler: Send + Sync + 'static {
    async fn handle(&self, message: WorkerMessage) -> Result<(), WorkerFailure>;
}

/// Deterministic SQS handler fake for application and worker contract tests.
///
/// Attempts are retained in call order. Failures are scoped to one message ID
/// and consumed once, so a test can prove retry behavior without sleeps,
/// provider state, or a generic mocking framework.
#[derive(Default)]
pub struct FakeMessageHandler {
    attempts: Mutex<Vec<WorkerMessage>>,
    failures: Mutex<BTreeMap<String, VecDeque<WorkerFailure>>>,
}

impl FakeMessageHandler {
    pub async fn fail_next(&self, message_id: impl Into<String>, failure: WorkerFailure) {
        self.failures
            .lock()
            .await
            .entry(message_id.into())
            .or_default()
            .push_back(failure);
    }

    pub async fn attempts(&self) -> Vec<WorkerMessage> {
        self.attempts.lock().await.clone()
    }

    pub async fn clear(&self) {
        self.attempts.lock().await.clear();
        self.failures.lock().await.clear();
    }
}

impl fmt::Debug for FakeMessageHandler {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("FakeMessageHandler")
            .finish_non_exhaustive()
    }
}

#[async_trait]
impl MessageHandler for FakeMessageHandler {
    async fn handle(&self, message: WorkerMessage) -> Result<(), WorkerFailure> {
        let message_id = message.message_id.clone();
        self.attempts.lock().await.push(message);
        let mut failures = self.failures.lock().await;
        let failure = failures.get_mut(&message_id).and_then(VecDeque::pop_front);
        if failures.get(&message_id).is_some_and(VecDeque::is_empty) {
            failures.remove(&message_id);
        }
        drop(failures);
        failure.map_or(Ok(()), Err)
    }
}

/// Runs the Lambda Runtime API until the environment shuts down.
///
/// The helper creates no timer, poller, event-source mapping, or detached task.
pub async fn run_sqs_worker<H>(handler: Arc<H>, config: WorkerConfig) -> Result<(), LambdaError>
where
    H: MessageHandler,
{
    let config = config
        .validate()
        .map_err(|error| -> LambdaError { Box::new(error) })?;
    lambda_runtime::run(service_fn(
        move |event: lambda_runtime::LambdaEvent<SqsEvent>| {
            let handler = Arc::clone(&handler);
            async move {
                process_sqs_event(event.payload, handler, config)
                    .await
                    .map_err(|error| -> LambdaError { Box::new(error) })
            }
        },
    ))
    .await
}

/// Processes one complete SQS invocation.
///
/// Failures retain input order. Missing or duplicate message identifiers fail
/// the invocation because Lambda cannot safely express their partial result.
pub async fn process_sqs_event<H>(
    event: SqsEvent,
    handler: Arc<H>,
    config: WorkerConfig,
) -> Result<SqsBatchResponse, WorkerError>
where
    H: MessageHandler,
{
    let config = config.validate()?;
    if event.records.len() > config.max_batch_size {
        return Err(WorkerError::BatchTooLarge {
            actual: event.records.len(),
            maximum: config.max_batch_size,
        });
    }
    validate_message_ids(&event.records)?;
    let fifo = detect_fifo(&event.records)?;
    let failures = if fifo {
        process_fifo(event.records, handler, config).await
    } else {
        process_standard(event.records, handler, config).await
    };
    let mut response = SqsBatchResponse::default();
    response.batch_item_failures = failures
        .into_iter()
        .map(|item_identifier| {
            let mut failure = BatchItemFailure::default();
            failure.item_identifier = item_identifier;
            failure
        })
        .collect();
    Ok(response)
}

fn validate_message_ids(records: &[SqsMessage]) -> Result<(), WorkerError> {
    let mut seen = BTreeSet::new();
    for (index, record) in records.iter().enumerate() {
        let Some(message_id) = record
            .message_id
            .as_deref()
            .map(str::trim)
            .filter(|message_id| !message_id.is_empty())
        else {
            return Err(WorkerError::MissingMessageId { index });
        };
        if !seen.insert(message_id) {
            return Err(WorkerError::DuplicateMessageId {
                message_id: message_id.to_owned(),
            });
        }
    }
    Ok(())
}

fn detect_fifo(records: &[SqsMessage]) -> Result<bool, WorkerError> {
    let fifo = records.iter().any(|record| {
        record.attributes.contains_key(MESSAGE_GROUP_ID)
            || record
                .event_source_arn
                .as_deref()
                .and_then(|arn| arn.rsplit(':').next())
                .and_then(|queue| queue.rsplit_once('.'))
                .is_some_and(|(_, extension)| extension.eq_ignore_ascii_case("fifo"))
    });
    if fifo
        && records
            .iter()
            .any(|record| !record.attributes.contains_key(MESSAGE_GROUP_ID))
    {
        return Err(WorkerError::IncompleteFifoMetadata);
    }
    Ok(fifo)
}

async fn process_fifo<H>(
    records: Vec<SqsMessage>,
    handler: Arc<H>,
    config: WorkerConfig,
) -> Vec<String>
where
    H: MessageHandler,
{
    let mut failures = Vec::new();
    let mut stopped = false;
    for record in records {
        let message_id = record.message_id.clone().expect("validated message ID");
        if stopped {
            failures.push(message_id);
            continue;
        }
        match worker_message(record, config.max_message_bytes) {
            Ok(message) => {
                if let Err(error) = handler.handle(message).await {
                    tracing::warn!(
                        message_id,
                        failure_code = error.code(),
                        "SQS handler rejected FIFO message"
                    );
                    failures.push(message_id);
                    stopped = true;
                }
            }
            Err(reason) => {
                tracing::warn!(message_id, reason, "invalid FIFO SQS message");
                failures.push(message_id);
                stopped = true;
            }
        }
    }
    failures
}

async fn process_standard<H>(
    records: Vec<SqsMessage>,
    handler: Arc<H>,
    config: WorkerConfig,
) -> Vec<String>
where
    H: MessageHandler,
{
    let results = stream::iter(records.into_iter().enumerate())
        .map(|(index, record)| {
            let handler = Arc::clone(&handler);
            async move {
                let message_id = record.message_id.clone().expect("validated message ID");
                let failed = match worker_message(record, config.max_message_bytes) {
                    Ok(message) => match handler.handle(message).await {
                        Ok(()) => false,
                        Err(error) => {
                            tracing::warn!(
                                message_id,
                                failure_code = error.code(),
                                "SQS handler rejected message"
                            );
                            true
                        }
                    },
                    Err(reason) => {
                        tracing::warn!(message_id, reason, "invalid SQS message");
                        true
                    }
                };
                (index, message_id, failed)
            }
        })
        .buffer_unordered(config.max_concurrency)
        .collect::<Vec<_>>()
        .await;
    let mut failures = results
        .into_iter()
        .filter(|(_, _, failed)| *failed)
        .map(|(index, message_id, _)| (index, message_id))
        .collect::<Vec<_>>();
    failures.sort_by_key(|(index, _)| *index);
    failures
        .into_iter()
        .map(|(_, message_id)| message_id)
        .collect()
}

fn worker_message(
    record: SqsMessage,
    max_message_bytes: usize,
) -> Result<WorkerMessage, &'static str> {
    let message_id = record.message_id.expect("validated message ID");
    let body = record
        .body
        .filter(|body| !body.is_empty())
        .ok_or("empty_body")?;
    if body.len() > max_message_bytes {
        return Err("body_too_large");
    }
    let attributes = record.attributes.into_iter().collect::<BTreeMap<_, _>>();
    let message_group_id = attributes.get(MESSAGE_GROUP_ID).cloned();
    Ok(WorkerMessage {
        message_id,
        body,
        attributes,
        message_group_id,
    })
}

#[cfg(feature = "events")]
pub async fn dispatch_outbox_once(
    events: &minco_plugin_events::EventServices,
    worker_id: &str,
    limit: usize,
    lease: std::time::Duration,
) -> Result<minco_plugin_events::DispatchReport, minco_plugin_events::EventError> {
    let lease = chrono::TimeDelta::from_std(lease)
        .map_err(|_| minco_plugin_events::EventError::InvalidClaim)?;
    events.dispatch_once(worker_id, limit, lease).await
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum WorkerError {
    #[error("SQS batch limit must be between 1 and {MAX_BATCH_SIZE}; found {0}")]
    InvalidBatchLimit(usize),
    #[error("SQS message limit must be between 1 and {MAX_MESSAGE_BYTES}; found {0}")]
    InvalidMessageLimit(usize),
    #[error("worker concurrency must be between 1 and the configured batch limit; found {0}")]
    InvalidConcurrency(usize),
    #[error("SQS batch has {actual} records, exceeding configured maximum {maximum}")]
    BatchTooLarge { actual: usize, maximum: usize },
    #[error("SQS record {index} has no usable message ID")]
    MissingMessageId { index: usize },
    #[error("SQS batch repeats message ID {message_id}")]
    DuplicateMessageId { message_id: String },
    #[error("FIFO SQS records must all provide MessageGroupId")]
    IncompleteFifoMetadata,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::sync::Mutex;

    #[derive(Debug, Default)]
    struct RecordingHandler {
        seen: Mutex<Vec<String>>,
        failures: BTreeSet<String>,
        active: AtomicUsize,
        maximum_active: AtomicUsize,
    }

    #[async_trait]
    impl MessageHandler for RecordingHandler {
        async fn handle(&self, message: WorkerMessage) -> Result<(), WorkerFailure> {
            let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
            self.maximum_active.fetch_max(active, Ordering::SeqCst);
            tokio::task::yield_now().await;
            self.seen.lock().await.push(message.message_id.clone());
            self.active.fetch_sub(1, Ordering::SeqCst);
            if self.failures.contains(&message.message_id) {
                Err(WorkerFailure::new("application_rejected"))
            } else {
                Ok(())
            }
        }
    }

    fn record(id: Option<&str>, body: Option<&str>) -> SqsMessage {
        let mut record = SqsMessage::default();
        record.message_id = id.map(str::to_owned);
        record.body = body.map(str::to_owned);
        record
    }

    fn event(records: Vec<SqsMessage>) -> SqsEvent {
        let mut event = SqsEvent::default();
        event.records = records;
        event
    }

    #[test]
    fn message_debug_redacts_payload_and_attribute_values() {
        let message = WorkerMessage {
            message_id: "message-1".to_owned(),
            body: "body-secret-value".to_owned(),
            attributes: BTreeMap::from([(
                "Authorization".to_owned(),
                "attribute-secret-value".to_owned(),
            )]),
            message_group_id: Some("group-secret-value".to_owned()),
        };
        let rendered = format!("{message:?}");
        assert!(rendered.contains("message-1"));
        assert!(rendered.contains("Authorization"));
        assert!(!rendered.contains("body-secret-value"));
        assert!(!rendered.contains("attribute-secret-value"));
        assert!(!rendered.contains("group-secret-value"));
    }

    #[tokio::test]
    async fn mixed_success_and_failure_returns_only_failed_ids_in_input_order() {
        let handler = Arc::new(RecordingHandler {
            failures: ["second".to_owned(), "fourth".to_owned()]
                .into_iter()
                .collect(),
            ..RecordingHandler::default()
        });
        let response = process_sqs_event(
            event(vec![
                record(Some("first"), Some("1")),
                record(Some("second"), Some("2")),
                record(Some("third"), Some("3")),
                record(Some("fourth"), Some("4")),
            ]),
            handler,
            WorkerConfig {
                max_batch_size: 10,
                max_message_bytes: 16,
                max_concurrency: 3,
            },
        )
        .await
        .unwrap();
        assert_eq!(
            response
                .batch_item_failures
                .into_iter()
                .map(|failure| failure.item_identifier)
                .collect::<Vec<_>>(),
            ["second", "fourth"]
        );
    }

    #[tokio::test]
    async fn missing_ids_fail_the_invocation() {
        let result = process_sqs_event(
            event(vec![record(None, Some("body"))]),
            Arc::new(RecordingHandler::default()),
            WorkerConfig::default(),
        )
        .await;
        assert_eq!(result, Err(WorkerError::MissingMessageId { index: 0 }));
    }

    #[tokio::test]
    async fn empty_and_oversized_bodies_are_partial_failures() {
        let response = process_sqs_event(
            event(vec![
                record(Some("empty"), None),
                record(Some("large"), Some("12345")),
                record(Some("valid"), Some("1234")),
            ]),
            Arc::new(RecordingHandler::default()),
            WorkerConfig {
                max_batch_size: 3,
                max_message_bytes: 4,
                max_concurrency: 1,
            },
        )
        .await
        .unwrap();
        assert_eq!(
            response
                .batch_item_failures
                .into_iter()
                .map(|failure| failure.item_identifier)
                .collect::<Vec<_>>(),
            ["empty", "large"]
        );
    }

    #[tokio::test]
    async fn configured_concurrency_is_bounded_and_fully_awaited() {
        let handler = Arc::new(RecordingHandler::default());
        let response = process_sqs_event(
            event(
                (0..8)
                    .map(|index| record(Some(&format!("m{index}")), Some("body")))
                    .collect(),
            ),
            Arc::clone(&handler),
            WorkerConfig {
                max_batch_size: 8,
                max_message_bytes: 16,
                max_concurrency: 2,
            },
        )
        .await
        .unwrap();
        assert!(response.batch_item_failures.is_empty());
        assert!(handler.maximum_active.load(Ordering::SeqCst) <= 2);
        assert_eq!(handler.seen.lock().await.len(), 8);
    }

    #[tokio::test]
    async fn fifo_stops_after_first_failure_and_fails_forward() {
        let handler = Arc::new(RecordingHandler {
            failures: std::iter::once("second".to_owned()).collect(),
            ..RecordingHandler::default()
        });
        let mut records = ["first", "second", "third"]
            .into_iter()
            .map(|id| record(Some(id), Some("body")))
            .collect::<Vec<_>>();
        for record in &mut records {
            record
                .attributes
                .insert(MESSAGE_GROUP_ID.to_owned(), "group-a".to_owned());
        }
        let response = process_sqs_event(
            event(records),
            Arc::clone(&handler),
            WorkerConfig::default(),
        )
        .await
        .unwrap();
        assert_eq!(*handler.seen.lock().await, ["first", "second"]);
        assert_eq!(
            response
                .batch_item_failures
                .into_iter()
                .map(|failure| failure.item_identifier)
                .collect::<Vec<_>>(),
            ["second", "third"]
        );
    }
}
