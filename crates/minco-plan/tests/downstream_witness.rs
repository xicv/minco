//! Downstream compatibility witness (exact-head review 5060065907).
//!
//! An integration test is a separate crate: this file constructs
//! `DeploymentPlan` with the FULL published struct literal — exactly
//! the v1.12 public field set, with no `inbound_mail` — and compiles.
//! If the plan ever gains (or re-gains) a public field, this witness
//! stops compiling, which is precisely the downstream break
//! cargo-semver-checks guards against.

use minco_contract::HttpMethod;
use minco_plan::{
    AuthPlan, CostPolicy, DatabaseDeployment, DeploymentPlan, FunctionPlan, FunctionRole,
    IngressPlan, NeonPlan, PerformancePolicy, RoutePlan, RuntimePlan,
};

fn v1_12_struct_literal_plan() -> DeploymentPlan {
    DeploymentPlan {
        schema_version: 1,
        application: "witness".into(),
        environment: "dev".into(),
        region: "ap-southeast-2".into(),
        runtime: RuntimePlan::LambdaZipArm64,
        ingress: IngressPlan::ApiGatewayHttpApi,
        auth: AuthPlan::Jwt {
            issuer: "https://issuer.example.invalid".into(),
            audiences: vec!["witness".into()],
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
            operation_id: "getHealth".into(),
            method: HttpMethod::Get,
            path: "/health".into(),
            authenticated: false,
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
            "if-match".into(),
            "if-none-match".into(),
            "x-request-id".into(),
        ],
        exposed_headers: vec![
            "deprecation".into(),
            "etag".into(),
            "link".into(),
            "location".into(),
            "retry-after".into(),
            "sunset".into(),
            "www-authenticate".into(),
            "x-request-id".into(),
        ],
        log_retention_days: 14,
        cost_policy: CostPolicy::default(),
        performance_policy: PerformancePolicy::default(),
    }
}

#[test]
fn downstream_v1_12_struct_literal_still_compiles() {
    let plan = v1_12_struct_literal_plan();
    assert_eq!(plan.application, "witness");
    // The inbound-mail sidecar remains an explicit, additive API on top
    // of the unchanged plan shape.
    let topology = minco_plan::inbound_mail::InboundMailTopology::default();
    let applied = minco_plan::inbound_mail::apply_inbound_mail(&plan, &topology);
    assert_eq!(
        applied, plan,
        "a disabled sidecar leaves the plan untouched"
    );
}
