use aws_lambda_events::event::sqs::{SqsEvent, SqsMessage};
use minco_aws_worker::{
    FakeMessageHandler, MessageHandler, WorkerConfig, WorkerFailure, WorkerMessage,
    process_sqs_event,
};
use std::{collections::BTreeMap, sync::Arc};

fn message(id: &str, body: &str) -> WorkerMessage {
    WorkerMessage {
        message_id: id.to_owned(),
        body: body.to_owned(),
        attributes: BTreeMap::from([("authorization".to_owned(), "secret-value".to_owned())]),
        message_group_id: Some("private-group".to_owned()),
    }
}

fn record(id: &str) -> SqsMessage {
    let mut record = SqsMessage::default();
    record.message_id = Some(id.to_owned());
    record.body = Some(format!("private body for {id}"));
    record
}

#[tokio::test]
async fn fake_handler_records_attempts_and_consumes_one_shot_failures() {
    let handler = FakeMessageHandler::default();
    handler
        .fail_next("message-2", WorkerFailure::new("retryable"))
        .await;

    handler
        .handle(message("message-1", "first secret"))
        .await
        .unwrap();
    let failure = handler
        .handle(message("message-2", "second secret"))
        .await
        .unwrap_err();
    assert_eq!(failure.code(), "retryable");
    handler
        .handle(message("message-2", "second secret"))
        .await
        .unwrap();

    let attempts = handler.attempts().await;
    assert_eq!(
        attempts
            .iter()
            .map(|attempt| attempt.message_id.as_str())
            .collect::<Vec<_>>(),
        ["message-1", "message-2", "message-2"]
    );

    let debug = format!("{handler:?}");
    for secret in [
        "first secret",
        "second secret",
        "secret-value",
        "private-group",
    ] {
        assert!(!debug.contains(secret));
    }
}

#[tokio::test]
async fn fake_handler_drives_the_real_partial_batch_contract() {
    let handler = Arc::new(FakeMessageHandler::default());
    handler
        .fail_next("message-2", WorkerFailure::new("retryable"))
        .await;
    let mut event = SqsEvent::default();
    event.records = vec![
        record("message-1"),
        record("message-2"),
        record("message-3"),
    ];

    let response = process_sqs_event(
        event,
        Arc::clone(&handler),
        WorkerConfig {
            max_batch_size: 3,
            max_message_bytes: 128,
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
        ["message-2"]
    );
}
