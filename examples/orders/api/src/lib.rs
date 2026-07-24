//! Axum delivery layer for the contract-first orders reference application.
#![forbid(unsafe_code)]

pub mod generated;

use axum::{
    Extension, Json, Router,
    extract::{Path, State},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use http::{HeaderMap, StatusCode};
use minco_contract::ContractOperation;
use minco_http::{ApiFailure, Principal, RequestMetadata, principal_from_headers};
use minco_plugin_health::HealthRegistry;
use orders_application::{
    Actor, ApplicationError, Clock, GetOrder, OrderStore, PlaceOrder, PlaceOrderCommand,
    PlaceOrderLine,
};
use orders_domain::{Order, OrderId, OrderStatus};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone)]
pub struct ApiState {
    store: Arc<dyn OrderStore>,
    clock: Arc<dyn Clock>,
    health: Arc<HealthRegistry>,
    allow_development_headers: bool,
}

impl std::fmt::Debug for ApiState {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ApiState")
            .field("allow_development_headers", &self.allow_development_headers)
            .finish_non_exhaustive()
    }
}

impl ApiState {
    #[must_use]
    pub fn new(
        store: Arc<dyn OrderStore>,
        clock: Arc<dyn Clock>,
        health: Arc<HealthRegistry>,
        allow_development_headers: bool,
    ) -> Self {
        Self {
            store,
            clock,
            health,
            allow_development_headers,
        }
    }
}

pub static BOUND_OPERATIONS: &[ContractOperation] = &[
    generated::GET_LIVE,
    generated::GET_READY,
    generated::PLACE_ORDER,
    generated::GET_ORDER,
];

pub fn build_router(state: ApiState) -> Router {
    Router::new()
        .route(generated::GET_LIVE.path, get(live))
        .route(generated::GET_READY.path, get(ready))
        .route(generated::PLACE_ORDER.path, post(place_order))
        .route(generated::GET_ORDER.path, get(get_order))
        .with_state(state)
}

async fn live() -> Json<generated::LivenessResponse> {
    Json(generated::LivenessResponse {
        live: true,
        service: "minco-orders".into(),
    })
}

async fn ready(State(state): State<ApiState>) -> Response {
    let results = state.health.run().await;
    let dependencies = results
        .iter()
        .map(|result| {
            (
                result.id.clone(),
                generated::DependencyHealth {
                    ready: result.ready,
                    detail: result.detail.clone(),
                },
            )
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    let is_ready = results
        .iter()
        .all(|result| result.ready || !result.critical);
    if is_ready {
        return Json(generated::ReadinessResponse {
            dependencies: serde_json::to_value(dependencies)
                .unwrap_or_else(|_| serde_json::json!({})),
            ready: true,
        })
        .into_response();
    }
    ApiFailure::new(
        StatusCode::SERVICE_UNAVAILABLE,
        "dependency_unavailable",
        "Dependency unavailable",
        "At least one critical dependency is not ready.",
        "readiness",
    )
    .into_response()
}

async fn place_order(
    State(state): State<ApiState>,
    principal: Option<Extension<Principal>>,
    headers: HeaderMap,
    Json(request): Json<generated::PlaceOrderRequest>,
) -> Result<Response, ApiFailure> {
    let (metadata, actor) =
        actor(&headers, principal, state.allow_development_headers).map_err(|failure| *failure)?;
    let idempotency_key = headers
        .get("idempotency-key")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    let command = PlaceOrderCommand {
        customer_reference: request.customer_reference,
        lines: request
            .lines
            .into_iter()
            .map(|line| PlaceOrderLine {
                sku: line.sku,
                quantity: line.quantity,
            })
            .collect(),
    };
    let result = PlaceOrder::new(Arc::clone(&state.store), Arc::clone(&state.clock))
        .execute(&actor, command, idempotency_key)
        .await
        .map_err(|error| map_application_error(error, &metadata.request_id))?;
    let status = if result.replayed {
        StatusCode::OK
    } else {
        StatusCode::CREATED
    };
    Ok((
        status,
        Json(generated::PlaceOrderResponse {
            order: order_response(result.order),
            replayed: result.replayed,
        }),
    )
        .into_response())
}

async fn get_order(
    State(state): State<ApiState>,
    principal: Option<Extension<Principal>>,
    headers: HeaderMap,
    Path(order_id): Path<Uuid>,
) -> Result<Json<generated::OrderResponse>, ApiFailure> {
    let (metadata, actor) =
        actor(&headers, principal, state.allow_development_headers).map_err(|failure| *failure)?;
    let order = GetOrder::new(Arc::clone(&state.store))
        .execute(&actor, OrderId::from_uuid(order_id))
        .await
        .map_err(|error| map_application_error(error, &metadata.request_id))?;
    Ok(Json(order_response(order)))
}

fn actor(
    headers: &HeaderMap,
    principal: Option<Extension<Principal>>,
    allow_development_headers: bool,
) -> Result<(RequestMetadata, Actor), Box<ApiFailure>> {
    let mut metadata =
        principal_from_headers(headers, allow_development_headers).map_err(|_| {
            Box::new(ApiFailure::new(
                StatusCode::UNAUTHORIZED,
                "invalid_principal",
                "Invalid principal",
                "The request identity is invalid.",
                "unknown",
            ))
        })?;
    if let Some(Extension(principal)) = principal {
        metadata.principal = Some(principal);
    }
    let principal = metadata.principal.as_ref().ok_or_else(|| {
        Box::new(ApiFailure::new(
            StatusCode::UNAUTHORIZED,
            "authentication_required",
            "Authentication required",
            "A valid request principal is required.",
            metadata.request_id.clone(),
        ))
    })?;
    Ok((
        metadata.clone(),
        Actor::service(
            principal.subject.clone(),
            principal.permissions.iter().cloned(),
        ),
    ))
}

fn order_response(order: Order) -> generated::OrderResponse {
    generated::OrderResponse {
        created_at: order.created_at,
        customer_reference: order.customer_reference.as_str().to_owned(),
        id: order.id.into_uuid(),
        lines: order
            .lines
            .into_iter()
            .map(|line| generated::OrderLineResponse {
                quantity: i32::try_from(line.quantity.get()).unwrap_or(i32::MAX),
                sku: line.sku.as_str().to_owned(),
            })
            .collect(),
        status: match order.status {
            OrderStatus::Accepted => generated::OrderStatus::Accepted,
        },
    }
}

fn map_application_error(error: ApplicationError, request_id: &str) -> ApiFailure {
    match error {
        ApplicationError::Forbidden => ApiFailure::new(
            StatusCode::FORBIDDEN,
            "forbidden",
            "Forbidden",
            "The caller is not permitted to perform this operation.",
            request_id,
        ),
        ApplicationError::NotFound => ApiFailure::new(
            StatusCode::NOT_FOUND,
            "not_found",
            "Not found",
            "The requested order does not exist.",
            request_id,
        ),
        ApplicationError::Validation(detail) => ApiFailure::validation(detail, request_id),
        ApplicationError::IdempotencyConflict => ApiFailure::new(
            StatusCode::CONFLICT,
            "idempotency_conflict",
            "Idempotency conflict",
            "The Idempotency-Key was already used for a different request.",
            request_id,
        ),
        ApplicationError::Unavailable => ApiFailure::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "dependency_unavailable",
            "Dependency unavailable",
            "The order store is unavailable.",
            request_id,
        ),
        ApplicationError::Internal => ApiFailure::internal(request_id),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use http_body_util::BodyExt;
    use orders_adapters::MemoryOrderStore;
    use orders_application::SystemClock;
    use tower::ServiceExt;

    fn app() -> Router {
        build_router(ApiState::new(
            Arc::new(MemoryOrderStore::new()),
            Arc::new(SystemClock),
            Arc::new(HealthRegistry::default()),
            true,
        ))
    }

    #[test]
    fn installed_operations_match_the_contract_inventory() {
        let mut contract = generated::OPERATIONS
            .iter()
            .map(|operation| operation.operation_id)
            .collect::<Vec<_>>();
        let mut bound = BOUND_OPERATIONS
            .iter()
            .map(|operation| operation.operation_id)
            .collect::<Vec<_>>();
        contract.sort_unstable();
        bound.sort_unstable();
        assert_eq!(contract, bound);
    }

    #[tokio::test]
    async fn place_and_get_order_through_the_real_router() {
        let app = app();
        let request = http::Request::post("/orders")
            .header("content-type", "application/json")
            .header("idempotency-key", "test-order-1")
            .header("x-minco-subject", "test-user")
            .header("x-minco-permissions", "orders.create,orders.read")
            .body(Body::from(
                r#"{"customerReference":"PO-42","lines":[{"sku":"SKU-1","quantity":2}]}"#,
            ))
            .expect("request");
        let response = app.clone().oneshot(request).await.expect("response");
        assert_eq!(response.status(), StatusCode::CREATED);
        let body = response
            .into_body()
            .collect()
            .await
            .expect("body")
            .to_bytes();
        let created: generated::PlaceOrderResponse = serde_json::from_slice(&body).expect("JSON");

        let response = app
            .oneshot(
                http::Request::get(format!("/orders/{}", created.order.id))
                    .header("x-minco-subject", "test-user")
                    .header("x-minco-permissions", "orders.read")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn protected_routes_fail_closed() {
        let response = app()
            .oneshot(
                http::Request::get(format!("/orders/{}", Uuid::now_v7()))
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
}
