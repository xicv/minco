//! Native Lambda HTTP runtime, API Gateway principal mapping and SSM configuration loading.
#![forbid(unsafe_code)]

use anyhow::{Context, Result};
use axum::{Router, extract::Request, middleware::Next, response::Response};
use lambda_http::{RequestExt, request::RequestContext};
use minco_http::Principal;
use std::collections::{BTreeMap, BTreeSet};

pub async fn run_router(router: Router) -> Result<()> {
    lambda_http::run(router)
        .await
        .map_err(|error| anyhow::anyhow!("Lambda HTTP runtime failed: {error}"))
}

pub async fn inject_api_gateway_principal(mut request: Request, next: Next) -> Response {
    if let Some(principal) = principal_from_request_context(request.request_context_ref()) {
        request.extensions_mut().insert(principal);
    }
    next.run(request).await
}

#[must_use]
pub fn principal_from_request_context(context: Option<&RequestContext>) -> Option<Principal> {
    let RequestContext::ApiGatewayV2(context) = context? else {
        return None;
    };
    let authorizer = context.authorizer.as_ref()?;
    let value = serde_json::to_value(authorizer).ok()?;
    let claims = value
        .pointer("/jwt/claims")
        .or_else(|| value.get("claims"))?
        .as_object()?;
    principal_from_claims(claims)
}

fn principal_from_claims(claims: &serde_json::Map<String, serde_json::Value>) -> Option<Principal> {
    let subject = claims.get("sub")?.as_str()?.trim();
    if subject.is_empty() {
        return None;
    }
    let claims = claims
        .iter()
        .filter_map(|(key, value)| value.as_str().map(|value| (key.clone(), value.to_owned())))
        .collect::<BTreeMap<_, _>>();
    let mut permissions = BTreeSet::new();
    for claim in ["scope", "permissions", "custom:permissions"] {
        if let Some(value) = claims.get(claim) {
            permissions.extend(
                value
                    .split([',', ' '])
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_owned),
            );
        }
    }
    Some(Principal {
        subject: subject.to_owned(),
        permissions,
        claims,
    })
}

pub async fn load_secure_parameter(name: &str) -> Result<String> {
    if name.trim().is_empty() {
        anyhow::bail!("SSM parameter name is empty");
    }
    let config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
    let response = aws_sdk_ssm::Client::new(&config)
        .get_parameter()
        .name(name)
        .with_decryption(true)
        .send()
        .await
        .with_context(|| format!("failed to load SSM parameter {name}"))?;
    response
        .parameter
        .and_then(|parameter| parameter.value)
        .filter(|value| !value.is_empty())
        .with_context(|| format!("SSM parameter {name} has no value"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absent_gateway_context_is_anonymous() {
        assert!(principal_from_request_context(None).is_none());
    }

    #[test]
    fn maps_locked_cognito_permission_attributes() {
        let claims = serde_json::json!({
            "sub": "smoke-user",
            "custom:permissions": "orders.create orders.read",
            "aud": "client-id"
        });
        let principal =
            principal_from_claims(claims.as_object().expect("claims")).expect("principal");
        assert_eq!(principal.subject, "smoke-user");
        assert!(principal.permissions.contains("orders.create"));
        assert!(principal.permissions.contains("orders.read"));
    }
}
