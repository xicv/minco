use std::{
    env,
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use aws_sdk_apigatewaymanagement::{Client as GatewayClient, primitives::Blob as GatewayBlob};
use aws_sdk_dynamodb::{Client as DynamoClient, types::AttributeValue};
use aws_sdk_lambda::{
    Client as LambdaClient, primitives::Blob as LambdaBlob, types::InvocationType,
};
use lambda_runtime::{Error, LambdaEvent, service_fn};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const ACTIVITY_TIMEOUT_SECONDS: u64 = 300;
const CONNECTION_TTL_SECONDS: u64 = 7_500;
const HANDSHAKE_DEADLINE: Duration = Duration::from_secs(10);
const HANDSHAKE_RETRY: Duration = Duration::from_millis(50);
const MAX_MESSAGE_BYTES: usize = 10_000;
const MAX_CHANNEL_BYTES: usize = 164;

#[derive(Clone)]
struct State {
    aws_config: aws_config::SdkConfig,
    dynamo: DynamoClient,
    lambda: LambdaClient,
    function_name: Arc<str>,
    table_name: Arc<str>,
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .without_time()
        .init();

    let aws_config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
    let state = State {
        dynamo: DynamoClient::new(&aws_config),
        lambda: LambdaClient::new(&aws_config),
        function_name: env::var("AWS_LAMBDA_FUNCTION_NAME")?.into(),
        table_name: env::var("CONNECTION_TABLE")?.into(),
        aws_config,
    };

    lambda_runtime::run(service_fn(move |event| {
        let state = state.clone();
        async move { handle(&state, event).await }
    }))
    .await
}

async fn handle(state: &State, event: LambdaEvent<Value>) -> Result<Value, Error> {
    if event.payload["proof_kind"] == "post_connect_handshake" {
        return handle_post_connect(state, &event.payload).await;
    }

    let context = &event.payload["requestContext"];
    let route = required_string(context, "routeKey")?;
    let connection_id = required_string(context, "connectionId")?;

    match route {
        "$connect" => handle_connect(state, context, connection_id).await,
        "$disconnect" => {
            state
                .dynamo
                .delete_item()
                .table_name(state.table_name.as_ref())
                .key("connection_id", AttributeValue::S(connection_id.to_owned()))
                .send()
                .await?;
            Ok(response(200))
        }
        _ => handle_message(state, &event.payload, context, connection_id).await,
    }
}

async fn handle_connect(
    state: &State,
    context: &Value,
    connection_id: &str,
) -> Result<Value, Error> {
    let endpoint = management_endpoint(
        required_string(context, "domainName")?,
        required_string(context, "stage")?,
        &env::var("AWS_REGION")?,
    )?;
    let socket_id = socket_id(connection_id);
    let expires_at =
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() + CONNECTION_TTL_SECONDS;

    state
        .dynamo
        .put_item()
        .table_name(state.table_name.as_ref())
        .item("connection_id", AttributeValue::S(connection_id.to_owned()))
        .item("socket_id", AttributeValue::S(socket_id.clone()))
        .item("expires_at", AttributeValue::N(expires_at.to_string()))
        .send()
        .await?;

    let child = json!({
        "proof_kind": "post_connect_handshake",
        "connection_id": connection_id,
        "socket_id": socket_id,
        "endpoint": endpoint,
    });
    let invoke = state
        .lambda
        .invoke()
        .function_name(state.function_name.as_ref())
        .invocation_type(InvocationType::Event)
        .payload(LambdaBlob::new(serde_json::to_vec(&child)?))
        .send()
        .await;

    if let Err(error) = invoke {
        state
            .dynamo
            .delete_item()
            .table_name(state.table_name.as_ref())
            .key("connection_id", AttributeValue::S(connection_id.to_owned()))
            .send()
            .await?;
        return Err(error.into());
    }

    Ok(response(200))
}

async fn handle_post_connect(state: &State, event: &Value) -> Result<Value, Error> {
    let connection_id = required_string(event, "connection_id")?;
    let socket_id = required_string(event, "socket_id")?;
    let endpoint = required_string(event, "endpoint")?;
    let gateway = gateway_client(&state.aws_config, endpoint);
    let started = Instant::now();
    let frame = json!({
        "event": "pusher:connection_established",
        "data": json!({
            "socket_id": socket_id,
            "activity_timeout": ACTIVITY_TIMEOUT_SECONDS,
        }).to_string(),
    });
    let payload = serde_json::to_vec(&frame)?;

    loop {
        match gateway
            .get_connection()
            .connection_id(connection_id)
            .send()
            .await
        {
            Ok(_) => {}
            Err(error)
                if error.as_service_error().is_some_and(|value| {
                    value.is_gone_exception() || value.is_limit_exceeded_exception()
                }) && started.elapsed() < HANDSHAKE_DEADLINE =>
            {
                tokio::time::sleep(HANDSHAKE_RETRY).await;
                continue;
            }
            Err(error)
                if error
                    .as_service_error()
                    .is_some_and(|value| value.is_gone_exception()) =>
            {
                tracing::info!(connection_id, outcome = "stale", "handshake abandoned");
                return Ok(response(202));
            }
            Err(error) => return Err(error.into()),
        }

        match gateway
            .post_to_connection()
            .connection_id(connection_id)
            .data(GatewayBlob::new(payload.clone()))
            .send()
            .await
        {
            Ok(_) => {
                tracing::info!(connection_id, outcome = "delivered", "handshake posted");
                return Ok(response(200));
            }
            Err(error)
                if error
                    .as_service_error()
                    .is_some_and(|value| value.is_gone_exception()) =>
            {
                tracing::info!(connection_id, outcome = "stale", "handshake abandoned");
                return Ok(response(202));
            }
            Err(error)
                if error
                    .as_service_error()
                    .is_some_and(|value| value.is_limit_exceeded_exception())
                    && started.elapsed() < HANDSHAKE_DEADLINE =>
            {
                tokio::time::sleep(HANDSHAKE_RETRY).await;
            }
            Err(error) => return Err(error.into()),
        }
    }
}

async fn handle_message(
    state: &State,
    event: &Value,
    context: &Value,
    connection_id: &str,
) -> Result<Value, Error> {
    let endpoint = management_endpoint(
        required_string(context, "domainName")?,
        required_string(context, "stage")?,
        &env::var("AWS_REGION")?,
    )?;
    let gateway = gateway_client(&state.aws_config, &endpoint);
    let body_text = required_string(event, "body")?;
    if body_text.len() > MAX_MESSAGE_BYTES {
        post_frame(
            &gateway,
            connection_id,
            &json!({
                "event": "pusher:error",
                "data": {"code": 4300, "message": "proof message exceeds 10000 bytes"},
            }),
        )
        .await?;
        return Ok(response(200));
    }
    let body: Value = serde_json::from_str(body_text)?;

    match body["event"].as_str() {
        Some("pusher:subscribe") => {
            let channel = required_string(&body["data"], "channel")?;
            if !valid_public_channel(channel) {
                post_frame(
                    &gateway,
                    connection_id,
                    &json!({
                        "event": "pusher:subscription_error",
                        "channel": channel,
                        "data": {
                            "code": "unsupported_channel",
                            "status": 403,
                        },
                    }),
                )
                .await?;
                return Ok(response(200));
            }
            post_frame(
                &gateway,
                connection_id,
                &json!({
                    "event": "pusher_internal:subscription_succeeded",
                    "channel": channel,
                    "data": "{}",
                }),
            )
            .await?;
            post_frame(
                &gateway,
                connection_id,
                &json!({
                    "event": "order.updated",
                    "channel": channel,
                    "data": {"order_id": "proof-order", "status": "ready"},
                }),
            )
            .await?;
        }
        Some("pusher:ping") => {
            post_frame(
                &gateway,
                connection_id,
                &json!({"event": "pusher:pong", "data": {}}),
            )
            .await?;
        }
        Some("pusher:pong") => {}
        _ => {
            post_frame(
                &gateway,
                connection_id,
                &json!({
                    "event": "pusher:error",
                    "data": {"code": 4001, "message": "unsupported proof event"},
                }),
            )
            .await?;
        }
    }

    Ok(response(200))
}

async fn post_frame(
    gateway: &GatewayClient,
    connection_id: &str,
    frame: &Value,
) -> Result<(), Error> {
    gateway
        .post_to_connection()
        .connection_id(connection_id)
        .data(GatewayBlob::new(serde_json::to_vec(frame)?))
        .send()
        .await?;
    Ok(())
}

fn gateway_client(config: &aws_config::SdkConfig, endpoint: &str) -> GatewayClient {
    let service_config = aws_sdk_apigatewaymanagement::config::Builder::from(config)
        .endpoint_url(endpoint)
        .build();
    GatewayClient::from_conf(service_config)
}

fn socket_id(connection_id: &str) -> String {
    let digest = Sha256::digest(connection_id.as_bytes());
    let left = u32::from_be_bytes(digest[0..4].try_into().expect("fixed digest slice"));
    let right = u32::from_be_bytes(digest[4..8].try_into().expect("fixed digest slice"));
    format!("{left}.{right}")
}

fn required_string<'a>(value: &'a Value, key: &str) -> Result<&'a str, Error> {
    value[key]
        .as_str()
        .ok_or_else(|| format!("missing string field {key}").into())
}

fn management_endpoint(domain: &str, stage: &str, region: &str) -> Result<String, Error> {
    let url_suffix = if region.starts_with("cn-") {
        "amazonaws.com.cn"
    } else {
        "amazonaws.com"
    };
    let expected_suffix = format!(".execute-api.{region}.{url_suffix}");
    let Some(api_id) = domain.strip_suffix(&expected_suffix) else {
        return Err("untrusted API Gateway management endpoint".into());
    };
    if api_id.len() != 10
        || !api_id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        || stage != "app"
    {
        return Err("untrusted API Gateway management endpoint".into());
    }
    Ok(format!("https://{domain}/{stage}"))
}

fn valid_public_channel(channel: &str) -> bool {
    !channel.is_empty()
        && channel.len() <= MAX_CHANNEL_BYTES
        && !channel.starts_with("private-")
        && !channel.starts_with("presence-")
        && channel
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"_-=@,.;".contains(&byte))
}

fn response(status_code: u16) -> Value {
    json!({"statusCode": status_code})
}

#[cfg(test)]
mod tests {
    use super::{management_endpoint, socket_id, valid_public_channel};

    #[test]
    fn socket_ids_are_stable_numeric_pairs() {
        let first = socket_id("abc123");
        assert_eq!(first, socket_id("abc123"));
        assert_ne!(first, socket_id("different"));
        assert!(first.split('.').all(|part| part.parse::<u32>().is_ok()));
    }

    #[test]
    fn management_endpoint_is_bound_to_the_proof_stage_and_region() {
        assert_eq!(
            management_endpoint(
                "aabbccddee.execute-api.ap-southeast-2.amazonaws.com",
                "app",
                "ap-southeast-2",
            )
            .unwrap(),
            "https://aabbccddee.execute-api.ap-southeast-2.amazonaws.com/app"
        );
        assert!(management_endpoint("attacker.invalid", "app", "ap-southeast-2").is_err());
        assert!(
            management_endpoint(
                "aabbccddee.execute-api.ap-southeast-2.amazonaws.com",
                "other",
                "ap-southeast-2",
            )
            .is_err()
        );
        assert_eq!(
            management_endpoint(
                "aabbccddee.execute-api.cn-north-1.amazonaws.com.cn",
                "app",
                "cn-north-1",
            )
            .unwrap(),
            "https://aabbccddee.execute-api.cn-north-1.amazonaws.com.cn/app"
        );
        assert!(
            management_endpoint(
                "extra.aabbccddee.execute-api.ap-southeast-2.amazonaws.com",
                "app",
                "ap-southeast-2",
            )
            .is_err()
        );
    }

    #[test]
    fn aws_proof_accepts_only_bounded_public_channel_names() {
        assert!(valid_public_channel("public-orders"));
        assert!(!valid_public_channel("private-orders"));
        assert!(!valid_public_channel("presence-orders"));
        assert!(!valid_public_channel("bad/channel"));
        assert!(!valid_public_channel(&"x".repeat(165)));
    }
}
