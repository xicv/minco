use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GatewayError {
    Gone,
    Throttled,
    Fatal,
}

pub trait ConnectionGateway {
    fn get_connection(&mut self, connection_id: &str) -> Result<(), GatewayError>;

    fn post_to_connection(
        &mut self,
        connection_id: &str,
        payload: &[u8],
    ) -> Result<(), GatewayError>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HandshakeOutcome {
    Delivered,
    RetryAfter(Duration),
    Stale,
    Abandoned,
    Failed,
}

const CONNECTION_VISIBILITY_RETRY: Duration = Duration::from_millis(50);
const HANDSHAKE_DEADLINE: Duration = Duration::from_secs(10);

pub fn deliver_after_connect(
    gateway: &mut impl ConnectionGateway,
    connection_id: &str,
    socket_id: &str,
    elapsed: Duration,
) -> HandshakeOutcome {
    match gateway.get_connection(connection_id) {
        Ok(()) => {
            let data = serde_json::json!({
                "socket_id": socket_id,
                "activity_timeout": 300,
            })
            .to_string();
            let Ok(payload) = serde_json::to_vec(&serde_json::json!({
                "event": "pusher:connection_established",
                "data": data,
            })) else {
                return HandshakeOutcome::Failed;
            };

            match gateway.post_to_connection(connection_id, &payload) {
                Ok(()) => HandshakeOutcome::Delivered,
                Err(GatewayError::Gone) => HandshakeOutcome::Stale,
                Err(GatewayError::Throttled) if elapsed < HANDSHAKE_DEADLINE => {
                    HandshakeOutcome::RetryAfter(CONNECTION_VISIBILITY_RETRY)
                }
                Err(GatewayError::Throttled) => HandshakeOutcome::Abandoned,
                Err(GatewayError::Fatal) => HandshakeOutcome::Failed,
            }
        }
        Err(GatewayError::Gone | GatewayError::Throttled) if elapsed < HANDSHAKE_DEADLINE => {
            HandshakeOutcome::RetryAfter(CONNECTION_VISIBILITY_RETRY)
        }
        Err(GatewayError::Gone | GatewayError::Throttled) => HandshakeOutcome::Abandoned,
        Err(GatewayError::Fatal) => HandshakeOutcome::Failed,
    }
}
