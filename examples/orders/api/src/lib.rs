//! Axum delivery layer for the contract-first orders reference application.
#![forbid(unsafe_code)]

pub mod generated;

use axum::{
    Extension, Json, Router,
    extract::{Path, RawQuery, State},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use http::{HeaderMap, HeaderValue, StatusCode};
use minco_contract::ContractOperation;
use minco_http::{
    ApiFailure, EntityTagError, Principal, RequestMetadata, ResourceListPolicy, StrongEntityTag,
    parse_if_match, parse_resource_list_query, principal_from_headers,
};
use minco_plugin_health::HealthRegistry;
use orders_application::{
    Actor, ApplicationError, Clock, DeleteOrder, DeleteOrderPort, GetOrder, GetOrderPort,
    ListOrderAuditHistory, ListOrderAuditHistoryPort, ListOrderAuditHistoryQuery, ListOrders,
    ListOrdersPort, ListOrdersQuery, OrderAuditActorKind, OrderAuditCursor,
    OrderAuditSortDirection, OrderAuditValue, OrderCursor, OrderSortField, OrderSortTerm,
    PlaceOrder, PlaceOrderCommand, PlaceOrderLine, PlaceOrderPort, SortDirection, UpdateOrder,
    UpdateOrderCommand, UpdateOrderPort,
};
use orders_domain::{Order, OrderId, OrderStatus};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone)]
pub struct ApiState {
    place_orders: Arc<dyn PlaceOrderPort>,
    get_orders: Arc<dyn GetOrderPort>,
    list_orders: Arc<dyn ListOrdersPort>,
    update_orders: Arc<dyn UpdateOrderPort>,
    delete_orders: Arc<dyn DeleteOrderPort>,
    audit_history: Arc<dyn ListOrderAuditHistoryPort>,
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
    pub fn new<S>(
        store: Arc<S>,
        clock: Arc<dyn Clock>,
        health: Arc<HealthRegistry>,
        allow_development_headers: bool,
    ) -> Self
    where
        S: PlaceOrderPort
            + GetOrderPort
            + ListOrdersPort
            + UpdateOrderPort
            + DeleteOrderPort
            + ListOrderAuditHistoryPort
            + 'static,
    {
        let place_orders: Arc<dyn PlaceOrderPort> = store.clone();
        let get_orders: Arc<dyn GetOrderPort> = store.clone();
        let list_orders: Arc<dyn ListOrdersPort> = store.clone();
        let update_orders: Arc<dyn UpdateOrderPort> = store.clone();
        let audit_history: Arc<dyn ListOrderAuditHistoryPort> = store.clone();
        let delete_orders: Arc<dyn DeleteOrderPort> = store;
        Self::from_ports(
            place_orders,
            get_orders,
            list_orders,
            update_orders,
            delete_orders,
            audit_history,
            clock,
            health,
            allow_development_headers,
        )
    }

    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn from_ports(
        place_orders: Arc<dyn PlaceOrderPort>,
        get_orders: Arc<dyn GetOrderPort>,
        list_orders: Arc<dyn ListOrdersPort>,
        update_orders: Arc<dyn UpdateOrderPort>,
        delete_orders: Arc<dyn DeleteOrderPort>,
        audit_history: Arc<dyn ListOrderAuditHistoryPort>,
        clock: Arc<dyn Clock>,
        health: Arc<HealthRegistry>,
        allow_development_headers: bool,
    ) -> Self {
        Self {
            place_orders,
            get_orders,
            list_orders,
            update_orders,
            delete_orders,
            audit_history,
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
    generated::LIST_ORDERS,
    generated::GET_ORDER,
    generated::UPDATE_ORDER,
    generated::DELETE_ORDER,
    generated::LIST_ORDER_AUDIT_HISTORY,
];

pub fn build_router(state: ApiState) -> Router {
    Router::new()
        .route(generated::GET_LIVE.path, get(live))
        .route(generated::GET_READY.path, get(ready))
        .route(
            generated::PLACE_ORDER.path,
            post(place_order).get(list_orders),
        )
        .route(
            generated::GET_ORDER.path,
            get(get_order).patch(update_order).delete(delete_order),
        )
        .route(
            generated::LIST_ORDER_AUDIT_HISTORY.path,
            get(list_order_audit_history),
        )
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
    let result = PlaceOrder::new(Arc::clone(&state.place_orders), Arc::clone(&state.clock))
        .execute_correlated(
            &actor,
            command,
            idempotency_key,
            audit_correlation_id(&metadata.request_id),
        )
        .await
        .map_err(|error| map_application_error(error, &metadata.request_id))?;
    let status = if result.replayed {
        StatusCode::OK
    } else {
        StatusCode::CREATED
    };
    resource_document_response(status, result.order, &metadata.request_id, true)
}

async fn get_order(
    State(state): State<ApiState>,
    principal: Option<Extension<Principal>>,
    headers: HeaderMap,
    Path(order_id): Path<Uuid>,
) -> Result<Response, ApiFailure> {
    let (metadata, actor) =
        actor(&headers, principal, state.allow_development_headers).map_err(|failure| *failure)?;
    let order = GetOrder::new(Arc::clone(&state.get_orders))
        .execute(&actor, OrderId::from_uuid(order_id))
        .await
        .map_err(|error| map_application_error(error, &metadata.request_id))?;
    resource_document_response(StatusCode::OK, order, &metadata.request_id, false)
}

async fn list_orders(
    State(state): State<ApiState>,
    principal: Option<Extension<Principal>>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Result<Json<generated::OrderCollection>, ApiFailure> {
    let (metadata, actor) =
        actor(&headers, principal, state.allow_development_headers).map_err(|failure| *failure)?;
    let policy = ResourceListPolicy::new(
        20,
        100,
        ["-createdAt", "-id"],
        ["createdAt", "id"],
        ["status"],
    )
    .map_err(|_| ApiFailure::internal(&metadata.request_id))?;
    let parsed = parse_resource_list_query(raw_query.as_deref(), &policy).map_err(|_| {
        ApiFailure::new(
            StatusCode::BAD_REQUEST,
            "invalid_resource_query",
            "Invalid resource query",
            "Pagination, sort, or filter parameters are invalid or unsupported.",
            &metadata.request_id,
        )
    })?;
    let (sort, sort_signature) = order_sort(parsed.sort(), &metadata.request_id)?;
    let status = match parsed.filters().get("status").map(String::as_str) {
        None => None,
        Some("accepted") => Some(OrderStatus::Accepted),
        Some(_) => {
            return Err(ApiFailure::new(
                StatusCode::BAD_REQUEST,
                "invalid_resource_query",
                "Invalid resource query",
                "The status filter is not supported.",
                &metadata.request_id,
            ));
        }
    };
    let after = parsed
        .after()
        .map(|cursor| decode_cursor(cursor.as_str(), &sort_signature, status))
        .transpose()
        .map_err(|()| invalid_cursor(&metadata.request_id))?;
    let result = ListOrders::new(Arc::clone(&state.list_orders))
        .execute(
            &actor,
            ListOrdersQuery {
                limit: parsed.limit(),
                after,
                sort,
                status,
            },
        )
        .await
        .map_err(|error| map_application_error(error, &metadata.request_id))?;
    let next_cursor = result
        .next_cursor
        .map(|cursor| encode_cursor(cursor, &sort_signature, status))
        .transpose()
        .map_err(|_| ApiFailure::internal(&metadata.request_id))?;
    let data = result
        .orders
        .into_iter()
        .map(|order| order_response(order, &metadata.request_id))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Json(generated::OrderCollection {
        data,
        page: generated::CursorPageInfo {
            has_more: next_cursor.is_some(),
            next_cursor,
        },
    }))
}

async fn update_order(
    State(state): State<ApiState>,
    principal: Option<Extension<Principal>>,
    headers: HeaderMap,
    Path(order_id): Path<Uuid>,
    Json(request): Json<generated::UpdateOrderRequest>,
) -> Result<Response, ApiFailure> {
    let (metadata, actor) =
        actor(&headers, principal, state.allow_development_headers).map_err(|failure| *failure)?;
    let expected_revision = expected_revision(&headers, order_id, &metadata.request_id)?;
    let command = UpdateOrderCommand {
        customer_reference: request.customer_reference,
        lines: request.lines.map(|lines| {
            lines
                .into_iter()
                .map(|line| PlaceOrderLine {
                    sku: line.sku,
                    quantity: line.quantity,
                })
                .collect()
        }),
    };
    let order = UpdateOrder::new(Arc::clone(&state.update_orders), Arc::clone(&state.clock))
        .execute_correlated(
            &actor,
            OrderId::from_uuid(order_id),
            expected_revision,
            command,
            audit_correlation_id(&metadata.request_id),
        )
        .await
        .map_err(|error| map_application_error(error, &metadata.request_id))?;
    resource_document_response(StatusCode::OK, order, &metadata.request_id, false)
}

async fn delete_order(
    State(state): State<ApiState>,
    principal: Option<Extension<Principal>>,
    headers: HeaderMap,
    Path(order_id): Path<Uuid>,
) -> Result<StatusCode, ApiFailure> {
    let (metadata, actor) =
        actor(&headers, principal, state.allow_development_headers).map_err(|failure| *failure)?;
    let expected_revision = expected_revision(&headers, order_id, &metadata.request_id)?;
    DeleteOrder::new(Arc::clone(&state.delete_orders), Arc::clone(&state.clock))
        .execute_correlated(
            &actor,
            OrderId::from_uuid(order_id),
            expected_revision,
            audit_correlation_id(&metadata.request_id),
        )
        .await
        .map_err(|error| map_application_error(error, &metadata.request_id))?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_order_audit_history(
    State(state): State<ApiState>,
    principal: Option<Extension<Principal>>,
    headers: HeaderMap,
    Path(order_id): Path<Uuid>,
    RawQuery(raw_query): RawQuery,
) -> Result<Json<generated::OrderAuditCollection>, ApiFailure> {
    let (metadata, actor) =
        actor(&headers, principal, state.allow_development_headers).map_err(|failure| *failure)?;
    let policy = ResourceListPolicy::new(
        50,
        100,
        ["-occurredAt", "-eventId"],
        ["occurredAt", "eventId"],
        std::iter::empty::<&str>(),
    )
    .map_err(|_| ApiFailure::internal(&metadata.request_id))?;
    let parsed = parse_resource_list_query(raw_query.as_deref(), &policy).map_err(|_| {
        ApiFailure::new(
            StatusCode::BAD_REQUEST,
            "invalid_resource_query",
            "Invalid resource query",
            "Audit pagination or sort parameters are invalid or unsupported.",
            &metadata.request_id,
        )
    })?;
    let (direction, sort_signature) = audit_sort(parsed.sort(), &metadata.request_id)?;
    let after = parsed
        .after()
        .map(|cursor| decode_audit_cursor(cursor.as_str(), &sort_signature))
        .transpose()
        .map_err(|()| invalid_cursor(&metadata.request_id))?;
    let page = ListOrderAuditHistory::new(Arc::clone(&state.audit_history))
        .execute(
            &actor,
            ListOrderAuditHistoryQuery {
                order_id: OrderId::from_uuid(order_id),
                limit: parsed.limit(),
                after,
                direction,
            },
        )
        .await
        .map_err(|error| map_application_error(error, &metadata.request_id))?;
    let next_cursor = page
        .next_cursor
        .map(|cursor| encode_audit_cursor(cursor, &sort_signature))
        .transpose()
        .map_err(|_| ApiFailure::internal(&metadata.request_id))?;
    let data = page
        .events
        .into_iter()
        .map(|event| {
            Ok(generated::OrderAuditEvent {
                action: event.action,
                actor: generated::OrderAuditActor {
                    kind: audit_actor_kind(event.actor_kind).into(),
                    subject: event.actor_subject,
                },
                changes: event
                    .changes
                    .into_iter()
                    .map(|(field, change)| generated::OrderAuditChange {
                        after: change.after.map(public_audit_value),
                        before: change.before.map(public_audit_value),
                        field,
                    })
                    .collect(),
                correlation_id: event.correlation_id,
                event_id: event.event_id,
                occurred_at: event.occurred_at,
                operation_id: event.operation_id,
                recorded_at: event.recorded_at,
                resource_revision: event
                    .resource_revision
                    .map(i64::try_from)
                    .transpose()
                    .map_err(|_| ApiFailure::internal(&metadata.request_id))?,
            })
        })
        .collect::<Result<Vec<_>, ApiFailure>>()?;
    Ok(Json(generated::OrderAuditCollection {
        data,
        page: generated::CursorPageInfo {
            has_more: next_cursor.is_some(),
            next_cursor,
        },
    }))
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct OrderCursorPayload {
    version: u8,
    created_at: chrono::DateTime<chrono::Utc>,
    id: Uuid,
    sort: String,
    status: Option<OrderStatus>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct OrderAuditCursorPayload {
    version: u8,
    occurred_at: chrono::DateTime<chrono::Utc>,
    event_id: Uuid,
    sort: String,
}

fn audit_sort(
    terms: &[minco_http::SortTerm],
    request_id: &str,
) -> Result<(OrderAuditSortDirection, String), ApiFailure> {
    let signature = terms
        .iter()
        .map(|term| {
            let prefix = match term.direction() {
                minco_http::SortDirection::Ascending => "",
                minco_http::SortDirection::Descending => "-",
            };
            format!("{prefix}{}", term.field())
        })
        .collect::<Vec<_>>()
        .join(",");
    match signature.as_str() {
        "occurredAt,eventId" => Ok((OrderAuditSortDirection::OldestFirst, signature)),
        "-occurredAt,-eventId" => Ok((OrderAuditSortDirection::NewestFirst, signature)),
        _ => Err(ApiFailure::new(
            StatusCode::BAD_REQUEST,
            "invalid_resource_query",
            "Invalid resource query",
            "Audit history requires occurredAt and eventId in the same direction.",
            request_id,
        )),
    }
}

fn encode_audit_cursor(cursor: OrderAuditCursor, sort: &str) -> Result<String, serde_json::Error> {
    serde_json::to_vec(&OrderAuditCursorPayload {
        version: 1,
        occurred_at: cursor.occurred_at,
        event_id: cursor.event_id,
        sort: sort.to_owned(),
    })
    .map(|payload| URL_SAFE_NO_PAD.encode(payload))
}

fn decode_audit_cursor(encoded: &str, sort: &str) -> Result<OrderAuditCursor, ()> {
    let bytes = URL_SAFE_NO_PAD.decode(encoded).map_err(|_| ())?;
    let payload: OrderAuditCursorPayload = serde_json::from_slice(&bytes).map_err(|_| ())?;
    if payload.version != 1 || payload.sort != sort {
        return Err(());
    }
    Ok(OrderAuditCursor {
        occurred_at: payload.occurred_at,
        event_id: payload.event_id,
    })
}

const fn audit_actor_kind(kind: OrderAuditActorKind) -> &'static str {
    match kind {
        OrderAuditActorKind::Human => "human",
        OrderAuditActorKind::Service => "service",
        OrderAuditActorKind::System => "system",
        OrderAuditActorKind::Migration => "migration",
        OrderAuditActorKind::DatabasePrincipal => "database_principal",
        OrderAuditActorKind::Unknown => "unknown",
    }
}

fn public_audit_value(value: OrderAuditValue) -> String {
    match value {
        OrderAuditValue::Literal(value) => value,
        OrderAuditValue::Digest(value) => format!("sha256:{value}"),
        OrderAuditValue::Redacted => "[redacted]".into(),
        OrderAuditValue::Omitted => "[omitted]".into(),
    }
}

fn audit_correlation_id(request_id: &str) -> Uuid {
    if let Ok(id) = Uuid::parse_str(request_id) {
        return id;
    }
    let mut hasher = Sha256::new();
    hasher.update(request_id.as_bytes());
    let digest = hasher.finalize();
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x50;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}

fn order_sort(
    terms: &[minco_http::SortTerm],
    request_id: &str,
) -> Result<(Vec<OrderSortTerm>, String), ApiFailure> {
    let mut mapped = Vec::with_capacity(terms.len());
    let mut signature = Vec::with_capacity(terms.len());
    for term in terms {
        let field = match term.field() {
            "createdAt" => OrderSortField::CreatedAt,
            "id" => OrderSortField::Id,
            _ => return Err(ApiFailure::internal(request_id)),
        };
        let (direction, prefix) = match term.direction() {
            minco_http::SortDirection::Ascending => (SortDirection::Ascending, ""),
            minco_http::SortDirection::Descending => (SortDirection::Descending, "-"),
        };
        mapped.push(OrderSortTerm { field, direction });
        signature.push(format!("{prefix}{}", term.field()));
    }
    Ok((mapped, signature.join(",")))
}

fn encode_cursor(
    cursor: OrderCursor,
    sort: &str,
    status: Option<OrderStatus>,
) -> Result<String, serde_json::Error> {
    serde_json::to_vec(&OrderCursorPayload {
        version: 1,
        created_at: cursor.created_at,
        id: cursor.id.into_uuid(),
        sort: sort.to_owned(),
        status,
    })
    .map(|payload| URL_SAFE_NO_PAD.encode(payload))
}

fn decode_cursor(
    encoded: &str,
    sort: &str,
    status: Option<OrderStatus>,
) -> Result<OrderCursor, ()> {
    let bytes = URL_SAFE_NO_PAD.decode(encoded).map_err(|_| ())?;
    let payload: OrderCursorPayload = serde_json::from_slice(&bytes).map_err(|_| ())?;
    if payload.version != 1 || payload.sort != sort || payload.status != status {
        return Err(());
    }
    Ok(OrderCursor {
        created_at: payload.created_at,
        id: OrderId::from_uuid(payload.id),
    })
}

fn invalid_cursor(request_id: &str) -> ApiFailure {
    ApiFailure::new(
        StatusCode::BAD_REQUEST,
        "invalid_resource_cursor",
        "Invalid resource cursor",
        "The cursor is invalid or does not match the current sort and filter.",
        request_id,
    )
}

fn expected_revision(
    headers: &HeaderMap,
    order_id: Uuid,
    request_id: &str,
) -> Result<u64, ApiFailure> {
    let tag = parse_if_match(headers).map_err(|error| match error {
        EntityTagError::PreconditionRequired => ApiFailure::precondition_required(request_id),
        EntityTagError::InvalidIfMatch => ApiFailure::invalid_if_match(request_id),
        EntityTagError::InvalidTag => ApiFailure::internal(request_id),
    })?;
    tag.resource_revision("order", &order_id.to_string())
        .map_err(|_| ApiFailure::invalid_if_match(request_id))
}

fn resource_document_response(
    status: StatusCode,
    order: Order,
    request_id: &str,
    include_location: bool,
) -> Result<Response, ApiFailure> {
    let tag =
        StrongEntityTag::for_resource("order", &order.id.into_uuid().to_string(), order.revision)
            .map_err(|_| ApiFailure::internal(request_id))?;
    let location = format!("/orders/{}", order.id.into_uuid());
    let mut response = (
        status,
        Json(generated::OrderDocument {
            data: order_response(order, request_id)?,
        }),
    )
        .into_response();
    response
        .headers_mut()
        .insert(http::header::ETAG, tag.to_header_value());
    if include_location {
        response.headers_mut().insert(
            http::header::LOCATION,
            HeaderValue::from_str(&location).map_err(|_| ApiFailure::internal(request_id))?,
        );
    }
    Ok(response)
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

fn order_response(order: Order, request_id: &str) -> Result<generated::OrderResponse, ApiFailure> {
    Ok(generated::OrderResponse {
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
        revision: i64::try_from(order.revision).map_err(|_| ApiFailure::internal(request_id))?,
        status: match order.status {
            OrderStatus::Accepted => generated::OrderStatus::Accepted,
        },
        updated_at: order.updated_at,
    })
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
        ApplicationError::PreconditionFailed => ApiFailure::precondition_failed(request_id),
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

    async fn place(app: &Router, key: &str, reference: &str) -> generated::OrderDocument {
        let response = app
            .clone()
            .oneshot(
                http::Request::post("/orders")
                    .header("content-type", "application/json")
                    .header("idempotency-key", key)
                    .header("x-minco-subject", "test-user")
                    .header("x-minco-permissions", "orders.create")
                    .body(Body::from(format!(
                        r#"{{"customerReference":"{reference}","lines":[{{"sku":"SKU-1","quantity":2}}]}}"#
                    )))
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::CREATED);
        serde_json::from_slice(
            &response
                .into_body()
                .collect()
                .await
                .expect("body")
                .to_bytes(),
        )
        .expect("order document")
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
    async fn resource_crud_is_shaped_and_conditionally_safe_through_the_real_router() {
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
        let created_tag = response.headers()[http::header::ETAG].clone();
        let location = response.headers()[http::header::LOCATION]
            .to_str()
            .expect("location")
            .to_owned();
        let body = response
            .into_body()
            .collect()
            .await
            .expect("body")
            .to_bytes();
        let created: generated::OrderDocument = serde_json::from_slice(&body).expect("JSON");
        assert_eq!(location, format!("/orders/{}", created.data.id));

        let response = app
            .clone()
            .oneshot(
                http::Request::get(format!("/orders/{}", created.data.id))
                    .header("x-minco-subject", "test-user")
                    .header(
                        "x-minco-permissions",
                        "orders.read,orders.update,orders.delete",
                    )
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[http::header::ETAG], created_tag);

        let response = app
            .clone()
            .oneshot(
                http::Request::patch(format!("/orders/{}", created.data.id))
                    .header("content-type", "application/json")
                    .header(http::header::IF_MATCH, created_tag.clone())
                    .header("x-minco-subject", "test-user")
                    .header("x-minco-permissions", "orders.update")
                    .body(Body::from(r#"{"customerReference":"PO-43"}"#))
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::OK);
        let updated_tag = response.headers()[http::header::ETAG].clone();
        assert_ne!(updated_tag, created_tag);

        let stale = app
            .clone()
            .oneshot(
                http::Request::patch(format!("/orders/{}", created.data.id))
                    .header("content-type", "application/json")
                    .header(http::header::IF_MATCH, created_tag.clone())
                    .header("x-minco-subject", "test-user")
                    .header("x-minco-permissions", "orders.update")
                    .body(Body::from(r#"{"customerReference":"PO-44"}"#))
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(stale.status(), StatusCode::PRECONDITION_FAILED);

        let missing = app
            .clone()
            .oneshot(
                http::Request::delete(format!("/orders/{}", created.data.id))
                    .header("x-minco-subject", "test-user")
                    .header("x-minco-permissions", "orders.delete")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(missing.status(), StatusCode::PRECONDITION_REQUIRED);

        let deleted = app
            .clone()
            .oneshot(
                http::Request::delete(format!("/orders/{}", created.data.id))
                    .header(http::header::IF_MATCH, updated_tag)
                    .header("x-minco-subject", "test-user")
                    .header("x-minco-permissions", "orders.delete")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(deleted.status(), StatusCode::NO_CONTENT);

        let replayed = app
            .clone()
            .oneshot(
                http::Request::post("/orders")
                    .header("content-type", "application/json")
                    .header("idempotency-key", "test-order-1")
                    .header("x-minco-subject", "test-user")
                    .header("x-minco-permissions", "orders.create")
                    .body(Body::from(
                        r#"{"customerReference":"PO-42","lines":[{"sku":"SKU-1","quantity":2}]}"#,
                    ))
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(replayed.status(), StatusCode::OK);
        assert_eq!(replayed.headers()[http::header::ETAG], created_tag);
        assert_eq!(
            replayed.headers()[http::header::LOCATION],
            location.as_str()
        );
        let replayed: generated::OrderDocument = serde_json::from_slice(
            &replayed
                .into_body()
                .collect()
                .await
                .expect("body")
                .to_bytes(),
        )
        .expect("replayed document");
        assert_eq!(replayed, created);

        let forbidden_history = app
            .clone()
            .oneshot(
                http::Request::get(format!("/orders/{}/audit", created.data.id))
                    .header("x-minco-subject", "test-user")
                    .header("x-minco-permissions", "orders.read")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(forbidden_history.status(), StatusCode::FORBIDDEN);

        let history = app
            .clone()
            .oneshot(
                http::Request::get(format!("/orders/{}/audit", created.data.id))
                    .header("x-minco-subject", "auditor")
                    .header("x-minco-permissions", "orders.audit.read")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(history.status(), StatusCode::OK);
        let history: generated::OrderAuditCollection = serde_json::from_slice(
            &history
                .into_body()
                .collect()
                .await
                .expect("body")
                .to_bytes(),
        )
        .expect("audit history");
        assert_eq!(history.data.len(), 3);
        assert_eq!(history.data[0].action, "order.deleted");
        assert_eq!(history.data[1].action, "order.updated");
        assert_eq!(history.data[2].action, "order.created");
        assert_eq!(history.data[0].actor.subject.as_deref(), Some("test-user"));
        let customer_reference = history.data[2]
            .changes
            .iter()
            .find(|change| change.field == "customer_reference")
            .and_then(|change| change.after.as_deref())
            .expect("digested customer reference");
        assert!(customer_reference.starts_with("sha256:"));
        assert!(!serde_json::to_string(&history).unwrap().contains("PO-42"));

        let gone = app
            .oneshot(
                http::Request::get(format!("/orders/{}", created.data.id))
                    .header("x-minco-subject", "test-user")
                    .header("x-minco-permissions", "orders.read")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(gone.status(), StatusCode::NOT_FOUND);
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

    #[tokio::test]
    async fn list_orders_uses_opaque_bound_cursors_and_a_stable_page_shape() {
        let app = app();
        for index in 0..3 {
            place(&app, &format!("list-{index}"), &format!("PO-{index}")).await;
        }
        let response = app
            .clone()
            .oneshot(
                http::Request::get("/orders?page%5Blimit%5D=2")
                    .header("x-minco-subject", "test-user")
                    .header("x-minco-permissions", "orders.read")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::OK);
        let first: generated::OrderCollection = serde_json::from_slice(
            &response
                .into_body()
                .collect()
                .await
                .expect("body")
                .to_bytes(),
        )
        .expect("collection");
        assert_eq!(first.data.len(), 2);
        assert!(first.page.has_more);
        let cursor = first.page.next_cursor.expect("next cursor");

        let response = app
            .clone()
            .oneshot(
                http::Request::get(format!(
                    "/orders?page%5Blimit%5D=2&page%5Bafter%5D={cursor}"
                ))
                .header("x-minco-subject", "test-user")
                .header("x-minco-permissions", "orders.read")
                .body(Body::empty())
                .expect("request"),
            )
            .await
            .expect("response");
        let second: generated::OrderCollection = serde_json::from_slice(
            &response
                .into_body()
                .collect()
                .await
                .expect("body")
                .to_bytes(),
        )
        .expect("collection");
        assert_eq!(second.data.len(), 1);
        assert!(!second.page.has_more);
        assert!(first.data.iter().all(|order| order.id != second.data[0].id));

        let mismatched = app
            .oneshot(
                http::Request::get(format!("/orders?page%5Bafter%5D={cursor}&sort=id"))
                    .header("x-minco-subject", "test-user")
                    .header("x-minco-permissions", "orders.read")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(mismatched.status(), StatusCode::BAD_REQUEST);
    }
}
