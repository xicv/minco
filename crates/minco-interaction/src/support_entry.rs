use chrono::{DateTime, TimeDelta, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fmt};
use subtle::ConstantTimeEq;
use url::Url;
use uuid::Uuid;
use zeroize::Zeroizing;

const MAX_HANDOFF_TTL_SECONDS: i64 = 15 * 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SupportSurface {
    Widget,
    Portal,
    Extension,
    Api,
    Mobile,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SupportResourceReference {
    pub system: String,
    pub resource_type: String,
    pub resource_id: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SupportContext {
    pub page_url: String,
    #[serde(default, alias = "page_title", skip_serializing_if = "Option::is_none")]
    pub optional_page_title: Option<String>,
    #[serde(default, alias = "route_name", skip_serializing_if = "Option::is_none")]
    pub optional_route_name: Option<String>,
    #[serde(default, alias = "release_id", skip_serializing_if = "Option::is_none")]
    pub optional_release_id: Option<String>,
    #[serde(default, alias = "request_id", skip_serializing_if = "Option::is_none")]
    pub optional_request_id: Option<String>,
    #[serde(default, alias = "locale", skip_serializing_if = "Option::is_none")]
    pub optional_locale: Option<String>,
    #[serde(default, alias = "timezone", skip_serializing_if = "Option::is_none")]
    pub optional_timezone: Option<String>,
    #[serde(default, alias = "viewport", skip_serializing_if = "Option::is_none")]
    pub optional_viewport: Option<String>,
    #[serde(
        default,
        alias = "selected_text",
        skip_serializing_if = "Option::is_none"
    )]
    pub optional_selected_text: Option<String>,
    #[serde(default)]
    pub resource_references: Vec<SupportResourceReference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SupportBootstrap {
    pub schema_version: u32,
    pub project_id: String,
    pub portal_origin: String,
    pub label: String,
    pub brand: String,
    pub enabled_surfaces: Vec<SupportSurface>,
    pub screenshot_enabled: bool,
    pub voice_enabled: bool,
    pub file_enabled: bool,
    pub attachment_limits: crate::AttachmentLimits,
    pub recording_limit: u64,
    pub privacy_notice: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SupportHandoffId(pub Uuid);

impl SupportHandoffId {
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }
}

impl Default for SupportHandoffId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for SupportHandoffId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct SupportHandoffToken(Zeroizing<String>);

impl SupportHandoffToken {
    /// Generates a 244-bit opaque bearer from two independent `UUIDv4` values.
    #[must_use]
    pub fn generate() -> Self {
        Self(Zeroizing::new(format!(
            "{}{}",
            Uuid::new_v4().simple(),
            Uuid::new_v4().simple()
        )))
    }

    pub fn parse(value: impl Into<String>) -> Result<Self, SupportEntryError> {
        let value = value.into();
        if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(SupportEntryError::InvalidToken);
        }
        Ok(Self(Zeroizing::new(value)))
    }

    #[must_use]
    pub fn expose_sensitive(&self) -> &str {
        self.0.as_str()
    }

    #[must_use]
    pub fn digest(&self) -> SupportHandoffDigest {
        SupportHandoffDigest(hex::encode(Sha256::digest(self.0.as_bytes())))
    }
}

impl fmt::Debug for SupportHandoffToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SupportHandoffToken([REDACTED])")
    }
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SupportHandoffDigest(String);

impl SupportHandoffDigest {
    pub fn parse(value: impl Into<String>) -> Result<Self, SupportEntryError> {
        let value = value.into().to_ascii_lowercase();
        if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(SupportEntryError::InvalidDigest);
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    #[must_use]
    pub fn matches_token(&self, token: &SupportHandoffToken) -> bool {
        let candidate = token.digest();
        self.0.as_bytes().ct_eq(candidate.0.as_bytes()).into()
    }
}

impl fmt::Debug for SupportHandoffDigest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SupportHandoffDigest([REDACTED])")
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupportHandoff {
    pub id: SupportHandoffId,
    pub digest: SupportHandoffDigest,
    pub project_id: String,
    pub portal_origin: String,
    pub return_location: String,
    pub requester_subject: String,
    pub requester_permissions: Vec<String>,
    pub surface: SupportSurface,
    pub context: SupportContext,
    pub correlation_id: Uuid,
    pub expires_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub consumed_result: Option<SupportHandoffResult>,
}

impl fmt::Debug for SupportHandoff {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SupportHandoff")
            .field("id", &self.id)
            .field("digest", &self.digest)
            .field("project_id", &self.project_id)
            .field("portal_origin", &self.portal_origin)
            .field("return_location", &"[BOUNDED]")
            .field("requester_subject", &"[TRUSTED]")
            .field("permission_count", &self.requester_permissions.len())
            .field("surface", &self.surface)
            .field("context", &"[BOUNDED]")
            .field("correlation_id", &self.correlation_id)
            .field("expires_at", &self.expires_at)
            .field("consumed", &self.consumed_result.is_some())
            .finish()
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct SupportHandoffGrant {
    pub id: SupportHandoffId,
    pub token: SupportHandoffToken,
    pub portal_origin: String,
    pub expires_at: DateTime<Utc>,
}

impl SupportHandoffGrant {
    #[must_use]
    pub fn launch_url(&self) -> String {
        format!(
            "{}/#handoff={}",
            self.portal_origin.trim_end_matches('/'),
            self.token.expose_sensitive()
        )
    }
}

impl fmt::Debug for SupportHandoffGrant {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SupportHandoffGrant")
            .field("id", &self.id)
            .field("token", &self.token)
            .field("portal_origin", &self.portal_origin)
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupportHandoffResult {
    pub ticket_id: Uuid,
    pub requester_session_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupportLocationPolicy {
    pub portal_origin: String,
    pub allowed_return_paths: BTreeMap<String, Vec<String>>,
}

impl SupportLocationPolicy {
    pub fn validate(&self) -> Result<(), SupportEntryError> {
        exact_origin(&self.portal_origin)?;
        for (origin, prefixes) in &self.allowed_return_paths {
            exact_origin(origin)?;
            if prefixes.is_empty()
                || prefixes
                    .iter()
                    .any(|prefix| !prefix.starts_with('/') || prefix.contains(['?', '#']))
            {
                return Err(SupportEntryError::InvalidReturnPolicy);
            }
        }
        Ok(())
    }

    pub fn validate_return_location(&self, value: &str) -> Result<String, SupportEntryError> {
        self.validate()?;
        let location = Url::parse(value).map_err(|_| SupportEntryError::InvalidReturnLocation)?;
        if location.username() != ""
            || location.password().is_some()
            || location.query().is_some()
            || location.fragment().is_some()
        {
            return Err(SupportEntryError::InvalidReturnLocation);
        }
        let origin = location.origin().ascii_serialization();
        let allowed = self
            .allowed_return_paths
            .get(&origin)
            .is_some_and(|prefixes| {
                prefixes
                    .iter()
                    .any(|prefix| path_matches(location.path(), prefix))
            });
        allowed
            .then(|| location.to_string())
            .ok_or(SupportEntryError::ReturnLocationDenied)
    }
}

#[allow(clippy::too_many_arguments)]
pub fn issue_support_handoff(
    project_id: impl Into<String>,
    requester_subject: impl Into<String>,
    requester_permissions: Vec<String>,
    surface: SupportSurface,
    context: SupportContext,
    return_location: &str,
    correlation_id: Uuid,
    policy: &SupportLocationPolicy,
    now: DateTime<Utc>,
    ttl: TimeDelta,
) -> Result<(SupportHandoff, SupportHandoffGrant), SupportEntryError> {
    if ttl <= TimeDelta::zero() || ttl > TimeDelta::seconds(MAX_HANDOFF_TTL_SECONDS) {
        return Err(SupportEntryError::InvalidTtl);
    }
    let portal_origin = exact_origin(&policy.portal_origin)?;
    let return_location = policy.validate_return_location(return_location)?;
    let project_id = bounded_required(project_id.into(), 100)?;
    let requester_subject = bounded_required(requester_subject.into(), 300)?;
    validate_context(&context)?;
    if requester_permissions.len() > 64
        || requester_permissions
            .iter()
            .any(|value| bounded_required(value.clone(), 160).is_err())
    {
        return Err(SupportEntryError::InvalidPermissions);
    }
    let id = SupportHandoffId::new();
    let token = SupportHandoffToken::generate();
    let expires_at = now + ttl;
    let handoff = SupportHandoff {
        id,
        digest: token.digest(),
        project_id,
        portal_origin: portal_origin.clone(),
        return_location,
        requester_subject,
        requester_permissions,
        surface,
        context,
        correlation_id,
        expires_at,
        consumed_result: None,
    };
    let grant = SupportHandoffGrant {
        id,
        token,
        portal_origin,
        expires_at,
    };
    Ok((handoff, grant))
}

fn validate_context(context: &SupportContext) -> Result<(), SupportEntryError> {
    let bounded_values = [
        (context.optional_page_title.as_deref(), 2_000),
        (context.optional_route_name.as_deref(), 2_000),
        (context.optional_release_id.as_deref(), 2_000),
        (context.optional_request_id.as_deref(), 2_000),
        (context.optional_locale.as_deref(), 40),
        (context.optional_timezone.as_deref(), 100),
        (context.optional_viewport.as_deref(), 32),
        (context.optional_selected_text.as_deref(), 2_000),
    ];
    if context.page_url.chars().count() > 4_096
        || context.resource_references.len() > 8
        || bounded_values.into_iter().any(|(value, maximum)| {
            value.is_some_and(|value| {
                value.trim().is_empty()
                    || value.chars().count() > maximum
                    || value.chars().any(char::is_control)
            })
        })
        || context.resource_references.iter().any(|reference| {
            invalid_bounded(&reference.system, 100)
                || invalid_bounded(&reference.resource_type, 100)
                || invalid_bounded(&reference.resource_id, 300)
        })
        || context.optional_viewport.as_ref().is_some_and(|viewport| {
            let Some((width, height)) = viewport.split_once('x') else {
                return true;
            };
            width.is_empty()
                || height.is_empty()
                || width.len() > 6
                || height.len() > 6
                || !width.bytes().all(|byte| byte.is_ascii_digit())
                || !height.bytes().all(|byte| byte.is_ascii_digit())
        })
    {
        return Err(SupportEntryError::InvalidContext);
    }
    let page = Url::parse(&context.page_url).map_err(|_| SupportEntryError::InvalidContext)?;
    if !web_url_is_transport_safe(&page)
        || page.username() != ""
        || page.password().is_some()
        || page.query().is_some()
        || page.fragment().is_some()
    {
        return Err(SupportEntryError::InvalidContext);
    }
    Ok(())
}

fn exact_origin(value: &str) -> Result<String, SupportEntryError> {
    let url = Url::parse(value).map_err(|_| SupportEntryError::InvalidPortalOrigin)?;
    if !web_url_is_transport_safe(&url)
        || url.username() != ""
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || !matches!(url.path(), "" | "/")
    {
        return Err(SupportEntryError::InvalidPortalOrigin);
    }
    Ok(url.origin().ascii_serialization())
}

fn web_url_is_transport_safe(url: &Url) -> bool {
    url.scheme() == "https"
        || (url.scheme() == "http"
            && matches!(
                url.host_str(),
                Some("localhost" | "127.0.0.1" | "[::1]" | "::1")
            ))
}

fn path_matches(path: &str, prefix: &str) -> bool {
    prefix == "/"
        || path == prefix
        || path
            .strip_prefix(prefix)
            .is_some_and(|rest| rest.starts_with('/'))
}

fn bounded_required(value: String, maximum: usize) -> Result<String, SupportEntryError> {
    if invalid_bounded(&value, maximum) {
        Err(SupportEntryError::InvalidTrustedValue)
    } else {
        Ok(value)
    }
}

fn invalid_bounded(value: &str, maximum: usize) -> bool {
    value.trim().is_empty()
        || value.chars().count() > maximum
        || value.chars().any(char::is_control)
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SupportEntryError {
    #[error("support handoff token is invalid")]
    InvalidToken,
    #[error("support handoff digest is invalid")]
    InvalidDigest,
    #[error("support handoff lifetime must be positive and no more than 15 minutes")]
    InvalidTtl,
    #[error("portal origin must be one exact HTTP or HTTPS origin")]
    InvalidPortalOrigin,
    #[error("return location policy is invalid")]
    InvalidReturnPolicy,
    #[error("return location is invalid")]
    InvalidReturnLocation,
    #[error("return location is not allowed")]
    ReturnLocationDenied,
    #[error("trusted handoff value is invalid")]
    InvalidTrustedValue,
    #[error("requester permissions are invalid")]
    InvalidPermissions,
    #[error("support context is invalid or exceeds its bounds")]
    InvalidContext,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy() -> SupportLocationPolicy {
        SupportLocationPolicy {
            portal_origin: "https://support.example.test".into(),
            allowed_return_paths: BTreeMap::from([(
                "https://app.example.test".into(),
                vec!["/orders".into()],
            )]),
        }
    }

    #[test]
    fn bearer_is_redacted_digest_only_and_location_is_exact() {
        let context = SupportContext {
            page_url: "https://app.example.test/orders/7".into(),
            ..SupportContext::default()
        };
        let (stored, grant) = issue_support_handoff(
            "project",
            "subject",
            vec!["ticketing.create".into()],
            SupportSurface::Widget,
            context,
            "https://app.example.test/orders/7",
            Uuid::now_v7(),
            &policy(),
            Utc::now(),
            TimeDelta::minutes(5),
        )
        .unwrap();
        assert!(stored.digest.matches_token(&grant.token));
        assert!(
            !serde_json::to_string(&stored)
                .unwrap()
                .contains(grant.token.expose_sensitive())
        );
        assert!(!format!("{stored:?}{grant:?}").contains(grant.token.expose_sensitive()));
        assert!(grant.launch_url().contains("#handoff="));
        assert!(!grant.launch_url().contains('?'));
    }

    #[test]
    fn path_prefix_is_segment_bounded() {
        assert!(
            policy()
                .validate_return_location("https://app.example.test/orders/7")
                .is_ok()
        );
        assert_eq!(
            policy().validate_return_location("https://app.example.test/orders-admin"),
            Err(SupportEntryError::ReturnLocationDenied)
        );
    }

    #[test]
    fn browser_context_aliases_are_accepted_and_all_context_is_bounded() {
        let context: SupportContext = serde_json::from_value(serde_json::json!({
            "page_url": "https://app.example.test/orders/7",
            "page_title": "Order",
            "locale": "en-AU",
            "viewport": "1440x900"
        }))
        .unwrap();
        assert_eq!(context.optional_page_title.as_deref(), Some("Order"));
        assert!(validate_context(&context).is_ok());

        let mut invalid = context;
        invalid.resource_references.push(SupportResourceReference {
            system: "orders".into(),
            resource_type: "order".into(),
            resource_id: "\n".into(),
        });
        assert_eq!(
            validate_context(&invalid),
            Err(SupportEntryError::InvalidContext)
        );
    }

    #[test]
    fn non_local_plain_http_origins_fail_closed() {
        assert_eq!(
            exact_origin("http://support.example.test"),
            Err(SupportEntryError::InvalidPortalOrigin)
        );
        assert_eq!(
            exact_origin("http://localhost:3000").unwrap(),
            "http://localhost:3000"
        );
    }
}
