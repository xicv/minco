use crate::{
    AgentManagementInput, CreateTicketInput, ExternalMessageIdentity, IssueTicketingHandoffInput,
    RequesterTicket, Ticket, TicketId, TicketListFilter, TicketStatus, TicketStoreError,
    TicketSummary, TicketingMutationResult, TicketingService, TicketingServiceError,
};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, FromRequestParts, Path, RawQuery, State},
    response::{IntoResponse, Response},
    routing::{get, patch, post, put},
};
use chrono::{DateTime, Utc};
use http::{HeaderMap, HeaderValue, StatusCode, header};
use minco_http::{
    ApiFailure, Cursor, ResourceCollection, StrongEntityTag, ValidatedJson, parse_if_match,
};
use minco_interaction::{
    AttachmentLimits, SupportBootstrap, SupportHandoffResult, SupportHandoffToken, SupportSurface,
};
use minco_plugin_identity::Identity;
use serde::Serialize;
use std::{collections::BTreeSet, str::FromStr};
use uuid::Uuid;

pub const TICKETING_BASE_PATH: &str = "/_minco/ticketing";
pub const HANDOFF_HEADER: &str = "x-minco-ticketing-handoff";
const SUPPORT_ENTRY_SOURCE: &str = include_str!("../assets/support-entry.js");
const AGENT_CONSOLE_PAGE: &str = include_str!("../assets/agent-console.html");
const AGENT_CONSOLE_SCRIPT: &str = include_str!("../assets/agent-console.js");
const AGENT_CONSOLE_STYLES: &str = include_str!("../assets/agent-console.css");
const MAX_JSON_BODY_BYTES: usize = 256 * 1024;

#[derive(Clone)]
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

pub const REQUESTER_SESSION_COOKIE: &str = "minco_ticketing_session";
const REQUESTER_SESSION_COOKIE_PATH: &str = "/_minco/ticketing";

/// Requester identity: a host-injected principal wins (API/BFF callers keep
/// their authority); otherwise a valid session cookie resolves to an
/// identity whose permissions are exactly the handoff-granted set.
struct RequesterIdentity {
    identity: Identity,
    session: Option<minco_plugin_sessions::SessionRecord>,
}

impl FromRequestParts<TicketingHttpState> for RequesterIdentity {
    type Rejection = ApiFailure;

    async fn from_request_parts(
        parts: &mut http::request::Parts,
        state: &TicketingHttpState,
    ) -> Result<Self, Self::Rejection> {
        if let Some(principal) = parts.extensions.get::<minco_http::Principal>().cloned() {
            return Ok(Self {
                identity: identity(principal),
                session: None,
            });
        }
        let request_id = request_id(&parts.headers);
        let token = session_cookie(parts.headers.get(header::COOKIE))
            .ok_or_else(|| identity_required(&request_id))?;
        let (record, identity) = state
            .service
            .resolve_requester_session(&token)
            .await
            .map_err(|error| map_error(error, &request_id))?;
        Ok(Self {
            identity,
            session: Some(record),
        })
    }
}

fn session_cookie(header: Option<&HeaderValue>) -> Option<minco_plugin_sessions::SessionToken> {
    let header = header?.to_str().ok()?;
    let value = header.split(';').map(str::trim).find_map(|part| {
        part.split_once('=')
            .filter(|(name, _)| name.trim() == REQUESTER_SESSION_COOKIE)
            .map(|(_, value)| value.trim().to_owned())
    })?;
    minco_plugin_sessions::SessionToken::parse(value).ok()
}

/// Session-sourced mutations must present the CSRF token bound to the
/// session; injected principals are not CSRF-checked.
fn require_session_csrf(
    state: &TicketingHttpState,
    requester: &RequesterIdentity,
    headers: &HeaderMap,
    request_id: &str,
) -> Result<(), ApiFailure> {
    let Some(record) = requester.session.as_ref() else {
        return Ok(());
    };
    let token = headers
        .get("x-minco-csrf")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| minco_plugin_sessions::CsrfToken::parse(value).ok())
        .ok_or_else(|| {
            ApiFailure::new(
                StatusCode::FORBIDDEN,
                "ticketing_csrf_required",
                "CSRF token required",
                "Supply the session CSRF token in X-Minco-CSRF.",
                request_id,
            )
        })?;
    state
        .service
        .verify_session_csrf(record.id, &token)
        .map_err(|error| map_error(error, request_id))
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
        .route("/agent/views/{viewId}", get(agent_view))
        .route("/agent/search", get(agent_search))
        .route(
            "/agent/tickets/{ticketId}/knowledge-links",
            put(replace_knowledge_links),
        )
        .route(
            "/agent/tickets/{ticketId}/automation",
            post(request_automation),
        )
        .route(
            "/agent/tickets/{ticketId}/automation-proposals",
            get(list_automation_proposals),
        )
        .route(
            "/agent/automation-proposals/{proposalId}",
            patch(decide_automation_proposal),
        )
        .route(
            "/agent/tickets/{ticketId}/clarifications",
            get(list_clarifications).post(create_clarification),
        )
        .route(
            "/agent/clarifications/{clarificationId}/send",
            post(send_clarification),
        )
        .route("/agent/macros", get(agent_macros).post(create_agent_macro))
        .route("/agent/macros/{macroId}", patch(update_agent_macro))
        .route(
            "/agent/tickets/{ticketId}/management",
            patch(manage_agent_ticket),
        )
        .route("/requester/tickets", get(requester_tickets))
        .route(
            "/requester/tickets/{ticketId}/csat",
            post(submit_requester_csat),
        )
        .route(
            "/requester/clarifications",
            get(list_requester_clarifications),
        )
        .route(
            "/requester/clarifications/{clarificationId}/reply",
            post(reply_requester_clarification),
        )
        .route("/requester/tickets/{ticketId}", get(requester_ticket))
        .route(
            "/requester/tickets/{ticketId}/replies",
            post(requester_reply),
        )
        .route(
            "/requester/tickets/{ticketId}/messages",
            get(requester_messages),
        )
        .route("/requester/sessions", post(requester_session_exchange))
        .route("/requester/logout", post(requester_logout))
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

/// Bootstrap response: the interaction crate's wire shape plus the
/// ticketing-owned additive capability truth (ADR-0053). Keeping the
/// extension local means the plugin never depends on an unpublished
/// interaction change.
#[derive(Debug, Serialize)]
struct TicketingBootstrapResponse {
    #[serde(flatten)]
    support: SupportBootstrap,
    capabilities: crate::SupportCapabilities,
}

async fn bootstrap(State(state): State<TicketingHttpState>) -> Json<TicketingBootstrapResponse> {
    let config = state.service.config();
    Json(TicketingBootstrapResponse {
        support: SupportBootstrap {
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
            // Truthful (ADR-0053): no capture operation exists yet.
            screenshot_enabled: false,
            voice_enabled: false,
            file_enabled: false,
            attachment_limits: AttachmentLimits {
                count: 8,
                screenshot_bytes: 4 * 1024 * 1024,
                audio_bytes: 5 * 1024 * 1024,
                file_bytes: 5 * 1024 * 1024,
                aggregate_bytes: 8 * 1024 * 1024,
            },
            recording_limit: 90,
            privacy_notice: config.privacy_notice.clone(),
        },
        capabilities: state.service.support_capabilities(),
    })
}

/// Contract-derived wire mappings (ADR-0057): generated DTOs map
/// field-for-field onto the application inputs; handlers stay logic-free.
mod wire {
    use crate::generated as wire;
    use crate::{
        TicketChannel, TicketFromHandoffInput, TicketPriority, TicketRequester, TicketStatus,
    };
    use minco_interaction::{SupportContext, SupportResourceReference, SupportSurface};

    pub(super) const fn ticket_type(value: wire::TicketType) -> crate::TicketType {
        match value {
            wire::TicketType::Question => crate::TicketType::Question,
            wire::TicketType::Incident => crate::TicketType::Incident,
            wire::TicketType::Problem => crate::TicketType::Problem,
            wire::TicketType::Task => crate::TicketType::Task,
        }
    }

    pub(super) fn form_answer(value: wire::TicketFormAnswer) -> crate::TicketFormAnswer {
        crate::TicketFormAnswer {
            field_id: value.field_id,
            kind: match value.kind {
                wire::TicketFormValueKind::Text => crate::TicketFormValueKind::Text,
                wire::TicketFormValueKind::Number => crate::TicketFormValueKind::Number,
                wire::TicketFormValueKind::Boolean => crate::TicketFormValueKind::Boolean,
                wire::TicketFormValueKind::DateTime => crate::TicketFormValueKind::DateTime,
            },
            text_value: value.text_value,
            number_value: value.number_value,
            boolean_value: value.boolean_value,
        }
    }

    pub(super) const fn channel(value: wire::TicketChannel) -> TicketChannel {
        match value {
            wire::TicketChannel::Portal => TicketChannel::Portal,
            wire::TicketChannel::Email => TicketChannel::Email,
            wire::TicketChannel::Api => TicketChannel::Api,
            wire::TicketChannel::Voice => TicketChannel::Voice,
            wire::TicketChannel::Internal => TicketChannel::Internal,
            wire::TicketChannel::Other => TicketChannel::Other,
        }
    }

    pub(super) const fn priority(value: wire::TicketPriority) -> TicketPriority {
        match value {
            wire::TicketPriority::Low => TicketPriority::Low,
            wire::TicketPriority::Normal => TicketPriority::Normal,
            wire::TicketPriority::High => TicketPriority::High,
            wire::TicketPriority::Urgent => TicketPriority::Urgent,
        }
    }

    pub(super) const fn status(value: wire::TicketStatus) -> TicketStatus {
        match value {
            wire::TicketStatus::New => TicketStatus::New,
            wire::TicketStatus::Open => TicketStatus::Open,
            wire::TicketStatus::PendingRequester => TicketStatus::PendingRequester,
            wire::TicketStatus::PendingInternal => TicketStatus::PendingInternal,
            wire::TicketStatus::OnHold => TicketStatus::OnHold,
            wire::TicketStatus::Resolved => TicketStatus::Resolved,
            wire::TicketStatus::Closed => TicketStatus::Closed,
        }
    }

    pub(super) const fn surface(value: wire::SupportSurface) -> SupportSurface {
        match value {
            wire::SupportSurface::Widget => SupportSurface::Widget,
            wire::SupportSurface::Portal => SupportSurface::Portal,
            wire::SupportSurface::Extension => SupportSurface::Extension,
            wire::SupportSurface::Api => SupportSurface::Api,
            wire::SupportSurface::Mobile => SupportSurface::Mobile,
        }
    }

    pub(super) fn requester(value: wire::TicketRequester) -> TicketRequester {
        TicketRequester {
            subject: value.subject,
            display_name: value.display_name,
            email: value.email,
        }
    }

    pub(super) fn resource_reference(value: wire::ResourceReference) -> SupportResourceReference {
        SupportResourceReference {
            system: value.system,
            resource_type: value.resource_type,
            resource_id: value.resource_id,
        }
    }

    pub(super) fn context(value: wire::SupportContext) -> SupportContext {
        SupportContext {
            page_url: value.page_url,
            optional_page_title: value.optional_page_title,
            optional_route_name: value.optional_route_name,
            optional_release_id: value.optional_release_id,
            optional_request_id: value.optional_request_id,
            optional_locale: value.optional_locale,
            optional_timezone: value.optional_timezone,
            optional_viewport: value.optional_viewport,
            optional_selected_text: value.optional_selected_text,
            resource_references: value
                .resource_references
                .unwrap_or_default()
                .into_iter()
                .map(resource_reference)
                .collect(),
        }
    }

    pub(super) fn from_handoff(value: wire::ExchangeHandoff) -> TicketFromHandoffInput {
        TicketFromHandoffInput {
            subject: value.subject,
            description: value.description,
            channel: channel(value.channel),
            priority: priority(value.priority),
            ticket_type: value
                .ticket_type
                .map_or_else(crate::TicketType::default, ticket_type),
            form_answers: value
                .form_answers
                .unwrap_or_default()
                .into_iter()
                .map(form_answer)
                .collect(),
            first_response_deadline: None,
            resolution_deadline: None,
        }
    }
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
    ValidatedJson(body): ValidatedJson<crate::generated::IssueHandoff>,
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
                surface: wire::surface(body.surface),
                context: wire::context(body.context),
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
    ValidatedJson(body): ValidatedJson<crate::generated::ExchangeHandoff>,
) -> Result<(StatusCode, Json<ConsumedHandoffResponse>), ApiFailure> {
    let request_id = request_id(&headers);
    let project_id = body.project_id.clone();
    let portal_origin = body.portal_origin.clone();
    let result = state
        .service
        .create_ticket_from_handoff(
            token,
            &project_id,
            &portal_origin,
            wire::from_handoff(body),
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
    ValidatedJson(input): ValidatedJson<crate::generated::CreateTicket>,
) -> Result<Response, ApiFailure> {
    let request_id = request_id(&headers);
    let input = CreateTicketInput {
        project_id: input.project_id,
        subject: input.subject,
        description: input.description,
        requester: wire::requester(input.requester),
        channel: wire::channel(input.channel),
        priority: input
            .priority
            .map_or(crate::TicketPriority::Normal, wire::priority),
        ticket_type: input
            .ticket_type
            .map_or_else(crate::TicketType::default, wire::ticket_type),
        form_answers: input
            .form_answers
            .unwrap_or_default()
            .into_iter()
            .map(wire::form_answer)
            .collect(),
        resource_references: input
            .resource_references
            .unwrap_or_default()
            .into_iter()
            .map(wire::resource_reference)
            .collect(),
    };
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

async fn requester_reply(
    State(state): State<TicketingHttpState>,
    requester: RequesterIdentity,
    Path(ticket_id): Path<String>,
    headers: HeaderMap,
    ValidatedJson(body): ValidatedJson<crate::generated::TicketReply>,
) -> Result<Response, ApiFailure> {
    let request_id = request_id(&headers);
    require_session_csrf(&state, &requester, &headers, &request_id)?;
    let principal = requester.identity;
    let id = parse_ticket_id(&ticket_id, &request_id)?;
    let revision = expected_revision(&headers, id, &request_id)?;

    let idempotency = state
        .service
        .portal_services()
        .idempotency
        .clone()
        .filter(|_| headers.contains_key("idempotency-key"));
    if let Some(service) = idempotency {
        let key = minco_plugin_idempotency::IdempotencyKey::parse(
            headers
                .get("idempotency-key")
                .and_then(|value| value.to_str().ok())
                .unwrap_or_default(),
        )
        .map_err(|_| {
            ApiFailure::validation(
                "idempotency key must be 1-200 visible characters",
                &request_id,
            )
        })?;
        let fingerprint = minco_plugin_idempotency::RequestFingerprint::from_serializable(
            &serde_json::json!({"ticket_id": id.to_string(), "body": body.body, "expected_revision": revision}),
        )
        .map_err(|_| ApiFailure::internal(&request_id))?;
        return match service.begin(key, fingerprint).await {
            Ok(minco_plugin_idempotency::BeginOutcome::Replay(record)) => {
                Ok((StatusCode::OK, Json(record.response)).into_response())
            }
            Ok(minco_plugin_idempotency::BeginOutcome::Conflict) => Err(ApiFailure::new(
                StatusCode::CONFLICT,
                "ticketing_idempotency_conflict",
                "Idempotency conflict",
                "This key was used with a different request.",
                &request_id,
            )),
            Ok(minco_plugin_idempotency::BeginOutcome::InProgress { .. }) => Err(ApiFailure::new(
                StatusCode::TOO_EARLY,
                "ticketing_idempotency_in_progress",
                "Request already in progress",
                "A request with this key is still being processed.",
                &request_id,
            )),
            Ok(minco_plugin_idempotency::BeginOutcome::Started(lease)) => {
                match perform_requester_reply(
                    &state,
                    &principal,
                    id,
                    body.body.clone(),
                    revision,
                    &request_id,
                )
                .await
                {
                    Ok(result) => {
                        if let Ok(value) = serde_json::to_value(&result) {
                            let _ = service.complete(lease, value).await;
                        }
                        requester_response(StatusCode::OK, result)
                    }
                    Err(error) => {
                        let _ = service.abort(&lease).await;
                        Err(error)
                    }
                }
            }
            Err(_) => Err(ApiFailure::internal(&request_id)),
        };
    }

    let result =
        perform_requester_reply(&state, &principal, id, body.body, revision, &request_id).await?;
    requester_response(StatusCode::OK, result)
}

async fn perform_requester_reply(
    state: &TicketingHttpState,
    principal: &Identity,
    id: TicketId,
    body: String,
    revision: u64,
    request_id: &str,
) -> Result<crate::RequesterTicketResult, ApiFailure> {
    state
        .service
        .reply_as_requester(
            principal,
            &state.service.config().project_id,
            id,
            body,
            revision,
            request_uuid(request_id),
            Utc::now(),
        )
        .await
        .map_err(|error| map_error(error, request_id))
}

fn sessions_unavailable(request_id: &str) -> ApiFailure {
    ApiFailure::new(
        StatusCode::SERVICE_UNAVAILABLE,
        "ticketing_sessions_unavailable",
        "Sessions unavailable",
        "This application has not registered the sessions, CSRF and idempotency plugins.",
        request_id,
    )
}

async fn requester_session_exchange(
    State(state): State<TicketingHttpState>,
    SensitiveHandoff(token): SensitiveHandoff,
    headers: HeaderMap,
    ValidatedJson(body): ValidatedJson<crate::generated::SessionExchange>,
) -> Result<Response, ApiFailure> {
    let request_id = request_id(&headers);
    let portal = state.service.portal_services();
    let Some(idempotency) = portal.idempotency.clone() else {
        return Err(sessions_unavailable(&request_id));
    };

    // The exchange is keyed by the handoff digest, so a browser retry
    // replays the original grant instead of minting a second session.
    let key = minco_plugin_idempotency::IdempotencyKey::parse(format!(
        "ticketing.session.{}",
        token.digest().as_str()
    ))
    .map_err(|_| ApiFailure::validation("handoff digest is invalid", &request_id))?;
    let fingerprint = minco_plugin_idempotency::RequestFingerprint::from_serializable(
        &serde_json::json!({ "portal_origin": body.portal_origin }),
    )
    .map_err(|_| ApiFailure::internal(&request_id))?;
    let lease = match idempotency.begin(key, fingerprint.clone()).await {
        Ok(minco_plugin_idempotency::BeginOutcome::Replay(record)) => {
            return Ok((StatusCode::OK, Json(record.response)).into_response());
        }
        Ok(minco_plugin_idempotency::BeginOutcome::Conflict) => {
            return Err(ApiFailure::new(
                StatusCode::CONFLICT,
                "ticketing_idempotency_conflict",
                "Idempotency conflict",
                "This handoff was exchanged with a different request.",
                &request_id,
            ));
        }
        Ok(minco_plugin_idempotency::BeginOutcome::InProgress { .. }) => {
            return Err(ApiFailure::new(
                StatusCode::TOO_EARLY,
                "ticketing_idempotency_in_progress",
                "Exchange already in progress",
                "This handoff exchange is still being processed.",
                &request_id,
            ));
        }
        Ok(minco_plugin_idempotency::BeginOutcome::Started(lease)) => lease,
        Err(_) => return Err(ApiFailure::internal(&request_id)),
    };

    match state
        .service
        .exchange_requester_session(token, &body.portal_origin, fingerprint.as_str(), Utc::now())
        .await
    {
        Ok(grant) => {
            let snapshot = serde_json::json!({
                "expires_at": grant.expires_at.to_rfc3339(),
                "csrf_token": grant.csrf_token.expose(),
            });
            let _ = idempotency.complete(lease, snapshot.clone()).await;
            let mut response = (StatusCode::CREATED, Json(snapshot)).into_response();
            response.headers_mut().insert(
                header::SET_COOKIE,
                HeaderValue::from_str(&format!(
                    "{REQUESTER_SESSION_COOKIE}={}; Secure; HttpOnly; SameSite=Lax; Path={REQUESTER_SESSION_COOKIE_PATH}",
                    grant.token.expose()
                ))
                .map_err(|_| ApiFailure::internal(&request_id))?,
            );
            Ok(response)
        }
        Err(error) => {
            let _ = idempotency.abort(&lease).await;
            Err(map_error(error, &request_id))
        }
    }
}

async fn requester_logout(
    State(state): State<TicketingHttpState>,
    requester: RequesterIdentity,
    headers: HeaderMap,
) -> Result<StatusCode, ApiFailure> {
    let request_id = request_id(&headers);
    let session = requester
        .session
        .as_ref()
        .ok_or_else(|| identity_required(&request_id))?;
    require_session_csrf(&state, &requester, &headers, &request_id)?;
    state
        .service
        .revoke_requester_session(session.id)
        .await
        .map_err(|error| map_error(error, &request_id))?;
    Ok(StatusCode::NO_CONTENT)
}

async fn agent_reply(
    State(state): State<TicketingHttpState>,
    RequiredIdentity(principal): RequiredIdentity,
    Path(ticket_id): Path<String>,
    headers: HeaderMap,
    ValidatedJson(body): ValidatedJson<crate::generated::TicketReply>,
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
    ValidatedJson(body): ValidatedJson<crate::generated::TicketReply>,
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

async fn change_assignment(
    State(state): State<TicketingHttpState>,
    RequiredIdentity(principal): RequiredIdentity,
    Path(ticket_id): Path<String>,
    headers: HeaderMap,
    ValidatedJson(body): ValidatedJson<crate::generated::TicketAssignment>,
) -> Result<Response, ApiFailure> {
    let request_id = request_id(&headers);
    let id = parse_ticket_id(&ticket_id, &request_id)?;
    let revision = expected_revision(&headers, id, &request_id)?;
    let mode = match body.mode {
        crate::generated::TicketAssignmentMode::Manual => crate::AssignmentMode::Manual,
        crate::generated::TicketAssignmentMode::RoundRobin => crate::AssignmentMode::RoundRobin,
        crate::generated::TicketAssignmentMode::LeastWorkload => {
            crate::AssignmentMode::LeastWorkload
        }
    };
    let result = state
        .service
        .assign_ticket_by_mode(
            &principal,
            &state.service.config().project_id,
            id,
            mode,
            body.assignee_subject,
            revision,
            request_uuid(&request_id),
            Utc::now(),
        )
        .await
        .map_err(|error| map_error(error, &request_id))?;
    mutation_response(StatusCode::OK, result)
}

async fn change_queue(
    State(state): State<TicketingHttpState>,
    RequiredIdentity(principal): RequiredIdentity,
    Path(ticket_id): Path<String>,
    headers: HeaderMap,
    ValidatedJson(body): ValidatedJson<crate::generated::TicketQueueTransfer>,
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

async fn change_priority(
    State(state): State<TicketingHttpState>,
    RequiredIdentity(principal): RequiredIdentity,
    Path(ticket_id): Path<String>,
    headers: HeaderMap,
    ValidatedJson(body): ValidatedJson<crate::generated::TicketPriorityChange>,
) -> Result<Response, ApiFailure> {
    let priority = wire::priority(body.priority);
    let request_id = request_id(&headers);
    let id = parse_ticket_id(&ticket_id, &request_id)?;
    let revision = expected_revision(&headers, id, &request_id)?;
    let result = state
        .service
        .change_priority(
            &principal,
            &state.service.config().project_id,
            id,
            priority,
            revision,
            request_uuid(&request_id),
            Utc::now(),
        )
        .await
        .map_err(|error| map_error(error, &request_id))?;
    mutation_response(StatusCode::OK, result)
}

async fn change_status(
    State(state): State<TicketingHttpState>,
    RequiredIdentity(principal): RequiredIdentity,
    Path(ticket_id): Path<String>,
    headers: HeaderMap,
    ValidatedJson(body): ValidatedJson<crate::generated::TicketStatusChange>,
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
            wire::status(body.status),
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

async fn ingest_external_message(
    State(state): State<TicketingHttpState>,
    RequiredIdentity(principal): RequiredIdentity,
    headers: HeaderMap,
    ValidatedJson(body): ValidatedJson<crate::generated::IngressMessage>,
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
        references: body.references.unwrap_or_default(),
    };
    let result = state
        .service
        .ingest_external_message(
            &principal,
            identity_record,
            TicketId(body.ticket_id),
            body.body,
            u64::try_from(body.expected_revision).map_err(|_| {
                ApiFailure::validation("expected_revision must be non-negative", &request_id)
            })?,
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
                unassigned: false,
                query: None,
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
    let (ticket, other_recent_viewers) = state
        .service
        .agent_ticket_with_viewers(&principal, &state.service.config().project_id, id)
        .await
        .map_err(|error| map_error(error, &request_id))?;
    let tag = ticket_etag(&ticket)?;
    let detail = AgentTicketDetail {
        ticket,
        other_recent_viewers,
    };
    let mut response = (StatusCode::OK, Json(detail)).into_response();
    response
        .headers_mut()
        .insert(header::ETAG, tag.to_header_value());
    Ok(response)
}

async fn agent_view(
    State(state): State<TicketingHttpState>,
    RequiredIdentity(principal): RequiredIdentity,
    Path(view_id): Path<String>,
    RawQuery(raw): RawQuery,
    headers: HeaderMap,
) -> Result<Response, ApiFailure> {
    let request_id = request_id(&headers);
    let view = crate::AgentCuratedView::from_slug(&view_id).ok_or_else(|| {
        ApiFailure::new(
            StatusCode::NOT_FOUND,
            "ticketing_view_unknown",
            "Unknown curated view",
            "The curated view set is closed; see the agent bootstrap.",
            &request_id,
        )
    })?;
    let query = parse_agent_list_query(raw.as_deref(), &request_id)?;
    let mut summaries = state
        .service
        .list_agent_view(&principal, view, query.limit + 1, query.before)
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
    Ok(Json(ResourceCollection::new(summaries, next)).into_response())
}

async fn agent_search(
    State(state): State<TicketingHttpState>,
    RequiredIdentity(principal): RequiredIdentity,
    raw: RawQuery,
    headers: HeaderMap,
) -> Result<Response, ApiFailure> {
    let request_id = request_id(&headers);
    let RawQuery(raw_query) = raw;
    let mut query = parse_agent_list_query(raw_query.as_deref(), &request_id)?;
    let needle = url::form_urlencoded::parse(raw_query.unwrap_or_default().as_bytes())
        .find(|(name, _)| name == "q")
        .map(|(_, value)| value.into_owned())
        .ok_or_else(|| ApiFailure::validation("q is required", &request_id))?;
    let mut summaries = state
        .service
        .search_tickets(&principal, &needle, query.limit + 1, query.before)
        .await
        .map_err(|error| map_error(error, &request_id))?;
    let _ = (&query.statuses, &query.queue_id);
    query.statuses = BTreeSet::new();
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
    Ok(Json(ResourceCollection::new(summaries, next)).into_response())
}

#[allow(clippy::too_many_arguments)]
async fn replace_knowledge_links(
    State(state): State<TicketingHttpState>,
    RequiredIdentity(principal): RequiredIdentity,
    Path(ticket_id): Path<String>,
    headers: HeaderMap,
    ValidatedJson(body): ValidatedJson<crate::generated::KnowledgeLinks>,
) -> Result<Response, ApiFailure> {
    let request_id = request_id(&headers);
    let id = parse_ticket_id(&ticket_id, &request_id)?;
    let revision = expected_revision(&headers, id, &request_id)?;
    let links = body
        .links
        .into_iter()
        .map(|link| crate::KnowledgeLink {
            article_id: link.article_id,
            title: link.title,
            url: link.url,
        })
        .collect();
    let outcome = state
        .service
        .replace_knowledge_links(
            &principal,
            &state.service.config().project_id,
            id,
            links,
            revision,
            request_uuid(&request_id),
            Utc::now(),
        )
        .await
        .map_err(|error| map_error(error, &request_id))?;
    mutation_response(StatusCode::OK, outcome)
}

async fn submit_requester_csat(
    State(state): State<TicketingHttpState>,
    requester: RequesterIdentity,
    Path(ticket_id): Path<String>,
    headers: HeaderMap,
    ValidatedJson(body): ValidatedJson<crate::generated::RequesterCsat>,
) -> Result<Response, ApiFailure> {
    let request_id = request_id(&headers);
    let id = parse_ticket_id(&ticket_id, &request_id)?;
    let outcome = state
        .service
        .submit_csat(
            &requester.identity,
            &state.service.config().project_id,
            id,
            body.score.try_into().unwrap_or(0),
            body.comment,
            Utc::now(),
        )
        .await
        .map_err(|error| map_error(error, &request_id))?;
    let requester = outcome.ticket.requester_projection();
    Ok(Json(serde_json::json!({ "ticket": requester })).into_response())
}

async fn request_automation(
    State(state): State<TicketingHttpState>,
    RequiredIdentity(principal): RequiredIdentity,
    Path(ticket_id): Path<String>,
    headers: HeaderMap,
) -> Result<Response, ApiFailure> {
    let request_id = request_id(&headers);
    let id = parse_ticket_id(&ticket_id, &request_id)?;
    let correlation = state
        .service
        .request_development_automation(
            &principal,
            &state.service.config().project_id,
            id,
            Utc::now(),
        )
        .await
        .map_err(|error| map_error(error, &request_id))?;
    Ok((
        StatusCode::ACCEPTED,
        Json(AutomationAccepted {
            correlation_id: correlation,
        }),
    )
        .into_response())
}

async fn list_automation_proposals(
    State(state): State<TicketingHttpState>,
    RequiredIdentity(principal): RequiredIdentity,
    Path(ticket_id): Path<String>,
    headers: HeaderMap,
) -> Result<Response, ApiFailure> {
    let request_id = request_id(&headers);
    let id = parse_ticket_id(&ticket_id, &request_id)?;
    let proposals = state
        .service
        .list_automation_proposals(&principal, &state.service.config().project_id, id)
        .await
        .map_err(|error| map_error(error, &request_id))?;
    Ok(Json(AutomationProposalCollection { data: proposals }).into_response())
}

async fn decide_automation_proposal(
    State(state): State<TicketingHttpState>,
    RequiredIdentity(principal): RequiredIdentity,
    Path(proposal_id): Path<String>,
    headers: HeaderMap,
    ValidatedJson(body): ValidatedJson<crate::generated::AutomationDecision>,
) -> Result<Response, ApiFailure> {
    let request_id = request_id(&headers);
    let id = Uuid::parse_str(&proposal_id)
        .map_err(|_| ApiFailure::validation("proposal id must be a UUID", &request_id))?;
    let accept = matches!(
        body.decision,
        crate::generated::AutomationDecisionKind::Accept
    );
    let proposal = state
        .service
        .decide_automation_proposal(
            &principal,
            &state.service.config().project_id,
            id,
            accept,
            Utc::now(),
        )
        .await
        .map_err(|error| map_error(error, &request_id))?;
    Ok(Json(AutomationProposalMutation {
        proposal: &proposal,
    })
    .into_response())
}

async fn create_clarification(
    State(state): State<TicketingHttpState>,
    RequiredIdentity(principal): RequiredIdentity,
    Path(ticket_id): Path<String>,
    headers: HeaderMap,
    ValidatedJson(body): ValidatedJson<crate::generated::CreateClarification>,
) -> Result<Response, ApiFailure> {
    let request_id = request_id(&headers);
    let id = parse_ticket_id(&ticket_id, &request_id)?;
    let reason = match body.reason.as_str() {
        "missing_requirement" => crate::ClarificationReason::MissingRequirement,
        "contradictory_requirement" => crate::ClarificationReason::ContradictoryRequirement,
        _ => {
            return Err(ApiFailure::validation(
                "reason must be a known clarification reason",
                &request_id,
            ));
        }
    };
    let questions = body
        .questions
        .into_iter()
        .map(|question| crate::ClarificationQuestion {
            id: question.id,
            text: question.text,
        })
        .collect();
    let clarification = state
        .service
        .create_clarification_draft(
            &principal,
            &state.service.config().project_id,
            id,
            crate::service::ClarificationDraftInput {
                reason,
                questions,
                checkpoint: body.checkpoint,
            },
            Utc::now(),
        )
        .await
        .map_err(|error| map_error(error, &request_id))?;
    Ok((
        StatusCode::CREATED,
        Json(ClarificationMutation { clarification }),
    )
        .into_response())
}

async fn list_clarifications(
    State(state): State<TicketingHttpState>,
    RequiredIdentity(principal): RequiredIdentity,
    Path(ticket_id): Path<String>,
    headers: HeaderMap,
) -> Result<Response, ApiFailure> {
    let request_id = request_id(&headers);
    let id = parse_ticket_id(&ticket_id, &request_id)?;
    let items = state
        .service
        .list_clarifications(&principal, &state.service.config().project_id, id)
        .await
        .map_err(|error| map_error(error, &request_id))?;
    Ok(Json(ClarificationCollection { data: items }).into_response())
}

async fn send_clarification(
    State(state): State<TicketingHttpState>,
    RequiredIdentity(principal): RequiredIdentity,
    Path(clarification_id): Path<String>,
    headers: HeaderMap,
) -> Result<Response, ApiFailure> {
    let request_id = request_id(&headers);
    let id = Uuid::parse_str(&clarification_id)
        .map_err(|_| ApiFailure::validation("clarification id must be a UUID", &request_id))?;
    let clarification = state
        .service
        .send_clarification(
            &principal,
            &state.service.config().project_id,
            id,
            Utc::now(),
        )
        .await
        .map_err(|error| map_error(error, &request_id))?;
    Ok(Json(ClarificationMutation { clarification }).into_response())
}

async fn list_requester_clarifications(
    State(state): State<TicketingHttpState>,
    requester: RequesterIdentity,
) -> Result<Response, ApiFailure> {
    let items = state
        .service
        .list_requester_clarifications(&requester.identity)
        .await
        .map_err(|error| map_error(error, "requester-clarifications"))?;
    Ok(Json(RequesterClarificationCollection { data: items }).into_response())
}

async fn reply_requester_clarification(
    State(state): State<TicketingHttpState>,
    requester: RequesterIdentity,
    Path(clarification_id): Path<String>,
    headers: HeaderMap,
    ValidatedJson(body): ValidatedJson<crate::generated::ReplyClarification>,
) -> Result<Response, ApiFailure> {
    let request_id = request_id(&headers);
    let id = Uuid::parse_str(&clarification_id)
        .map_err(|_| ApiFailure::validation("clarification id must be a UUID", &request_id))?;
    let clarification = state
        .service
        .reply_to_clarification(
            &requester.identity,
            &state.service.config().project_id,
            id,
            body.answers,
            Utc::now(),
        )
        .await
        .map_err(|error| map_error(error, &request_id))?;
    Ok(Json(RequesterClarificationMutation { clarification }).into_response())
}

async fn agent_macros(
    State(state): State<TicketingHttpState>,
    RequiredIdentity(principal): RequiredIdentity,
) -> Result<Json<AgentMacroCollection>, ApiFailure> {
    Ok(Json(AgentMacroCollection {
        data: state
            .service
            .list_agent_macros(&principal)
            .await
            .map_err(|error| map_error(error, "agent-macros"))?,
    }))
}

async fn create_agent_macro(
    State(state): State<TicketingHttpState>,
    RequiredIdentity(principal): RequiredIdentity,
    headers: HeaderMap,
    ValidatedJson(input): ValidatedJson<crate::generated::CreateAgentMacro>,
) -> Result<Response, ApiFailure> {
    let request_id = request_id(&headers);
    let macro_ = state
        .service
        .create_agent_macro(&principal, &input.title, &input.body, Utc::now())
        .await
        .map_err(|error| map_error(error, &request_id))?;
    macro_response(StatusCode::CREATED, &macro_)
}

async fn update_agent_macro(
    State(state): State<TicketingHttpState>,
    RequiredIdentity(principal): RequiredIdentity,
    Path(macro_id): Path<String>,
    headers: HeaderMap,
    ValidatedJson(input): ValidatedJson<crate::generated::UpdateAgentMacro>,
) -> Result<Response, ApiFailure> {
    let request_id = request_id(&headers);
    let id = Uuid::parse_str(&macro_id)
        .map_err(|_| ApiFailure::validation("macro id must be a UUID", &request_id))?;
    let revision = expected_macro_revision(&headers, id, &request_id)?;
    let macro_ = state
        .service
        .update_agent_macro(
            &principal,
            id,
            revision,
            &input.title,
            &input.body,
            Utc::now(),
        )
        .await
        .map_err(|error| map_error(error, &request_id))?;
    macro_response(StatusCode::OK, &macro_)
}

async fn manage_agent_ticket(
    State(state): State<TicketingHttpState>,
    RequiredIdentity(principal): RequiredIdentity,
    Path(ticket_id): Path<String>,
    headers: HeaderMap,
    ValidatedJson(body): ValidatedJson<crate::generated::AgentManagement>,
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
                priority: body.priority.map(wire::priority),
                assignee_subject: body.assignee_subject,
                clear_assignee: body.clear_assignee.unwrap_or(false),
                queue_id: body.queue_id,
                status: body.status.map(wire::status),
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
    requester: RequesterIdentity,
    RawQuery(raw): RawQuery,
    headers: HeaderMap,
) -> Result<Json<ResourceCollection<crate::PublicTicketSummary>>, ApiFailure> {
    let principal = requester.identity;
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
                unassigned: false,
                query: None,
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
    requester: RequesterIdentity,
    Path(ticket_id): Path<String>,
    headers: HeaderMap,
) -> Result<Response, ApiFailure> {
    let principal = requester.identity;
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

async fn requester_messages(
    State(state): State<TicketingHttpState>,
    requester: RequesterIdentity,
    Path(ticket_id): Path<String>,
    RawQuery(raw): RawQuery,
    headers: HeaderMap,
) -> Result<Json<ResourceCollection<crate::PublicTicketMessage>>, ApiFailure> {
    let principal = requester.identity;
    let request_id = request_id(&headers);
    let id = parse_ticket_id(&ticket_id, &request_id)?;
    let mut limit = 50usize;
    let mut before: Option<(DateTime<Utc>, crate::TicketMessageId)> = None;
    let mut seen = BTreeSet::new();
    for (name, value) in url::form_urlencoded::parse(raw.unwrap_or_default().as_bytes()) {
        let name = name.into_owned();
        let value = value.into_owned();
        if !seen.insert(name.clone()) {
            return Err(ApiFailure::validation(
                "message list query repeats a parameter",
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
                let (created, message_id) = decode_cursor(&value)
                    .ok_or_else(|| ApiFailure::validation("page cursor is invalid", &request_id))?;
                before = Some((created, crate::TicketMessageId(message_id.0)));
            }
            _ => {
                return Err(ApiFailure::validation(
                    "message list query contains an unsupported parameter",
                    &request_id,
                ));
            }
        }
    }
    let mut messages = state
        .service
        .list_requester_messages(
            &principal,
            &state.service.config().project_id,
            id,
            before,
            limit + 1,
        )
        .await
        .map_err(|error| map_error(error, &request_id))?;
    let has_more = messages.len() > limit;
    messages.truncate(limit);
    let next = if has_more {
        messages
            .last()
            .map(|message| {
                Cursor::new(encode_cursor_parts(
                    message.created_at,
                    TicketId(message.id.0),
                ))
            })
            .transpose()
            .map_err(|_| ApiFailure::internal(&request_id))?
    } else {
        None
    };
    Ok(Json(ResourceCollection::new(messages, next)))
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

#[derive(Debug, Serialize)]
struct AgentTicketDetail {
    ticket: Ticket,
    other_recent_viewers: Vec<String>,
}

#[derive(serde::Serialize)]
struct ClarificationCollection {
    data: Vec<crate::Clarification>,
}

#[derive(serde::Serialize)]
struct ClarificationMutation {
    clarification: crate::Clarification,
}

#[derive(serde::Serialize)]
struct RequesterClarificationCollection {
    data: Vec<crate::RequesterClarification>,
}

#[derive(serde::Serialize)]
struct RequesterClarificationMutation {
    clarification: crate::RequesterClarification,
}

#[derive(serde::Serialize)]
struct AutomationAccepted {
    correlation_id: Uuid,
}

#[derive(serde::Serialize)]
struct AutomationProposalCollection {
    data: Vec<crate::AutomationProposal>,
}

#[derive(serde::Serialize)]
struct AutomationProposalMutation<'a> {
    proposal: &'a crate::AutomationProposal,
}

#[derive(Debug, Serialize)]
struct AgentMacroCollection {
    data: Vec<crate::AgentMacro>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "snake_case")]
struct MacroEnvelope<'a> {
    r#macro: &'a crate::AgentMacro,
}

fn macro_response(status: StatusCode, macro_: &crate::AgentMacro) -> Result<Response, ApiFailure> {
    let tag = StrongEntityTag::for_resource("macro", &macro_.id.to_string(), macro_.revision + 1)
        .map_err(|_| ApiFailure::internal("unavailable"))?;
    let mut response = (status, Json(MacroEnvelope { r#macro: macro_ })).into_response();
    response
        .headers_mut()
        .insert(header::ETAG, tag.to_header_value());
    Ok(response)
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

fn expected_macro_revision(
    headers: &HeaderMap,
    id: Uuid,
    request_id: &str,
) -> Result<u64, ApiFailure> {
    let tag = parse_if_match(headers).map_err(|error| match error {
        minco_http::EntityTagError::PreconditionRequired => {
            ApiFailure::precondition_required(request_id)
        }
        _ => ApiFailure::invalid_if_match(request_id),
    })?;
    tag.resource_revision("macro", &id.to_string())
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
        TicketingServiceError::Store(TicketStoreError::MacroNotFound(_)) => ApiFailure::new(
            StatusCode::NOT_FOUND,
            "macro_not_found",
            "Saved reply not found",
            "The saved reply does not exist in this project.",
            request_id,
        ),
        TicketingServiceError::Store(TicketStoreError::DuplicateMacroTitle) => ApiFailure::new(
            StatusCode::CONFLICT,
            "macro_title_taken",
            "Saved reply title taken",
            "A saved reply with this title already exists in the project.",
            request_id,
        ),
        TicketingServiceError::Validation(error) => {
            ApiFailure::validation(error.to_string(), request_id)
        }
        TicketingServiceError::InvalidManagementRequest => {
            ApiFailure::validation(error.to_string(), request_id)
        }
        value @ (TicketingServiceError::SupportEntry(_)
        | TicketingServiceError::InvalidContentDigest
        | TicketingServiceError::InvalidExternalIdentity
        | TicketingServiceError::InvalidDeliveryFeedback) => {
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
        TicketingServiceError::SessionsUnavailable => ApiFailure::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "ticketing_sessions_unavailable",
            "Sessions unavailable",
            "This application has not registered the sessions, CSRF and idempotency plugins.",
            request_id,
        ),
        TicketingServiceError::SessionUnauthenticated => ApiFailure::new(
            StatusCode::UNAUTHORIZED,
            "ticketing_session_unauthenticated",
            "Session required",
            "The requester session is unknown, expired or revoked.",
            request_id,
        ),
        TicketingServiceError::CsrfRejected => ApiFailure::new(
            StatusCode::FORBIDDEN,
            "ticketing_csrf_invalid",
            "CSRF token invalid",
            "The session CSRF token did not match this session.",
            request_id,
        ),
        TicketingServiceError::EventsUnavailable => ApiFailure::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "ticketing_events_unavailable",
            "Events unavailable",
            "This application has not registered the events plugin.",
            request_id,
        ),
        TicketingServiceError::JobsUnavailable => ApiFailure::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "ticketing_jobs_unavailable",
            "Jobs unavailable",
            "This application has not registered the jobs plugin.",
            request_id,
        ),
        TicketingServiceError::InboundThreadUnresolved => ApiFailure::validation(
            "inbound threading does not reference a known ticket",
            request_id,
        ),
        TicketingServiceError::InboundObjectMissing | TicketingServiceError::InboundMimeInvalid => {
            ApiFailure::validation(
                "the inbound raw object is missing or not parseable MIME",
                request_id,
            )
        }
        TicketingServiceError::ObjectsUnavailable => ApiFailure::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "ticketing_objects_unavailable",
            "Object storage unavailable",
            "This application has not registered the object-storage plugin.",
            request_id,
        ),
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
        // The generated boundary (ADR-0057) answers with the standard
        // contract problem code for malformed JSON.
        assert_eq!(problem["code"], "invalid_json");
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
                ticket_type: crate::TicketType::default(),
                form_answers: Vec::new(),
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
                ticket_type: crate::TicketType::default(),
                form_answers: Vec::new(),
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
    async fn typed_tickets_carry_ticket_type_and_form_answers() {
        let app = ticketing_router(service()).layer(axum::Extension(agent_principal()));
        let response = app
            .clone()
            .oneshot(
                Request::post("/_minco/ticketing/tickets")
                    .header(header::CONTENT_TYPE, "application/json")
                    .extension(agent_principal())
                    .body(Body::from(
                        serde_json::json!({
                            "project_id": "project-a",
                            "subject": "Checkout fails",
                            "description": "Payments error after login.",
                            "requester": {"subject": "user-a"},
                            "channel": "portal",
                            "ticket_type": "incident",
                            "form_answers": [
                                {"field_id": "order-id", "kind": "text", "text_value": "ord-91"},
                                {"field_id": "reproduced", "kind": "boolean", "boolean_value": true},
                                {"field_id": "attempts", "kind": "number", "number_value": 3}
                            ]
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let ticket = &value["ticket"];
        assert_eq!(ticket["ticket_type"], "incident");
        let answers = ticket["form_answers"].as_array().unwrap();
        assert_eq!(answers.len(), 3);
        assert!(
            answers
                .iter()
                .any(|answer| answer["field_id"] == "order-id" && answer["text_value"] == "ord-91")
        );
        assert!(
            answers
                .iter()
                .any(|answer| answer["field_id"] == "attempts" && answer["number_value"] == 3)
        );

        // Omitting the type keeps the default taxonomy home.
        let plain = create_tickets_through_api(&app, 1).await;
        assert_eq!(plain[0]["ticket_type"], "question");
        assert_eq!(plain[0]["form_answers"].as_array().unwrap().len(), 0);

        // Two value slots on one answer is a validation failure, not a
        // silent coercion.
        let rejected = app
            .oneshot(
                Request::post("/_minco/ticketing/tickets")
                    .header(header::CONTENT_TYPE, "application/json")
                    .extension(agent_principal())
                    .body(Body::from(
                        serde_json::json!({
                            "project_id": "project-a",
                            "subject": "Bad form",
                            "description": "Two slots set.",
                            "requester": {"subject": "user-a"},
                            "channel": "portal",
                            "form_answers": [
                                {"field_id": "both", "kind": "text", "text_value": "a", "boolean_value": true}
                            ]
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(rejected.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body = to_bytes(rejected.into_body(), usize::MAX).await.unwrap();
        let problem: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(problem["status"], 422);
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
        // The agent detail envelope carries the ticket plus advisory
        // collision viewers (ADR-0067).
        let detail = app
            .oneshot(
                Request::get(format!("/_minco/ticketing/agent/tickets/{id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let detail_value: serde_json::Value =
            serde_json::from_slice(&to_bytes(detail.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        let ticket = &detail_value["ticket"];
        assert_eq!(ticket["priority"], "urgent");
        assert_eq!(ticket["revision"], managed["ticket"]["revision"]);
        assert!(detail_value["other_recent_viewers"].is_array());
    }

    #[tokio::test]
    async fn assignment_modes_and_sla_deadlines_surface_through_the_api() {
        let service = crate::TicketingService::new(
            crate::TicketingStoreService::new(Arc::new(crate::MemoryTicketingStore::default())),
            crate::TicketingConfig {
                project_id: "project-a".into(),
                assignment_pool: vec!["agent-a".into(), "agent-b".into()],
                sla: Some(crate::TicketSlaConfig {
                    first_response_hours: 4,
                    resolution_hours: 48,
                }),
                ..crate::TicketingConfig::default()
            },
        )
        .unwrap();
        let app = ticketing_router(service).layer(axum::Extension(agent_principal()));
        let created = create_tickets_through_api(&app, 1).await;
        let id = created[0]["id"].as_str().unwrap().to_owned();
        assert!(created[0]["first_response_deadline"].is_string());
        assert!(created[0]["resolution_deadline"].is_string());

        let assigned = app
            .oneshot(
                Request::patch(format!("/_minco/ticketing/tickets/{id}/assignment"))
                    .header(header::CONTENT_TYPE, "application/json")
                    .header(header::IF_MATCH, format!("\"ticket:{id}:1\""))
                    .body(Body::from(
                        serde_json::json!({"mode": "round_robin"}).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(assigned.status(), StatusCode::OK);
        let body: serde_json::Value =
            serde_json::from_slice(&to_bytes(assigned.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        assert_eq!(body["ticket"]["assignee_subject"], "agent-a");
    }

    #[tokio::test]
    async fn curated_views_macros_and_collision_indication_work_end_to_end() {
        let shared = service();
        let app = ticketing_router(shared.clone()).layer(axum::Extension(agent_principal()));
        let tickets = create_tickets_through_api(&app, 2).await;
        let first = tickets[0].clone();

        // Curated views: the closed set answers with filtered summaries;
        // unknown views are rejected, not guessed.
        let view = app
            .clone()
            .oneshot(
                Request::get("/_minco/ticketing/agent/views/new-unassigned")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(view.status(), StatusCode::OK);
        let body: serde_json::Value =
            serde_json::from_slice(&to_bytes(view.into_body(), usize::MAX).await.unwrap()).unwrap();
        let summaries = body["data"].as_array().unwrap();
        assert!(!summaries.is_empty());
        assert!(
            summaries
                .iter()
                .all(|summary| summary["status"] == "new" && summary["assignee_subject"].is_null())
        );
        let unknown = app
            .clone()
            .oneshot(
                Request::get("/_minco/ticketing/agent/views/everything")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(unknown.status(), StatusCode::NOT_FOUND);

        // Macros: create, list, revision-guarded update, duplicate title.
        let create = app
            .clone()
            .oneshot(
                Request::post("/_minco/ticketing/agent/macros")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        serde_json::json!({"title": "Greeting", "body": "Hi there!"}).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(create.status(), StatusCode::CREATED);
        let created: serde_json::Value =
            serde_json::from_slice(&to_bytes(create.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        let macro_id = created["macro"]["id"].as_str().unwrap().to_owned();
        assert_eq!(created["macro"]["revision"], 0);

        let duplicate = app
            .clone()
            .oneshot(
                Request::post("/_minco/ticketing/agent/macros")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        serde_json::json!({"title": "Greeting", "body": "Other"}).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(duplicate.status(), StatusCode::CONFLICT);

        let update = app
            .clone()
            .oneshot(
                Request::patch(format!("/_minco/ticketing/agent/macros/{macro_id}"))
                    .header(header::CONTENT_TYPE, "application/json")
                    .header(header::IF_MATCH, format!("\"macro:{macro_id}:1\""))
                    .body(Body::from(
                        serde_json::json!({"title": "Greeting", "body": "Hi! Edits welcome."})
                            .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(update.status(), StatusCode::OK);
        let updated: serde_json::Value =
            serde_json::from_slice(&to_bytes(update.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        assert_eq!(updated["macro"]["revision"], 1);

        let stale = app
            .clone()
            .oneshot(
                Request::patch(format!("/_minco/ticketing/agent/macros/{macro_id}"))
                    .header(header::CONTENT_TYPE, "application/json")
                    .header(header::IF_MATCH, format!("\"macro:{macro_id}:1\""))
                    .body(Body::from(
                        serde_json::json!({"title": "Greeting", "body": "Overwrite"}).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(stale.status(), StatusCode::PRECONDITION_FAILED);

        let list = app
            .clone()
            .oneshot(
                Request::get("/_minco/ticketing/agent/macros")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let listed: serde_json::Value =
            serde_json::from_slice(&to_bytes(list.into_body(), usize::MAX).await.unwrap()).unwrap();
        assert_eq!(listed["data"].as_array().unwrap().len(), 1);

        // Collision indication: another agent's detail view surfaces for
        // this agent's next detail fetch, and never the viewer themself.
        let ticket_id = first["id"].as_str().unwrap().to_owned();
        let other =
            ticketing_router(shared.clone()).layer(axum::Extension(minco_http::Principal {
                subject: "agent-2".into(),
                permissions: ["ticketing.agent-console", "ticketing.agent.read"]
                    .into_iter()
                    .map(str::to_owned)
                    .collect(),
                claims: BTreeMap::new(),
            }));
        let _ = other
            .oneshot(
                Request::get(format!("/_minco/ticketing/agent/tickets/{ticket_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let mine = app
            .oneshot(
                Request::get(format!("/_minco/ticketing/agent/tickets/{ticket_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let detail: serde_json::Value =
            serde_json::from_slice(&to_bytes(mine.into_body(), usize::MAX).await.unwrap()).unwrap();
        assert_eq!(detail["other_recent_viewers"][0], "agent-2");
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

    async fn portal_app() -> (Router, SupportHandoffToken) {
        use minco_plugin_idempotency::{IdempotencyService, MemoryIdempotencyStore};
        use minco_plugin_sessions::{CsrfService, MemorySessionStore, SessionService};
        let store = Arc::new(MemoryTicketingStore::default());
        let service = TicketingService::new(
            TicketingStoreService::new(store.clone()),
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
        .with_portal_services(crate::TicketingPortalServices {
            sessions: Some(Arc::new(SessionService::new(Arc::new(
                MemorySessionStore::default(),
            )))),
            csrf: Some(Arc::new(
                CsrfService::new(b"test-csrf-secret-0123456789abcdef".to_vec()).unwrap(),
            )),
            idempotency: Some(Arc::new(
                IdempotencyService::new(
                    Arc::new(MemoryIdempotencyStore::default()),
                    chrono::TimeDelta::seconds(300),
                )
                .unwrap(),
            )),
            events: None,
            #[cfg(feature = "jobs")]
            jobs: None,
            objects: None,
        });
        let integration = identity(minco_http::Principal {
            subject: "integration".into(),
            permissions: std::iter::once("ticketing.integrate".into()).collect(),
            claims: BTreeMap::new(),
        });
        let now = Utc::now();
        let grant = service
            .issue_ticketing_handoff(
                &integration,
                IssueTicketingHandoffInput {
                    project_id: "project-a".into(),
                    requester_subject: "user-1".into(),
                    requester_permissions: vec!["ticketing.read".into(), "ticketing.reply".into()],
                    surface: minco_interaction::SupportSurface::Portal,
                    context: minco_interaction::SupportContext {
                        page_url: "https://app.example.test/orders/1".into(),
                        ..minco_interaction::SupportContext::default()
                    },
                    return_location: "https://app.example.test/orders/1".into(),
                    correlation_id: Uuid::now_v7(),
                },
                now,
            )
            .await
            .unwrap();
        // The requester's own ticket exists before the session starts.
        service
            .create_ticket(
                &identity(minco_http::Principal {
                    subject: "user-1".into(),
                    permissions: std::iter::once("ticketing.create".into()).collect(),
                    claims: BTreeMap::new(),
                }),
                CreateTicketInput {
                    project_id: "project-a".into(),
                    subject: "Own ticket".into(),
                    description: "It broke and the requester needs help.".into(),
                    requester: crate::TicketRequester {
                        subject: "user-1".into(),
                        display_name: None,
                        email: None,
                    },
                    channel: crate::TicketChannel::Portal,
                    ticket_type: crate::TicketType::default(),
                    form_answers: Vec::new(),
                    priority: crate::TicketPriority::Normal,
                    resource_references: Vec::new(),
                },
                Uuid::now_v7(),
                now,
            )
            .await
            .unwrap();
        (ticketing_router(service), grant.token)
    }

    #[tokio::test]
    async fn session_exchange_issues_cookie_and_replays_identically() {
        let (app, token) = portal_app().await;
        let exchange = |app: &Router, token: &SupportHandoffToken, origin: &str| {
            app.clone().oneshot(
                Request::post("/_minco/ticketing/requester/sessions")
                    .header(header::CONTENT_TYPE, "application/json")
                    .header(HANDOFF_HEADER, token.expose_sensitive())
                    .body(Body::from(
                        serde_json::json!({ "portal_origin": origin }).to_string(),
                    ))
                    .unwrap(),
            )
        };

        let created = exchange(&app, &token, "https://support.example.test")
            .await
            .unwrap();
        assert_eq!(created.status(), StatusCode::CREATED);
        let cookie = created
            .headers()
            .get(header::SET_COOKIE)
            .and_then(|value| value.to_str().ok())
            .expect("session cookie is set")
            .to_owned();
        assert!(cookie.starts_with("minco_ticketing_session="));
        assert!(cookie.contains("Secure"));
        assert!(cookie.contains("HttpOnly"));
        assert!(cookie.contains("SameSite=Lax"));
        assert!(cookie.contains("Path=/_minco/ticketing"));
        let grant: serde_json::Value =
            serde_json::from_slice(&to_bytes(created.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        assert!(grant["csrf_token"].as_str().is_some_and(|v| !v.is_empty()));
        assert!(grant["expires_at"].as_str().is_some());

        let replay = exchange(&app, &token, "https://support.example.test")
            .await
            .unwrap();
        assert_eq!(replay.status(), StatusCode::OK);
        let replayed: serde_json::Value =
            serde_json::from_slice(&to_bytes(replay.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        assert_eq!(replayed, grant);

        let conflict = exchange(&app, &token, "https://other.example.test")
            .await
            .unwrap();
        assert_eq!(conflict.status(), StatusCode::CONFLICT);

        // The cookie authenticates the requester list with own isolation.
        let session_value = cookie
            .split(';')
            .next()
            .and_then(|pair| pair.split_once('='))
            .map(|(_, value)| value.to_owned())
            .unwrap();
        let listed = app
            .clone()
            .oneshot(
                Request::get("/_minco/ticketing/requester/tickets")
                    .header(
                        header::COOKIE,
                        format!("minco_ticketing_session={session_value}"),
                    )
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(listed.status(), StatusCode::OK);
        let page: serde_json::Value =
            serde_json::from_slice(&to_bytes(listed.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        assert_eq!(page["data"].as_array().unwrap().len(), 1);
        assert_eq!(page["data"][0]["subject"], "Own ticket");
    }

    #[tokio::test]
    async fn session_mutations_require_csrf_and_logout_revokes() {
        let (app, token) = portal_app().await;
        let created = app
            .clone()
            .oneshot(
                Request::post("/_minco/ticketing/requester/sessions")
                    .header(header::CONTENT_TYPE, "application/json")
                    .header(HANDOFF_HEADER, token.expose_sensitive())
                    .body(Body::from(
                        serde_json::json!({ "portal_origin": "https://support.example.test" })
                            .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(created.status(), StatusCode::CREATED);
        let cookie_header = created.headers()[header::SET_COOKIE]
            .to_str()
            .unwrap()
            .to_owned();
        let session_value = cookie_header
            .split(';')
            .next()
            .and_then(|pair| pair.split_once('='))
            .map(|(_, value)| value.to_owned())
            .unwrap();
        let grant: serde_json::Value =
            serde_json::from_slice(&to_bytes(created.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        let csrf = grant["csrf_token"].as_str().unwrap().to_owned();

        let detail = app
            .clone()
            .oneshot(
                Request::get("/_minco/ticketing/requester/tickets")
                    .header(
                        header::COOKIE,
                        format!("minco_ticketing_session={session_value}"),
                    )
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let page: serde_json::Value =
            serde_json::from_slice(&to_bytes(detail.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        let ticket_id = page["data"][0]["id"].as_str().unwrap().to_owned();
        let etag = page["data"][0]["revision"].as_u64().unwrap() + 1;

        let reply_path = format!("/_minco/ticketing/requester/tickets/{ticket_id}/replies");
        let missing_csrf = app
            .clone()
            .oneshot(
                Request::post(&reply_path)
                    .header(header::CONTENT_TYPE, "application/json")
                    .header(
                        header::COOKIE,
                        format!("minco_ticketing_session={session_value}"),
                    )
                    .header(header::IF_MATCH, format!("\"ticket:{ticket_id}:{etag}\""))
                    .body(Body::from(
                        serde_json::json!({"body": "More detail."}).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(missing_csrf.status(), StatusCode::FORBIDDEN);

        let replied = app
            .clone()
            .oneshot(
                Request::post(&reply_path)
                    .header(header::CONTENT_TYPE, "application/json")
                    .header(
                        header::COOKIE,
                        format!("minco_ticketing_session={session_value}"),
                    )
                    .header("x-minco-csrf", &csrf)
                    .header(header::IF_MATCH, format!("\"ticket:{ticket_id}:{etag}\""))
                    .body(Body::from(
                        serde_json::json!({"body": "More detail."}).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(replied.status(), StatusCode::OK);

        let logout_no_csrf = app
            .clone()
            .oneshot(
                Request::post("/_minco/ticketing/requester/logout")
                    .header(
                        header::COOKIE,
                        format!("minco_ticketing_session={session_value}"),
                    )
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(logout_no_csrf.status(), StatusCode::FORBIDDEN);

        let logout = app
            .clone()
            .oneshot(
                Request::post("/_minco/ticketing/requester/logout")
                    .header(
                        header::COOKIE,
                        format!("minco_ticketing_session={session_value}"),
                    )
                    .header("x-minco-csrf", &csrf)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(logout.status(), StatusCode::NO_CONTENT);

        let after_logout = app
            .clone()
            .oneshot(
                Request::get("/_minco/ticketing/requester/tickets")
                    .header(
                        header::COOKIE,
                        format!("minco_ticketing_session={session_value}"),
                    )
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(after_logout.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn without_portal_services_the_exchange_fails_closed() {
        let app = ticketing_router(service());
        let response = app
            .oneshot(
                Request::post("/_minco/ticketing/requester/sessions")
                    .header(header::CONTENT_TYPE, "application/json")
                    .header(HANDOFF_HEADER, "a".repeat(64))
                    .body(Body::from(
                        serde_json::json!({ "portal_origin": "https://support.example.test" })
                            .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn requester_messages_paginate_newest_first_without_internal_notes() {
        let shared = service();
        let app =
            ticketing_router(shared.clone()).layer(axum::Extension(requester_principal("user-a")));
        let created = create_requester_ticket(&app, "user-a", "Own ticket").await;
        let id = created["ticket"]["id"].as_str().unwrap().to_owned();

        let mut revision = created["ticket"]["revision"].as_u64().unwrap();
        for body in ["first reply", "second reply"] {
            let response = app
                .clone()
                .oneshot(
                    Request::post(format!("/_minco/ticketing/tickets/{id}/requester-replies"))
                        .header(header::CONTENT_TYPE, "application/json")
                        .header(
                            header::IF_MATCH,
                            format!("\"ticket:{id}:{}\"", revision + 1),
                        )
                        .body(Body::from(serde_json::json!({"body": body}).to_string()))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            revision += 1;
        }
        // An agent note that must never appear on the requester surface.
        let mut agent = requester_principal("user-a");
        agent.permissions = std::iter::once("ticketing.manage".to_owned()).collect();
        let agent_app = ticketing_router(shared.clone()).layer(axum::Extension(agent));
        let note = agent_app
            .oneshot(
                Request::post(format!("/_minco/ticketing/tickets/{id}/internal-notes"))
                    .header(header::CONTENT_TYPE, "application/json")
                    .header(
                        header::IF_MATCH,
                        format!("\"ticket:{id}:{}\"", revision + 1),
                    )
                    .body(Body::from(
                        serde_json::json!({"body": "secret internal note"}).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(note.status(), StatusCode::OK);

        let page = app
            .clone()
            .oneshot(
                Request::get(format!(
                    "/_minco/ticketing/requester/tickets/{id}/messages?page[limit]=2"
                ))
                .body(Body::empty())
                .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(page.status(), StatusCode::OK);
        let body: serde_json::Value =
            serde_json::from_slice(&to_bytes(page.into_body(), usize::MAX).await.unwrap()).unwrap();
        let encoded = serde_json::to_string(&body).unwrap();
        assert!(!encoded.contains("secret internal note"));
        assert!(!encoded.contains("author_subject"));
        assert_eq!(body["data"].as_array().unwrap().len(), 2);
        assert_eq!(body["data"][0]["body"], "second reply");
        assert_eq!(body["data"][0]["author"], "requester");
        assert_eq!(body["page"]["hasMore"], true);
        let cursor = body["page"]["nextCursor"].as_str().unwrap().to_owned();

        let next = app
            .clone()
            .oneshot(
                Request::get(format!(
                    "/_minco/ticketing/requester/tickets/{id}/messages?page[limit]=2&page[after]={cursor}"
                ))
                .body(Body::empty())
                .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(next.status(), StatusCode::OK);
        let rest: serde_json::Value =
            serde_json::from_slice(&to_bytes(next.into_body(), usize::MAX).await.unwrap()).unwrap();
        let mut seen = body["data"].as_array().unwrap().clone();
        seen.extend(rest["data"].as_array().unwrap().iter().cloned());
        assert_eq!(seen.len(), 3);
        let unique: BTreeSet<_> = seen
            .iter()
            .map(|message| message["id"].as_str().unwrap().to_owned())
            .collect();
        assert_eq!(unique.len(), 3);

        let invalid = app
            .clone()
            .oneshot(
                Request::get(format!(
                    "/_minco/ticketing/requester/tickets/{id}/messages?page[after]=bad.cursor"
                ))
                .body(Body::empty())
                .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(invalid.status(), StatusCode::UNPROCESSABLE_ENTITY);

        let foreign_app =
            ticketing_router(shared.clone()).layer(axum::Extension(requester_principal("user-b")));
        let foreign = foreign_app
            .oneshot(
                Request::get(format!("/_minco/ticketing/requester/tickets/{id}/messages"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(foreign.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn bootstrap_is_truthful_about_capabilities_and_portal_sessions() {
        let app = ticketing_router(service());
        let response = app
            .oneshot(
                Request::get("/_minco/ticketing/bootstrap")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bootstrap: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        assert_eq!(bootstrap["screenshot_enabled"], false);
        assert_eq!(bootstrap["voice_enabled"], false);
        assert_eq!(bootstrap["file_enabled"], false);
        assert_eq!(bootstrap["capabilities"]["portal_sessions"], false);
        assert_eq!(bootstrap["capabilities"]["history"], true);
        assert_eq!(bootstrap["capabilities"]["files"], false);
        assert_eq!(bootstrap["capabilities"]["email"], false);
        assert_eq!(bootstrap["capabilities"]["automation"], false);

        // With the sessions/CSRF services registered the portal-session
        // capability flips to true and nothing else changes.
        let (portal_app, _token) = portal_app().await;
        let response = portal_app
            .oneshot(
                Request::get("/_minco/ticketing/bootstrap")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let bootstrap: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        assert_eq!(bootstrap["capabilities"]["portal_sessions"], true);
        assert_eq!(bootstrap["capabilities"]["files"], false);
    }
}
