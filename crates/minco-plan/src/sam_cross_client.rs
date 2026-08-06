use crate::{DeploymentPlan, PlanError, sam};
use std::{collections::BTreeMap, fmt::Write as _};

const STANDARD_REQUEST_HEADERS: [&str; 2] = ["if-match", "if-none-match"];
const STANDARD_EXPOSED_HEADERS: [&str; 8] = [
    "deprecation",
    "etag",
    "link",
    "location",
    "retry-after",
    "sunset",
    "www-authenticate",
    "x-request-id",
];

pub fn render_sam(plan: &DeploymentPlan) -> Result<String, PlanError> {
    let plan = normalized_plan(plan);
    inject_exposed_headers(sam::render_sam(&plan)?)
}

pub fn render_sam_with_code_uri(
    plan: &DeploymentPlan,
    code_uri: Option<&str>,
) -> Result<String, PlanError> {
    let plan = normalized_plan(plan);
    inject_exposed_headers(sam::render_sam_with_code_uri(&plan, code_uri)?)
}

pub fn render_sam_with_code_uris(
    plan: &DeploymentPlan,
    code_uris: &BTreeMap<String, String>,
) -> Result<String, PlanError> {
    let plan = normalized_plan(plan);
    inject_exposed_headers(sam::render_sam_with_code_uris(&plan, code_uris)?)
}

fn normalized_plan(plan: &DeploymentPlan) -> DeploymentPlan {
    let mut plan = plan.clone();
    normalize_allowed_headers(&mut plan.allowed_headers);
    plan
}

fn normalize_allowed_headers(headers: &mut Vec<String>) {
    for required in STANDARD_REQUEST_HEADERS {
        if !headers
            .iter()
            .any(|configured| configured.eq_ignore_ascii_case(required))
        {
            headers.push(required.to_owned());
        }
    }
    headers.sort_by_cached_key(|header| header.to_ascii_lowercase());
}

fn inject_exposed_headers(mut rendered: String) -> Result<String, PlanError> {
    if rendered.contains("        ExposeHeaders:\n") {
        return Ok(rendered);
    }
    let marker = "        AllowOrigins:\n";
    let Some(index) = rendered.find(marker) else {
        return Err(PlanError::UnsupportedDeployment(
            "SAM renderer did not produce the expected HTTP API CORS origin block".into(),
        ));
    };
    let mut block = String::from("        ExposeHeaders:\n");
    for header in STANDARD_EXPOSED_HEADERS {
        writeln!(block, "          - '{header}'").expect("writing to String cannot fail");
    }
    rendered.insert_str(index, &block);
    Ok(rendered)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        AuthPlan, CostPolicy, DatabaseDeployment, FunctionPlan, FunctionRole, IngressPlan,
        NeonPlan, PerformancePolicy, RoutePlan, RuntimePlan,
    };
    use minco_contract::HttpMethod;

    fn minimal_http_plan() -> DeploymentPlan {
        DeploymentPlan {
            schema_version: 1,
            application: "demo".into(),
            environment: "dev".into(),
            region: "ap-southeast-2".into(),
            runtime: RuntimePlan::LambdaZipArm64,
            ingress: IngressPlan::ApiGatewayHttpApi,
            auth: AuthPlan::Jwt {
                issuer: "https://issuer.example.invalid".into(),
                audiences: vec!["orders".into()],
            },
            database: DatabaseDeployment::NeonPostgres {
                plan: NeonPlan::Free,
                compute_unit_hours: 1.0,
                storage_gb_month: 0.1,
                history_storage_gb_month: 0.0,
            },
            functions: vec![FunctionPlan {
                name: "api".into(),
                role: FunctionRole::HttpApi,
                artifact_path: "artifact.zip".into(),
                memory_mb: 512,
                timeout_seconds: 15,
                reserved_concurrency: 2,
                provisioned_concurrency: 0,
                database_connections_per_instance: 2,
            }],
            queues: Vec::new(),
            triggers: Vec::new(),
            iam_intents: Vec::new(),
            routes: vec![RoutePlan {
                operation_id: "getOrder".into(),
                method: HttpMethod::Get,
                path: "/orders/{orderId}".into(),
                authenticated: true,
            }],
            application_graph: minco_core::ApplicationGraph::default(),
            static_site: None,
            realtime: None,
            preview: None,
            local_aws_services: vec!["ssm".into(), "sts".into()],
            scheduled_wakeups: Vec::new(),
            uses_nat_gateway: false,
            allowed_origins: vec!["https://app.example.invalid".into()],
            allowed_headers: vec![
                "authorization".into(),
                "content-type".into(),
                "idempotency-key".into(),
                "x-request-id".into(),
            ],
            log_retention_days: 14,
            cost_policy: CostPolicy::default(),
            performance_policy: PerformancePolicy::default(),
        }
    }

    #[test]
    fn rendered_sam_contains_authoritative_cross_client_cors() {
        let yaml = render_sam(&minimal_http_plan()).unwrap();

        assert!(yaml.contains("          - 'if-match'\n"));
        assert!(yaml.contains("          - 'if-none-match'\n"));
        assert!(yaml.contains("        ExposeHeaders:\n"));
        for header in STANDARD_EXPOSED_HEADERS {
            assert!(yaml.contains(&format!("          - '{header}'\n")));
        }
    }

    #[test]
    fn required_request_headers_are_added_once_and_sorted() {
        let mut headers = vec![
            "x-request-id".to_owned(),
            "IF-MATCH".to_owned(),
            "authorization".to_owned(),
        ];
        normalize_allowed_headers(&mut headers);

        assert_eq!(
            headers,
            ["authorization", "IF-MATCH", "if-none-match", "x-request-id"]
        );
    }

    #[test]
    fn exposed_headers_are_inserted_before_exact_origins() {
        let rendered = "      CorsConfiguration:\n        AllowMethods: [GET]\n        AllowHeaders:\n          - 'authorization'\n        AllowOrigins:\n          - 'https://app.example.invalid'\n";
        let rendered = inject_exposed_headers(rendered.to_owned()).unwrap();

        let exposed = rendered.find("        ExposeHeaders:\n").unwrap();
        let origins = rendered.find("        AllowOrigins:\n").unwrap();
        assert!(exposed < origins);
        for header in STANDARD_EXPOSED_HEADERS {
            assert!(rendered.contains(&format!("          - '{header}'\n")));
        }
    }
}
