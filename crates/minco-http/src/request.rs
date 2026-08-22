use std::ops::{Deref, DerefMut};

use axum::{
    Json,
    extract::{FromRequest, FromRequestParts, Path, Query, Request},
};
use http::{StatusCode, request::Parts};
use minco_contract::{ContractValidate, ContractValidationErrors};
use serde::de::DeserializeOwned;

use crate::{ApiFailure, request_id_from_headers};

/// One native Axum JSON extraction followed by static contract validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ValidatedJson<T>(pub T);

/// One native Axum query extraction followed by static contract validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ValidatedQuery<T>(pub T);

/// One native Axum path extraction followed by static contract validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ValidatedPath<T>(pub T);

macro_rules! validated_accessors {
    ($name:ident) => {
        impl<T> $name<T> {
            #[must_use]
            pub fn into_inner(self) -> T {
                self.0
            }
        }

        impl<T> Deref for $name<T> {
            type Target = T;

            fn deref(&self) -> &Self::Target {
                &self.0
            }
        }

        impl<T> DerefMut for $name<T> {
            fn deref_mut(&mut self) -> &mut Self::Target {
                &mut self.0
            }
        }
    };
}

validated_accessors!(ValidatedJson);
validated_accessors!(ValidatedQuery);
validated_accessors!(ValidatedPath);

impl<T, S> FromRequest<S> for ValidatedJson<T>
where
    T: DeserializeOwned + ContractValidate + Send,
    S: Send + Sync,
{
    type Rejection = ApiFailure;

    async fn from_request(request: Request, state: &S) -> Result<Self, Self::Rejection> {
        let request_id = request_id_from_headers(request.headers());
        let Json(value) = Json::<T>::from_request(request, state)
            .await
            .map_err(|rejection| json_failure(rejection, request_id.clone()))?;
        validate(value, request_id).map(Self)
    }
}

impl<T, S> FromRequestParts<S> for ValidatedQuery<T>
where
    T: DeserializeOwned + ContractValidate + Send,
    S: Send + Sync,
{
    type Rejection = ApiFailure;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let request_id = request_id_from_headers(&parts.headers);
        let Query(value) = Query::<T>::from_request_parts(parts, state)
            .await
            .map_err(|_| {
                ApiFailure::new(
                    StatusCode::BAD_REQUEST,
                    "invalid_query",
                    "Invalid query",
                    "Query parameters do not match the operation contract.",
                    request_id.clone(),
                )
            })?;
        validate(value, request_id).map(Self)
    }
}

impl<T, S> FromRequestParts<S> for ValidatedPath<T>
where
    T: DeserializeOwned + ContractValidate + Send,
    S: Send + Sync,
{
    type Rejection = ApiFailure;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let request_id = request_id_from_headers(&parts.headers);
        let Path(value) = Path::<T>::from_request_parts(parts, state)
            .await
            .map_err(|_| {
                ApiFailure::new(
                    StatusCode::BAD_REQUEST,
                    "invalid_path",
                    "Invalid path",
                    "Path parameters do not match the operation contract.",
                    request_id.clone(),
                )
            })?;
        validate(value, request_id).map(Self)
    }
}

fn validate<T: ContractValidate>(value: T, request_id: String) -> Result<T, ApiFailure> {
    let mut errors = ContractValidationErrors::new();
    value.validate_contract(&mut errors);
    if errors.is_empty() {
        return Ok(value);
    }
    let mut failure =
        ApiFailure::validation("Request fields violate the operation contract.", request_id);
    failure.errors = errors.into_fields();
    Err(failure)
}

fn json_failure(
    rejection: axum::extract::rejection::JsonRejection,
    request_id: String,
) -> ApiFailure {
    use axum::extract::rejection::JsonRejection;

    match rejection {
        JsonRejection::MissingJsonContentType(_) => ApiFailure::new(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "unsupported_media_type",
            "Unsupported media type",
            "A supported application/json Content-Type is required.",
            request_id,
        ),
        JsonRejection::JsonSyntaxError(_) => ApiFailure::new(
            StatusCode::BAD_REQUEST,
            "invalid_json",
            "Invalid JSON",
            "Request body is not valid JSON.",
            request_id,
        ),
        JsonRejection::BytesRejection(rejection)
            if rejection.status() == StatusCode::PAYLOAD_TOO_LARGE =>
        {
            ApiFailure::new(
                StatusCode::PAYLOAD_TOO_LARGE,
                "payload_too_large",
                "Payload too large",
                "Request body exceeds the configured limit.",
                request_id,
            )
        }
        _ => ApiFailure::new(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "Invalid request",
            "Request body does not match the operation contract.",
            request_id,
        ),
    }
}
