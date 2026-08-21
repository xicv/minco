use http::StatusCode;
use minco_contract::ContractAuthorizationPolicy;

use crate::{ApiFailure, Principal};

/// Enforce generated authentication, permission and scope requirements.
///
/// This boundary is deliberately coarse and performs no database, tenancy,
/// ownership or business-state checks. Applications retain those checks.
pub fn authorize_operation(
    principal: Option<&Principal>,
    policy: &ContractAuthorizationPolicy,
    request_id: &str,
) -> Result<(), ApiFailure> {
    if policy.anonymous {
        return Ok(());
    }
    let Some(principal) = principal else {
        return Err(ApiFailure::new(
            StatusCode::UNAUTHORIZED,
            "unauthenticated",
            "Authentication required",
            "A valid bearer credential is required for this operation.",
            request_id,
        ));
    };

    let permissions_allowed = policy
        .permissions
        .iter()
        .all(|permission| principal.has_permission(permission));
    let scopes_allowed = policy.alternatives.is_empty()
        || policy.alternatives.iter().any(|alternative| {
            alternative
                .scopes
                .iter()
                .all(|scope| principal.has_scope(scope))
        });
    if permissions_allowed && scopes_allowed {
        Ok(())
    } else {
        Err(ApiFailure::new(
            StatusCode::FORBIDDEN,
            "forbidden",
            "Forbidden",
            "The authenticated principal is not permitted to perform this operation.",
            request_id,
        ))
    }
}
