//! Provider-neutral deployment plan, database profiles, structural policy checks and cost estimation.
#![forbid(unsafe_code)]

mod cost;
mod model;
mod sam;

pub use cost::{estimate_database_cost, CostComponent, DatabaseCostEstimate};
pub use model::{
    AuthPlan, CostPolicy, DatabaseDeployment, DeploymentConfig, DeploymentPlan, FunctionPlan, IngressPlan, NeonPlan,
    PerformancePolicy, PlanDiagnostic, PlanError, RoutePlan, RuntimePlan, Severity,
};
pub use sam::render_sam;
