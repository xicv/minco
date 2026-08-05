use minco_plugin_realtime::{MemoryRealtimePublisher, RealtimePublisher};
use minco_realtime_appsync_live_proof::publish_request;
use serde_json::json;
use std::sync::Arc;

#[tokio::test]
async fn valid_request_publishes_the_bounded_proof_envelope() {
    let publisher = Arc::new(MemoryRealtimePublisher::default());
    let result = publish_request(
        publisher.clone() as Arc<dyn RealtimePublisher>,
        json!({"channel": "subject-7/orders", "sequence": 2}),
    )
    .await
    .expect("publish");

    assert_eq!(result, json!({"published": true, "id": "live-2"}));
    let publications = publisher.published();
    assert_eq!(publications.len(), 1);
    assert_eq!(publications[0].channel.as_str(), "subject-7/orders");
    assert_eq!(publications[0].envelope.event_type, "proof.realtime");
    assert_eq!(publications[0].envelope.payload, json!({"sequence": 2}));
}

#[tokio::test]
async fn unknown_or_unbounded_input_fails_before_publication() {
    let publisher = Arc::new(MemoryRealtimePublisher::default());

    let unknown = publish_request(
        publisher.clone() as Arc<dyn RealtimePublisher>,
        json!({"channel": "subject-7/orders", "sequence": 2, "token": "must-not-pass"}),
    )
    .await;
    let invalid_channel = publish_request(
        publisher.clone() as Arc<dyn RealtimePublisher>,
        json!({"channel": "private/../../escape", "sequence": 3}),
    )
    .await;

    assert!(unknown.is_err());
    assert!(invalid_channel.is_err());
    assert!(publisher.published().is_empty());
}
