use std::{
    collections::BTreeSet,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use axum::{
    Json, Router,
    extract::{
        Form, Path, State, WebSocketUpgrade,
        ws::{CloseFrame, Message, WebSocket},
    },
    http::StatusCode,
    response::{Html, IntoResponse, Response},
    routing::{any, get, post},
};
use hmac::{Hmac, Mac};
use serde::Deserialize;
use serde_json::Value;
use sha2::Sha256;
use subtle::ConstantTimeEq;
use tokio::{
    net::TcpListener,
    sync::broadcast::{self, error::RecvError},
};

const APP_KEY: &str = "proof-key";
const APP_SECRET: &[u8] = b"proof-secret";

#[derive(Clone, Debug)]
struct ApplicationState {
    next_socket: Arc<AtomicU64>,
    published: broadcast::Sender<PublishedEvent>,
    disconnects: broadcast::Sender<String>,
}

#[derive(Clone, Debug)]
struct PublishedEvent {
    channel: String,
    event: String,
    data: Value,
    exclude_socket_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AuthorizationRequest {
    socket_id: String,
    channel_name: String,
}

#[derive(Debug, Deserialize)]
struct PublishRequest {
    channel: String,
    event: String,
    data: Value,
    exclude_socket_id: Option<String>,
}

#[tokio::main]
async fn main() {
    let (published, _) = broadcast::channel(256);
    let (disconnects, _) = broadcast::channel(16);
    let state = ApplicationState {
        next_socket: Arc::new(AtomicU64::new(1)),
        published,
        disconnects,
    };
    let application = Router::new()
        .route(
            "/",
            get(|| async { Html("<!doctype html><title>Minco realtime proof</title>") }),
        )
        .route("/realtime/auth", post(authorize))
        .route("/proof/publish", post(publish))
        .route("/proof/disconnect/{socket_id}", post(disconnect))
        .route("/app/{key}", any(connect))
        .with_state(state);
    let listener = TcpListener::bind("127.0.0.1:3210")
        .await
        .expect("bind local proof server");
    axum::serve(listener, application)
        .await
        .expect("serve local proof server");
}

async fn connect(
    State(state): State<ApplicationState>,
    Path(key): Path<String>,
    websocket: WebSocketUpgrade,
) -> Response {
    if key != APP_KEY {
        return StatusCode::NOT_FOUND.into_response();
    }

    let socket_id = format!("1.{}", state.next_socket.fetch_add(1, Ordering::Relaxed));
    let published = state.published.subscribe();
    let disconnects = state.disconnects.subscribe();
    websocket
        .max_message_size(10_000)
        .on_upgrade(move |socket| handle_connection(socket, socket_id, published, disconnects))
}

async fn handle_connection(
    mut socket: WebSocket,
    socket_id: String,
    mut published: broadcast::Receiver<PublishedEvent>,
    mut disconnects: broadcast::Receiver<String>,
) {
    let established_data = serde_json::json!({
        "socket_id": socket_id,
        "activity_timeout": 300,
    });
    let established = serde_json::json!({
        "event": "pusher:connection_established",
        "data": established_data.to_string(),
    });
    if send_json(&mut socket, established).await.is_err() {
        return;
    }
    if send_json(
        &mut socket,
        serde_json::json!({"event": "pusher:ping", "data": "{}"}),
    )
    .await
    .is_err()
    {
        return;
    }

    let mut channels = BTreeSet::new();
    loop {
        tokio::select! {
            message = socket.recv() => {
                let Some(Ok(Message::Text(message))) = message else {
                    return;
                };
                if handle_client_message(&mut socket, &socket_id, &mut channels, &message)
                    .await
                    .is_err()
                {
                    return;
                }
            }
            event = published.recv() => {
                match event {
                    Ok(event) => {
                        if channels.contains(&event.channel)
                            && event.exclude_socket_id.as_deref() != Some(socket_id.as_str())
                        {
                            let message = serde_json::json!({
                                "event": event.event,
                                "channel": event.channel,
                                "data": event.data,
                            });
                            if send_json(&mut socket, message).await.is_err() {
                                return;
                            }
                        }
                    }
                    Err(RecvError::Lagged(_)) => continue,
                    Err(RecvError::Closed) => return,
                }
            }
            target = disconnects.recv() => {
                match target {
                    Ok(target) if target == socket_id => {
                        let _ = socket
                            .send(Message::Close(Some(CloseFrame {
                                code: 1001,
                                reason: "gateway connection lifetime reached".into(),
                            })))
                            .await;
                        return;
                    }
                    Ok(_) | Err(RecvError::Lagged(_)) => continue,
                    Err(RecvError::Closed) => return,
                }
            }
        }
    }
}

async fn handle_client_message(
    socket: &mut WebSocket,
    socket_id: &str,
    channels: &mut BTreeSet<String>,
    message: &str,
) -> Result<(), ()> {
    let Ok(message) = serde_json::from_str::<Value>(message) else {
        return Ok(());
    };
    let event = message.get("event").and_then(Value::as_str);
    if event == Some("pusher:pong") {
        return send_json(
            socket,
            serde_json::json!({"event": "proof.pong_observed", "data": "{}"}),
        )
        .await;
    }
    if event != Some("pusher:subscribe") {
        return Ok(());
    }

    let Some(channel) = message
        .get("data")
        .and_then(|data| data.get("channel"))
        .and_then(Value::as_str)
    else {
        return Ok(());
    };
    if channel.starts_with("private-") {
        let provided = message
            .get("data")
            .and_then(|data| data.get("auth"))
            .and_then(Value::as_str)
            .unwrap_or_default();
        let expected = channel_authorization(socket_id, channel);
        if !bool::from(expected.as_bytes().ct_eq(provided.as_bytes())) {
            return send_json(
                socket,
                serde_json::json!({
                    "event": "pusher:subscription_error",
                    "channel": channel,
                    "data": {
                        "code": "invalid_channel_authorization",
                        "status": 403,
                    },
                }),
            )
            .await;
        }
    }

    channels.insert(channel.to_owned());
    send_json(
        socket,
        serde_json::json!({
            "event": "pusher_internal:subscription_succeeded",
            "channel": channel,
            "data": "{}",
        }),
    )
    .await?;
    send_json(
        socket,
        serde_json::json!({
            "event": "order.updated",
            "channel": channel,
            "data": {"order_id": "ord-123", "version": 7},
        }),
    )
    .await
}

async fn authorize(Form(request): Form<AuthorizationRequest>) -> Result<Json<Value>, StatusCode> {
    if !valid_socket_id(&request.socket_id) || !request.channel_name.starts_with("private-") {
        return Err(StatusCode::FORBIDDEN);
    }
    Ok(Json(serde_json::json!({
        "auth": channel_authorization(&request.socket_id, &request.channel_name),
    })))
}

async fn publish(
    State(state): State<ApplicationState>,
    Json(request): Json<PublishRequest>,
) -> StatusCode {
    let event = PublishedEvent {
        channel: request.channel,
        event: request.event,
        data: request.data,
        exclude_socket_id: request.exclude_socket_id,
    };
    if state.published.send(event).is_err() {
        return StatusCode::SERVICE_UNAVAILABLE;
    }
    StatusCode::ACCEPTED
}

async fn disconnect(
    State(state): State<ApplicationState>,
    Path(socket_id): Path<String>,
) -> StatusCode {
    if state.disconnects.send(socket_id).is_err() {
        return StatusCode::GONE;
    }
    StatusCode::ACCEPTED
}

async fn send_json(socket: &mut WebSocket, value: Value) -> Result<(), ()> {
    socket
        .send(Message::Text(value.to_string().into()))
        .await
        .map_err(|_| ())
}

fn valid_socket_id(socket_id: &str) -> bool {
    let Some((first, second)) = socket_id.split_once('.') else {
        return false;
    };
    !first.is_empty()
        && !second.is_empty()
        && first.bytes().all(|byte| byte.is_ascii_digit())
        && second.bytes().all(|byte| byte.is_ascii_digit())
}

fn channel_authorization(socket_id: &str, channel: &str) -> String {
    let mut hmac = Hmac::<Sha256>::new_from_slice(APP_SECRET).expect("HMAC accepts any key size");
    hmac.update(format!("{socket_id}:{channel}").as_bytes());
    let signature = hmac.finalize().into_bytes();
    let mut encoded = String::with_capacity(signature.len() * 2);
    for byte in signature {
        use std::fmt::Write as _;
        write!(&mut encoded, "{byte:02x}").expect("write to String");
    }
    format!("{APP_KEY}:{encoded}")
}
