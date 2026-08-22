use http::HeaderMap;
use uuid::Uuid;

use crate::REQUEST_ID_HEADER;

/// Maximum accepted byte length of an untrusted correlation ID.
pub const MAX_REQUEST_ID_BYTES: usize = 128;

/// Return whether a request ID uses Minco's bounded correlation-safe grammar.
#[must_use]
pub fn is_valid_request_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_REQUEST_ID_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
}

/// Preserve a safe request ID or replace untrusted input with `UUIDv7`.
#[must_use]
pub fn safe_request_id(value: Option<&str>) -> String {
    value
        .filter(|value| is_valid_request_id(value))
        .map_or_else(|| Uuid::now_v7().to_string(), str::to_owned)
}

/// Read a safe correlation ID from request headers.
#[must_use]
pub fn request_id_from_headers(headers: &HeaderMap) -> String {
    safe_request_id(
        headers
            .get(&REQUEST_ID_HEADER)
            .and_then(|value| value.to_str().ok()),
    )
}
