//! Transport-neutral workspace, project and integration-profile model
//! (ADR-0076).
//!
//! This module owns bounded identifiers, the workspace/project registry
//! shapes, integration profiles as persisted policy (never credentials), and
//! the resolved-scope types compositions hand to application use cases. It
//! deliberately knows nothing about HTTP, SQL, cookies, or credential
//! verification: origins are stored as restrictions, `kind` is classification,
//! and no type here authenticates anybody.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use std::collections::BTreeSet;
use std::fmt::Write as _;

/// Maximum identifier length shared by minted workspace/profile identifiers.
pub const IDENTIFIER_MAXIMUM: usize = 64;

/// Maximum project-identifier length, mirroring the ticketing project
/// validation exactly so every historically valid project ID registers
/// verbatim.
pub const PROJECT_MAXIMUM: usize = 100;

/// A bounded identifier failed validation.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "{field} must contain 1-{maximum} visible characters without leading or trailing whitespace"
)]
pub struct InvalidIdentifier {
    pub field: &'static str,
    pub maximum: usize,
}

/// Stable workspace identity minted by provisioning and bound to one
/// database. Spelling is preserved exactly; validation never trims,
/// lowercases, or rewrites a provided value.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct WorkspaceId(String);

impl WorkspaceId {
    /// Mint a fresh workspace identity for first provisioning.
    #[must_use]
    pub fn mint() -> Self {
        Self(format!("ws-{}", uuid::Uuid::new_v4().simple()))
    }

    /// The identifier exactly as provisioned.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for WorkspaceId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl std::fmt::Display for WorkspaceId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl TryFrom<String> for WorkspaceId {
    type Error = InvalidIdentifier;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        validate_minted_identifier("workspace_id", &value, IDENTIFIER_MAXIMUM)?;
        Ok(Self(value))
    }
}

impl From<WorkspaceId> for String {
    fn from(value: WorkspaceId) -> Self {
        value.0
    }
}

/// Project identity carrying the exact spelling ticketing already persists.
///
/// The validation rule mirrors ticketing's `project_id` rule (visible,
/// bounded, no control characters) so historical identifiers — including
/// case and interior spacing — register without silent rewriting.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ProjectId(String);

impl ProjectId {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for ProjectId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl std::fmt::Display for ProjectId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl TryFrom<String> for ProjectId {
    type Error = InvalidIdentifier;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.trim().is_empty()
            || value.chars().count() > PROJECT_MAXIMUM
            || value.chars().any(char::is_control)
        {
            return Err(InvalidIdentifier {
                field: "project_id",
                maximum: PROJECT_MAXIMUM,
            });
        }
        Ok(Self(value))
    }
}

impl From<ProjectId> for String {
    fn from(value: ProjectId) -> Self {
        value.0
    }
}

/// Bounded identifier for one integration profile.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ProfileId(String);

impl ProfileId {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for ProfileId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl std::fmt::Display for ProfileId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl TryFrom<String> for ProfileId {
    type Error = InvalidIdentifier;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        validate_minted_identifier("profile_id", &value, IDENTIFIER_MAXIMUM)?;
        Ok(Self(value))
    }
}

impl From<ProfileId> for String {
    fn from(value: ProfileId) -> Self {
        value.0
    }
}

fn validate_minted_identifier(
    field: &'static str,
    value: &str,
    maximum: usize,
) -> Result<(), InvalidIdentifier> {
    if value.trim().is_empty()
        || value.trim() != value
        || value.chars().count() > maximum
        || value.chars().any(char::is_control)
    {
        return Err(InvalidIdentifier { field, maximum });
    }
    Ok(())
}

/// The provisioned workspace bound to this deployment's database.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Workspace {
    pub id: WorkspaceId,
    pub display_name: String,
    pub provisioned_at: DateTime<Utc>,
}

/// A project registered under its workspace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectRegistration {
    pub workspace: WorkspaceId,
    pub project: ProjectId,
    pub registered_at: DateTime<Utc>,
}

/// Classification of an integration surface; taxonomy reserved by ADR-0076.
///
/// Only the combinations in [`ProfileKind::supports`](ProfileKind) are
/// enabled in this release, and every unsupported combination fails closed
/// at provisioning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProfileKind {
    WebBff,
    Portal,
    Widget,
    BrowserExtension,
    NativeHandoff,
    NativeOidc,
    Email,
    ServiceApi,
    Webhook,
    Mcp,
    A2a,
}

impl ProfileKind {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::WebBff => "web_bff",
            Self::Portal => "portal",
            Self::Widget => "widget",
            Self::BrowserExtension => "browser_extension",
            Self::NativeHandoff => "native_handoff",
            Self::NativeOidc => "native_oidc",
            Self::Email => "email",
            Self::ServiceApi => "service_api",
            Self::Webhook => "webhook",
            Self::Mcp => "mcp",
            Self::A2a => "a2a",
        }
    }

    /// The concrete authentication modes this kind may carry in this
    /// release. This is the only supported set; anything else is reserved
    /// taxonomy and must fail closed rather than fall through to a bearer
    /// path.
    #[must_use]
    pub const fn supported_modes(&self) -> &'static [ProfileAuthMode] {
        match self {
            Self::ServiceApi => &[ProfileAuthMode::ServiceBearerToken],
            Self::Portal => &[ProfileAuthMode::PortalSession],
            Self::Email => &[ProfileAuthMode::InboundEmail],
            Self::WebBff
            | Self::Widget
            | Self::BrowserExtension
            | Self::NativeHandoff
            | Self::NativeOidc
            | Self::Webhook
            | Self::Mcp
            | Self::A2a => &[],
        }
    }

    #[must_use]
    pub fn supports(&self, mode: ProfileAuthMode) -> bool {
        self.supported_modes().contains(&mode)
    }
}

/// The concrete credential mechanism one profile supports. `kind` is never
/// the verifier: the composition owns the concrete verification for the
/// declared mode, and only for these modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProfileAuthMode {
    /// Deployment-bound shared bearer secret verified by constant-time
    /// comparison; the profile's service principal represents the
    /// machine-to-machine activity.
    ServiceBearerToken,
    /// Cookie sessions minted only through the handoff exchange; CSRF-bound
    /// mutations.
    PortalSession,
    /// One-time handoff tokens (reserved: handoffs are minted credentials,
    /// not an independent entry surface in this release).
    SupportHandoff,
    /// Verified inbound mail (transport-verified sender, digest-checked raw
    /// object) processed by a scoped worker principal.
    InboundEmail,
}

impl ProfileAuthMode {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::ServiceBearerToken => "service_bearer_token",
            Self::PortalSession => "portal_session",
            Self::SupportHandoff => "support_handoff",
            Self::InboundEmail => "inbound_email",
        }
    }
}

/// Lifecycle status of a profile. Provisioning never re-enables a disabled
/// profile or recreates a removed one without explicit reconciliation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProfileStatus {
    Enabled,
    Disabled,
}

impl ProfileStatus {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Enabled => "enabled",
            Self::Disabled => "disabled",
        }
    }
}

/// One persisted integration profile: policy, ownership, status, and a
/// secret **reference** (a name, never a credential value).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntegrationProfile {
    pub id: ProfileId,
    pub workspace: WorkspaceId,
    pub kind: ProfileKind,
    pub auth_mode: ProfileAuthMode,
    pub status: ProfileStatus,
    /// The single project this profile is bound to for this release.
    pub bound_project: ProjectId,
    /// Principal subject this profile acts as. It represents
    /// machine-to-machine activity and never replaces a human requester.
    pub service_subject: String,
    /// Permission ceiling: the most this profile can ever contribute. It is
    /// intersected with principal grants — never unioned into a human.
    pub permission_ceiling: BTreeSet<String>,
    /// Exact origins (scheme, host, port) this profile accepts as a
    /// restriction. Origins never authenticate and never select a weaker
    /// mode.
    pub allowed_origins: BTreeSet<String>,
    /// Resource types this profile may reference or consume.
    pub resource_types: BTreeSet<String>,
    /// Secret name looked up by the composition at verification time.
    #[serde(default)]
    pub secret_reference: Option<String>,
    /// Digest of every policy field above, used to detect drift that demands
    /// explicit reconciliation.
    pub policy_digest: String,
}

impl IntegrationProfile {
    /// Re-derive the policy digest from every policy-bearing field and
    /// return the sealed profile. Bounded collections keep canonical
    /// ordering, so equal policy digests identically regardless of
    /// insertion order.
    #[must_use]
    pub fn with_recomputed_digest(mut self) -> Self {
        self.policy_digest = digest_policy(&self);
        self
    }

    /// Re-derive this profile's policy digest from its current fields.
    #[must_use]
    pub fn current_policy_digest(&self) -> String {
        digest_policy(self)
    }
}

fn digest_policy(profile: &IntegrationProfile) -> String {
    #[derive(Serialize)]
    struct Policy<'a> {
        id: &'a str,
        workspace: &'a str,
        kind: &'a str,
        auth_mode: &'a str,
        status: &'a str,
        bound_project: &'a str,
        service_subject: &'a str,
        permission_ceiling: &'a BTreeSet<String>,
        allowed_origins: &'a BTreeSet<String>,
        resource_types: &'a BTreeSet<String>,
        secret_reference: Option<&'a str>,
    }
    let canonical = serde_json::to_string(&Policy {
        id: profile.id.as_str(),
        workspace: profile.workspace.as_str(),
        kind: profile.kind.as_str(),
        auth_mode: profile.auth_mode.as_str(),
        status: profile.status.as_str(),
        bound_project: profile.bound_project.as_str(),
        service_subject: &profile.service_subject,
        permission_ceiling: &profile.permission_ceiling,
        allowed_origins: &profile.allowed_origins,
        resource_types: &profile.resource_types,
        secret_reference: profile.secret_reference.as_deref(),
    })
    .unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(canonical.as_bytes());
    hex(&hasher.finalize())
}

/// An explicit principal-to-workspace/project grant. Valid identity never
/// implies membership: without a grant (or a profile's own service
/// principal), scope resolution fails closed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrincipalGrant {
    pub workspace: WorkspaceId,
    pub subject: String,
    pub project: ProjectId,
    pub granted_at: DateTime<Utc>,
}

/// Workspace-level scope, distinct from project scope. No wildcard or empty
/// project exists at this level.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceScope {
    pub workspace: WorkspaceId,
}

/// Project-level scope: the resolved authority context application use
/// cases run against. The type ticketing consumes as its transport-neutral
/// scope (ADR-0076).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectScope {
    pub workspace: WorkspaceId,
    pub project: ProjectId,
}

/// Either resolution level. Workspace-level operations (health, registry
/// reads) use the workspace variant; every business operation requires the
/// project variant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResolvedScope {
    Workspace(WorkspaceScope),
    Project(ProjectScope),
}

impl ProjectScope {
    /// Scope-token prefix carrying the bound workspace identity.
    pub const WORKSPACE_TOKEN_PREFIX: &'static str = "workspace:";
    /// Scope-token prefix carrying the bound project identity.
    pub const PROJECT_TOKEN_PREFIX: &'static str = "project:";

    /// The canonical scope tokens for a checked caller context. Only the
    /// composition sets these after resolving scope; request input never
    /// reaches them.
    #[must_use]
    pub fn scope_tokens(&self) -> [String; 2] {
        [
            format!("{}{}", Self::WORKSPACE_TOKEN_PREFIX, self.workspace),
            format!("{}{}", Self::PROJECT_TOKEN_PREFIX, self.project),
        ]
    }

    /// Parse a checked caller's scope tokens back into a scope. Exactly one
    /// workspace and one project token must be present; missing, duplicated,
    /// or malformed tokens fail closed rather than guessing.
    pub fn from_scope_tokens(scopes: &BTreeSet<String>) -> Result<Self, ScopeTokenError> {
        let workspace = single_token(scopes, Self::WORKSPACE_TOKEN_PREFIX)?;
        let project = single_token(scopes, Self::PROJECT_TOKEN_PREFIX)?;
        Ok(Self {
            workspace: WorkspaceId::try_from(workspace)
                .map_err(ScopeTokenError::InvalidWorkspace)?,
            project: ProjectId::try_from(project).map_err(ScopeTokenError::InvalidProject)?,
        })
    }
}

fn single_token(scopes: &BTreeSet<String>, prefix: &str) -> Result<String, ScopeTokenError> {
    let mut matches = scopes
        .iter()
        .filter(|scope| scope.starts_with(prefix))
        .map(|scope| scope[prefix.len()..].to_owned());
    let Some(first) = matches.next() else {
        return Err(match prefix {
            ProjectScope::WORKSPACE_TOKEN_PREFIX => ScopeTokenError::MissingWorkspace,
            _ => ScopeTokenError::MissingProject,
        });
    };
    if matches.next().is_some() {
        return Err(match prefix {
            ProjectScope::WORKSPACE_TOKEN_PREFIX => ScopeTokenError::AmbiguousWorkspace,
            _ => ScopeTokenError::AmbiguousProject,
        });
    }
    Ok(first)
}

/// A checked caller's scope tokens do not carry exactly one resolvable
/// workspace/project pair.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ScopeTokenError {
    #[error("no workspace scope token is present")]
    MissingWorkspace,
    #[error("no project scope token is present")]
    MissingProject,
    #[error("more than one workspace scope token is present")]
    AmbiguousWorkspace,
    #[error("more than one project scope token is present")]
    AmbiguousProject,
    #[error("the workspace scope token is not a valid identifier")]
    InvalidWorkspace(#[source] InvalidIdentifier),
    #[error("the project scope token is not a valid identifier")]
    InvalidProject(#[source] InvalidIdentifier),
}

impl ResolvedScope {
    #[must_use]
    pub const fn workspace(&self) -> &WorkspaceId {
        match self {
            Self::Workspace(scope) => &scope.workspace,
            Self::Project(scope) => &scope.workspace,
        }
    }
}

/// A resolved service principal: the checked, immutable context a
/// composition builds after verifying the profile's concrete credential.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServicePrincipalScope {
    pub scope: ProjectScope,
    pub service_subject: String,
    pub permission_ceiling: BTreeSet<String>,
    pub allowed_origins: BTreeSet<String>,
    pub resource_types: BTreeSet<String>,
}

/// Validate an exact canonical origin: `scheme://host[:port]` and nothing
/// else.
///
/// Origins are restrictions, so anything unparsable or non-canonical is
/// rejected rather than guessed at.
pub fn validate_exact_origin(origin: &str) -> Result<(), InvalidOrigin> {
    let parsed = url::Url::parse(origin).map_err(InvalidOrigin::Unparsable)?;
    let has_host = !parsed.host_str().unwrap_or_default().is_empty();
    let port = parsed.port_or_known_default();
    if !matches!(parsed.scheme(), "http" | "https") || !has_host {
        return Err(InvalidOrigin::NotAnOrigin);
    }
    let mut normalized = format!(
        "{}://{}",
        parsed.scheme(),
        parsed.host_str().unwrap_or_default()
    );
    if let Some(port) = port {
        let default_port = matches!((parsed.scheme(), port), ("http", 80) | ("https", 443));
        if !default_port {
            let _ = write!(normalized, ":{port}");
        }
    }
    if parsed.path() != "/" || parsed.query().is_some() || parsed.fragment().is_some() {
        return Err(InvalidOrigin::NotAnOrigin);
    }
    if normalized != origin {
        return Err(InvalidOrigin::NonCanonical { normalized });
    }
    Ok(())
}

/// An origin string is not an exact canonical origin.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InvalidOrigin {
    #[error("origin is not an absolute http(s) URL")]
    Unparsable(#[source] url::ParseError),
    #[error("origin must be scheme://host[:port] with no path, query, or fragment")]
    NotAnOrigin,
    #[error("origin must be canonical: {normalized}")]
    NonCanonical { normalized: String },
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(DIGITS[(byte >> 4) as usize] as char);
        output.push(DIGITS[(byte & 0x0f) as usize] as char);
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_identifiers_preserve_historical_spelling_exactly() {
        // The rule mirrors ticketing's validate_text: visible, bounded, no
        // control characters. Case and interior spacing survive because the
        // registry must register existing projects verbatim.
        let legacy = ProjectId::try_from("Desk_Prj-2024".to_owned()).expect("valid project");
        assert_eq!(legacy.as_str(), "Desk_Prj-2024");
        let spaced = ProjectId::try_from("legacy project".to_owned()).expect("interior space");
        assert_eq!(spaced.as_str(), "legacy project");
        let long = "x".repeat(100);
        assert!(ProjectId::try_from(long).is_ok());
        assert!(
            ProjectId::try_from("x".repeat(101)).is_err(),
            "over-length identifiers fail closed"
        );
        for invalid in ["", "   ", "\u{0}desk"] {
            assert!(ProjectId::try_from(invalid.to_owned()).is_err());
        }
    }

    #[test]
    fn minted_identifiers_reject_instead_of_rewriting() {
        // Workspace and profile identifiers are minted by this plugin, so
        // surrounding whitespace is rejected rather than silently trimmed.
        assert!(WorkspaceId::try_from("ws-1".to_owned()).is_ok());
        assert!(WorkspaceId::try_from(" ws-1".to_owned()).is_err());
        assert!(ProfileId::try_from("desk-agent".to_owned()).is_ok());
        assert!(ProfileId::try_from("desk-agent ".to_owned()).is_err());
        assert!(WorkspaceId::try_from("w".repeat(65)).is_err());
        let minted = WorkspaceId::mint();
        assert!(minted.as_str().starts_with("ws-"));
    }

    #[test]
    fn policy_digest_is_deterministic_over_canonical_field_order() {
        let id = ProfileId::try_from("desk-agent".to_owned()).expect("valid");
        let workspace = WorkspaceId::try_from("ws-1".to_owned()).expect("valid");
        let project = ProjectId::try_from("desk".to_owned()).expect("valid");
        let profile = |status, ceiling: BTreeSet<String>| {
            IntegrationProfile {
                id: id.clone(),
                workspace: workspace.clone(),
                kind: ProfileKind::ServiceApi,
                auth_mode: ProfileAuthMode::ServiceBearerToken,
                status,
                bound_project: project.clone(),
                service_subject: "desk-agent".into(),
                permission_ceiling: ceiling,
                allowed_origins: BTreeSet::new(),
                resource_types: BTreeSet::new(),
                secret_reference: None,
                policy_digest: String::new(),
            }
            .with_recomputed_digest()
        };
        let first = profile(
            ProfileStatus::Enabled,
            ["ticketing.agent.read", "ticketing.create"]
                .map(str::to_owned)
                .into(),
        );
        // The same fields in different insertion order digest identically.
        let second = profile(
            ProfileStatus::Enabled,
            ["ticketing.create", "ticketing.agent.read"]
                .map(str::to_owned)
                .into(),
        );
        assert_eq!(first.policy_digest, second.policy_digest);
        assert_eq!(first.policy_digest.len(), 64);
        assert_eq!(first.policy_digest, first.current_policy_digest());
        // Any policy change moves the digest.
        let changed = profile(
            ProfileStatus::Disabled,
            ["ticketing.agent.read", "ticketing.create"]
                .map(str::to_owned)
                .into(),
        );
        assert_ne!(first.policy_digest, changed.policy_digest);
    }

    #[test]
    fn reserved_taxonomy_fails_closed_instead_of_falling_through() {
        for kind in [
            ProfileKind::WebBff,
            ProfileKind::Widget,
            ProfileKind::BrowserExtension,
            ProfileKind::NativeHandoff,
            ProfileKind::NativeOidc,
            ProfileKind::Webhook,
            ProfileKind::Mcp,
            ProfileKind::A2a,
        ] {
            assert!(
                kind.supported_modes().is_empty(),
                "{} must be reserved in this release",
                kind.as_str()
            );
            assert!(!kind.supports(ProfileAuthMode::ServiceBearerToken));
        }
        assert!(ProfileKind::ServiceApi.supports(ProfileAuthMode::ServiceBearerToken));
        assert!(!ProfileKind::ServiceApi.supports(ProfileAuthMode::PortalSession));
        assert!(ProfileKind::Portal.supports(ProfileAuthMode::PortalSession));
        assert!(ProfileKind::Email.supports(ProfileAuthMode::InboundEmail));
    }

    #[test]
    fn exact_origin_validation_rejects_non_canonical_origins() {
        assert!(validate_exact_origin("https://desk.example.test").is_ok());
        assert!(validate_exact_origin("http://127.0.0.1:8090").is_ok());
        assert!(validate_exact_origin("https://desk.example.test/").is_err());
        assert!(validate_exact_origin("https://desk.example.test/app").is_err());
        assert!(validate_exact_origin("https://desk.example.test?q=1").is_err());
        assert!(validate_exact_origin("HTTPS://desk.example.test").is_err());
        assert!(validate_exact_origin("ftp://desk.example.test").is_err());
        assert!(validate_exact_origin("https://").is_err());
        assert!(validate_exact_origin("not-a-url").is_err());
    }

    #[test]
    fn scopes_stay_level_distinct() {
        let workspace = WorkspaceId::try_from("ws-1".to_owned()).expect("valid");
        let project = ProjectId::try_from("desk".to_owned()).expect("valid");
        let workspace_scope = ResolvedScope::Workspace(WorkspaceScope {
            workspace: workspace.clone(),
        });
        let project_scope = ResolvedScope::Project(ProjectScope {
            workspace: workspace.clone(),
            project,
        });
        assert_eq!(workspace_scope.workspace(), &workspace);
        assert_eq!(project_scope.workspace(), &workspace);
        assert_ne!(workspace_scope, project_scope);
    }

    #[test]
    fn scope_tokens_round_trip_and_ambiguous_sets_fail_closed() {
        let workspace = WorkspaceId::try_from("ws-1".to_owned()).expect("valid");
        let project = ProjectId::try_from("legacy Prj".to_owned()).expect("valid");
        let scope = ProjectScope { workspace, project };
        let tokens: BTreeSet<String> = scope.scope_tokens().into_iter().collect();
        assert_eq!(
            ProjectScope::from_scope_tokens(&tokens).expect("round trip"),
            scope
        );
        // Foreign scope vocabulary is ignored, not rejected.
        let mut extended = tokens.clone();
        extended.insert("orders:read".into());
        assert_eq!(
            ProjectScope::from_scope_tokens(&extended).expect("foreign scopes ignored"),
            scope
        );
        // Missing, duplicated, and malformed tokens all fail closed.
        let mut missing = tokens.clone();
        missing.remove("workspace:ws-1");
        assert_eq!(
            ProjectScope::from_scope_tokens(&missing),
            Err(ScopeTokenError::MissingWorkspace)
        );
        let mut ambiguous = tokens.clone();
        ambiguous.insert("project:other".into());
        assert_eq!(
            ProjectScope::from_scope_tokens(&ambiguous),
            Err(ScopeTokenError::AmbiguousProject)
        );
        let mut malformed = tokens;
        malformed.insert("workspace:".into());
        assert!(matches!(
            ProjectScope::from_scope_tokens(&malformed),
            Err(ScopeTokenError::AmbiguousWorkspace)
        ));
    }
}
