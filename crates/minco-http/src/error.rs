use axum::{
    Json,
    response::{IntoResponse, Response},
};
use http::{HeaderValue, StatusCode};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::REQUEST_ID_HEADER;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProblemDetails {
    #[serde(rename = "type")]
    pub type_uri: String,
    pub title: String,
    pub status: u16,
    pub detail: String,
    pub code: String,
    pub request_id: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub errors: BTreeMap<String, Vec<String>>,
}

#[derive(Debug, Clone)]
pub struct ApiFailure {
    pub status: StatusCode,
    pub code: Box<str>,
    pub title: String,
    pub detail: String,
    pub request_id: String,
    pub errors: BTreeMap<String, Vec<String>>,
}

impl ApiFailure {
    pub fn new(
        status: StatusCode,
        code: impl Into<String>,
        title: impl Into<String>,
        detail: impl Into<String>,
        request_id: impl Into<String>,
    ) -> Self {
        Self {
            status,
            code: code.into().into_boxed_str(),
            title: title.into(),
            detail: detail.into(),
            request_id: request_id.into(),
            errors: BTreeMap::new(),
        }
    }

    pub fn validation(detail: impl Into<String>, request_id: impl Into<String>) -> Self {
        Self::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "validation_failed",
            "Validation failed",
            detail,
            request_id,
        )
    }

    pub fn precondition_required(request_id: impl Into<String>) -> Self {
        Self::new(
            StatusCode::PRECONDITION_REQUIRED,
            "precondition_required",
            "Precondition required",
            "This operation requires an If-Match header containing the current entity tag.",
            request_id,
        )
    }

    pub fn precondition_failed(request_id: impl Into<String>) -> Self {
        Self::new(
            StatusCode::PRECONDITION_FAILED,
            "precondition_failed",
            "Precondition failed",
            "The resource changed after it was read. Fetch the current representation and retry.",
            request_id,
        )
    }

    pub fn invalid_if_match(request_id: impl Into<String>) -> Self {
        Self::new(
            StatusCode::BAD_REQUEST,
            "invalid_if_match",
            "Invalid If-Match header",
            "If-Match must contain exactly one strong entity tag returned by this API.",
            request_id,
        )
    }

    pub fn internal(request_id: impl Into<String>) -> Self {
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            "Internal server error",
            "The request could not be completed.",
            request_id,
        )
    }
}

impl IntoResponse for ApiFailure {
    fn into_response(self) -> Response {
        problem_response(self)
    }
}

pub fn problem_response(failure: ApiFailure) -> Response {
    let problem = ProblemDetails {
        type_uri: format!("https://minco.dev/problems/{}", failure.code),
        title: failure.title,
        status: failure.status.as_u16(),
        detail: failure.detail,
        code: failure.code.into(),
        request_id: failure.request_id.clone(),
        errors: failure.errors,
    };
    let mut response = (failure.status, Json(problem)).into_response();
    response.headers_mut().insert(
        http::header::CONTENT_TYPE,
        HeaderValue::from_static("application/problem+json"),
    );
    if let Ok(value) = HeaderValue::from_str(&failure.request_id) {
        response
            .headers_mut()
            .insert(REQUEST_ID_HEADER.clone(), value);
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn problem_type_is_stable_and_machine_readable() {
        let response = ApiFailure::validation("bad input", "request-1").into_response();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(
            response.headers()[http::header::CONTENT_TYPE],
            "application/problem+json"
        );
        assert_eq!(response.headers()[&REQUEST_ID_HEADER], "request-1");
    }
}
