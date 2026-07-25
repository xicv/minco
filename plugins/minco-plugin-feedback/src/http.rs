use crate::{
    AttachmentUpload, AudioInput, ClientReplyInput, CreateFeedbackInput, DeveloperReplyInput,
    FEEDBACK_BASE_PATH, FeedbackAccessToken, FeedbackAttachment, FeedbackAttachmentKind,
    FeedbackConfig, FeedbackId, FeedbackListFilter, FeedbackMutationResult, FeedbackService,
    FeedbackServiceError, FeedbackStatus, FeedbackThread, FeedbackWarning, FeedbackWidgetConfig,
    Transcript, TransitionFeedbackInput,
};
use axum::{
    Extension, Json, Router,
    body::Body,
    extract::{DefaultBodyLimit, Multipart, Path, Query, State},
    response::{IntoResponse, Response},
    routing::{get, patch, post},
};
use http::{HeaderMap, HeaderValue, StatusCode, header};
use minco_http::{ApiFailure, Principal};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{str::FromStr, sync::Arc};
use subtle::ConstantTimeEq;
use uuid::Uuid;

const WIDGET_SOURCE: &str = include_str!("../assets/widget.js");
const CLIENT_TOKEN_HEADER: &str = "x-minco-feedback-token";
const PROJECT_KEY_HEADER: &str = "x-minco-feedback-project-key";
const MAX_PAYLOAD_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone)]
struct FeedbackHttpState {
    service: FeedbackService,
    config: Arc<FeedbackConfig>,
}

pub fn feedback_router(service: FeedbackService) -> Router {
    let config = Arc::new(service.config().clone());
    let maximum_body = feedback_request_body_budget(&config);
    let state = FeedbackHttpState { service, config };
    let routes = Router::new()
        .route("/widget.js", get(widget_source))
        .route("/widget-config", get(widget_config))
        .route("/threads", post(create_feedback))
        .route("/threads/{id}", get(get_client_feedback))
        .route("/threads/{id}/messages", post(client_reply))
        .route(
            "/threads/{id}/attachments/{attachment_id}",
            get(client_attachment),
        )
        .route("/transcriptions", post(transcribe_audio))
        .route("/developer/threads", get(list_developer_feedback))
        .route("/developer/threads/{id}", get(get_developer_feedback))
        .route("/developer/threads/{id}/messages", post(developer_reply))
        .route("/developer/threads/{id}/status", patch(transition_feedback))
        .route(
            "/developer/threads/{id}/ai-context",
            get(feedback_ai_context),
        )
        .route(
            "/developer/threads/{id}/attachments/{attachment_id}",
            get(developer_attachment),
        )
        .layer(DefaultBodyLimit::max(maximum_body))
        .with_state(state);
    Router::new().nest(FEEDBACK_BASE_PATH, routes)
}

async fn widget_source() -> Response {
    let mut response = WIDGET_SOURCE.into_response();
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

async fn widget_config(State(state): State<FeedbackHttpState>) -> Json<FeedbackWidgetConfig> {
    Json(state.config.widget_config())
}

async fn create_feedback(
    State(state): State<FeedbackHttpState>,
    principal: Option<Extension<Principal>>,
    headers: HeaderMap,
    multipart: Multipart,
) -> Result<(StatusCode, Json<ClientCreateResponse>), ApiFailure> {
    let request_id = request_id(&headers);
    let principal = principal.as_ref().map(|Extension(value)| value);
    authorize_submission(&headers, &state.config, principal, &request_id)?;
    let (mut input, attachments) = read_submission(multipart, &state.config, &request_id).await?;
    bind_client_subject(&mut input, principal);
    let result = state
        .service
        .create(input, attachments, Uuid::now_v7())
        .await
        .map_err(|error| map_error(error, &request_id))?;
    Ok((
        StatusCode::CREATED,
        Json(ClientCreateResponse {
            thread: ClientFeedbackThread::from(&result.thread),
            client_token: result.client_token.expose().to_owned(),
            warnings: result.warnings,
        }),
    ))
}

async fn get_client_feedback(
    State(state): State<FeedbackHttpState>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<ClientFeedbackThread>, ApiFailure> {
    let request_id = request_id(&headers);
    let id = parse_feedback_id(&id, &request_id)?;
    let token = client_token(&headers, &request_id)?;
    let thread = state
        .service
        .get_for_client(id, &token)
        .await
        .map_err(|error| map_error(error, &request_id))?;
    Ok(Json(ClientFeedbackThread::from(&thread)))
}

async fn client_reply(
    State(state): State<FeedbackHttpState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(input): Json<ClientReplyInput>,
) -> Result<Json<ClientMutationResponse>, ApiFailure> {
    let request_id = request_id(&headers);
    let id = parse_feedback_id(&id, &request_id)?;
    let token = client_token(&headers, &request_id)?;
    let result = state
        .service
        .reply_as_client(id, &token, input.body, Uuid::now_v7())
        .await
        .map_err(|error| map_error(error, &request_id))?;
    Ok(Json(ClientMutationResponse::from(result)))
}

async fn client_attachment(
    State(state): State<FeedbackHttpState>,
    Path((id, attachment_id)): Path<(String, String)>,
    headers: HeaderMap,
) -> Result<Response, ApiFailure> {
    let request_id = request_id(&headers);
    let id = parse_feedback_id(&id, &request_id)?;
    let attachment_id = parse_uuid(&attachment_id, "attachment_id", &request_id)?;
    let token = client_token(&headers, &request_id)?;
    let object = state
        .service
        .attachment_for_client(id, &token, attachment_id)
        .await
        .map_err(|error| map_error(error, &request_id))?;
    Ok(object_response(object))
}

async fn transcribe_audio(
    State(state): State<FeedbackHttpState>,
    principal: Option<Extension<Principal>>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Result<Json<Transcript>, ApiFailure> {
    let request_id = request_id(&headers);
    authorize_transcription(
        &headers,
        &state.config,
        principal.as_ref().map(|Extension(value)| value),
        &request_id,
    )?;
    let mut audio = None;
    let mut language = None;
    let mut prompt = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|error| multipart_failure(&error.to_string(), &request_id))?
    {
        let name = field.name().unwrap_or_default().to_owned();
        match name.as_str() {
            "language" => {
                language = Some(
                    field
                        .text()
                        .await
                        .map_err(|error| multipart_failure(&error.to_string(), &request_id))?,
                );
            }
            "prompt" => {
                prompt = Some(
                    field
                        .text()
                        .await
                        .map_err(|error| multipart_failure(&error.to_string(), &request_id))?,
                );
            }
            "audio" => {
                let file_name = field.file_name().unwrap_or("feedback.webm").to_owned();
                let content_type = field
                    .content_type()
                    .unwrap_or("application/octet-stream")
                    .to_owned();
                let bytes = field
                    .bytes()
                    .await
                    .map_err(|error| multipart_failure(&error.to_string(), &request_id))?;
                if bytes.len() > state.config.max_audio_bytes {
                    return Err(ApiFailure::new(
                        StatusCode::PAYLOAD_TOO_LARGE,
                        "feedback_audio_too_large",
                        "Audio is too large",
                        format!(
                            "Audio is {} bytes; configured limit is {} bytes.",
                            bytes.len(),
                            state.config.max_audio_bytes
                        ),
                        request_id,
                    ));
                }
                audio = Some(AudioInput {
                    bytes: bytes.to_vec(),
                    content_type,
                    file_name,
                    language: None,
                    prompt: None,
                });
            }
            _ => {
                return Err(ApiFailure::validation(
                    format!("unsupported transcription multipart field {name:?}"),
                    request_id,
                ));
            }
        }
    }
    let mut audio = audio.ok_or_else(|| {
        ApiFailure::validation("multipart field `audio` is required", request_id.clone())
    })?;
    audio.language = bounded_optional_text(language, "language", 64, &request_id)?;
    audio.prompt = bounded_optional_text(prompt, "prompt", 2_000, &request_id)?;
    let transcript = state
        .service
        .transcribe(audio)
        .await
        .map_err(|error| map_error(error, &request_id))?;
    Ok(Json(transcript))
}

async fn list_developer_feedback(
    State(state): State<FeedbackHttpState>,
    principal: Option<Extension<Principal>>,
    Query(filter): Query<FeedbackListFilter>,
    headers: HeaderMap,
) -> Result<Json<Vec<crate::FeedbackSummary>>, ApiFailure> {
    let request_id = request_id(&headers);
    let _developer_actor = authorize_developer(
        &headers,
        &state.config,
        principal.as_ref().map(|Extension(value)| value),
        &request_id,
    )?;
    Ok(Json(
        state
            .service
            .list(filter)
            .await
            .map_err(|error| map_error(error, &request_id))?,
    ))
}

async fn get_developer_feedback(
    State(state): State<FeedbackHttpState>,
    principal: Option<Extension<Principal>>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<FeedbackThread>, ApiFailure> {
    let request_id = request_id(&headers);
    let _developer_actor = authorize_developer(
        &headers,
        &state.config,
        principal.as_ref().map(|Extension(value)| value),
        &request_id,
    )?;
    let id = parse_feedback_id(&id, &request_id)?;
    Ok(Json(
        state
            .service
            .get_for_developer(id)
            .await
            .map_err(|error| map_error(error, &request_id))?,
    ))
}

async fn developer_reply(
    State(state): State<FeedbackHttpState>,
    principal: Option<Extension<Principal>>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(mut input): Json<DeveloperReplyInput>,
) -> Result<Json<FeedbackMutationResult>, ApiFailure> {
    let request_id = request_id(&headers);
    let developer_actor = authorize_developer(
        &headers,
        &state.config,
        principal.as_ref().map(|Extension(value)| value),
        &request_id,
    )?;
    if let Some(Extension(principal)) = principal {
        input.author_display = Some(principal.subject);
    }
    let id = parse_feedback_id(&id, &request_id)?;
    Ok(Json(
        state
            .service
            .reply_as_developer(id, input, developer_actor, Uuid::now_v7())
            .await
            .map_err(|error| map_error(error, &request_id))?,
    ))
}

async fn transition_feedback(
    State(state): State<FeedbackHttpState>,
    principal: Option<Extension<Principal>>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(mut input): Json<TransitionFeedbackInput>,
) -> Result<Json<FeedbackMutationResult>, ApiFailure> {
    let request_id = request_id(&headers);
    let developer_actor = authorize_developer(
        &headers,
        &state.config,
        principal.as_ref().map(|Extension(value)| value),
        &request_id,
    )?;
    if let Some(Extension(principal)) = principal {
        input.author_display = Some(principal.subject);
    }
    let id = parse_feedback_id(&id, &request_id)?;
    Ok(Json(
        state
            .service
            .transition(id, input, developer_actor, Uuid::now_v7())
            .await
            .map_err(|error| map_error(error, &request_id))?,
    ))
}

async fn feedback_ai_context(
    State(state): State<FeedbackHttpState>,
    principal: Option<Extension<Principal>>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<Response, ApiFailure> {
    let request_id = request_id(&headers);
    let _developer_actor = authorize_developer(
        &headers,
        &state.config,
        principal.as_ref().map(|Extension(value)| value),
        &request_id,
    )?;
    let id = parse_feedback_id(&id, &request_id)?;
    let context = state
        .service
        .ai_context(id)
        .await
        .map_err(|error| map_error(error, &request_id))?;
    let accept = headers
        .get(header::ACCEPT)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    if accept.contains("application/json") {
        return Ok(Json(context).into_response());
    }
    let mut response = context.to_markdown().into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/markdown; charset=utf-8"),
    );
    Ok(response)
}

async fn developer_attachment(
    State(state): State<FeedbackHttpState>,
    principal: Option<Extension<Principal>>,
    Path((id, attachment_id)): Path<(String, String)>,
    headers: HeaderMap,
) -> Result<Response, ApiFailure> {
    let request_id = request_id(&headers);
    let _developer_actor = authorize_developer(
        &headers,
        &state.config,
        principal.as_ref().map(|Extension(value)| value),
        &request_id,
    )?;
    let id = parse_feedback_id(&id, &request_id)?;
    let attachment_id = parse_uuid(&attachment_id, "attachment_id", &request_id)?;
    let object = state
        .service
        .attachment_for_developer(id, attachment_id)
        .await
        .map_err(|error| map_error(error, &request_id))?;
    Ok(object_response(object))
}

async fn read_submission(
    mut multipart: Multipart,
    config: &FeedbackConfig,
    request_id: &str,
) -> Result<(CreateFeedbackInput, Vec<AttachmentUpload>), ApiFailure> {
    let mut payload = None;
    let mut attachments = Vec::new();
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|error| multipart_failure(&error.to_string(), request_id))?
    {
        let name = field.name().unwrap_or_default().to_owned();
        match name.as_str() {
            "payload" => {
                if payload.is_some() {
                    return Err(ApiFailure::validation(
                        "multipart field `payload` may appear only once",
                        request_id,
                    ));
                }
                let bytes = field
                    .bytes()
                    .await
                    .map_err(|error| multipart_failure(&error.to_string(), request_id))?;
                if bytes.len() > MAX_PAYLOAD_BYTES {
                    return Err(ApiFailure::new(
                        StatusCode::PAYLOAD_TOO_LARGE,
                        "feedback_payload_too_large",
                        "Feedback payload is too large",
                        format!(
                            "The JSON payload is {} bytes; limit is {MAX_PAYLOAD_BYTES} bytes.",
                            bytes.len()
                        ),
                        request_id,
                    ));
                }
                payload = Some(
                    serde_json::from_slice::<CreateFeedbackInput>(&bytes)
                        .map_err(|error| ApiFailure::validation(error.to_string(), request_id))?,
                );
            }
            "screenshot" | "audio" | "file" => {
                if attachments.len() >= config.max_attachments {
                    return Err(ApiFailure::validation(
                        format!(
                            "no more than {} attachments are allowed",
                            config.max_attachments
                        ),
                        request_id,
                    ));
                }
                let kind = match name.as_str() {
                    "screenshot" => FeedbackAttachmentKind::Screenshot,
                    "audio" => FeedbackAttachmentKind::Audio,
                    _ => FeedbackAttachmentKind::File,
                };
                let file_name = field.file_name().unwrap_or("attachment.bin").to_owned();
                let content_type = field
                    .content_type()
                    .unwrap_or("application/octet-stream")
                    .to_owned();
                let bytes = field
                    .bytes()
                    .await
                    .map_err(|error| multipart_failure(&error.to_string(), request_id))?;
                let maximum = match kind {
                    FeedbackAttachmentKind::Screenshot => config.max_screenshot_bytes,
                    FeedbackAttachmentKind::Audio => config.max_audio_bytes,
                    FeedbackAttachmentKind::File => config.max_file_bytes,
                };
                if bytes.len() > maximum {
                    return Err(ApiFailure::new(
                        StatusCode::PAYLOAD_TOO_LARGE,
                        "feedback_attachment_too_large",
                        "Feedback attachment is too large",
                        format!(
                            "Attachment is {} bytes; configured limit is {maximum} bytes.",
                            bytes.len()
                        ),
                        request_id,
                    ));
                }
                attachments.push(AttachmentUpload {
                    kind,
                    file_name,
                    content_type,
                    bytes: bytes.to_vec(),
                });
            }
            _ => {
                return Err(ApiFailure::validation(
                    format!("unsupported feedback multipart field {name:?}"),
                    request_id,
                ));
            }
        }
    }
    let payload = payload.ok_or_else(|| {
        ApiFailure::validation("multipart field `payload` is required", request_id)
    })?;
    Ok((payload, attachments))
}

fn bounded_optional_text(
    value: Option<String>,
    field: &str,
    maximum: usize,
    request_id: &str,
) -> Result<Option<String>, ApiFailure> {
    let Some(value) = value else {
        return Ok(None);
    };
    let value = value.trim().to_owned();
    if value.is_empty() {
        return Ok(None);
    }
    if value.chars().count() > maximum || value.chars().any(char::is_control) {
        return Err(ApiFailure::validation(
            format!("{field} must not exceed {maximum} visible characters"),
            request_id,
        ));
    }
    Ok(Some(value))
}

fn authorize_submission(
    headers: &HeaderMap,
    config: &FeedbackConfig,
    principal: Option<&Principal>,
    request_id: &str,
) -> Result<(), ApiFailure> {
    if let Some(principal) = principal {
        if principal.has_permission("feedback.create") {
            return Ok(());
        }
        return Err(ApiFailure::new(
            StatusCode::FORBIDDEN,
            "feedback_submission_forbidden",
            "Feedback submission is forbidden",
            "The authenticated principal does not have feedback.create permission.",
            request_id,
        ));
    }
    if let Some(expected) = config.project_key.as_deref() {
        let supplied = headers
            .get(PROJECT_KEY_HEADER)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default();
        if constant_time_equals(expected, supplied) {
            return Ok(());
        }
        return Err(ApiFailure::new(
            StatusCode::UNAUTHORIZED,
            "feedback_project_key_invalid",
            "Feedback submission is not authorized",
            "The feedback project key is missing or invalid.",
            request_id,
        ));
    }
    if config.allow_anonymous {
        Ok(())
    } else {
        Err(ApiFailure::new(
            StatusCode::UNAUTHORIZED,
            "feedback_authentication_required",
            "Feedback authentication is required",
            "Sign in before submitting feedback.",
            request_id,
        ))
    }
}

fn authorize_transcription(
    headers: &HeaderMap,
    config: &FeedbackConfig,
    principal: Option<&Principal>,
    request_id: &str,
) -> Result<(), ApiFailure> {
    authorize_submission(headers, config, principal, request_id)?;
    if principal.is_some() {
        return Ok(());
    }
    Err(ApiFailure::new(
        StatusCode::FORBIDDEN,
        "feedback_transcription_authentication_required",
        "Voice transcription requires authentication",
        "Sign in with feedback.create permission before requesting voice transcription.",
        request_id,
    ))
}

fn bind_client_subject(input: &mut CreateFeedbackInput, principal: Option<&Principal>) {
    input.context.client_subject = principal.map(|principal| principal.subject.clone());
}

fn authorize_developer(
    headers: &HeaderMap,
    config: &FeedbackConfig,
    principal: Option<&Principal>,
    request_id: &str,
) -> Result<String, ApiFailure> {
    if let Some(principal) = principal {
        if principal.has_permission("feedback.manage") {
            return Ok(principal.subject.clone());
        }
        return Err(ApiFailure::new(
            StatusCode::FORBIDDEN,
            "feedback_management_forbidden",
            "Feedback management is forbidden",
            "The authenticated principal does not have feedback.manage permission.",
            request_id,
        ));
    }
    let expected = config.developer_token.as_deref().ok_or_else(|| {
        ApiFailure::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "feedback_developer_access_unconfigured",
            "Developer access is not configured",
            "Configure identity middleware or a fallback developer token before exposing management routes.",
            request_id,
        )
    })?;
    let supplied = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .unwrap_or_default();
    if constant_time_equals(expected, supplied) {
        Ok("feedback-developer-token".into())
    } else {
        Err(ApiFailure::new(
            StatusCode::UNAUTHORIZED,
            "feedback_developer_token_invalid",
            "Developer authentication failed",
            "The feedback developer token is missing or invalid.",
            request_id,
        ))
    }
}

fn client_token(headers: &HeaderMap, request_id: &str) -> Result<FeedbackAccessToken, ApiFailure> {
    let value = headers
        .get(CLIENT_TOKEN_HEADER)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    FeedbackAccessToken::parse(value).map_err(|_| {
        ApiFailure::new(
            StatusCode::UNAUTHORIZED,
            "feedback_client_token_invalid",
            "Feedback access failed",
            "The client feedback token is missing or invalid.",
            request_id,
        )
    })
}

fn parse_feedback_id(value: &str, request_id: &str) -> Result<FeedbackId, ApiFailure> {
    FeedbackId::from_str(value)
        .map_err(|_| ApiFailure::validation("feedback ID must be a UUID", request_id))
}

fn parse_uuid(value: &str, field: &str, request_id: &str) -> Result<Uuid, ApiFailure> {
    Uuid::parse_str(value)
        .map_err(|_| ApiFailure::validation(format!("{field} must be a UUID"), request_id))
}

fn request_id(headers: &HeaderMap) -> String {
    headers
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.trim().is_empty())
        .map_or_else(|| Uuid::now_v7().to_string(), str::to_owned)
}

fn constant_time_equals(expected: &str, supplied: &str) -> bool {
    let expected = Sha256::digest(expected.as_bytes());
    let supplied = Sha256::digest(supplied.as_bytes());
    expected.ct_eq(&supplied).into()
}

fn multipart_failure(detail: &str, request_id: &str) -> ApiFailure {
    ApiFailure::validation(
        format!("invalid multipart feedback request: {detail}"),
        request_id,
    )
}

fn map_error(error: FeedbackServiceError, request_id: &str) -> ApiFailure {
    match error {
        FeedbackServiceError::Validation(error) => {
            ApiFailure::validation(error.to_string(), request_id)
        }
        FeedbackServiceError::NotFound(_) | FeedbackServiceError::ClientAccessDenied => {
            ApiFailure::new(
                StatusCode::NOT_FOUND,
                "feedback_not_found",
                "Feedback was not found",
                "The feedback thread does not exist or is not accessible.",
                request_id,
            )
        }
        FeedbackServiceError::AttachmentNotFound(_) => ApiFailure::new(
            StatusCode::NOT_FOUND,
            "feedback_attachment_not_found",
            "Feedback attachment was not found",
            "The requested attachment does not exist or is not accessible.",
            request_id,
        ),
        value @ FeedbackServiceError::AttachmentTooLarge { .. } => ApiFailure::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            "feedback_attachment_too_large",
            "Feedback attachment is too large",
            value.to_string(),
            request_id,
        ),
        FeedbackServiceError::InvalidAttachment(detail) => {
            ApiFailure::validation(detail, request_id)
        }
        FeedbackServiceError::Transcription(crate::TranscriptionError::NotConfigured) => {
            ApiFailure::new(
                StatusCode::NOT_IMPLEMENTED,
                "feedback_transcription_unavailable",
                "Voice transcription is unavailable",
                "This deployment has not enabled a transcription provider.",
                request_id,
            )
        }
        FeedbackServiceError::Transcription(_) => ApiFailure::new(
            StatusCode::BAD_GATEWAY,
            "feedback_transcription_failed",
            "Voice transcription failed",
            "The transcription provider did not complete the request. Retry shortly.",
            request_id,
        ),
        FeedbackServiceError::Store(crate::FeedbackStoreError::ConcurrentModification {
            ..
        }) => ApiFailure::new(
            StatusCode::CONFLICT,
            "feedback_concurrent_update",
            "Feedback changed concurrently",
            "Refresh the feedback thread and retry the operation.",
            request_id,
        ),
        FeedbackServiceError::Store(crate::FeedbackStoreError::NotFound(_)) => ApiFailure::new(
            StatusCode::NOT_FOUND,
            "feedback_not_found",
            "Feedback was not found",
            "The feedback thread no longer exists.",
            request_id,
        ),
        FeedbackServiceError::Configuration(_) => ApiFailure::internal(request_id),
        FeedbackServiceError::Store(_) | FeedbackServiceError::ObjectStore(_) => ApiFailure::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "feedback_infrastructure_unavailable",
            "Feedback service is temporarily unavailable",
            "The feedback could not be stored or retrieved. Retry shortly.",
            request_id,
        ),
    }
}

fn object_response(object: minco_plugin_object_storage::StoredObject) -> Response {
    let file_name = object
        .metadata
        .attributes
        .get("file_name")
        .map_or("attachment.bin", String::as_str)
        .chars()
        .map(|character| {
            if matches!(character, '\r' | '\n' | '"') {
                '-'
            } else {
                character
            }
        })
        .collect::<String>();
    let mut response = Response::new(Body::from(object.bytes));
    *response.status_mut() = StatusCode::OK;
    if let Ok(value) = HeaderValue::from_str(&object.metadata.content_type) {
        response.headers_mut().insert(header::CONTENT_TYPE, value);
    }
    if let Ok(value) = HeaderValue::from_str(&format!("attachment; filename=\"{file_name}\"")) {
        response
            .headers_mut()
            .insert(header::CONTENT_DISPOSITION, value);
    }
    if let Ok(value) = HeaderValue::from_str(&format!("\"{}\"", object.metadata.sha256)) {
        response.headers_mut().insert(header::ETAG, value);
    }
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    response.headers_mut().insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    response
}

/// Largest multipart body accepted by the Feedback HTTP module.
///
/// The limit is explicit rather than derived from every attachment limit. This keeps the
/// default Lambda/API-Gateway request path bounded even when direct-object upload profiles allow
/// larger individual objects.
#[must_use]
pub const fn feedback_request_body_budget(config: &FeedbackConfig) -> usize {
    config.max_http_body_bytes
}

#[derive(Debug, Clone, Serialize)]
pub struct ClientFeedbackAttachment {
    pub id: Uuid,
    pub kind: FeedbackAttachmentKind,
    pub file_name: String,
    pub content_type: String,
    pub size_bytes: u64,
    pub created_at: chrono::DateTime<chrono::Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transcript: Option<String>,
}

impl From<&FeedbackAttachment> for ClientFeedbackAttachment {
    fn from(value: &FeedbackAttachment) -> Self {
        Self {
            id: value.id,
            kind: value.kind,
            file_name: value.file_name.clone(),
            content_type: value.content_type.clone(),
            size_bytes: value.size_bytes,
            created_at: value.created_at,
            transcript: value.transcript.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ClientFeedbackThread {
    pub id: FeedbackId,
    pub kind: crate::FeedbackKind,
    pub priority: crate::FeedbackPriority,
    pub status: FeedbackStatus,
    pub title: String,
    pub description: String,
    pub context: crate::FeedbackContext,
    pub messages: Vec<crate::FeedbackMessage>,
    pub attachments: Vec<ClientFeedbackAttachment>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub revision: u64,
}

impl From<&FeedbackThread> for ClientFeedbackThread {
    fn from(value: &FeedbackThread) -> Self {
        let value = value.client_view();
        Self {
            id: value.id,
            kind: value.kind,
            priority: value.priority,
            status: value.status,
            title: value.title,
            description: value.description,
            context: value.context,
            messages: value.messages,
            attachments: value
                .attachments
                .iter()
                .map(ClientFeedbackAttachment::from)
                .collect(),
            created_at: value.created_at,
            updated_at: value.updated_at,
            revision: value.revision,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ClientCreateResponse {
    pub thread: ClientFeedbackThread,
    pub client_token: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<FeedbackWarning>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClientMutationResponse {
    pub thread: ClientFeedbackThread,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<FeedbackWarning>,
}

impl From<FeedbackMutationResult> for ClientMutationResponse {
    fn from(value: FeedbackMutationResult) -> Self {
        Self {
            thread: ClientFeedbackThread::from(&value.thread),
            warnings: value.warnings,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FeedbackKind, FeedbackPriority, FeedbackStoreService, MemoryFeedbackStore};
    use axum::body::Body;
    use minco_plugin_audit::{AuditService, MemoryAuditSink};
    use minco_plugin_events::{EventServices, MemoryEventBus};
    use minco_plugin_notifications::{MemoryNotificationSink, NotificationService};
    use minco_plugin_object_storage::{MemoryObjectStore, ObjectStoreService};
    use std::collections::{BTreeMap, BTreeSet};
    use tower::ServiceExt;

    fn service() -> FeedbackService {
        let events = Arc::new(MemoryEventBus::default());
        FeedbackService::new(
            FeedbackStoreService::new(Arc::new(MemoryFeedbackStore::default())),
            ObjectStoreService::new(Arc::new(MemoryObjectStore::default())),
            NotificationService::new(Arc::new(MemoryNotificationSink::default())),
            AuditService::new(Arc::new(MemoryAuditSink::default())),
            EventServices {
                publisher: events.clone(),
                outbox: events,
            },
            None,
            FeedbackConfig {
                project_id: "example".into(),
                developer_token: Some("developer-token-with-enough-entropy".into()),
                ..FeedbackConfig::default()
            },
        )
        .unwrap()
    }

    #[tokio::test]
    async fn widget_asset_and_configuration_are_served_without_a_frontend_framework() {
        let app = feedback_router(service());
        for path in [
            "/_minco/feedback/widget.js",
            "/_minco/feedback/widget-config",
        ] {
            let response = app
                .clone()
                .oneshot(http::Request::get(path).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK, "{path}");
        }
    }

    #[tokio::test]
    async fn a_principal_with_management_permission_can_use_the_inbox_without_a_token() {
        let app = feedback_router(service()).layer(Extension(Principal {
            subject: "developer-1".into(),
            permissions: BTreeSet::from(["feedback.manage".into()]),
            claims: BTreeMap::new(),
        }));
        let response = app
            .oneshot(
                http::Request::get("/_minco/feedback/developer/threads")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn developer_routes_fail_closed_without_identity_or_a_bearer_token() {
        let response = feedback_router(service())
            .oneshot(
                http::Request::get("/_minco/feedback/developer/threads")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn feedback_submission_is_not_anonymous_by_default() {
        let failure = authorize_submission(
            &HeaderMap::new(),
            &FeedbackConfig::default(),
            None,
            "request-1",
        )
        .expect_err("anonymous feedback must require explicit configuration");

        assert_eq!(failure.status, StatusCode::UNAUTHORIZED);
        assert_eq!(failure.code.as_ref(), "feedback_authentication_required");
    }

    #[test]
    fn anonymous_submission_cannot_choose_an_audit_or_notification_subject() {
        let mut input = CreateFeedbackInput {
            project_id: "example".into(),
            kind: FeedbackKind::Bug,
            priority: FeedbackPriority::Normal,
            title: "Example".into(),
            description: "Example problem".into(),
            context: crate::FeedbackContext {
                page_url: "https://example.test".into(),
                route_name: None,
                release_id: None,
                environment: None,
                request_id: None,
                user_agent: None,
                viewport: None,
                client_subject: Some("spoofed-subject".into()),
            },
            tags: BTreeSet::new(),
        };

        bind_client_subject(&mut input, None);

        assert!(input.context.client_subject.is_none());
    }

    #[test]
    fn transcription_requires_an_authenticated_principal() {
        let config = FeedbackConfig {
            allow_anonymous: true,
            ..FeedbackConfig::default()
        };
        let failure = authorize_transcription(&HeaderMap::new(), &config, None, "request-1")
            .expect_err("anonymous transcription must fail closed");

        assert_eq!(failure.status, StatusCode::FORBIDDEN);
        assert_eq!(
            failure.code.as_ref(),
            "feedback_transcription_authentication_required"
        );
    }

    #[test]
    fn client_projection_excludes_internal_notes_and_object_keys() {
        let mut thread = FeedbackThread::create(CreateFeedbackInput {
            project_id: "example".into(),
            kind: FeedbackKind::Bug,
            priority: FeedbackPriority::Normal,
            title: "Example".into(),
            description: "Example problem".into(),
            context: crate::FeedbackContext {
                page_url: "https://example.test".into(),
                route_name: None,
                release_id: None,
                environment: None,
                request_id: None,
                user_agent: None,
                viewport: None,
                client_subject: None,
            },
            tags: BTreeSet::new(),
        })
        .unwrap();
        thread.append_message(crate::FeedbackMessage::developer(None, "internal", false).unwrap());
        let projected = ClientFeedbackThread::from(&thread);
        assert!(projected.messages.is_empty());
    }
    #[test]
    fn request_budget_uses_the_explicit_serverless_http_limit() {
        let config = FeedbackConfig {
            max_http_body_bytes: 6 * 1024 * 1024,
            ..FeedbackConfig::default()
        };
        assert_eq!(feedback_request_body_budget(&config), 6 * 1024 * 1024);
    }

    #[test]
    fn provider_error_details_are_not_exposed_to_clients() {
        let failure = map_error(
            FeedbackServiceError::Transcription(crate::TranscriptionError::Provider(
                "provider response containing sensitive diagnostics".into(),
            )),
            "request-1",
        );
        assert_eq!(failure.code.as_ref(), "feedback_transcription_failed");
        assert!(!failure.detail.contains("sensitive diagnostics"));
    }

    #[test]
    fn configured_tokens_are_compared_by_digest() {
        assert!(constant_time_equals("configured-token", "configured-token"));
        assert!(!constant_time_equals(
            "configured-token",
            "configured-token-with-a-different-length"
        ));
    }

    #[test]
    fn attachment_downloads_are_private_and_not_content_sniffed() {
        let response = object_response(minco_plugin_object_storage::StoredObject {
            key: minco_plugin_object_storage::ObjectKey::parse("feedback/example/attachment.png")
                .unwrap(),
            bytes: vec![1, 2, 3],
            metadata: minco_plugin_object_storage::ObjectMetadata {
                content_type: "image/png".into(),
                size_bytes: 3,
                sha256: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
                created_at: chrono::Utc::now(),
                attributes: BTreeMap::from([("file_name".into(), "attachment.png".into())]),
            },
        });
        assert_eq!(
            response.headers()[header::CACHE_CONTROL],
            "private, no-store"
        );
        assert_eq!(
            response.headers()[header::X_CONTENT_TYPE_OPTIONS],
            "nosniff"
        );
        assert_eq!(
            response.headers()[header::CONTENT_DISPOSITION],
            "attachment; filename=\"attachment.png\""
        );
    }
}
