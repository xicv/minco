//! Production AWS and signed-webhook adapters for Minco official plugin ports.
#![forbid(unsafe_code)]

#[cfg(feature = "appsync-events")]
pub mod appsync_events;
#[cfg(feature = "cognito")]
pub mod cognito;
pub mod iam;
#[cfg(any(feature = "ses", feature = "webhook"))]
pub mod notification;
pub mod plugin;
#[cfg(feature = "s3")]
pub mod s3;
#[cfg(feature = "s3")]
pub mod s3_storage;
#[cfg(feature = "ses")]
pub mod ses;
#[cfg(feature = "sqs")]
pub mod sqs;
#[cfg(feature = "static-site")]
pub mod static_site;
#[cfg(feature = "webhook")]
pub mod webhook;

#[derive(Debug, thiserror::Error)]
pub enum AwsAdapterError {
    #[error("adapter configuration is invalid: {0}")]
    InvalidConfiguration(String),
    #[error("AWS provider request failed: {0}")]
    Provider(String),
    #[error("provider response was incomplete: {0}")]
    IncompleteResponse(String),
    #[error("adapter serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
}

#[cfg(any(feature = "s3", feature = "sqs"))]
pub(crate) fn validated_service_uri(value: &str) -> Option<http::Uri> {
    let uri = value.parse::<http::Uri>().ok()?;
    let authority = uri.authority()?;
    let host = uri.host()?;
    if authority.as_str().contains('@')
        || uri.query().is_some()
        || !valid_uri_host(host)
        || !matches!(uri.scheme_str(), Some("https" | "http"))
        || (uri.scheme_str() == Some("http") && !is_loopback_host(host))
    {
        return None;
    }
    Some(uri)
}

#[cfg(any(feature = "s3", feature = "sqs"))]
fn is_loopback_host(host: &str) -> bool {
    let host = host
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
        .unwrap_or(host);
    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|address| address.is_loopback())
}

#[cfg(any(feature = "s3", feature = "sqs"))]
fn valid_uri_host(host: &str) -> bool {
    let unbracketed = host
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
        .unwrap_or(host);
    if unbracketed.parse::<std::net::IpAddr>().is_ok() {
        return true;
    }
    !unbracketed.is_empty()
        && unbracketed.len() <= 253
        && unbracketed.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        })
}

#[cfg(feature = "s3")]
pub(crate) fn provider_error(context: &str, error: &(dyn std::error::Error + 'static)) -> String {
    let mut parts = vec![context.to_owned()];
    let mut current = Some(error);
    while let Some(source) = current {
        let message = source
            .to_string()
            .chars()
            .map(|character| {
                if character.is_control() {
                    ' '
                } else {
                    character
                }
            })
            .collect::<String>();
        if !message.is_empty() && parts.last() != Some(&message) {
            parts.push(message);
        }
        current = source.source();
        if parts.len() == 6 {
            break;
        }
    }
    let mut message = parts.join(": ");
    if message.len() > 2048 {
        let mut boundary = 2045;
        while !message.is_char_boundary(boundary) {
            boundary -= 1;
        }
        message.truncate(boundary);
        message.push_str("...");
    }
    message
}

#[cfg(all(test, feature = "s3"))]
mod diagnostic_tests {
    use super::*;

    #[cfg(feature = "s3")]
    #[test]
    fn provider_errors_preserve_bounded_source_context_without_controls() {
        let source = std::io::Error::other("connector\nfailed");
        let message = provider_error("S3 PutObject", &source);
        assert_eq!(message, "S3 PutObject: connector failed");
        assert!(message.len() <= 2048);
    }

    #[cfg(feature = "s3")]
    #[test]
    fn provider_error_truncates_unicode_only_at_character_boundaries() {
        let source = std::io::Error::other("é".repeat(2000));
        let message = provider_error("provider", &source);
        assert!(message.len() <= 2048);
        assert!(message.ends_with("..."));
    }

    #[cfg(feature = "s3")]
    #[test]
    fn service_uri_validation_rejects_userinfo_queries_and_remote_plaintext() {
        assert!(validated_service_uri("https://sqs.ap-southeast-2.amazonaws.com/queue").is_some());
        assert!(validated_service_uri("http://127.0.0.1:4566").is_some());
        assert!(validated_service_uri("https://user@example.com").is_none());
        assert!(validated_service_uri("https://example.com/path?token=value").is_none());
        assert!(validated_service_uri("http://example.com").is_none());
    }
}
