use axum::response::{IntoResponse, Response};
use http::{HeaderName, HeaderValue, header};
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;

pub static DEPRECATION_HEADER: HeaderName = HeaderName::from_static("deprecation");
pub static SUNSET_HEADER: HeaderName = HeaderName::from_static("sunset");

/// Standard bearer challenges for protected API operations.
///
/// Applications that need additional RFC 6750 parameters such as `realm` or
/// `scope` can use [`ApiResponseMetadata::www_authenticate`] with a validated
/// [`HeaderValue`].
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BearerChallenge {
    Required,
    InvalidRequest,
    InvalidToken,
    InsufficientScope,
}

impl BearerChallenge {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Required => "Bearer",
            Self::InvalidRequest => r#"Bearer error="invalid_request""#,
            Self::InvalidToken => r#"Bearer error="invalid_token""#,
            Self::InsufficientScope => r#"Bearer error="insufficient_scope""#,
        }
    }
}

/// Standard HTTP response metadata used by browser, native, and machine clients.
///
/// The wrapper is intentionally transport-only. It does not implement rate
/// limiting, token validation, or API lifecycle policy for the application.
#[must_use]
#[derive(Debug, Clone, Default)]
pub struct ApiResponseMetadata {
    retry_after: Option<HeaderValue>,
    www_authenticate: Option<HeaderValue>,
    deprecation: Option<HeaderValue>,
    sunset: Option<HeaderValue>,
    links: Vec<HeaderValue>,
}

impl ApiResponseMetadata {
    pub const fn new() -> Self {
        Self {
            retry_after: None,
            www_authenticate: None,
            deprecation: None,
            sunset: None,
            links: Vec::new(),
        }
    }

    /// Adds either a delay-seconds value or an HTTP-date supplied by the application.
    pub fn retry_after(mut self, value: HeaderValue) -> Self {
        self.retry_after = Some(value);
        self
    }

    pub fn retry_after_seconds(self, seconds: u64) -> Self {
        self.retry_after(
            HeaderValue::from_str(&seconds.to_string())
                .expect("a retry delay generated from u64 is a valid header value"),
        )
    }

    pub fn www_authenticate(mut self, value: HeaderValue) -> Self {
        self.www_authenticate = Some(value);
        self
    }

    pub fn bearer_challenge(self, challenge: BearerChallenge) -> Self {
        self.www_authenticate(HeaderValue::from_static(challenge.as_str()))
    }

    /// Adds an RFC 9745 Structured Field Date (`@<unix-seconds>`).
    pub fn deprecation_at(
        mut self,
        deprecation: SystemTime,
    ) -> Result<Self, ApiResponseMetadataError> {
        let seconds = deprecation
            .duration_since(UNIX_EPOCH)
            .map_err(|_| ApiResponseMetadataError::BeforeUnixEpoch)?
            .as_secs();
        self.deprecation = Some(
            HeaderValue::from_str(&format!("@{seconds}"))
                .expect("a deprecation date generated from u64 is a valid header value"),
        );
        Ok(self)
    }

    /// Adds an RFC 8594 Sunset HTTP-date supplied by the application.
    pub fn sunset(mut self, sunset: HeaderValue) -> Self {
        self.sunset = Some(sunset);
        self
    }

    /// Adds one Link field value. Repeated calls preserve separate field values.
    pub fn link(mut self, link: HeaderValue) -> Self {
        self.links.push(link);
        self
    }

    pub fn apply(self, response: &mut Response) {
        let headers = response.headers_mut();
        if let Some(value) = self.retry_after {
            headers.insert(header::RETRY_AFTER, value);
        }
        if let Some(value) = self.www_authenticate {
            headers.insert(header::WWW_AUTHENTICATE, value);
        }
        if let Some(value) = self.deprecation {
            headers.insert(DEPRECATION_HEADER.clone(), value);
        }
        if let Some(value) = self.sunset {
            headers.insert(SUNSET_HEADER.clone(), value);
        }
        for link in self.links {
            headers.append(header::LINK, link);
        }
    }

    pub const fn wrap<T>(self, inner: T) -> ApiResponse<T> {
        ApiResponse {
            inner,
            metadata: self,
        }
    }
}

/// An Axum response plus standard cross-client response metadata.
#[must_use]
#[derive(Debug, Clone)]
pub struct ApiResponse<T> {
    inner: T,
    metadata: ApiResponseMetadata,
}

impl<T> IntoResponse for ApiResponse<T>
where
    T: IntoResponse,
{
    fn into_response(self) -> Response {
        let mut response = self.inner.into_response();
        self.metadata.apply(&mut response);
        response
    }
}

#[non_exhaustive]
#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum ApiResponseMetadataError {
    #[error("deprecation timestamps before the Unix epoch are unsupported")]
    BeforeUnixEpoch,
}

#[cfg(test)]
mod tests {
    use super::*;
    use http::StatusCode;
    use std::time::Duration;

    #[test]
    fn metadata_wraps_any_axum_response_without_changing_its_status() {
        let response = ApiResponseMetadata::new()
            .retry_after_seconds(30)
            .bearer_challenge(BearerChallenge::InvalidToken)
            .deprecation_at(UNIX_EPOCH + Duration::from_hours(500_000))
            .unwrap()
            .sunset(HeaderValue::from_static("Sun, 06 Nov 1994 08:49:37 GMT"))
            .link(HeaderValue::from_static(
                r#"<https://api.example.invalid/migration>; rel="deprecation""#,
            ))
            .link(HeaderValue::from_static(
                r#"<https://api.example.invalid/replacement>; rel="successor-version""#,
            ))
            .wrap((StatusCode::TOO_MANY_REQUESTS, "slow down"))
            .into_response();

        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(response.headers()[header::RETRY_AFTER], "30");
        assert_eq!(
            response.headers()[header::WWW_AUTHENTICATE],
            r#"Bearer error="invalid_token""#
        );
        assert_eq!(response.headers()[&DEPRECATION_HEADER], "@1800000000");
        assert_eq!(
            response.headers()[&SUNSET_HEADER],
            "Sun, 06 Nov 1994 08:49:37 GMT"
        );
        assert_eq!(response.headers().get_all(header::LINK).iter().count(), 2);
    }

    #[test]
    fn retry_after_accepts_an_http_date() {
        let response = ApiResponseMetadata::new()
            .retry_after(HeaderValue::from_static("Sun, 06 Nov 1994 08:49:37 GMT"))
            .wrap((StatusCode::SERVICE_UNAVAILABLE, "try later"))
            .into_response();

        assert_eq!(
            response.headers()[header::RETRY_AFTER],
            "Sun, 06 Nov 1994 08:49:37 GMT"
        );
    }

    #[test]
    fn deprecation_rejects_dates_before_the_unix_epoch() {
        let before_epoch = UNIX_EPOCH.checked_sub(Duration::from_secs(1)).unwrap();
        let error = ApiResponseMetadata::new()
            .deprecation_at(before_epoch)
            .unwrap_err();
        assert_eq!(error, ApiResponseMetadataError::BeforeUnixEpoch);
    }
}
