use http::HeaderMap;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Principal {
    pub subject: String,
    #[serde(default)]
    pub permissions: BTreeSet<String>,
    #[serde(default)]
    pub claims: BTreeMap<String, String>,
}

impl Principal {
    pub fn has_permission(&self, permission: &str) -> bool {
        self.permissions.contains(permission)
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
    let request_id = headers
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.trim().is_empty())
        .map_or_else(|| Uuid::now_v7().to_string(), str::to_owned);
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
