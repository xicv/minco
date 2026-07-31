//! Provider-neutral deployment plan, database profiles, structural policy checks and cost estimation.
#![forbid(unsafe_code)]

mod cost;
mod model;
mod sam;

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
    QueueCostDimension, RuntimeCostEstimate, ScheduleCostDimension, SqsMappingCostDimension,
    WorkerCostDimension, estimate_database_cost, estimate_runtime_cost,
};
pub use model::{
    AuthPlan, CostPolicy, DatabaseDeployment, DeploymentConfig, DeploymentPlan, FunctionPlan,
    FunctionRole, IamIntent, IamResource, IngressPlan, NeonPlan, PerformancePolicy, PlanDiagnostic,
    PlanError, QueuePlan, RoutePlan, RuntimePlan, ScheduleCleanupPlan, ScheduleCompletionAction,
    Severity, TriggerPlan,
};
pub use sam::{render_sam, render_sam_with_code_uri, render_sam_with_code_uris};
