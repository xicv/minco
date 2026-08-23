use crate::{
    AgentManagementInput, CreateTicketInput, ExternalMessageIdentity, IssueTicketingHandoffInput,
    RequesterTicket, Ticket, TicketChannel, TicketFromHandoffInput, TicketId, TicketListFilter,
    TicketPriority, TicketStatus, TicketStoreError, TicketSummary, TicketingMutationResult,
    TicketingService, TicketingServiceError,
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
const AGENT_CONSOLE_PAGE: &str = include_str!("../assets/agent-console.html");
const AGENT_CONSOLE_SCRIPT: &str = include_str!("../assets/agent-console.js");
const AGENT_CONSOLE_STYLES: &str = include_str!("../assets/agent-console.css");
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
        .route("/agent", get(agent_console_page))
        .route("/agent/console.js", get(agent_console_script))
        .route("/agent/console.css", get(agent_console_styles))
        .route("/agent/bootstrap", get(agent_bootstrap))
        .route("/agent/tickets", get(agent_tickets))
        .route("/agent/tickets/{ticketId}", get(agent_ticket))
        .route(
            "/agent/tickets/{ticketId}/management",
            patch(manage_agent_ticket),
        )
        .route("/requester/tickets", get(requester_tickets))
        .route("/requester/tickets/{ticketId}", get(requester_ticket))
        .route(
            "/requester/tickets/{ticketId}/replies",
            post(requester_reply),
        )
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

fn hardened_asset(mut response: Response, content_type: &'static str) -> Response {
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=300"),
    );
    headers.insert(
        http::header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        http::header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    response
}

async fn agent_console_page() -> Response {
    let mut response = AGENT_CONSOLE_PAGE.into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    let headers = response.headers_mut();
    headers.insert(
        http::header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(
            "default-src 'none'; script-src 'self'; style-src 'self'; \
             connect-src 'self'; img-src 'self' data:; base-uri 'none'; \
             form-action 'none'; frame-ancestors 'none'",
        ),
    );
    headers.insert(
        http::header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        http::header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    response
}

async fn agent_console_script() -> Response {
    hardened_asset(
        AGENT_CONSOLE_SCRIPT.into_response(),
        "application/javascript; charset=utf-8",
    )
}

async fn agent_console_styles() -> Response {
    hardened_asset(
        AGENT_CONSOLE_STYLES.into_response(),
        "text/css; charset=utf-8",
    )
}

async fn agent_bootstrap(
    State(state): State<TicketingHttpState>,
    RequiredIdentity(principal): RequiredIdentity,
) -> Result<Json<crate::AgentConsoleBootstrap>, ApiFailure> {
    state
        .service
        .agent_bootstrap(&principal)
        .map(Json)
        .map_err(|error| map_error(error, "agent-bootstrap"))
}

#[derive(Debug, Default)]
struct AgentListQuery {
    limit: usize,
    before: Option<(DateTime<Utc>, TicketId)>,
    statuses: BTreeSet<TicketStatus>,
    queue_id: Option<String>,
    assignee_subject: Option<String>,
    requester_subject: Option<String>,
}

fn parse_agent_list_query(
    raw: Option<&str>,
    request_id: &str,
) -> Result<AgentListQuery, ApiFailure> {
    let mut query = AgentListQuery {
        limit: 50,
        ..AgentListQuery::default()
    };
    let mut seen = BTreeSet::new();
    for (name, value) in url::form_urlencoded::parse(raw.unwrap_or_default().as_bytes()) {
        let name = name.into_owned();
        let value = value.into_owned();
        if !seen.insert(name.clone()) {
            return Err(ApiFailure::validation(
                "agent ticket list query repeats a parameter",
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
                query.before =
                    Some(decode_cursor(&value).ok_or_else(|| {
                        ApiFailure::validation("page cursor is invalid", request_id)
                    })?);
            }
            "filter[status]" => {
                let status: TicketStatus = serde_json::from_value(serde_json::Value::String(value))
                    .map_err(|_| ApiFailure::validation("status filter is invalid", request_id))?;
                query.statuses.insert(status);
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
                    "agent ticket list query contains an unsupported parameter",
                    request_id,
                ));
            }
        }
    }
    Ok(query)
}

async fn agent_tickets(
    State(state): State<TicketingHttpState>,
    RequiredIdentity(principal): RequiredIdentity,
    RawQuery(raw): RawQuery,
    headers: HeaderMap,
) -> Result<Json<ResourceCollection<TicketSummary>>, ApiFailure> {
    let request_id = request_id(&headers);
    let query = parse_agent_list_query(raw.as_deref(), &request_id)?;
    let mut summaries = state
        .service
        .list_ticket_summaries(
            &principal,
            crate::TicketSummaryFilter {
                project_id: state.service.config().project_id.clone(),
                statuses: query.statuses,
                queue_id: query.queue_id,
                assignee_subject: query.assignee_subject,
                requester_subject: query.requester_subject,
                before_updated_at: query.before.map(|value| value.0),
                before_id: query.before.map(|value| value.1),
                limit: query.limit + 1,
            },
        )
        .await
        .map_err(|error| map_error(error, &request_id))?;
    let has_more = summaries.len() > query.limit;
    summaries.truncate(query.limit);
    let next = if has_more {
        summaries
            .last()
            .map(|summary| Cursor::new(encode_summary_cursor(summary)))
            .transpose()
            .map_err(|_| ApiFailure::internal(&request_id))?
    } else {
        None
    };
    Ok(Json(ResourceCollection::new(summaries, next)))
}

async fn agent_ticket(
    State(state): State<TicketingHttpState>,
    RequiredIdentity(principal): RequiredIdentity,
    Path(ticket_id): Path<String>,
    headers: HeaderMap,
) -> Result<Response, ApiFailure> {
    let request_id = request_id(&headers);
    let id = parse_ticket_id(&ticket_id, &request_id)?;
    let ticket = state
        .service
        .get_agent_ticket(&principal, &state.service.config().project_id, id)
        .await
        .map_err(|error| map_error(error, &request_id))?;
    ticket_response(StatusCode::OK, &ticket)
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AgentManagementBody {
    priority: Option<TicketPriority>,
    assignee_subject: Option<String>,
    #[serde(default)]
    clear_assignee: bool,
    queue_id: Option<String>,
    status: Option<TicketStatus>,
    resolution: Option<String>,
    close_reason: Option<String>,
}

async fn manage_agent_ticket(
    State(state): State<TicketingHttpState>,
    RequiredIdentity(principal): RequiredIdentity,
    Path(ticket_id): Path<String>,
    headers: HeaderMap,
    ApiJson(body): ApiJson<AgentManagementBody>,
) -> Result<Response, ApiFailure> {
    let request_id = request_id(&headers);
    let id = parse_ticket_id(&ticket_id, &request_id)?;
    let revision = expected_revision(&headers, id, &request_id)?;
    let result = state
        .service
        .manage_ticket(
            &principal,
            &state.service.config().project_id,
            id,
            AgentManagementInput {
                priority: body.priority,
                assignee_subject: body.assignee_subject,
                clear_assignee: body.clear_assignee,
                queue_id: body.queue_id,
                status: body.status,
                resolution: body.resolution,
                close_reason: body.close_reason,
            },
            revision,
            request_uuid(&request_id),
            Utc::now(),
        )
        .await
        .map_err(|error| map_error(error, &request_id))?;
    mutation_response(StatusCode::OK, result)
}

async fn requester_tickets(
    State(state): State<TicketingHttpState>,
    RequiredIdentity(principal): RequiredIdentity,
    RawQuery(raw): RawQuery,
    headers: HeaderMap,
) -> Result<Json<ResourceCollection<crate::PublicTicketSummary>>, ApiFailure> {
    let request_id = request_id(&headers);
    let mut limit = 50usize;
    let mut before: Option<(DateTime<Utc>, TicketId)> = None;
    let mut statuses = BTreeSet::new();
    let mut seen = BTreeSet::new();
    for (name, value) in url::form_urlencoded::parse(raw.unwrap_or_default().as_bytes()) {
        let name = name.into_owned();
        let value = value.into_owned();
        if !seen.insert(name.clone()) {
            return Err(ApiFailure::validation(
                "requester ticket list query repeats a parameter",
                &request_id,
            ));
        }
        match name.as_str() {
            "page[limit]" => {
                limit = value
                    .parse()
                    .ok()
                    .filter(|value| (1..=200).contains(value))
                    .ok_or_else(|| {
                        ApiFailure::validation("page limit must be between 1 and 200", &request_id)
                    })?;
            }
            "page[after]" => {
                if value.len() > 512 {
                    return Err(ApiFailure::validation(
                        "page cursor is invalid",
                        &request_id,
                    ));
                }
                before = Some(decode_cursor(&value).ok_or_else(|| {
                    ApiFailure::validation("page cursor is invalid", &request_id)
                })?);
            }
            "filter[status]" => {
                let public: crate::PublicTicketStatus =
                    serde_json::from_value(serde_json::Value::String(value)).map_err(|_| {
                        ApiFailure::validation("status filter is invalid", &request_id)
                    })?;
                let internal: Vec<TicketStatus> = match public {
                    crate::PublicTicketStatus::Open => {
                        vec![TicketStatus::New, TicketStatus::Open]
                    }
                    crate::PublicTicketStatus::InProgress => vec![TicketStatus::PendingInternal],
                    crate::PublicTicketStatus::WaitingForYou => {
                        vec![TicketStatus::PendingRequester]
                    }
                    crate::PublicTicketStatus::OnHold => vec![TicketStatus::OnHold],
                    crate::PublicTicketStatus::Resolved => vec![TicketStatus::Resolved],
                    crate::PublicTicketStatus::Closed => vec![TicketStatus::Closed],
                };
                statuses.extend(internal);
            }
            _ => {
                return Err(ApiFailure::validation(
                    "requester ticket list query contains an unsupported parameter",
                    &request_id,
                ));
            }
        }
    }
    let mut summaries = state
        .service
        .list_requester_summaries(
            &principal,
            crate::TicketSummaryFilter {
                project_id: state.service.config().project_id.clone(),
                statuses,
                queue_id: None,
                assignee_subject: None,
                requester_subject: None,
                before_updated_at: before.map(|value| value.0),
                before_id: before.map(|value| value.1),
                limit: limit + 1,
            },
        )
        .await
        .map_err(|error| map_error(error, &request_id))?;
    let has_more = summaries.len() > limit;
    summaries.truncate(limit);
    let next = if has_more {
        summaries
            .last()
            .map(|summary| Cursor::new(encode_cursor_parts(summary.updated_at, summary.id)))
            .transpose()
            .map_err(|_| ApiFailure::internal(&request_id))?
    } else {
        None
    };
    Ok(Json(ResourceCollection::new(summaries, next)))
}

async fn requester_ticket(
    State(state): State<TicketingHttpState>,
    RequiredIdentity(principal): RequiredIdentity,
    Path(ticket_id): Path<String>,
    headers: HeaderMap,
) -> Result<Response, ApiFailure> {
    let request_id = request_id(&headers);
    let id = parse_ticket_id(&ticket_id, &request_id)?;
    let ticket = state
        .service
        .get_ticket_for_requester(&principal, &state.service.config().project_id, id)
        .await
        .map_err(|error| map_error(error, &request_id))?;
    let tag = StrongEntityTag::for_resource("ticket", &ticket.id.to_string(), ticket.revision + 1)
        .map_err(|_| ApiFailure::internal(&request_id))?;
    let mut response = Json(ticket).into_response();
    response
        .headers_mut()
        .insert(header::ETAG, tag.to_header_value());
    Ok(response)
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
    encode_cursor_parts(ticket.updated_at, ticket.id)
}

fn encode_summary_cursor(summary: &TicketSummary) -> String {
    encode_cursor_parts(summary.updated_at, summary.id)
}

/// `minco_http::Cursor` accepts only `[A-Za-z0-9_-]`, so the composite
/// `(updated_at, id)` cursor joins seconds and nanoseconds without a `.`.
fn encode_cursor_parts(updated_at: DateTime<Utc>, id: TicketId) -> String {
    format!(
        "{}{:09}_{}",
        updated_at.timestamp(),
        updated_at.timestamp_subsec_nanos(),
        id.0.simple()
    )
}

fn decode_cursor(value: &str) -> Option<(DateTime<Utc>, TicketId)> {
    let (timestamp, id) = value.split_once('_')?;
    if timestamp.len() < 10 || !timestamp.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let seconds = &timestamp[..timestamp.len() - 9];
    let nanos = &timestamp[timestamp.len() - 9..];
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
        TicketingServiceError::InvalidManagementRequest => {
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

    #[test]
    fn cursor_encoding_only_uses_characters_minco_cursor_accepts() {
        let now = DateTime::from_timestamp(1_777_777_777, 123_456_789).unwrap();
        let encoded = encode_cursor_parts(now, TicketId::new());
        assert!(Cursor::new(encoded.clone()).is_ok(), "{encoded}");
        assert!(
            encoded
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        );
        assert!(Cursor::new(String::from("1777777777.123456789_not-accepted")).is_err());
    }

    fn agent_principal() -> minco_http::Principal {
        minco_http::Principal {
            subject: "agent-1".into(),
            permissions: [
                "ticketing.agent-console",
                "ticketing.agent.read",
                "ticketing.agent.manage",
                "ticketing.create",
                "ticketing.reply",
                "ticketing.manage",
            ]
            .into_iter()
            .map(String::from)
            .collect(),
            claims: BTreeMap::new(),
        }
    }

    async fn create_tickets_through_api(app: &Router, count: usize) -> Vec<serde_json::Value> {
        let mut created = Vec::new();
        for index in 0..count {
            let response = app
                .clone()
                .oneshot(
                    Request::post("/_minco/ticketing/tickets")
                        .header(header::CONTENT_TYPE, "application/json")
                        .extension(agent_principal())
                        .body(Body::from(
                            serde_json::json!({
                                "project_id": "project-a",
                                "subject": format!("Ticket {index}"),
                                "description": "It broke and needs an agent.",
                                "requester": {"subject": "user-a"},
                                "channel": "api"
                            })
                            .to_string(),
                        ))
                        .unwrap(),
                )
                .await
                .unwrap();
            let debug_status = response.status();
            let debug_body = to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap()
                .to_vec();
            assert_eq!(
                debug_status,
                StatusCode::CREATED,
                "{}",
                String::from_utf8_lossy(&debug_body)
            );
            let value: serde_json::Value = serde_json::from_slice(&debug_body).unwrap();
            created.push(value["ticket"].clone());
        }
        created
    }

    #[tokio::test]
    async fn agent_console_assets_are_hardened_public_and_credential_free() {
        let app = ticketing_router(service());
        let page = app
            .clone()
            .oneshot(
                Request::get("/_minco/ticketing/agent")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(page.status(), StatusCode::OK);
        assert_eq!(
            page.headers()[header::CONTENT_TYPE],
            "text/html; charset=utf-8"
        );
        assert_eq!(
            page.headers()["content-security-policy"],
            "default-src 'none'; script-src 'self'; style-src 'self'; connect-src 'self'; img-src 'self' data:; base-uri 'none'; form-action 'none'; frame-ancestors 'none'"
        );
        assert_eq!(page.headers()["x-content-type-options"], "nosniff");
        assert_eq!(page.headers()["referrer-policy"], "no-referrer");
        let body = to_bytes(page.into_body(), usize::MAX).await.unwrap();
        let html = String::from_utf8_lossy(&body);
        assert!(!html.contains("token"));
        assert!(!html.contains("secret"));

        let script = app
            .clone()
            .oneshot(
                Request::get("/_minco/ticketing/agent/console.js")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(script.status(), StatusCode::OK);
        assert_eq!(
            script.headers()[header::CONTENT_TYPE],
            "application/javascript; charset=utf-8"
        );
        assert_eq!(script.headers()["x-content-type-options"], "nosniff");
        let styles = app
            .oneshot(
                Request::get("/_minco/ticketing/agent/console.css")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(styles.status(), StatusCode::OK);
        assert_eq!(
            styles.headers()[header::CONTENT_TYPE],
            "text/css; charset=utf-8"
        );
    }

    #[tokio::test]
    async fn agent_bootstrap_requires_identity_and_agent_console_permission() {
        let app = ticketing_router(service());
        let unauthenticated = app
            .clone()
            .oneshot(
                Request::get("/_minco/ticketing/agent/bootstrap")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(unauthenticated.status(), StatusCode::UNAUTHORIZED);

        let mut wrong_permission = agent_principal();
        wrong_permission.permissions = std::iter::once("ticketing.read".into()).collect();
        let response = app
            .clone()
            .oneshot(
                Request::get("/_minco/ticketing/agent/bootstrap")
                    .extension(wrong_permission)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);

        let response = app
            .oneshot(
                Request::get("/_minco/ticketing/agent/bootstrap")
                    .extension(agent_principal())
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bootstrap: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        assert_eq!(bootstrap["subject"], "agent-1");
        assert_eq!(bootstrap["capabilities"]["manage"], true);
        assert!(bootstrap.get("token").is_none());
    }

    #[tokio::test]
    async fn agent_ticket_list_paginates_without_gaps_and_rejects_invalid_cursors() {
        let app = ticketing_router(service()).layer(axum::Extension(agent_principal()));
        create_tickets_through_api(&app, 5).await;

        let mut seen = Vec::new();
        let mut cursor: Option<String> = None;
        loop {
            let mut path = String::from("/_minco/ticketing/agent/tickets?page[limit]=2");
            if let Some(value) = cursor.as_deref() {
                path.push_str("&page[after]=");
                path.push_str(value);
            }
            let response = app
                .clone()
                .oneshot(Request::get(path).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let page: serde_json::Value =
                serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap())
                    .unwrap();
            let has_more = page["page"]["hasMore"].as_bool().unwrap();
            for summary in page["data"].as_array().unwrap() {
                seen.push(summary["id"].as_str().unwrap().to_owned());
            }
            if !has_more {
                break;
            }
            cursor = page["page"]["nextCursor"].as_str().map(str::to_owned);
        }
        assert_eq!(seen.len(), 5);
        assert_eq!(seen.iter().collect::<BTreeSet<_>>().len(), 5);

        let invalid = app
            .clone()
            .oneshot(
                Request::get("/_minco/ticketing/agent/tickets?page[after]=not.a.cursor")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(invalid.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn agent_summary_excludes_private_payload_and_update_moves_ticket_to_first_page() {
        let app = ticketing_router(service()).layer(axum::Extension(agent_principal()));
        let tickets = create_tickets_through_api(&app, 3).await;

        let response = app
            .clone()
            .oneshot(
                Request::get("/_minco/ticketing/agent/tickets?page[limit]=3")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let page: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        let encoded = serde_json::to_string(&page).unwrap();
        assert!(!encoded.contains("It broke and needs an agent."));
        assert!(!encoded.contains("description"));
        assert!(!encoded.contains("object_key"));
        assert!(page["data"][0].get("subject").is_some());
        assert_eq!(page["data"][0]["message_count"], 1);

        // Reply to the ticket currently last; it must move to the top.
        let last_id = page["data"][2]["id"].as_str().unwrap();
        let etag = page["data"][2]["revision"].as_u64().unwrap() + 1;
        let reply = app
            .clone()
            .oneshot(
                Request::post(format!("/_minco/ticketing/tickets/{last_id}/agent-replies"))
                    .header(header::CONTENT_TYPE, "application/json")
                    .header(header::IF_MATCH, format!("\"ticket:{last_id}:{etag}\""))
                    .body(Body::from(
                        serde_json::json!({"body": "Working on it."}).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(reply.status(), StatusCode::OK);
        let refreshed = app
            .clone()
            .oneshot(
                Request::get("/_minco/ticketing/agent/tickets?page[limit]=3")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let page_two: serde_json::Value =
            serde_json::from_slice(&to_bytes(refreshed.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        assert_eq!(page_two["data"][0]["id"].as_str().unwrap(), last_id);
        let _ = tickets;
    }

    #[tokio::test]
    async fn management_patch_is_atomic_with_problem_details_and_etag() {
        let app = ticketing_router(service()).layer(axum::Extension(agent_principal()));
        let tickets = create_tickets_through_api(&app, 1).await;
        let id = tickets[0]["id"].as_str().unwrap();

        let missing_if_match = app
            .clone()
            .oneshot(
                Request::patch(format!("/_minco/ticketing/agent/tickets/{id}/management"))
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        serde_json::json!({"priority": "high"}).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(missing_if_match.status(), StatusCode::PRECONDITION_REQUIRED);

        let revision = tickets[0]["revision"].as_u64().unwrap() + 1;
        let stale = app
            .clone()
            .oneshot(
                Request::patch(format!("/_minco/ticketing/agent/tickets/{id}/management"))
                    .header(header::CONTENT_TYPE, "application/json")
                    .header(header::IF_MATCH, format!("\"ticket:{id}:5\""))
                    .body(Body::from(
                        serde_json::json!({"priority": "high"}).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(stale.status(), StatusCode::PRECONDITION_FAILED);

        let invalid = app
            .clone()
            .oneshot(
                Request::patch(format!("/_minco/ticketing/agent/tickets/{id}/management"))
                    .header(header::CONTENT_TYPE, "application/json")
                    .header(header::IF_MATCH, format!("\"ticket:{id}:{revision}\""))
                    .body(Body::from(
                        serde_json::json!({"priority": "urgent", "status": "closed"}).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(invalid.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(
            invalid.headers()[header::CONTENT_TYPE],
            "application/problem+json"
        );

        let valid = app
            .clone()
            .oneshot(
                Request::patch(format!("/_minco/ticketing/agent/tickets/{id}/management"))
                    .header(header::CONTENT_TYPE, "application/json")
                    .header(header::IF_MATCH, format!("\"ticket:{id}:{revision}\""))
                    .body(Body::from(
                        serde_json::json!({
                            "priority": "urgent",
                            "assignee_subject": "agent-1",
                            "queue_id": "tier-1",
                            "status": "pending_requester"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(valid.status(), StatusCode::OK);
        let etag = valid.headers()[header::ETAG].to_str().unwrap().to_owned();
        let managed: serde_json::Value =
            serde_json::from_slice(&to_bytes(valid.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        assert_eq!(
            etag,
            format!(
                "\"ticket:{id}:{}\"",
                managed["ticket"]["revision"].as_u64().unwrap() + 1
            )
        );
        assert_eq!(managed["ticket"]["priority"], "urgent");
        assert_eq!(managed["ticket"]["assignee_subject"], "agent-1");
        assert_eq!(managed["ticket"]["queue_id"], "tier-1");
        assert_eq!(managed["ticket"]["status"], "pending_requester");

        // The rejected atomic request must not have partially applied.
        let detail = app
            .oneshot(
                Request::get(format!("/_minco/ticketing/agent/tickets/{id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let ticket: serde_json::Value =
            serde_json::from_slice(&to_bytes(detail.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        assert_eq!(ticket["priority"], "urgent");
        assert_eq!(ticket["revision"], managed["ticket"]["revision"]);
    }

    fn requester_principal(subject: &str) -> minco_http::Principal {
        minco_http::Principal {
            subject: subject.into(),
            permissions: ["ticketing.create", "ticketing.read", "ticketing.reply"]
                .into_iter()
                .map(String::from)
                .collect(),
            claims: BTreeMap::new(),
        }
    }

    async fn create_requester_ticket(
        app: &Router,
        subject: &str,
        reference: &str,
    ) -> serde_json::Value {
        let response = app
            .clone()
            .oneshot(
                Request::post("/_minco/ticketing/tickets")
                    .header(header::CONTENT_TYPE, "application/json")
                    .extension(requester_principal(subject))
                    .body(Body::from(
                        serde_json::json!({
                            "project_id": "project-a",
                            "subject": reference,
                            "description": "It broke and the requester needs help.",
                            "requester": {"subject": subject},
                            "channel": "portal"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap()
    }

    #[tokio::test]
    async fn requester_surface_lists_only_own_tickets_with_public_shapes() {
        let app = ticketing_router(service());
        let ticket_a = create_requester_ticket(&app, "user-a", "Own ticket").await;
        create_requester_ticket(&app, "user-b", "Foreign ticket").await;

        let response = app
            .clone()
            .oneshot(
                Request::get("/_minco/ticketing/requester/tickets")
                    .extension(requester_principal("user-a"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let page: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        let encoded = serde_json::to_string(&page).unwrap();
        assert_eq!(page["data"].as_array().unwrap().len(), 1);
        assert_eq!(page["data"][0]["subject"], "Own ticket");
        assert_eq!(page["data"][0]["status"], "open");
        assert!(!encoded.contains("Foreign ticket"));
        assert!(!encoded.contains("assignee_subject"));
        assert!(!encoded.contains("requester_subject"));
        assert!(page["page"]["hasMore"].is_boolean());

        let foreign_id = ticket_a["ticket"]["id"].as_str().unwrap();
        let foreign = app
            .clone()
            .oneshot(
                Request::get(format!("/_minco/ticketing/requester/tickets/{foreign_id}"))
                    .extension(requester_principal("user-b"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(foreign.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn requester_detail_is_public_and_reply_alias_round_trips() {
        let app = ticketing_router(service()).layer(axum::Extension(requester_principal("user-a")));
        let created = create_requester_ticket(&app, "user-a", "Own ticket").await;
        let id = created["ticket"]["id"].as_str().unwrap();
        let revision = created["ticket"]["revision"].as_u64().unwrap();

        let detail = app
            .clone()
            .oneshot(
                Request::get(format!("/_minco/ticketing/requester/tickets/{id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(detail.status(), StatusCode::OK);
        assert_eq!(
            detail.headers()[header::ETAG],
            format!("\"ticket:{id}:{}\"", revision + 1)
        );
        let projection: serde_json::Value =
            serde_json::from_slice(&to_bytes(detail.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        let encoded = serde_json::to_string(&projection).unwrap();
        assert!(!encoded.contains("author_subject"));
        assert_eq!(projection["status"], "open");
        assert_eq!(projection["messages"][0]["author"], "requester");

        let reply = app
            .clone()
            .oneshot(
                Request::post(format!("/_minco/ticketing/requester/tickets/{id}/replies"))
                    .header(header::CONTENT_TYPE, "application/json")
                    .header(
                        header::IF_MATCH,
                        format!("\"ticket:{id}:{}\"", revision + 1),
                    )
                    .body(Body::from(
                        serde_json::json!({"body": "Here is more detail."}).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(reply.status(), StatusCode::OK);
        let answered: serde_json::Value =
            serde_json::from_slice(&to_bytes(reply.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        assert_eq!(answered["ticket"]["messages"].as_array().unwrap().len(), 2);
        assert_eq!(answered["ticket"]["messages"][1]["author"], "requester");
    }

    #[tokio::test]
    async fn requester_public_status_filter_maps_to_internal_statuses() {
        let app = ticketing_router(service()).layer(axum::Extension(requester_principal("user-a")));
        create_requester_ticket(&app, "user-a", "Own ticket").await;
        let response = app
            .clone()
            .oneshot(
                Request::get("/_minco/ticketing/requester/tickets?filter[status]=waiting_for_you")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let page: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        assert_eq!(page["data"].as_array().unwrap().len(), 0);

        let invalid = app
            .clone()
            .oneshot(
                Request::get("/_minco/ticketing/requester/tickets?filter[status]=pending_internal")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(invalid.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }
}
