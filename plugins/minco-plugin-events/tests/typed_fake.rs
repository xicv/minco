use chrono::{DateTime, Utc};
use minco_plugin_events::{DomainEvent, EventError, EventPublisher, FakeEventPublisher};
use std::collections::BTreeMap;
use uuid::Uuid;

fn event() -> DomainEvent {
    DomainEvent {
        id: Uuid::nil(),
        event_type: "order.placed".to_owned(),
        aggregate_type: "order".to_owned(),
        aggregate_id: "order-1".to_owned(),
        correlation_id: Uuid::from_u128(1),
        occurred_at: DateTime::<Utc>::UNIX_EPOCH,
        payload: serde_json::json!({"private": "payload-secret"}),
        metadata: BTreeMap::from([("token".to_owned(), serde_json::json!("metadata-secret"))]),
    }
}

#[tokio::test]
async fn fake_publisher_records_attempts_and_consumes_one_shot_failures() {
    let publisher = FakeEventPublisher::default();
    publisher.fail_next("temporarily unavailable").await;

    let event = event();
    assert!(matches!(
        publisher.publish(&event).await,
        Err(EventError::Infrastructure(message)) if message == "temporarily unavailable"
    ));
    publisher.publish(&event).await.unwrap();

    let attempts = publisher.attempts().await;
    assert_eq!(
        attempts
            .iter()
            .map(|attempt| &attempt.event)
            .collect::<Vec<_>>(),
        [&event, &event]
    );
    let attempt_debug = format!("{:?}", attempts[0]);
    assert!(!attempt_debug.contains("payload-secret"));
    assert!(!attempt_debug.contains("metadata-secret"));
    let debug = format!("{publisher:?}");
    assert!(!debug.contains("payload-secret"));
    assert!(!debug.contains("metadata-secret"));
}
