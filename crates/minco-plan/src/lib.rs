//! Provider-neutral deployment plan, database profiles, structural policy checks and cost estimation.
#![forbid(unsafe_code)]

mod cost;
mod model;
#[allow(unreachable_pub)]
mod sam;
mod sam_cross_client;

fn realtime_oidc_auth_is_valid(auth: &model::AuthPlan) -> bool {
    let model::AuthPlan::Jwt { issuer, audiences } = auth else {
        return false;
    };
    let Ok(uri) = issuer.parse::<http::Uri>() else {
        return false;
    };
    let issuer_valid = uri.scheme_str() == Some("https")
        && uri.authority().is_some_and(|authority| {
            !authority.as_str().contains('@') && !authority.host().is_empty()
        })
        && uri.query().is_none()
        && !issuer.chars().any(char::is_control);
    let audiences_valid = !audiences.is_empty()
        && audiences.iter().all(|audience| {
            !audience.is_empty()
                && audience.len() <= 128
                && audience.bytes().all(|byte| {
                    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':' | b'/')
                })
        });
    issuer_valid && audiences_valid && oidc_client_id_pattern(audiences).len() <= 128
}

fn oidc_client_id_pattern(audiences: &[String]) -> String {
    let escaped = audiences
        .iter()
        .map(|audience| {
            audience
                .chars()
                .flat_map(|character| {
                    if matches!(character, '.' | '-' | '/') {
                        vec!['\\', character]
                    } else {
                        vec![character]
                    }
                })
                .collect::<String>()
        })
        .collect::<Vec<_>>();
    format!("^({})$", escaped.join("|"))
}

pub(crate) fn sam_logical_id(value: &str) -> String {
    let mut output = String::new();
    let mut uppercase = true;
    for character in value.chars() {
        if character.is_ascii_alphanumeric() {
            if uppercase {
                output.push(character.to_ascii_uppercase());
                uppercase = false;
            } else {
                output.push(character);
            }
        } else {
            uppercase = true;
        }
    }
    output
}

pub use cost::{
    CostClass, CostComponent, CostEvidence, DatabaseCostEstimate, PricingConfidence,
    QueueCostDimension, RealtimeCostDimension, RuntimeCostEstimate, ScheduleCostDimension,
    SqsMappingCostDimension, WorkerCostDimension, estimate_database_cost, estimate_runtime_cost,
};
pub use model::{
    AuthPlan, CostPolicy, DatabaseDeployment, DeploymentConfig, DeploymentPlan,
    DynamoDbDeletionPolicy, DynamoDbGlobalSecondaryIndex, DynamoDbKeyAttribute, DynamoDbProjection,
    DynamoDbScalarType, DynamoDbTablePlan, FunctionPlan, FunctionRole, IamIntent, IamResource,
    IngressPlan, NeonPlan, PerformancePolicy, PlanDiagnostic, PlanError, PreviewCleanupSchedule,
    PreviewLifecyclePlan, PreviewResource, PreviewResourceRetention, QueuePlan, RealtimeDeployment,
    RoutePlan, RuntimePlan, ScheduleCleanupPlan, ScheduleCompletionAction, Severity,
    StaticSiteDeployment, TriggerPlan,
};
pub use sam_cross_client::{render_sam, render_sam_with_code_uri, render_sam_with_code_uris};
