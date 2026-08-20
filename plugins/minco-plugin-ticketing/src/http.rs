use crate::{
    CreateTicketInput, ExternalMessageIdentity, IssueTicketingHandoffInput, RequesterTicket,
    Ticket, TicketChannel, TicketFromHandoffInput, TicketId, TicketListFilter, TicketPriority,
    TicketStatus, TicketStoreError, TicketingMutationResult, TicketingService,
    TicketingServiceError,
};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, FromRequest, FromRequestParts, Path, RawQuery, Request, State},
    response::{IntoResponse, Response},
    routing::{get, patch, post},
};
use chrono::{DateTime, Utc};
use http::{HeaderMap, HeaderValue, StatusCode, header};
use minco_http::{ApiFailure, Cursor, ResourceCollection, StrongEntityTag, parse_if_match};
use minco_interaction::{
    AttachmentLimits, SupportBootstrap, SupportContext, SupportHandoffResult, SupportHandoffToken,
    SupportSurface,
};
use minco_plugin_identity::Identity;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{collections::BTreeSet, str::FromStr};
use uuid::Uuid;

pub const TICKETING_BASE_PATH: &str = "/_minco/ticketing";
pub const HANDOFF_HEADER: &str = "x-minco-ticketing-handoff";
const SUPPORT_ENTRY_SOURCE: &str = include_str!("../assets/support-entry.js");
const MAX_JSON_BODY_BYTES: usize = 256 * 1024;

#[derive(Debug, Clone)]
struct TicketingHttpState {
    service: TicketingService,
}

struct RequiredIdentity(Identity);

impl<S> FromRequestParts<S> for RequiredIdentity
where
    S: Send + Sync,
{
    type Rejection = ApiFailure;

    async fn from_request_parts(
        parts: &mut http::request::Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        let request_id = request_id(&parts.headers);
        let principal = parts
            .extensions
            .get::<minco_http::Principal>()
            .cloned()
            .ok_or_else(|| identity_required(&request_id))?;
        Ok(Self(identity(principal)))
    }
}

struct SensitiveHandoff(SupportHandoffToken);

impl<S> FromRequestParts<S> for SensitiveHandoff
where
    S: Send + Sync,
{
    type Rejection = ApiFailure;

    async fn from_request_parts(
        parts: &mut http::request::Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        let request_id = request_id(&parts.headers);
        sensitive_handoff(&parts.headers, &request_id).map(Self)
    }
}

struct ApiJson<T>(T);

impl<S, T> FromRequest<S> for ApiJson<T>
where
    S: Send + Sync,
    T: DeserializeOwned,
{
    type Rejection = ApiFailure;

    async fn from_request(request: Request, state: &S) -> Result<Self, Self::Rejection> {
        let request_id = request_id(request.headers());
        Json::<T>::from_request(request, state)
            .await
            .map(|Json(value)| Self(value))
            .map_err(|rejection| json_rejection(rejection.status(), &request_id))
    }
}

pub fn ticketing_router(service: TicketingService) -> Router {
    let routes = Router::new()
        .route("/support-entry.js", get(support_entry_source))
        .route("/bootstrap", get(bootstrap))
        .route("/integrations/handoffs", post(issue_handoff))
        .route("/handoffs/exchange", post(exchange_handoff))
        .route("/tickets/from-handoff", post(exchange_handoff))
        .route("/tickets", post(create_ticket).get(list_tickets))
        .route("/tickets/{ticketId}", get(get_ticket))
        .route(
            "/tickets/{ticketId}/requester-replies",
            post(requester_reply),
        )
        .route("/tickets/{ticketId}/agent-replies", post(agent_reply))
        .route("/tickets/{ticketId}/internal-notes", post(internal_note))
        .route("/tickets/{ticketId}/assignment", patch(change_assignment))
        .route("/tickets/{ticketId}/queue", patch(change_queue))
        .route("/tickets/{ticketId}/priority", patch(change_priority))
        .route("/tickets/{ticketId}/status", patch(change_status))
        .route("/ingress/messages", post(ingest_external_message))
        .route("/tickets/{ticketId}/ai-context", get(ai_context))
        .layer(DefaultBodyLimit::max(MAX_JSON_BODY_BYTES))
        .with_state(TicketingHttpState { service });
    Router::new().nest(TICKETING_BASE_PATH, routes)
}

async fn support_entry_source() -> Response {
    let mut response = SUPPORT_ENTRY_SOURCE.into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/javascript; charset=utf-8"),
    );
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=300"),
    );
    response
}

async fn bootstrap(State(state): State<TicketingHttpState>) -> Json<SupportBootstrap> {
    let config = state.service.config();
    Json(SupportBootstrap {
        schema_version: 1,
        project_id: config.project_id.clone(),
        portal_origin: config.portal_origin.clone(),
        label: config.support_label.clone(),
        brand: config.support_brand.clone(),
        enabled_surfaces: vec![
            SupportSurface::Widget,
            SupportSurface::Portal,
            SupportSurface::Api,
        ],
        screenshot_enabled: true,
        voice_enabled: true,
        file_enabled: true,
        attachment_limits: AttachmentLimits {
            count: 8,
            screenshot_bytes: 4 * 1024 * 1024,
            audio_bytes: 5 * 1024 * 1024,
            file_bytes: 5 * 1024 * 1024,
            aggregate_bytes: 8 * 1024 * 1024,
        },
        recording_limit: 90,
        privacy_notice: config.privacy_notice.clone(),
    })
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct IssueHandoffBody {
    project_id: String,
    requester_subject: String,
    #[serde(default)]
    requester_permissions: Vec<String>,
    surface: SupportSurface,
    context: SupportContext,
    return_location: String,
}

#[derive(Debug, Serialize)]
struct IssuedHandoffResponse {
    launch_url: String,
    expires_at: String,
}

async fn issue_handoff(
    State(state): State<TicketingHttpState>,
    RequiredIdentity(identity): RequiredIdentity,
    headers: HeaderMap,
    ApiJson(body): ApiJson<IssueHandoffBody>,
) -> Result<Json<IssuedHandoffResponse>, ApiFailure> {
    let request_id = request_id(&headers);
    let grant = state
        .service
        .issue_ticketing_handoff(
            &identity,
            IssueTicketingHandoffInput {
                project_id: body.project_id,
                requester_subject: body.requester_subject,
                requester_permissions: body.requester_permissions,
                surface: body.surface,
                context: body.context,
                return_location: body.return_location,
                correlation_id: request_uuid(&request_id),
            },
            Utc::now(),
        )
        .await
        .map_err(|error| map_error(error, &request_id))?;
    Ok(Json(IssuedHandoffResponse {
        launch_url: grant.launch_url(),
        expires_at: grant.expires_at.to_rfc3339(),
    }))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExchangeHandoffBody {
    project_id: String,
    portal_origin: String,
    subject: String,
    description: String,
    channel: TicketChannel,
    #[serde(default)]
    priority: TicketPriority,
}

#[derive(Debug, Serialize)]
struct ConsumedHandoffResponse {
    ticket: RequesterTicket,
    result: SupportHandoffResult,
    repeated: bool,
}

async fn exchange_handoff(
    State(state): State<TicketingHttpState>,
    SensitiveHandoff(token): SensitiveHandoff,
    headers: HeaderMap,
    ApiJson(body): ApiJson<ExchangeHandoffBody>,
) -> Result<(StatusCode, Json<ConsumedHandoffResponse>), ApiFailure> {
    let request_id = request_id(&headers);
    let result = state
        .service
        .create_ticket_from_handoff(
            token,
            &body.project_id,
            &body.portal_origin,
            TicketFromHandoffInput {
                subject: body.subject,
                description: body.description,
                channel: body.channel,
                priority: body.priority,
            },
            Utc::now(),
        )
        .await
        .map_err(|error| map_error(error, &request_id))?;
    let status = if result.repeated {
        StatusCode::OK
    } else {
        StatusCode::CREATED
    };
    Ok((
        status,
        Json(ConsumedHandoffResponse {
            ticket: result.ticket.requester_projection(),
            result: result.result,
            repeated: result.repeated,
        }),
    ))
}

async fn create_ticket(
    State(state): State<TicketingHttpState>,
    RequiredIdentity(principal): RequiredIdentity,
    headers: HeaderMap,
    ApiJson(input): ApiJson<CreateTicketInput>,
) -> Result<Response, ApiFailure> {
    let request_id = request_id(&headers);
    let result = state
        .service
        .create_ticket(&principal, input, request_uuid(&request_id), Utc::now())
        .await
        .map_err(|error| map_error(error, &request_id))?;
    mutation_response(StatusCode::CREATED, result)
}

async fn get_ticket(
    State(state): State<TicketingHttpState>,
    RequiredIdentity(principal): RequiredIdentity,
    Path(ticket_id): Path<String>,
    headers: HeaderMap,
) -> Result<Response, ApiFailure> {
    let request_id = request_id(&headers);
    let id = parse_ticket_id(&ticket_id, &request_id)?;
    let ticket = state
        .service
        .get_ticket_for_agent(&principal, &state.service.config().project_id, id)
        .await
        .map_err(|error| map_error(error, &request_id))?;
    ticket_response(StatusCode::OK, &ticket)
}

async fn list_tickets(
    State(state): State<TicketingHttpState>,
    RequiredIdentity(principal): RequiredIdentity,
    RawQuery(raw): RawQuery,
    headers: HeaderMap,
) -> Result<Json<ResourceCollection<Ticket>>, ApiFailure> {
    let request_id = request_id(&headers);
    let query = parse_list_query(raw.as_deref(), &request_id)?;
    let mut tickets = state
        .service
        .list_tickets(
            &principal,
            TicketListFilter {
                project_id: state.service.config().project_id.clone(),
                statuses: query.status.into_iter().collect(),
                queue_id: query.queue_id,
                assignee_subject: query.assignee_subject,
                requester_subject: query.requester_subject,
                after_updated_at: query.after.map(|value| value.0),
                after_id: query.after.map(|value| value.1),
                limit: query.limit + 1,
            },
        )
        .await
        .map_err(|error| map_error(error, &request_id))?;
    let has_more = tickets.len() > query.limit;
    tickets.truncate(query.limit);
    let next = if has_more {
        tickets
            .last()
            .map(|ticket| Cursor::new(encode_cursor(ticket)))
            .transpose()
            .map_err(|_| ApiFailure::internal(&request_id))?
    } else {
        None
    };
    Ok(Json(ResourceCollection::new(tickets, next)))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReplyBody {
    body: String,
}

async fn requester_reply(
    State(state): State<TicketingHttpState>,
    RequiredIdentity(principal): RequiredIdentity,
    Path(ticket_id): Path<String>,
    headers: HeaderMap,
    ApiJson(body): ApiJson<ReplyBody>,
) -> Result<Response, ApiFailure> {
    let request_id = request_id(&headers);
    let id = parse_ticket_id(&ticket_id, &request_id)?;
    let revision = expected_revision(&headers, id, &request_id)?;
    let result = state
        .service
        .reply_as_requester(
            &principal,
            &state.service.config().project_id,
            id,
            body.body,
            revision,
            request_uuid(&request_id),
            Utc::now(),
        )
        .await
        .map_err(|error| map_error(error, &request_id))?;
    requester_response(StatusCode::OK, result)
}

async fn agent_reply(
    State(state): State<TicketingHttpState>,
    RequiredIdentity(principal): RequiredIdentity,
    Path(ticket_id): Path<String>,
    headers: HeaderMap,
    ApiJson(body): ApiJson<ReplyBody>,
) -> Result<Response, ApiFailure> {
    mutation_with_body(
        &state,
        principal,
        ticket_id,
        headers,
        body.body,
        MutationKind::AgentReply,
    )
    .await
}

async fn internal_note(
    State(state): State<TicketingHttpState>,
    RequiredIdentity(principal): RequiredIdentity,
    Path(ticket_id): Path<String>,
    headers: HeaderMap,
    ApiJson(body): ApiJson<ReplyBody>,
) -> Result<Response, ApiFailure> {
    mutation_with_body(
        &state,
        principal,
        ticket_id,
        headers,
        body.body,
        MutationKind::InternalNote,
    )
    .await
}

enum MutationKind {
    AgentReply,
    InternalNote,
}

async fn mutation_with_body(
    state: &TicketingHttpState,
    principal: Identity,
    ticket_id: String,
    headers: HeaderMap,
    body: String,
    kind: MutationKind,
) -> Result<Response, ApiFailure> {
    let request_id = request_id(&headers);
    let id = parse_ticket_id(&ticket_id, &request_id)?;
    let revision = expected_revision(&headers, id, &request_id)?;
    let result = match kind {
        MutationKind::AgentReply => {
            state
                .service
                .reply_as_agent(
                    &principal,
                    &state.service.config().project_id,
                    id,
                    body,
                    revision,
                    request_uuid(&request_id),
                    Utc::now(),
                )
                .await
        }
        MutationKind::InternalNote => {
            state
                .service
                .add_internal_note(
                    &principal,
                    &state.service.config().project_id,
                    id,
                    body,
                    revision,
                    request_uuid(&request_id),
                    Utc::now(),
                )
                .await
        }
    }
    .map_err(|error| map_error(error, &request_id))?;
    mutation_response(StatusCode::OK, result)
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AssignmentBody {
    assignee_subject: Option<String>,
}

async fn change_assignment(
    State(state): State<TicketingHttpState>,
    RequiredIdentity(principal): RequiredIdentity,
    Path(ticket_id): Path<String>,
    headers: HeaderMap,
    ApiJson(body): ApiJson<AssignmentBody>,
) -> Result<Response, ApiFailure> {
    let request_id = request_id(&headers);
    let id = parse_ticket_id(&ticket_id, &request_id)?;
    let revision = expected_revision(&headers, id, &request_id)?;
    let result = if let Some(assignee) = body.assignee_subject {
        state
            .service
            .assign_ticket(
                &principal,
                &state.service.config().project_id,
                id,
                assignee,
                revision,
                request_uuid(&request_id),
                Utc::now(),
            )
            .await
    } else {
        state
            .service
            .unassign_ticket(
                &principal,
                &state.service.config().project_id,
                id,
                revision,
                request_uuid(&request_id),
                Utc::now(),
            )
            .await
    }
    .map_err(|error| map_error(error, &request_id))?;
    mutation_response(StatusCode::OK, result)
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct QueueBody {
    queue_id: String,
}
async fn change_queue(
    State(state): State<TicketingHttpState>,
    RequiredIdentity(principal): RequiredIdentity,
    Path(ticket_id): Path<String>,
    headers: HeaderMap,
    ApiJson(body): ApiJson<QueueBody>,
) -> Result<Response, ApiFailure> {
    let request_id = request_id(&headers);
    let id = parse_ticket_id(&ticket_id, &request_id)?;
    let revision = expected_revision(&headers, id, &request_id)?;
    let result = state
        .service
        .transfer_queue(
            &principal,
            &state.service.config().project_id,
            id,
            body.queue_id,
            revision,
            request_uuid(&request_id),
            Utc::now(),
        )
        .await
        .map_err(|error| map_error(error, &request_id))?;
    mutation_response(StatusCode::OK, result)
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PriorityBody {
    priority: TicketPriority,
}
async fn change_priority(
    State(state): State<TicketingHttpState>,
    RequiredIdentity(principal): RequiredIdentity,
    Path(ticket_id): Path<String>,
    headers: HeaderMap,
    ApiJson(body): ApiJson<PriorityBody>,
) -> Result<Response, ApiFailure> {
    let request_id = request_id(&headers);
    let id = parse_ticket_id(&ticket_id, &request_id)?;
    let revision = expected_revision(&headers, id, &request_id)?;
    let result = state
        .service
        .change_priority(
            &principal,
            &state.service.config().project_id,
            id,
            body.priority,
            revision,
            request_uuid(&request_id),
            Utc::now(),
        )
        .await
        .map_err(|error| map_error(error, &request_id))?;
    mutation_response(StatusCode::OK, result)
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StatusBody {
    status: TicketStatus,
    resolution: Option<String>,
    close_reason: Option<String>,
}
async fn change_status(
    State(state): State<TicketingHttpState>,
    RequiredIdentity(principal): RequiredIdentity,
    Path(ticket_id): Path<String>,
    headers: HeaderMap,
    ApiJson(body): ApiJson<StatusBody>,
) -> Result<Response, ApiFailure> {
    let request_id = request_id(&headers);
    let id = parse_ticket_id(&ticket_id, &request_id)?;
    let revision = expected_revision(&headers, id, &request_id)?;
    let result = state
        .service
        .change_status(
            &principal,
            &state.service.config().project_id,
            id,
            body.status,
            body.resolution,
            body.close_reason,
            revision,
            request_uuid(&request_id),
            Utc::now(),
        )
        .await
        .map_err(|error| map_error(error, &request_id))?;
    mutation_response(StatusCode::OK, result)
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct IngressBody {
    provider: String,
    mailbox_scope: String,
    external_message_id: String,
    content_sha256: String,
    raw_message_object_key: Option<String>,
    internet_message_id: Option<String>,
    in_reply_to: Option<String>,
    #[serde(default)]
    references: Vec<String>,
    ticket_id: TicketId,
    body: String,
    expected_revision: u64,
}
async fn ingest_external_message(
    State(state): State<TicketingHttpState>,
    RequiredIdentity(principal): RequiredIdentity,
    headers: HeaderMap,
    ApiJson(body): ApiJson<IngressBody>,
) -> Result<Response, ApiFailure> {
    let request_id = request_id(&headers);
    let identity_record = ExternalMessageIdentity {
        project_id: state.service.config().project_id.clone(),
        provider: body.provider,
        mailbox_scope: body.mailbox_scope,
        external_id: body.external_message_id,
        content_sha256: body.content_sha256,
        raw_message_object_key: body.raw_message_object_key,
        internet_message_id: body.internet_message_id,
        in_reply_to: body.in_reply_to,
        references: body.references,
    };
    let result = state
        .service
        .ingest_external_message(
            &principal,
            identity_record,
            body.ticket_id,
            body.body,
            body.expected_revision,
            request_uuid(&request_id),
            Utc::now(),
        )
        .await
        .map_err(|error| map_error(error, &request_id))?;
    mutation_response(StatusCode::OK, result)
}

async fn ai_context(
    State(state): State<TicketingHttpState>,
    RequiredIdentity(principal): RequiredIdentity,
    Path(ticket_id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<crate::TicketAiContext>, ApiFailure> {
    let request_id = request_id(&headers);
    let id = parse_ticket_id(&ticket_id, &request_id)?;
    state
        .service
        .export_ai_context(&principal, &state.service.config().project_id, id)
        .await
        .map(Json)
        .map_err(|error| map_error(error, &request_id))
}

#[derive(Debug)]
struct ListQuery {
    limit: usize,
    after: Option<(DateTime<Utc>, TicketId)>,
    status: Option<TicketStatus>,
    queue_id: Option<String>,
    assignee_subject: Option<String>,
    requester_subject: Option<String>,
}

fn parse_list_query(raw: Option<&str>, request_id: &str) -> Result<ListQuery, ApiFailure> {
    let mut query = ListQuery {
        limit: 50,
        after: None,
        status: None,
        queue_id: None,
        assignee_subject: None,
        requester_subject: None,
    };
    let mut seen = BTreeSet::new();
    for (name, value) in url::form_urlencoded::parse(raw.unwrap_or_default().as_bytes()) {
        let name = name.into_owned();
        let value = value.into_owned();
        if !seen.insert(name.clone()) {
            return Err(ApiFailure::validation(
                "ticket list query repeats a parameter",
                request_id,
            ));
        }
        match name.as_str() {
            "page[limit]" => {
                query.limit = value
                    .parse()
                    .ok()
                    .filter(|value| (1..=200).contains(value))
                    .ok_or_else(|| {
                        ApiFailure::validation("page limit must be between 1 and 200", request_id)
                    })?;
            }
            "page[after]" => {
                if value.len() > 512 {
                    return Err(ApiFailure::validation("page cursor is invalid", request_id));
                }
                query.after =
                    Some(decode_cursor(&value).ok_or_else(|| {
                        ApiFailure::validation("page cursor is invalid", request_id)
                    })?);
            }
            "filter[status]" => {
                query.status = Some(
                    serde_json::from_value(serde_json::Value::String(value)).map_err(|_| {
                        ApiFailure::validation("status filter is invalid", request_id)
                    })?,
                );
            }
            "filter[queue_id]" => {
                query.queue_id = Some(bounded_query_value(value, 200, request_id)?);
            }
            "filter[assignee_subject]" => {
                query.assignee_subject = Some(bounded_query_value(value, 300, request_id)?);
            }
            "filter[requester_subject]" => {
                query.requester_subject = Some(bounded_query_value(value, 300, request_id)?);
            }
            _ => {
                return Err(ApiFailure::validation(
                    "ticket list query contains an unsupported parameter",
                    request_id,
                ));
            }
        }
    }
    Ok(query)
}

fn bounded_query_value(
    value: String,
    maximum: usize,
    request_id: &str,
) -> Result<String, ApiFailure> {
    if value.trim().is_empty()
        || value.chars().count() > maximum
        || value.chars().any(char::is_control)
    {
        Err(ApiFailure::validation(
            "ticket list filter is invalid",
            request_id,
        ))
    } else {
        Ok(value)
    }
}

fn encode_cursor(ticket: &Ticket) -> String {
    format!(
        "{}.{:09}_{}",
        ticket.updated_at.timestamp(),
        ticket.updated_at.timestamp_subsec_nanos(),
        ticket.id.0.simple()
    )
}

fn decode_cursor(value: &str) -> Option<(DateTime<Utc>, TicketId)> {
    let (timestamp, id) = value.split_once('_')?;
    let (seconds, nanos) = timestamp.split_once('.')?;
    if nanos.len() != 9 {
        return None;
    }
    let updated = DateTime::from_timestamp(seconds.parse().ok()?, nanos.parse().ok()?)?;
    Some((updated, TicketId(Uuid::parse_str(id).ok()?)))
}

fn ticket_response(status: StatusCode, ticket: &Ticket) -> Result<Response, ApiFailure> {
    let mut response = (status, Json(ticket)).into_response();
    response
        .headers_mut()
        .insert(header::ETAG, ticket_etag(ticket)?.to_header_value());
    Ok(response)
}

fn mutation_response(
    status: StatusCode,
    result: TicketingMutationResult,
) -> Result<Response, ApiFailure> {
    let tag = ticket_etag(&result.ticket)?;
    let mut response = (status, Json(result)).into_response();
    response
        .headers_mut()
        .insert(header::ETAG, tag.to_header_value());
    Ok(response)
}

fn requester_response(
    status: StatusCode,
    result: crate::RequesterTicketResult,
) -> Result<Response, ApiFailure> {
    let tag = StrongEntityTag::for_resource(
        "ticket",
        &result.ticket.id.to_string(),
        result.ticket.revision + 1,
    )
    .map_err(|_| ApiFailure::internal("unavailable"))?;
    let mut response = (status, Json(result)).into_response();
    response
        .headers_mut()
        .insert(header::ETAG, tag.to_header_value());
    Ok(response)
}

fn ticket_etag(ticket: &Ticket) -> Result<StrongEntityTag, ApiFailure> {
    StrongEntityTag::for_resource("ticket", &ticket.id.to_string(), ticket.revision + 1)
        .map_err(|_| ApiFailure::internal("unavailable"))
}

fn expected_revision(
    headers: &HeaderMap,
    id: TicketId,
    request_id: &str,
) -> Result<u64, ApiFailure> {
    let tag = parse_if_match(headers).map_err(|error| match error {
        minco_http::EntityTagError::PreconditionRequired => {
            ApiFailure::precondition_required(request_id)
        }
        _ => ApiFailure::invalid_if_match(request_id),
    })?;
    tag.resource_revision("ticket", &id.to_string())
        .map_err(|_| ApiFailure::invalid_if_match(request_id))?
        .checked_sub(1)
        .ok_or_else(|| ApiFailure::invalid_if_match(request_id))
}

fn sensitive_handoff(
    headers: &HeaderMap,
    request_id: &str,
) -> Result<SupportHandoffToken, ApiFailure> {
    let value = headers
        .get(HANDOFF_HEADER)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| {
            ApiFailure::new(
                StatusCode::UNAUTHORIZED,
                "ticketing_handoff_required",
                "Handoff required",
                "Supply the one-time handoff in X-Minco-Ticketing-Handoff.",
                request_id,
            )
        })?;
    SupportHandoffToken::parse(value.to_owned()).map_err(|_| {
        ApiFailure::new(
            StatusCode::UNAUTHORIZED,
            "ticketing_handoff_invalid",
            "Handoff invalid",
            "The one-time handoff is invalid.",
            request_id,
        )
    })
}

fn identity_required(request_id: &str) -> ApiFailure {
    ApiFailure::new(
        StatusCode::UNAUTHORIZED,
        "ticketing_identity_required",
        "Identity required",
        "This operation requires an authenticated principal.",
        request_id,
    )
}

fn json_rejection(status: StatusCode, request_id: &str) -> ApiFailure {
    match status {
        StatusCode::PAYLOAD_TOO_LARGE => ApiFailure::new(
            status,
            "ticketing_body_too_large",
            "Request body too large",
            "The JSON request body exceeds the 256 KiB limit.",
            request_id,
        ),
        StatusCode::UNSUPPORTED_MEDIA_TYPE => ApiFailure::new(
            status,
            "ticketing_json_required",
            "JSON required",
            "Use Content-Type application/json for this operation.",
            request_id,
        ),
        StatusCode::UNPROCESSABLE_ENTITY => ApiFailure::validation(
            "The JSON request body does not match the operation schema.",
            request_id,
        ),
        StatusCode::BAD_REQUEST => ApiFailure::new(
            status,
            "ticketing_json_invalid",
            "Invalid JSON",
            "The request body is not valid JSON.",
            request_id,
        ),
        _ => ApiFailure::internal(request_id),
    }
}

fn identity(principal: minco_http::Principal) -> Identity {
    Identity {
        subject: principal.subject,
        permissions: principal.permissions,
        scopes: BTreeSet::new(),
        claims: principal.claims,
    }
}

fn parse_ticket_id(value: &str, request_id: &str) -> Result<TicketId, ApiFailure> {
    TicketId::from_str(value)
        .map_err(|_| ApiFailure::validation("ticket ID must be a UUID", request_id))
}

fn request_id(headers: &HeaderMap) -> String {
    headers
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .filter(|value| {
            !value.trim().is_empty()
                && value.len() <= 128
                && value.bytes().all(|byte| byte.is_ascii_graphic())
        })
        .map_or_else(|| Uuid::now_v7().to_string(), str::to_owned)
}

fn request_uuid(request_id: &str) -> Uuid {
    Uuid::parse_str(request_id).unwrap_or_else(|_| Uuid::now_v7())
}

fn map_error(error: TicketingServiceError, request_id: &str) -> ApiFailure {
    match error {
        TicketingServiceError::PermissionDenied(_) => ApiFailure::new(
            StatusCode::FORBIDDEN,
            "ticketing_permission_denied",
            "Permission denied",
            "The authenticated principal cannot perform this ticketing operation.",
            request_id,
        ),
        TicketingServiceError::ProjectDenied
        | TicketingServiceError::RequesterMismatch
        | TicketingServiceError::NotFound(_)
        | TicketingServiceError::Store(TicketStoreError::NotFound(_)) => ApiFailure::new(
            StatusCode::NOT_FOUND,
            "ticket_not_found",
            "Ticket not found",
            "The ticket does not exist or is not accessible.",
            request_id,
        ),
        TicketingServiceError::StaleRevision { .. }
        | TicketingServiceError::Store(TicketStoreError::StaleRevision { .. }) => {
            ApiFailure::precondition_failed(request_id)
        }
        TicketingServiceError::Validation(error) => {
            ApiFailure::validation(error.to_string(), request_id)
        }
        value @ (TicketingServiceError::SupportEntry(_)
        | TicketingServiceError::InvalidContentDigest
        | TicketingServiceError::InvalidExternalIdentity) => {
            ApiFailure::validation(value.to_string(), request_id)
        }
        TicketingServiceError::Store(
            TicketStoreError::ExpiredHandoff
            | TicketStoreError::UnknownHandoff
            | TicketStoreError::WrongHandoffProject
            | TicketStoreError::WrongHandoffPortal,
        ) => ApiFailure::new(
            StatusCode::UNAUTHORIZED,
            "ticketing_handoff_invalid",
            "Handoff invalid",
            "The handoff is unknown, expired, or not valid for this project and portal.",
            request_id,
        ),
        TicketingServiceError::Store(TicketStoreError::HandoffAlreadyConsumed) => ApiFailure::new(
            StatusCode::CONFLICT,
            "ticketing_handoff_consumed",
            "Handoff consumed",
            "The handoff has already completed a different request.",
            request_id,
        ),
        TicketingServiceError::Configuration(_) => ApiFailure::internal(request_id),
        TicketingServiceError::Store(_) => ApiFailure::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "ticketing_unavailable",
            "Ticketing unavailable",
            "Ticketing persistence is temporarily unavailable.",
            request_id,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        CreateTicketInput, MemoryTicketingStore, TicketChannel, TicketPriority, TicketRequester,
        TicketingConfig, TicketingStoreService,
    };
    use axum::body::Body;
    use axum::body::to_bytes;
    use http::Request;
    use std::{collections::BTreeMap, sync::Arc};
    use tower::ServiceExt;

    fn service() -> TicketingService {
        TicketingService::new(
            TicketingStoreService::new(Arc::new(MemoryTicketingStore::default())),
            TicketingConfig {
                project_id: "project-a".into(),
                portal_origin: "https://support.example.test".into(),
                allowed_return_paths: BTreeMap::from([(
                    "https://app.example.test".into(),
                    vec!["/orders".into()],
                )]),
                ..TicketingConfig::default()
            },
        )
        .unwrap()
    }

    #[tokio::test]
    async fn launcher_is_public_javascript_and_bootstrap_contains_no_secret() {
        let app = ticketing_router(service());
        let response = app
            .clone()
            .oneshot(
                Request::get("/_minco/ticketing/support-entry.js")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers()[header::CONTENT_TYPE],
            "application/javascript; charset=utf-8"
        );
        let bootstrap = app
            .oneshot(
                Request::get("/_minco/ticketing/bootstrap")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let body = to_bytes(bootstrap.into_body(), usize::MAX).await.unwrap();
        assert!(!String::from_utf8_lossy(&body).contains("handoff="));
    }

    #[tokio::test]
    async fn private_issuance_returns_exact_browser_contract_and_token_is_header_only() {
        let app = ticketing_router(service()).layer(axum::Extension(minco_http::Principal {
            subject: "integration".into(),
            permissions: std::iter::once("ticketing.integrate".into()).collect(),
            claims: BTreeMap::new(),
        }));
        let body = serde_json::json!({
            "project_id":"project-a", "requester_subject":"user-1", "requester_permissions":["ticketing.create"],
            "surface":"widget", "context":{"page_url":"https://app.example.test/orders/1"}, "return_location":"https://app.example.test/orders/1"
        });
        let response = app
            .oneshot(
                Request::post("/_minco/ticketing/integrations/handoffs")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let value: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        assert_eq!(
            value.as_object().unwrap().keys().collect::<BTreeSet<_>>(),
            BTreeSet::from([&"expires_at".to_owned(), &"launch_url".to_owned()])
        );
        assert!(value["launch_url"].as_str().unwrap().contains("#handoff="));
        assert!(!value["launch_url"].as_str().unwrap().contains('?'));
    }

    #[tokio::test]
    async fn authenticated_json_rejections_are_problem_details_with_request_ids() {
        let app = ticketing_router(service());
        let unauthenticated = app
            .clone()
            .oneshot(
                Request::post("/_minco/ticketing/tickets")
                    .header(header::CONTENT_TYPE, "application/json")
                    .header("x-request-id", "req-unauthenticated")
                    .body(Body::from("{"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(unauthenticated.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(
            unauthenticated.headers()[header::CONTENT_TYPE],
            "application/problem+json"
        );
        let missing_handoff = app
            .clone()
            .oneshot(
                Request::post("/_minco/ticketing/handoffs/exchange")
                    .header(header::CONTENT_TYPE, "application/json")
                    .header("x-request-id", "req-handoff")
                    .body(Body::from("{"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(missing_handoff.status(), StatusCode::UNAUTHORIZED);

        let authenticated = app.layer(axum::Extension(minco_http::Principal {
            subject: "user-1".into(),
            permissions: std::iter::once("ticketing.create".into()).collect(),
            claims: BTreeMap::new(),
        }));
        let malformed = authenticated
            .oneshot(
                Request::post("/_minco/ticketing/tickets")
                    .header(header::CONTENT_TYPE, "application/json")
                    .header("x-request-id", "req-malformed")
                    .body(Body::from("{"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(malformed.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            malformed.headers()[header::CONTENT_TYPE],
            "application/problem+json"
        );
        assert_eq!(malformed.headers()["x-request-id"], "req-malformed");
        let problem: serde_json::Value =
            serde_json::from_slice(&to_bytes(malformed.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        assert_eq!(problem["code"], "ticketing_json_invalid");
        assert_eq!(problem["requestId"], "req-malformed");
        assert!(problem.get("request_id").is_none());
    }

    #[test]
    fn exchange_response_never_serializes_internal_notes_or_object_keys() {
        let now = Utc::now();
        let mut ticket = Ticket::create(
            CreateTicketInput {
                project_id: "project-a".into(),
                subject: "Help".into(),
                description: "Broken".into(),
                requester: TicketRequester {
                    subject: "user-1".into(),
                    display_name: None,
                    email: None,
                },
                channel: TicketChannel::Portal,
                priority: TicketPriority::Normal,
                resource_references: Vec::new(),
            },
            "TKT-PRIVATE",
            now,
        )
        .unwrap();
        ticket
            .add_internal_note("agent", "private note", now)
            .unwrap();
        let response = ConsumedHandoffResponse {
            ticket: ticket.requester_projection(),
            result: SupportHandoffResult {
                ticket_id: ticket.id.0,
                requester_session_id: Uuid::now_v7(),
            },
            repeated: false,
        };
        let encoded = serde_json::to_string(&response).unwrap();
        assert!(!encoded.contains("private note"));
        assert!(!encoded.contains("object_key"));
    }

    #[test]
    fn packaged_launcher_matches_the_canonical_hardened_example() {
        let canonical = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/ticketing-entry/support-entry.js");
        if canonical.exists() {
            assert_eq!(
                std::fs::read_to_string(canonical).unwrap(),
                SUPPORT_ENTRY_SOURCE
            );
        }
    }

    #[test]
    fn page_cursor_round_trips_sub_millisecond_precision() {
        let now = DateTime::from_timestamp(1_777_777_777, 123_456_789).unwrap();
        let ticket = Ticket::create(
            CreateTicketInput {
                project_id: "project-a".into(),
                subject: "Help".into(),
                description: "Broken".into(),
                requester: TicketRequester {
                    subject: "user-1".into(),
                    display_name: None,
                    email: None,
                },
                channel: TicketChannel::Api,
                priority: TicketPriority::Normal,
                resource_references: Vec::new(),
            },
            "TKT-CURSOR",
            now,
        )
        .unwrap();

        assert_eq!(
            decode_cursor(&encode_cursor(&ticket)),
            Some((now, ticket.id))
        );
    }
}
