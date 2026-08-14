//! Authenticated HTTP control plane for direct object transfers.
#![forbid(unsafe_code)]

use crate::{
    MultipartPartGrant, MultipartPartReceipt, MultipartUploadGrant, ObjectByteRange,
    ObjectDownloadGrant, ObjectKey, ObjectValidationState, PresignedObjectRequest,
};
use async_trait::async_trait;
use axum::{
    Extension, Json, Router,
    extract::{DefaultBodyLimit, Path, State},
    response::{IntoResponse, Response},
    routing::{delete, get, post},
};
use http::{HeaderMap, StatusCode, header};
use minco_core::{OperationDescriptor, PluginId};
use minco_http::{
    ApiFailure, ApiResponseMetadata, BearerChallenge, HttpModule, Principal, REQUEST_ID_HEADER,
    StrongEntityTag, parse_if_match,
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fmt, sync::Arc};
use uuid::Uuid;

pub const OBJECT_TRANSFER_BASE_PATH: &str = "/_minco/objects";
/// Bounded for the provider's maximum 10,000-part completion manifest while
/// remaining below API Gateway's and synchronous Lambda's payload ceilings.
pub const OBJECT_TRANSFER_HTTP_BODY_BYTES: usize = 3 * 1024 * 1024;
pub const MAX_MULTIPART_ENTITY_TAG_BYTES: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectTransferRequestContext {
    pub request_id: String,
    pub principal: Principal,
    pub idempotency_key: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InitiateTransferUpload {
    /// Application-defined closed purpose/profile name. It never selects a
    /// provider, bucket, credentials, or arbitrary object prefix.
    pub purpose: String,
    pub content_type: String,
    pub size_bytes: u64,
    pub sha256: Option<String>,
    pub file_name: Option<String>,
    /// Logical application object being replaced. The HTTP handler binds the
    /// optional `If-Match` value separately before invoking the use case.
    pub replaces_object_id: Option<String>,
    #[serde(skip)]
    pub if_match: Option<String>,
    #[serde(default)]
    pub attributes: BTreeMap<String, String>,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SingleTransferUploadGrant {
    pub upload_id: Uuid,
    pub key: ObjectKey,
    pub request: PresignedObjectRequest,
}

impl fmt::Debug for SingleTransferUploadGrant {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SingleTransferUploadGrant")
            .field("upload_id", &self.upload_id)
            .field("key", &self.key)
            .field("request", &self.request)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum TransferUploadGrant {
    Single(SingleTransferUploadGrant),
    Multipart(MultipartUploadGrant),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferUploadResponse {
    pub upload: TransferUploadGrant,
    pub validation: ObjectValidationState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IssueTransferPart {
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompleteTransferUpload {
    #[serde(default)]
    pub parts: Vec<MultipartPartReceipt>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompletedTransferUpload {
    /// Stable application identifier, never a provider key used as
    /// authorization.
    pub object_id: String,
    pub revision: String,
    pub entity_tag: String,
    pub validation: ObjectValidationState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IssueTransferDownload {
    pub object_id: String,
    pub range: Option<ObjectByteRange>,
    pub expected_entity_tag: Option<String>,
    pub download_file_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectTransferMetadata {
    pub object_id: String,
    pub revision: String,
    pub content_type: String,
    pub size_bytes: u64,
    /// Strong validator for this authorized metadata representation. The
    /// application must change it when the resolved object revision,
    /// validation state, or other download-eligibility state changes. It is
    /// distinct from the provider byte validator in a download grant.
    pub entity_tag: String,
    pub last_modified: chrono::DateTime<chrono::Utc>,
    pub validation: ObjectValidationState,
}

/// Application-owned authorization and durable transfer-session boundary.
/// Each HTTP handler invokes exactly one method on this port.
#[async_trait]
pub trait ObjectTransferHttpUseCases: Send + Sync + fmt::Debug {
    async fn initiate_upload(
        &self,
        context: ObjectTransferRequestContext,
        request: InitiateTransferUpload,
    ) -> Result<TransferUploadResponse, ObjectTransferApiError>;

    async fn issue_part(
        &self,
        context: ObjectTransferRequestContext,
        upload_id: Uuid,
        part_number: u32,
        request: IssueTransferPart,
    ) -> Result<MultipartPartGrant, ObjectTransferApiError>;

    async fn complete_upload(
        &self,
        context: ObjectTransferRequestContext,
        upload_id: Uuid,
        request: CompleteTransferUpload,
    ) -> Result<CompletedTransferUpload, ObjectTransferApiError>;

    async fn abort_upload(
        &self,
        context: ObjectTransferRequestContext,
        upload_id: Uuid,
    ) -> Result<(), ObjectTransferApiError>;

    async fn issue_download(
        &self,
        context: ObjectTransferRequestContext,
        request: IssueTransferDownload,
    ) -> Result<ObjectDownloadGrant, ObjectTransferApiError>;

    async fn get_metadata(
        &self,
        context: ObjectTransferRequestContext,
        object_id: String,
    ) -> Result<ObjectTransferMetadata, ObjectTransferApiError>;
}

#[derive(Clone)]
pub struct ObjectTransferHttpService(Arc<dyn ObjectTransferHttpUseCases>);

impl ObjectTransferHttpService {
    pub fn new(use_cases: Arc<dyn ObjectTransferHttpUseCases>) -> Self {
        Self(use_cases)
    }
}

impl fmt::Debug for ObjectTransferHttpService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_tuple("ObjectTransferHttpService").finish()
    }
}

pub fn object_transfer_operations() -> Vec<OperationDescriptor> {
    [
        (
            "initiateObjectUpload",
            "POST",
            "/_minco/objects/uploads",
            false,
        ),
        (
            "issueObjectUploadPart",
            "POST",
            "/_minco/objects/uploads/{uploadId}/parts/{partNumber}",
            true,
        ),
        (
            "completeObjectUpload",
            "POST",
            "/_minco/objects/uploads/{uploadId}/complete",
            true,
        ),
        (
            "abortObjectUpload",
            "DELETE",
            "/_minco/objects/uploads/{uploadId}",
            true,
        ),
        (
            "issueObjectDownload",
            "POST",
            "/_minco/objects/downloads",
            true,
        ),
        (
            "getObjectTransferMetadata",
            "GET",
            "/_minco/objects/{objectId}",
            true,
        ),
    ]
    .into_iter()
    .map(
        |(operation_id, method, path, idempotent)| OperationDescriptor {
            operation_id: operation_id.into(),
            method: method.into(),
            path: path.into(),
            public: false,
            idempotent,
        },
    )
    .collect()
}

pub fn object_transfer_http_module(
    plugin_id: PluginId,
    service: ObjectTransferHttpService,
) -> HttpModule {
    HttpModule::new(plugin_id, object_transfer_router(service))
        .with_operations(
            object_transfer_operations()
                .into_iter()
                .map(|operation| operation.operation_id),
        )
        .with_max_request_body_bytes(OBJECT_TRANSFER_HTTP_BODY_BYTES)
}

pub fn object_transfer_router(service: ObjectTransferHttpService) -> Router {
    let routes = Router::new()
        .route("/uploads", post(initiate_upload))
        .route("/uploads/{upload_id}/parts/{part_number}", post(issue_part))
        .route("/uploads/{upload_id}/complete", post(complete_upload))
        .route("/uploads/{upload_id}", delete(abort_upload))
        .route("/downloads", post(issue_download))
        .route("/{object_id}", get(get_metadata))
        .layer(DefaultBodyLimit::max(OBJECT_TRANSFER_HTTP_BODY_BYTES))
        .with_state(service);
    Router::new().nest(OBJECT_TRANSFER_BASE_PATH, routes)
}

async fn initiate_upload(
    State(service): State<ObjectTransferHttpService>,
    principal: Option<Extension<Principal>>,
    headers: HeaderMap,
    Json(mut request): Json<InitiateTransferUpload>,
) -> Result<Response, Response> {
    let context = context(principal, &headers)?;
    validate_initiate(&request, &context.request_id)?;
    request.if_match = if headers.contains_key(header::IF_MATCH) {
        let tag = parse_if_match(&headers)
            .map_err(|_| ApiFailure::invalid_if_match(&context.request_id).into_response())?;
        Some(
            tag.to_header_value()
                .to_str()
                .expect("validated entity tag is ASCII")
                .to_owned(),
        )
    } else {
        None
    };
    if request.replaces_object_id.is_some() && request.if_match.is_none() {
        return Err(ApiFailure::precondition_required(context.request_id).into_response());
    }
    let response = service
        .0
        .initiate_upload(context.clone(), request)
        .await
        .map_err(|error| api_error(error, context.request_id))?;
    Ok((StatusCode::CREATED, Json(response)).into_response())
}

async fn issue_part(
    State(service): State<ObjectTransferHttpService>,
    principal: Option<Extension<Principal>>,
    headers: HeaderMap,
    Path((upload_id, part_number)): Path<(String, String)>,
    Json(request): Json<IssueTransferPart>,
) -> Result<Response, Response> {
    let context = context(principal, &headers)?;
    let upload_id = parse_uuid(&upload_id, "upload_id", &context.request_id)?;
    let part_number = part_number.parse::<u32>().map_err(|_| {
        ApiFailure::validation("part_number must be an integer", &context.request_id)
            .into_response()
    })?;
    if !(1..=crate::MAX_MULTIPART_PARTS).contains(&part_number) || !valid_sha256(&request.sha256) {
        return Err(ApiFailure::validation(
            "part_number or SHA-256 is invalid",
            &context.request_id,
        )
        .into_response());
    }
    let response = service
        .0
        .issue_part(context.clone(), upload_id, part_number, request)
        .await
        .map_err(|error| api_error(error, context.request_id))?;
    Ok(Json(response).into_response())
}

async fn complete_upload(
    State(service): State<ObjectTransferHttpService>,
    principal: Option<Extension<Principal>>,
    headers: HeaderMap,
    Path(upload_id): Path<String>,
    Json(request): Json<CompleteTransferUpload>,
) -> Result<Response, Response> {
    let context = context(principal, &headers)?;
    let upload_id = parse_uuid(&upload_id, "upload_id", &context.request_id)?;
    if request.parts.len() > crate::MAX_MULTIPART_PARTS as usize
        || request.parts.iter().any(|part| {
            !(1..=crate::MAX_MULTIPART_PARTS).contains(&part.part_number)
                || !valid_sha256(&part.sha256)
                || part.entity_tag.is_empty()
                || part.entity_tag.len() > MAX_MULTIPART_ENTITY_TAG_BYTES
                || part.entity_tag.chars().any(char::is_control)
        })
    {
        return Err(ApiFailure::validation(
            "multipart completion manifest is invalid",
            &context.request_id,
        )
        .into_response());
    }
    let response = service
        .0
        .complete_upload(context.clone(), upload_id, request)
        .await
        .map_err(|error| api_error(error, context.request_id))?;
    Ok(Json(response).into_response())
}

async fn abort_upload(
    State(service): State<ObjectTransferHttpService>,
    principal: Option<Extension<Principal>>,
    headers: HeaderMap,
    Path(upload_id): Path<String>,
) -> Result<Response, Response> {
    let context = context(principal, &headers)?;
    let upload_id = parse_uuid(&upload_id, "upload_id", &context.request_id)?;
    service
        .0
        .abort_upload(context.clone(), upload_id)
        .await
        .map_err(|error| api_error(error, context.request_id))?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

async fn issue_download(
    State(service): State<ObjectTransferHttpService>,
    principal: Option<Extension<Principal>>,
    headers: HeaderMap,
    Json(request): Json<IssueTransferDownload>,
) -> Result<Response, Response> {
    let context = context(principal, &headers)?;
    validate_download(&request, &context.request_id)?;
    let response = service
        .0
        .issue_download(context.clone(), request)
        .await
        .map_err(|error| api_error(error, context.request_id))?;
    Ok(Json(response).into_response())
}

async fn get_metadata(
    State(service): State<ObjectTransferHttpService>,
    principal: Option<Extension<Principal>>,
    headers: HeaderMap,
    Path(object_id): Path<String>,
) -> Result<Response, Response> {
    let context = context(principal, &headers)?;
    if !valid_bounded_text(&object_id, 256) {
        return Err(
            ApiFailure::validation("object_id is invalid", context.request_id).into_response(),
        );
    }
    let metadata = service
        .0
        .get_metadata(context.clone(), object_id)
        .await
        .map_err(|error| api_error(error, context.request_id.clone()))?;
    let entity_tag = parse_application_entity_tag(&metadata.entity_tag)
        .map_err(|()| ApiFailure::internal(context.request_id.clone()).into_response())?;
    let not_modified = if_none_match_matches(&headers, entity_tag.opaque());
    let mut response = if not_modified {
        StatusCode::NOT_MODIFIED.into_response()
    } else {
        Json(metadata).into_response()
    };
    response
        .headers_mut()
        .insert(header::ETAG, entity_tag.to_header_value());
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        http::HeaderValue::from_static("private, no-cache"),
    );
    response.headers_mut().insert(
        header::VARY,
        http::HeaderValue::from_static("Authorization"),
    );
    Ok(response)
}

#[allow(clippy::result_large_err)]
fn context(
    principal: Option<Extension<Principal>>,
    headers: &HeaderMap,
) -> Result<ObjectTransferRequestContext, Response> {
    let request_id = request_id(headers);
    let Some(Extension(principal)) = principal else {
        return Err(ApiResponseMetadata::new()
            .bearer_challenge(BearerChallenge::Required)
            .wrap(ApiFailure::new(
                StatusCode::UNAUTHORIZED,
                "authentication_required",
                "Authentication required",
                "A valid authenticated principal is required.",
                request_id,
            ))
            .into_response());
    };
    let idempotency_key = parse_idempotency_key(headers, &request_id)?;
    Ok(ObjectTransferRequestContext {
        request_id,
        principal,
        idempotency_key,
    })
}

fn request_id(headers: &HeaderMap) -> String {
    headers
        .get(&REQUEST_ID_HEADER)
        .and_then(|value| value.to_str().ok())
        .filter(|value| valid_bounded_text(value, 200))
        .map_or_else(|| Uuid::now_v7().to_string(), str::to_owned)
}

#[allow(clippy::result_large_err)]
fn parse_uuid(value: &str, field: &str, request_id: &str) -> Result<Uuid, Response> {
    Uuid::parse_str(value).map_err(|_| {
        ApiFailure::validation(format!("{field} must be a UUID"), request_id).into_response()
    })
}

#[allow(clippy::result_large_err)]
fn parse_idempotency_key(
    headers: &HeaderMap,
    request_id: &str,
) -> Result<Option<String>, Response> {
    let mut values = headers.get_all("idempotency-key").iter();
    let Some(value) = values.next() else {
        return Ok(None);
    };
    if values.next().is_some() {
        return Err(
            ApiFailure::validation("Idempotency-Key is invalid", request_id).into_response(),
        );
    }
    let value = value.to_str().map_err(|_| {
        ApiFailure::validation("Idempotency-Key is invalid", request_id).into_response()
    })?;
    if !valid_bounded_text(value, 200) {
        return Err(
            ApiFailure::validation("Idempotency-Key is invalid", request_id).into_response(),
        );
    }
    Ok(Some(value.to_owned()))
}

#[allow(clippy::result_large_err)]
fn validate_initiate(request: &InitiateTransferUpload, request_id: &str) -> Result<(), Response> {
    let valid_purpose = !request.purpose.is_empty()
        && request.purpose.len() <= 128
        && request.purpose.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_' | b'.')
        });
    let valid_type = request
        .content_type
        .split_once('/')
        .is_some_and(|(top, subtype)| {
            !top.is_empty()
                && !subtype.is_empty()
                && !subtype.contains('/')
                && request.content_type.len() <= 255
                && request.content_type.bytes().all(|byte| {
                    byte.is_ascii_alphanumeric()
                        || matches!(
                            byte,
                            b'!' | b'#' | b'$' | b'&' | b'^' | b'_' | b'.' | b'+' | b'-' | b'/'
                        )
                })
        });
    let valid_checksum = request.sha256.as_deref().is_none_or(valid_sha256);
    let valid_file_name = request
        .file_name
        .as_deref()
        .is_none_or(|value| valid_bounded_text(value, 255));
    let valid_replacement = request
        .replaces_object_id
        .as_deref()
        .is_none_or(|value| valid_bounded_text(value, 256));
    let valid_attributes = request.attributes.len() <= 31
        && request.attributes.iter().all(|(key, value)| {
            valid_bounded_text(key, 128)
                && !key.starts_with("minco.")
                && value.len() <= 1_024
                && !value.chars().any(char::is_control)
        });
    if valid_purpose
        && valid_type
        && request.size_bytes > 0
        && valid_checksum
        && valid_file_name
        && valid_replacement
        && valid_attributes
    {
        Ok(())
    } else {
        Err(
            ApiFailure::validation("object upload declaration is invalid", request_id)
                .into_response(),
        )
    }
}

#[allow(clippy::result_large_err)]
fn validate_download(request: &IssueTransferDownload, request_id: &str) -> Result<(), Response> {
    let valid = valid_bounded_text(&request.object_id, 256)
        && request.range.is_none_or(|range| range.validate().is_ok())
        && request
            .expected_entity_tag
            .as_deref()
            .is_none_or(|value| valid_bounded_text(value, 256) && !value.starts_with("W/"))
        && request
            .download_file_name
            .as_deref()
            .is_none_or(|value| valid_bounded_text(value, 255));
    if valid {
        Ok(())
    } else {
        Err(
            ApiFailure::validation("object download declaration is invalid", request_id)
                .into_response(),
        )
    }
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn valid_bounded_text(value: &str, maximum: usize) -> bool {
    !value.is_empty() && value.len() <= maximum && !value.chars().any(char::is_control)
}

fn parse_application_entity_tag(value: &str) -> Result<StrongEntityTag, ()> {
    let opaque = value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .ok_or(())?;
    StrongEntityTag::from_opaque(opaque).map_err(|_| ())
}

fn if_none_match_matches(headers: &HeaderMap, current_opaque: &str) -> bool {
    headers.get_all(header::IF_NONE_MATCH).iter().any(|value| {
        value.to_str().is_ok_and(|value| {
            value.split(',').any(|candidate| {
                let candidate = candidate.trim();
                if candidate == "*" {
                    return true;
                }
                let candidate = candidate.strip_prefix("W/").unwrap_or(candidate);
                candidate
                    .strip_prefix('"')
                    .and_then(|candidate| candidate.strip_suffix('"'))
                    .and_then(|opaque| StrongEntityTag::from_opaque(opaque).ok())
                    .is_some_and(|candidate| candidate.opaque() == current_opaque)
            })
        })
    })
}

fn api_error(error: ObjectTransferApiError, request_id: String) -> Response {
    let (status, code, title, detail) = match error {
        ObjectTransferApiError::Forbidden => (
            StatusCode::FORBIDDEN,
            "object_transfer_forbidden",
            "Object transfer forbidden",
            "The principal is not authorized for this object transfer.",
        ),
        ObjectTransferApiError::NotFound => (
            StatusCode::NOT_FOUND,
            "object_transfer_not_found",
            "Object transfer not found",
            "The requested object or transfer session does not exist.",
        ),
        ObjectTransferApiError::Conflict => (
            StatusCode::CONFLICT,
            "object_transfer_conflict",
            "Object transfer conflict",
            "The transfer is not in a state that accepts this operation.",
        ),
        ObjectTransferApiError::PreconditionFailed => (
            StatusCode::PRECONDITION_FAILED,
            "precondition_failed",
            "Precondition failed",
            "The object changed after it was read. Fetch the current revision and retry.",
        ),
        ObjectTransferApiError::Validation(detail) => {
            return ApiFailure::validation(detail, request_id).into_response();
        }
        ObjectTransferApiError::Expired => (
            StatusCode::GONE,
            "object_transfer_expired",
            "Object transfer expired",
            "The transfer session expired; start a new session.",
        ),
        ObjectTransferApiError::Unavailable => (
            StatusCode::SERVICE_UNAVAILABLE,
            "object_transfer_unavailable",
            "Object transfer unavailable",
            "The object provider is temporarily unavailable.",
        ),
        ObjectTransferApiError::Internal => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            "Internal server error",
            "The request could not be completed.",
        ),
    };
    ApiFailure::new(status, code, title, detail, request_id).into_response()
}

#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub enum ObjectTransferApiError {
    #[error("object transfer is forbidden")]
    Forbidden,
    #[error("object or transfer session was not found")]
    NotFound,
    #[error("object transfer state conflicts with the request")]
    Conflict,
    #[error("object transfer precondition failed")]
    PreconditionFailed,
    #[error("object transfer validation failed: {0}")]
    Validation(String),
    #[error("object transfer session expired")]
    Expired,
    #[error("object provider is unavailable")]
    Unavailable,
    #[error("object transfer failed internally")]
    Internal,
}
