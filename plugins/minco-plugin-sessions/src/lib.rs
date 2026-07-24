//! Provider-neutral, revocable browser and API session primitives.
#![forbid(unsafe_code)]

use async_trait::async_trait;
use chrono::{DateTime, TimeDelta, Utc};
use hmac::{Hmac, Mac};
use minco_core::{
    CapabilityProvision, DataClass, Plugin, PluginContext, PluginDescriptor, PluginError, PluginId,
    PluginStability,
};
use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, sync::Arc};
use subtle::ConstantTimeEq;
use tokio::sync::RwLock;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SessionId(pub Uuid);

impl SessionId {
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }
}

impl Default for SessionId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct SessionToken(String);

impl SessionToken {
    /// Creates a high-entropy opaque token from two independently generated `UUIDv4` values.
    pub fn generate() -> Self {
        Self(format!(
            "{}.{}",
            Uuid::new_v4().simple(),
            Uuid::new_v4().simple()
        ))
    }

    pub fn parse(value: impl Into<String>) -> Result<Self, SessionError> {
        let value = value.into();
        if value.len() < 48 || value.len() > 256 || value.chars().any(char::is_control) {
            return Err(SessionError::InvalidToken);
        }
        Ok(Self(value))
    }

    pub fn expose(&self) -> &str {
        &self.0
    }

    fn hash(&self) -> SessionTokenHash {
        SessionTokenHash(Sha256::digest(self.0.as_bytes()).into())
    }
}

impl std::fmt::Debug for SessionToken {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("SessionToken([REDACTED])")
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct SessionTokenHash([u8; 32]);

impl SessionTokenHash {
    pub fn constant_time_eq(&self, other: &Self) -> bool {
        bool::from(self.0.ct_eq(&other.0))
    }
}

impl std::fmt::Debug for SessionTokenHash {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("SessionTokenHash([REDACTED])")
    }
}

type HmacSha256 = Hmac<Sha256>;

/// Signed double-submit token bound to one session identifier.
#[derive(Clone, PartialEq, Eq)]
pub struct CsrfToken(String);

impl CsrfToken {
    pub fn parse(value: impl Into<String>) -> Result<Self, SessionError> {
        let value = value.into();
        let Some((nonce, signature)) = value.split_once('.') else {
            return Err(SessionError::InvalidCsrfToken);
        };
        if nonce.len() != 32
            || signature.len() != 64
            || Uuid::parse_str(nonce).is_err()
            || decode_hex(signature).is_none()
        {
            return Err(SessionError::InvalidCsrfToken);
        }
        Ok(Self(value))
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for CsrfToken {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("CsrfToken([REDACTED])")
    }
}

/// HMAC-backed CSRF token issuer and verifier.
///
/// Production applications must inject the same secret into every application instance and rotate
/// it independently from session records. The token is suitable for a signed double-submit cookie
/// flow when the cookie and request header values are compared by the HTTP adapter.
#[derive(Clone)]
pub struct CsrfService {
    secret: Arc<[u8]>,
}

impl std::fmt::Debug for CsrfService {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CsrfService")
            .field("secret", &"[REDACTED]")
            .finish()
    }
}

impl CsrfService {
    pub fn new(secret: impl Into<Vec<u8>>) -> Result<Self, SessionError> {
        let secret = secret.into();
        if secret.len() < 32 {
            return Err(SessionError::InvalidCsrfSecret);
        }
        Ok(Self {
            secret: Arc::from(secret),
        })
    }

    pub fn issue(&self, session_id: SessionId) -> CsrfToken {
        let nonce = Uuid::new_v4().simple().to_string();
        let signature = self.signature(session_id, &nonce);
        CsrfToken(format!("{nonce}.{}", encode_hex(&signature)))
    }

    pub fn verify(&self, session_id: SessionId, token: &CsrfToken) -> Result<(), SessionError> {
        let (nonce, encoded_signature) = token
            .0
            .split_once('.')
            .ok_or(SessionError::InvalidCsrfToken)?;
        let signature = decode_hex(encoded_signature).ok_or(SessionError::InvalidCsrfToken)?;
        let mut mac = HmacSha256::new_from_slice(&self.secret)
            .map_err(|_| SessionError::InvalidCsrfSecret)?;
        mac.update(session_id.0.as_bytes());
        mac.update(nonce.as_bytes());
        mac.verify_slice(&signature)
            .map_err(|_| SessionError::InvalidCsrfToken)
    }

    fn signature(&self, session_id: SessionId, nonce: &str) -> Vec<u8> {
        let mut mac =
            HmacSha256::new_from_slice(&self.secret).expect("validated HMAC secret length");
        mac.update(session_id.0.as_bytes());
        mac.update(nonce.as_bytes());
        mac.finalize().into_bytes().to_vec()
    }
}

fn encode_hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;

    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut encoded, "{byte:02x}").expect("writing to String cannot fail");
    }
    encoded
}

fn decode_hex(value: &str) -> Option<Vec<u8>> {
    if !value.len().is_multiple_of(2) {
        return None;
    }
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let text = std::str::from_utf8(pair).ok()?;
            u8::from_str_radix(text, 16).ok()
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionRecord {
    pub id: SessionId,
    pub subject: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revoked_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub attributes: BTreeMap<String, String>,
}

impl SessionRecord {
    pub fn active_at(&self, now: DateTime<Utc>) -> bool {
        self.revoked_at.is_none() && self.expires_at > now
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssuedSession {
    pub token: SessionToken,
    pub session: SessionRecord,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateSession {
    pub subject: String,
    pub ttl: TimeDelta,
    pub attributes: BTreeMap<String, String>,
}

#[async_trait]
pub trait SessionStore: Send + Sync + std::fmt::Debug {
    async fn create(
        &self,
        token_hash: SessionTokenHash,
        session: SessionRecord,
    ) -> Result<(), SessionError>;

    async fn find_by_token_hash(
        &self,
        token_hash: SessionTokenHash,
    ) -> Result<Option<SessionRecord>, SessionError>;

    async fn revoke(&self, id: SessionId, at: DateTime<Utc>) -> Result<bool, SessionError>;

    async fn revoke_subject(&self, subject: &str, at: DateTime<Utc>)
    -> Result<usize, SessionError>;
}

#[derive(Debug, Clone)]
pub struct SessionService {
    store: Arc<dyn SessionStore>,
}

impl SessionService {
    pub fn new(store: Arc<dyn SessionStore>) -> Self {
        Self { store }
    }

    pub async fn issue(&self, command: CreateSession) -> Result<IssuedSession, SessionError> {
        if command.subject.trim().is_empty() || command.ttl <= TimeDelta::zero() {
            return Err(SessionError::InvalidSession);
        }
        let token = SessionToken::generate();
        let now = Utc::now();
        let session = SessionRecord {
            id: SessionId::new(),
            subject: command.subject,
            created_at: now,
            expires_at: now + command.ttl,
            revoked_at: None,
            attributes: command.attributes,
        };
        self.store.create(token.hash(), session.clone()).await?;
        Ok(IssuedSession { token, session })
    }

    pub async fn resolve(&self, token: &SessionToken) -> Result<SessionRecord, SessionError> {
        let session = self
            .store
            .find_by_token_hash(token.hash())
            .await?
            .ok_or(SessionError::Unauthenticated)?;
        if session.active_at(Utc::now()) {
            Ok(session)
        } else {
            Err(SessionError::Unauthenticated)
        }
    }

    pub async fn revoke(&self, id: SessionId) -> Result<bool, SessionError> {
        self.store.revoke(id, Utc::now()).await
    }

    pub async fn revoke_subject(&self, subject: &str) -> Result<usize, SessionError> {
        if subject.trim().is_empty() {
            return Err(SessionError::InvalidSession);
        }
        self.store.revoke_subject(subject, Utc::now()).await
    }
}

#[derive(Debug, Default)]
pub struct MemorySessionStore {
    sessions: RwLock<BTreeMap<SessionId, (SessionTokenHash, SessionRecord)>>,
}

#[async_trait]
impl SessionStore for MemorySessionStore {
    async fn create(
        &self,
        token_hash: SessionTokenHash,
        session: SessionRecord,
    ) -> Result<(), SessionError> {
        let mut sessions = self.sessions.write().await;
        if sessions.contains_key(&session.id)
            || sessions
                .values()
                .any(|(existing, _)| existing.constant_time_eq(&token_hash))
        {
            return Err(SessionError::Duplicate);
        }
        sessions.insert(session.id, (token_hash, session));
        drop(sessions);
        Ok(())
    }

    async fn find_by_token_hash(
        &self,
        token_hash: SessionTokenHash,
    ) -> Result<Option<SessionRecord>, SessionError> {
        Ok(self
            .sessions
            .read()
            .await
            .values()
            .find(|(candidate, _)| candidate.constant_time_eq(&token_hash))
            .map(|(_, session)| session.clone()))
    }

    async fn revoke(&self, id: SessionId, at: DateTime<Utc>) -> Result<bool, SessionError> {
        let mut sessions = self.sessions.write().await;
        let Some((_, session)) = sessions.get_mut(&id) else {
            return Ok(false);
        };
        session.revoked_at.get_or_insert(at);
        drop(sessions);
        Ok(true)
    }

    async fn revoke_subject(
        &self,
        subject: &str,
        at: DateTime<Utc>,
    ) -> Result<usize, SessionError> {
        let mut count = 0;
        for (_, session) in self.sessions.write().await.values_mut() {
            if session.subject == subject && session.revoked_at.is_none() {
                session.revoked_at = Some(at);
                count += 1;
            }
        }
        Ok(count)
    }
}

#[derive(Debug, Clone)]
pub struct SessionsPlugin {
    service: SessionService,
    csrf: Option<CsrfService>,
}

impl SessionsPlugin {
    pub fn new(store: Arc<dyn SessionStore>) -> Self {
        Self {
            service: SessionService::new(store),
            csrf: None,
        }
    }

    pub fn memory() -> Self {
        let secret = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
        Self::new(Arc::new(MemorySessionStore::default()))
            .with_csrf_secret(secret.into_bytes())
            .expect("ephemeral CSRF secret is sufficiently long")
    }

    pub fn with_csrf_secret(mut self, secret: impl Into<Vec<u8>>) -> Result<Self, SessionError> {
        self.csrf = Some(CsrfService::new(secret)?);
        Ok(self)
    }
}

impl Default for SessionsPlugin {
    fn default() -> Self {
        Self::memory()
    }
}

impl Plugin for SessionsPlugin {
    fn descriptor(&self) -> PluginDescriptor {
        let mut descriptor = PluginDescriptor::new(
            PluginId::new("sessions").expect("static plugin ID"),
            Version::new(1, 0, 0),
            "Provider-neutral session issuance, lookup, expiry, and revocation",
        );
        descriptor.core_compatibility =
            VersionReq::parse(concat!("^", env!("CARGO_PKG_VERSION"))).expect("package version");
        descriptor.stability = PluginStability::Beta;
        descriptor.documentation = Some("https://docs.rs/minco-plugin-sessions".into());
        descriptor
            .data_classes
            .extend([DataClass::Personal, DataClass::Secret]);
        descriptor.provides.extend([
            CapabilityProvision {
                name: "sessions.issue".into(),
                version: Version::new(1, 0, 0),
            },
            CapabilityProvision {
                name: "sessions.resolve".into(),
                version: Version::new(1, 0, 0),
            },
            CapabilityProvision {
                name: "sessions.revoke".into(),
                version: Version::new(1, 0, 0),
            },
        ]);
        if self.csrf.is_some() {
            descriptor.provides.push(CapabilityProvision {
                name: "sessions.csrf".into(),
                version: Version::new(1, 0, 0),
            });
        }
        descriptor
    }

    fn install(&self, context: &mut PluginContext<'_>) -> Result<(), PluginError> {
        context.services().insert(Arc::new(self.service.clone()))?;
        if let Some(csrf) = &self.csrf {
            context.services().insert(Arc::new(csrf.clone()))?;
        }
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("session token is malformed")]
    InvalidToken,
    #[error("session subject and positive TTL are required")]
    InvalidSession,
    #[error("session is not authenticated")]
    Unauthenticated,
    #[error("session identifier or token already exists")]
    Duplicate,
    #[error("CSRF token is malformed or does not match the session")]
    InvalidCsrfToken,
    #[error("CSRF signing secret must contain at least 32 bytes")]
    InvalidCsrfSecret,
    #[error("session store failed: {0}")]
    Store(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn sessions_resolve_and_revoke_without_storing_plaintext_tokens() {
        let service = SessionService::new(Arc::new(MemorySessionStore::default()));
        let issued = service
            .issue(CreateSession {
                subject: "client-1".into(),
                ttl: TimeDelta::hours(1),
                attributes: BTreeMap::new(),
            })
            .await
            .unwrap();
        assert_eq!(
            service.resolve(&issued.token).await.unwrap().subject,
            "client-1"
        );
        assert!(service.revoke(issued.session.id).await.unwrap());
        assert!(matches!(
            service.resolve(&issued.token).await,
            Err(SessionError::Unauthenticated)
        ));
    }

    #[test]
    fn csrf_tokens_are_bound_to_one_session_and_tamper_evident() {
        let service = CsrfService::new(vec![7_u8; 32]).unwrap();
        let first = SessionId::new();
        let second = SessionId::new();
        let token = service.issue(first);
        assert!(service.verify(first, &token).is_ok());
        assert!(matches!(
            service.verify(second, &token),
            Err(SessionError::InvalidCsrfToken)
        ));
    }

    #[tokio::test]
    async fn subject_revocation_ends_all_sessions() {
        let service = SessionService::new(Arc::new(MemorySessionStore::default()));
        let first = service
            .issue(CreateSession {
                subject: "client-1".into(),
                ttl: TimeDelta::hours(1),
                attributes: BTreeMap::new(),
            })
            .await
            .unwrap();
        let second = service
            .issue(CreateSession {
                subject: "client-1".into(),
                ttl: TimeDelta::hours(1),
                attributes: BTreeMap::new(),
            })
            .await
            .unwrap();
        assert_eq!(service.revoke_subject("client-1").await.unwrap(), 2);
        assert!(service.resolve(&first.token).await.is_err());
        assert!(service.resolve(&second.token).await.is_err());
    }
}
