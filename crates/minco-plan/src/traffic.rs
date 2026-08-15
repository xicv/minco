use crate::{
    model::{DeploymentPlan, IngressPlan},
    sam,
};
use minco_contract::HttpMethod;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;

/// One API Gateway token-bucket target.
///
/// API Gateway documents both values as best-effort throttling targets rather
/// than hard ceilings. The policy intentionally contains no identity key,
/// distributed counter, or application-side request state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrafficBudget {
    /// Steady-state request target in requests per second.
    pub rate_per_second: f64,
    /// Token-bucket burst target. API Gateway exposes this as an int32.
    pub burst: i32,
}

impl TrafficBudget {
    #[must_use]
    pub const fn new(rate_per_second: f64, burst: i32) -> Self {
        Self {
            rate_per_second,
            burst,
        }
    }

    fn validate(&self, target: &str) -> Result<(), HttpTrafficPolicyError> {
        if !self.rate_per_second.is_finite() || self.rate_per_second <= 0.0 {
            return Err(HttpTrafficPolicyError::InvalidBudget {
                target: target.to_owned(),
                reason: "rate_per_second must be finite and greater than zero",
            });
        }
        if self.burst <= 0 {
            return Err(HttpTrafficPolicyError::InvalidBudget {
                target: target.to_owned(),
                reason: "burst must be greater than zero and fit API Gateway's int32 field",
            });
        }
        Ok(())
    }
}

/// Explicit API Gateway HTTP API traffic policy.
///
/// `default` applies to every route unless an operation-specific override is
/// present. Override keys are canonical Minco/OpenAPI operation IDs rather than
/// duplicated method/path strings, keeping application contracts authoritative.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HttpTrafficPolicy {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<TrafficBudget>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub operations: BTreeMap<String, TrafficBudget>,
}

impl HttpTrafficPolicy {
    #[must_use]
    pub fn new(default: Option<TrafficBudget>) -> Self {
        Self {
            default,
            operations: BTreeMap::new(),
        }
    }

    #[must_use]
    pub fn with_operation(
        mut self,
        operation_id: impl Into<String>,
        budget: TrafficBudget,
    ) -> Self {
        self.operations.insert(operation_id.into(), budget);
        self
    }

    /// Validates policy values and resolves every operation override against
    /// the already-reviewed deployment routes.
    pub fn validate(&self, plan: &DeploymentPlan) -> Result<(), HttpTrafficPolicyError> {
        if !matches!(&plan.ingress, IngressPlan::ApiGatewayHttpApi) {
            return Err(HttpTrafficPolicyError::UnsupportedIngress);
        }
        if self.default.is_none() && self.operations.is_empty() {
            return Err(HttpTrafficPolicyError::EmptyPolicy);
        }
        if let Some(default) = &self.default {
            default.validate("default")?;
        }

        let routes = plan
            .routes
            .iter()
            .map(|route| (route.operation_id.as_str(), route))
            .collect::<BTreeMap<_, _>>();
        let mut route_keys = BTreeSet::new();
        for (operation_id, budget) in &self.operations {
            budget.validate(operation_id)?;
            let route = routes
                .get(operation_id.as_str())
                .ok_or_else(|| HttpTrafficPolicyError::UnknownOperation(operation_id.clone()))?;
            let route_key = route_key(route.method, &route.path);
            if !route_keys.insert(route_key.clone()) {
                return Err(HttpTrafficPolicyError::DuplicateRouteKey(route_key));
            }
        }
        Ok(())
    }

    fn resolved_overrides<'a>(
        &'a self,
        plan: &'a DeploymentPlan,
    ) -> Result<Vec<(String, &'a TrafficBudget)>, HttpTrafficPolicyError> {
        let routes = plan
            .routes
            .iter()
            .map(|route| (route.operation_id.as_str(), route))
            .collect::<BTreeMap<_, _>>();
        let mut resolved = self
            .operations
            .iter()
            .map(|(operation_id, budget)| {
                let route = routes
                    .get(operation_id.as_str())
                    .ok_or_else(|| HttpTrafficPolicyError::UnknownOperation(operation_id.clone()))?;
                Ok((route_key(route.method, &route.path), budget))
            })
            .collect::<Result<Vec<_>, HttpTrafficPolicyError>>()?;
        resolved.sort_by(|left, right| left.0.cmp(&right.0));
        Ok(resolved)
    }
}

/// Renders the existing Minco SAM topology plus explicit API Gateway traffic
/// targets for both the default and candidate stages.
pub fn render_sam_with_traffic_policy(
    plan: &DeploymentPlan,
    policy: &HttpTrafficPolicy,
) -> Result<String, HttpTrafficPolicyError> {
    policy.validate(plan)?;
    let template = sam::render_sam(plan)?;
    apply_policy(template, plan, policy)
}

/// Traffic-aware equivalent of [`crate::render_sam_with_code_uri`].
pub fn render_sam_with_code_uri_and_traffic_policy(
    plan: &DeploymentPlan,
    code_uri: Option<&str>,
    policy: &HttpTrafficPolicy,
) -> Result<String, HttpTrafficPolicyError> {
    policy.validate(plan)?;
    let template = sam::render_sam_with_code_uri(plan, code_uri)?;
    apply_policy(template, plan, policy)
}

/// Traffic-aware equivalent of [`crate::render_sam_with_code_uris`].
pub fn render_sam_with_code_uris_and_traffic_policy(
    plan: &DeploymentPlan,
    code_uris: &BTreeMap<String, String>,
    policy: &HttpTrafficPolicy,
) -> Result<String, HttpTrafficPolicyError> {
    policy.validate(plan)?;
    let template = sam::render_sam_with_code_uris(plan, code_uris)?;
    apply_policy(template, plan, policy)
}

fn apply_policy(
    template: String,
    plan: &DeploymentPlan,
    policy: &HttpTrafficPolicy,
) -> Result<String, HttpTrafficPolicyError> {
    let stage_settings = render_stage_settings(plan, policy)?;
    let template = insert_after_once(
        template,
        "      StageName: '$default'\n",
        &stage_settings,
        "$default",
    )?;
    insert_after_once(
        template,
        "      StageName: candidate\n",
        &stage_settings,
        "candidate",
    )
}

fn render_stage_settings(
    plan: &DeploymentPlan,
    policy: &HttpTrafficPolicy,
) -> Result<String, HttpTrafficPolicyError> {
    let mut output = String::new();
    if let Some(default) = &policy.default {
        output.push_str("      DefaultRouteSettings:\n");
        render_budget(&mut output, "        ", default);
    }
    let overrides = policy.resolved_overrides(plan)?;
    if !overrides.is_empty() {
        output.push_str("      RouteSettings:\n");
        for (route_key, budget) in overrides {
            let route_key = serde_json::to_string(&route_key)
                .expect("route key serialization to JSON string cannot fail");
            output.push_str("        ");
            output.push_str(&route_key);
            output.push_str(":\n");
            render_budget(&mut output, "          ", budget);
        }
    }
    Ok(output)
}

fn render_budget(output: &mut String, indent: &str, budget: &TrafficBudget) {
    use std::fmt::Write as _;
    writeln!(output, "{indent}ThrottlingBurstLimit: {}", budget.burst)
        .expect("writing to String cannot fail");
    writeln!(
        output,
        "{indent}ThrottlingRateLimit: {}",
        budget.rate_per_second
    )
    .expect("writing to String cannot fail");
}

fn insert_after_once(
    mut template: String,
    marker: &str,
    insertion: &str,
    stage: &'static str,
) -> Result<String, HttpTrafficPolicyError> {
    let Some(index) = template.find(marker) else {
        return Err(HttpTrafficPolicyError::TemplateShape(stage));
    };
    let position = index + marker.len();
    template.insert_str(position, insertion);
    Ok(template)
}

fn route_key(method: HttpMethod, path: &str) -> String {
    format!("{} {path}", method.as_str())
}

#[derive(Debug, Error)]
pub enum HttpTrafficPolicyError {
    #[error("API Gateway HTTP traffic policy cannot be empty")]
    EmptyPolicy,
    #[error("API Gateway HTTP traffic policy requires api_gateway_http_api ingress")]
    UnsupportedIngress,
    #[error("traffic budget for {target} is invalid: {reason}")]
    InvalidBudget {
        target: String,
        reason: &'static str,
    },
    #[error("traffic policy references unknown operation {0}")]
    UnknownOperation(String),
    #[error("traffic policy resolves multiple operation overrides to route {0}")]
    DuplicateRouteKey(String),
    #[error("rendered SAM template is missing the expected {0} stage marker")]
    TemplateShape(&'static str),
    #[error(transparent)]
    Plan(#[from] crate::model::PlanError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        AuthPlan, CostPolicy, DatabaseDeployment, DeploymentConfig, FunctionPlan, FunctionRole,
        NeonPlan, PerformancePolicy, RoutePlan, RuntimePlan,
    };
    use minco_contract::{ContractDocument, OwnedOperation};

    fn plan() -> DeploymentPlan {
        let contract = ContractDocument {
            source: "inline".into(),
            openapi_version: "3.1.0".into(),
            title: "traffic-test".into(),
            version: "1".into(),
            sha256: "hash".into(),
            operations: vec![
                OwnedOperation {
                    operation_id: "getHealth".into(),
                    method: HttpMethod::Get,
                    path: "/health".into(),
                    authenticated: false,
                    idempotent: false,
                },
                OwnedOperation {
                    operation_id: "createOrder".into(),
                    method: HttpMethod::Post,
                    path: "/orders".into(),
                    authenticated: true,
                    idempotent: true,
                },
            ],
            schema_names: Vec::new(),
            raw: serde_json::json!({}),
        };
        DeploymentConfig {
            schema_version: 1,
            application: "traffic-test".into(),
            environment: "dev".into(),
            region: "ap-southeast-2".into(),
            runtime: RuntimePlan::LambdaZipArm64,
            ingress: IngressPlan::ApiGatewayHttpApi,
            auth: AuthPlan::Jwt {
                issuer: "https://issuer.example.invalid".into(),
                audiences: vec!["traffic-test".into()],
            },
            database: DatabaseDeployment::NeonPostgres {
                plan: NeonPlan::Free,
                compute_unit_hours: 0.0,
                storage_gb_month: 0.0,
                history_storage_gb_month: 0.0,
            },
            functions: vec![FunctionPlan {
                name: "api".into(),
                role: FunctionRole::HttpApi,
                artifact_path: "target/lambda/traffic/bootstrap.zip".into(),
                memory_mb: 256,
                timeout_seconds: 10,
                reserved_concurrency: 2,
                provisioned_concurrency: 0,
                database_connections_per_instance: 1,
            }],
            queues: Vec::new(),
            triggers: Vec::new(),
            scheduled_wakeups: Vec::new(),
            uses_nat_gateway: false,
            allowed_origins: vec!["https://app.example.invalid".into()],
            allowed_headers: vec!["authorization".into(), "content-type".into()],
            log_retention_days: 14,
            cost_policy: CostPolicy::default(),
            performance_policy: PerformancePolicy::default(),
        }
        .into_plan(&contract)
    }

    #[test]
    fn traffic_policy_renders_the_same_default_and_route_limits_on_both_stages() {
        let policy = HttpTrafficPolicy::new(Some(TrafficBudget::new(20.0, 40)))
            .with_operation("createOrder", TrafficBudget::new(2.5, 5));
        let template = render_sam_with_traffic_policy(&plan(), &policy).unwrap();

        assert_eq!(template.matches("DefaultRouteSettings:\n").count(), 2);
        assert_eq!(template.matches("ThrottlingBurstLimit: 40").count(), 2);
        assert_eq!(template.matches("ThrottlingRateLimit: 20").count(), 2);
        assert_eq!(template.matches("\"POST /orders\":\n").count(), 2);
        assert_eq!(template.matches("ThrottlingBurstLimit: 5").count(), 2);
        assert_eq!(template.matches("ThrottlingRateLimit: 2.5").count(), 2);

        let parsed: serde_yaml_ng::Value = serde_yaml_ng::from_str(&template).unwrap();
        for stage in ["HttpApi", "CandidateStage"] {
            let properties = &parsed["Resources"][stage]["Properties"];
            assert_eq!(
                properties["DefaultRouteSettings"]["ThrottlingBurstLimit"].as_i64(),
                Some(40)
            );
            assert_eq!(
                properties["DefaultRouteSettings"]["ThrottlingRateLimit"].as_f64(),
                Some(20.0)
            );
            assert_eq!(
                properties["RouteSettings"]["POST /orders"]["ThrottlingBurstLimit"].as_i64(),
                Some(5)
            );
            assert_eq!(
                properties["RouteSettings"]["POST /orders"]["ThrottlingRateLimit"].as_f64(),
                Some(2.5)
            );
        }
    }

    #[test]
    fn ordinary_sam_rendering_remains_unthrottled() {
        let template = sam::render_sam(&plan()).unwrap();

        assert!(!template.contains("DefaultRouteSettings:"));
        assert!(!template.contains("RouteSettings:"));
        assert!(!template.contains("ThrottlingBurstLimit:"));
        assert!(!template.contains("ThrottlingRateLimit:"));
    }

    #[test]
    fn unknown_operation_fails_closed_before_rendering() {
        let policy = HttpTrafficPolicy::default()
            .with_operation("missingOperation", TrafficBudget::new(1.0, 1));

        assert!(matches!(
            render_sam_with_traffic_policy(&plan(), &policy),
            Err(HttpTrafficPolicyError::UnknownOperation(operation))
                if operation == "missingOperation"
        ));
    }

    #[test]
    fn invalid_numeric_budgets_fail_closed() {
        for budget in [
            TrafficBudget::new(0.0, 1),
            TrafficBudget::new(f64::NAN, 1),
            TrafficBudget::new(1.0, 0),
            TrafficBudget::new(1.0, -1),
        ] {
            let policy = HttpTrafficPolicy::new(Some(budget));
            assert!(matches!(
                policy.validate(&plan()),
                Err(HttpTrafficPolicyError::InvalidBudget { .. })
            ));
        }
    }

    #[test]
    fn maximum_provider_burst_value_is_renderable() {
        let policy = HttpTrafficPolicy::new(Some(TrafficBudget::new(1.0, i32::MAX)));
        let template = render_sam_with_traffic_policy(&plan(), &policy).unwrap();

        assert!(template.contains("ThrottlingBurstLimit: 2147483647"));
    }

    #[test]
    fn policy_rejects_non_api_gateway_ingress() {
        let mut plan = plan();
        plan.ingress = IngressPlan::LocalTcp;
        let policy = HttpTrafficPolicy::new(Some(TrafficBudget::new(10.0, 10)));

        assert!(matches!(
            policy.validate(&plan),
            Err(HttpTrafficPolicyError::UnsupportedIngress)
        ));
    }

    #[test]
    fn duplicate_route_keys_do_not_silently_overwrite_settings() {
        let mut plan = plan();
        plan.routes.push(RoutePlan {
            operation_id: "createOrderAgain".into(),
            method: HttpMethod::Post,
            path: "/orders".into(),
            authenticated: true,
        });
        let policy = HttpTrafficPolicy::default()
            .with_operation("createOrder", TrafficBudget::new(2.0, 2))
            .with_operation("createOrderAgain", TrafficBudget::new(3.0, 3));

        assert!(matches!(
            policy.validate(&plan),
            Err(HttpTrafficPolicyError::DuplicateRouteKey(route)) if route == "POST /orders"
        ));
    }
}
