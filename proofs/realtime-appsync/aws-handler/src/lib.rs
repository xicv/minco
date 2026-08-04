#![forbid(unsafe_code)]

use minco_plugin_realtime::{
    RealtimeChannel, RealtimeEnvelope, RealtimeError, RealtimePlan, RealtimePublication,
    RealtimePublisher, RealtimePublisherService,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;

#[derive(Debug, thiserror::Error)]
pub enum ProofError {
    #[error("invalid live proof request")]
    InvalidRequest,
    #[error("live proof publication failed: {0}")]
    Publication(#[from] RealtimeError),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProofRequest {
    channel: String,
    sequence: u16,
}

pub async fn publish_request(
    publisher: Arc<dyn RealtimePublisher>,
    payload: Value,
) -> Result<Value, ProofError> {
    let request: ProofRequest =
        serde_json::from_value(payload).map_err(|_| ProofError::InvalidRequest)?;
    if request.sequence == 0 || request.sequence > 100 {
        return Err(ProofError::InvalidRequest);
    }
    let channel =
        RealtimeChannel::parse(request.channel).map_err(|_| ProofError::InvalidRequest)?;
    let id = format!("live-{}", request.sequence);
    let publication = RealtimePublication {
        channel,
        envelope: RealtimeEnvelope {
            id: id.clone(),
            event_type: "proof.realtime".into(),
            occurred_at: chrono::Utc::now().to_rfc3339(),
            payload: json!({"sequence": request.sequence}),
        },
    };
    let service = RealtimePublisherService::new(
        publisher,
        RealtimePlan {
            namespace: "orders".into(),
            max_event_bytes: 5 * 1024,
            subscriber_claim: "sub".into(),
        },
    );
    service.publish(&publication).await?;
    Ok(json!({"published": true, "id": id}))
}
