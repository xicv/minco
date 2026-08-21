use http::HeaderMap;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;

use crate::request_id_from_headers;

/// Reserved provider-neutral claim used to carry normalized exact scope tokens.
pub const PRINCIPAL_SCOPES_CLAIM: &str = "urn:minco:principal:scopes";

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Principal {
    pub subject: String,
    #[serde(default)]
    pub permissions: BTreeSet<String>,
    #[serde(default)]
    pub claims: BTreeMap<String, String>,
}

impl std::fmt::Debug for Principal {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Principal")
            .field("subject", &self.subject)
            .field("permissions", &self.permissions)
            .field("claim_keys", &self.claims.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl Principal {
    pub fn has_permission(&self, permission: &str) -> bool {
        self.permissions.contains(permission)
    }

    #[must_use]
    pub fn with_scopes<I, S>(mut self, scopes: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let scopes = scopes
            .into_iter()
            .map(|scope| scope.as_ref().to_owned())
            .filter(|scope| {
                !scope.is_empty() && !scope.bytes().any(|byte| byte.is_ascii_whitespace())
            })
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>()
            .join(" ");
        if scopes.is_empty() {
            self.claims.remove(PRINCIPAL_SCOPES_CLAIM);
        } else {
            self.claims
                .insert(PRINCIPAL_SCOPES_CLAIM.to_owned(), scopes);
        }
        self
    }

    #[must_use]
    pub fn has_scope(&self, scope: &str) -> bool {
        self.claims
            .get(PRINCIPAL_SCOPES_CLAIM)
            .is_some_and(|scopes| scopes.split_ascii_whitespace().any(|value| value == scope))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestMetadata {
    pub request_id: String,
    pub principal: Option<Principal>,
}

pub fn principal_from_headers(
    headers: &HeaderMap,
    allow_development_headers: bool,
) -> Result<RequestMetadata, PrincipalError> {
    let request_id = request_id_from_headers(headers);
    if !allow_development_headers {
        return Ok(RequestMetadata {
            request_id,
            principal: None,
        });
    }
    let Some(subject) = headers
        .get("x-minco-subject")
        .and_then(|value| value.to_str().ok())
    else {
        return Ok(RequestMetadata {
            request_id,
            principal: None,
        });
    };
    if subject.trim().is_empty() {
        return Err(PrincipalError::InvalidSubject);
    }
    let permissions = headers
        .get("x-minco-permissions")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .collect();
    Ok(RequestMetadata {
        request_id,
        principal: Some(Principal {
            subject: subject.to_owned(),
            permissions,
            claims: BTreeMap::new(),
        }),
    })
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum PrincipalError {
    #[error("principal subject is invalid")]
    InvalidSubject,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn development_headers_are_explicitly_opted_in() {
        let mut headers = HeaderMap::new();
        headers.insert("x-minco-subject", "user-1".parse().unwrap());
        headers.insert(
            "x-minco-permissions",
            "orders.read,orders.create".parse().unwrap(),
        );
        assert!(
            principal_from_headers(&headers, false)
                .unwrap()
                .principal
                .is_none()
        );
        let principal = principal_from_headers(&headers, true)
            .unwrap()
            .principal
            .unwrap();
        assert!(principal.has_permission("orders.create"));
    }
}
