use std::time::Duration;

use minco_realtime_pusher_proof::{
    ConnectionGateway, GatewayError, HandshakeOutcome, deliver_after_connect,
};

#[derive(Debug)]
struct FakeGateway {
    probe: Result<(), GatewayError>,
    post: Result<(), GatewayError>,
    calls: Vec<String>,
    payload: Vec<u8>,
}

impl ConnectionGateway for FakeGateway {
    fn get_connection(&mut self, connection_id: &str) -> Result<(), GatewayError> {
        self.calls.push(format!("get:{connection_id}"));
        self.probe
    }

    fn post_to_connection(
        &mut self,
        connection_id: &str,
        payload: &[u8],
    ) -> Result<(), GatewayError> {
        self.calls.push(format!("post:{connection_id}"));
        self.payload = payload.to_vec();
        self.post
    }
}

#[test]
fn a_connection_not_yet_visible_is_retried_without_posting() {
    let mut gateway = FakeGateway {
        probe: Err(GatewayError::Gone),
        post: Ok(()),
        calls: Vec::new(),
        payload: Vec::new(),
    };

    let outcome = deliver_after_connect(
        &mut gateway,
        "gateway-connection",
        "1.42",
        Duration::from_millis(20),
    );

    assert_eq!(
        outcome,
        HandshakeOutcome::RetryAfter(Duration::from_millis(50))
    );
    assert_eq!(gateway.calls, ["get:gateway-connection"]);
    assert!(gateway.payload.is_empty());
}

#[test]
fn a_visible_connection_receives_the_pusher_handshake() {
    let mut gateway = FakeGateway {
        probe: Ok(()),
        post: Ok(()),
        calls: Vec::new(),
        payload: Vec::new(),
    };

    let outcome = deliver_after_connect(
        &mut gateway,
        "gateway-connection",
        "1.42",
        Duration::from_millis(50),
    );

    assert_eq!(outcome, HandshakeOutcome::Delivered);
    assert_eq!(
        gateway.calls,
        ["get:gateway-connection", "post:gateway-connection"]
    );

    let frame: serde_json::Value = serde_json::from_slice(&gateway.payload).unwrap();
    assert_eq!(frame["event"], "pusher:connection_established");
    let data: serde_json::Value = serde_json::from_str(frame["data"].as_str().unwrap()).unwrap();
    assert_eq!(data["socket_id"], "1.42");
    assert_eq!(data["activity_timeout"], 300);
}

#[test]
fn a_throttled_post_is_retried_within_the_deadline() {
    let mut gateway = FakeGateway {
        probe: Ok(()),
        post: Err(GatewayError::Throttled),
        calls: Vec::new(),
        payload: Vec::new(),
    };

    let outcome = deliver_after_connect(
        &mut gateway,
        "gateway-connection",
        "1.42",
        Duration::from_millis(100),
    );

    assert_eq!(
        outcome,
        HandshakeOutcome::RetryAfter(Duration::from_millis(50))
    );
    assert_eq!(
        gateway.calls,
        ["get:gateway-connection", "post:gateway-connection"]
    );
}

#[test]
fn a_stale_connection_is_abandoned_at_the_deadline() {
    let mut gateway = FakeGateway {
        probe: Err(GatewayError::Gone),
        post: Ok(()),
        calls: Vec::new(),
        payload: Vec::new(),
    };

    let outcome = deliver_after_connect(
        &mut gateway,
        "gateway-connection",
        "1.42",
        Duration::from_secs(10),
    );

    assert_eq!(outcome, HandshakeOutcome::Abandoned);
    assert_eq!(gateway.calls, ["get:gateway-connection"]);
}

#[test]
fn a_non_transient_gateway_error_fails_without_posting() {
    let mut gateway = FakeGateway {
        probe: Err(GatewayError::Fatal),
        post: Ok(()),
        calls: Vec::new(),
        payload: Vec::new(),
    };

    let outcome = deliver_after_connect(
        &mut gateway,
        "gateway-connection",
        "1.42",
        Duration::from_millis(50),
    );

    assert_eq!(outcome, HandshakeOutcome::Failed);
    assert_eq!(gateway.calls, ["get:gateway-connection"]);
}

#[test]
fn a_connection_gone_after_visibility_is_terminal() {
    let mut gateway = FakeGateway {
        probe: Ok(()),
        post: Err(GatewayError::Gone),
        calls: Vec::new(),
        payload: Vec::new(),
    };

    let outcome = deliver_after_connect(
        &mut gateway,
        "gateway-connection",
        "1.42",
        Duration::from_millis(50),
    );

    assert_eq!(outcome, HandshakeOutcome::Stale);
    assert_eq!(
        gateway.calls,
        ["get:gateway-connection", "post:gateway-connection"]
    );
}
