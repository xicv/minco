//! Renders a durable-work SAM template artifact for external validation.
//!
//! The template is rendered from the public sidecar API into a temporary
//! directory outside the repository so `sam validate` can check the exact
//! structure callers produce without tracking generated build output.

use minco_plan::durable_work::{
    DurableWorkTopology, JobRoutePlan, JobSchedulePlan, WorkerProfilePlan, apply_durable_work,
    render_sam_with_durable_work,
};
use minco_plan::{DeploymentConfig, DeploymentPlan};

fn config() -> DeploymentConfig {
    toml::from_str(
        r#"
schema_version = 2
application = "orders"
environment = "dev"
region = "ap-southeast-2"
runtime = "lambda_zip_arm64"
ingress = "api_gateway_http_api"
allowed_origins = ["https://orders.example.com"]
scheduled_wakeups = []
uses_nat_gateway = false

[auth]
kind = "none"

[database]
kind = "neon_postgres"
plan = "free"
compute_unit_hours = 0.0
storage_gb_month = 0.0
history_storage_gb_month = 0.0

[[functions]]
name = "api"
role = "http_api"
artifact_path = "target/lambda/api.zip"
memory_mb = 512
timeout_seconds = 15
reserved_concurrency = 2
provisioned_concurrency = 0
database_connections_per_instance = 2
"#,
    )
    .expect("deployment config")
}

fn contract() -> minco_contract::ContractDocument {
    use minco_contract::{HttpMethod, OwnedOperation};
    minco_contract::ContractDocument {
        source: "inline".into(),
        openapi_version: "3.1.0".into(),
        title: "orders".into(),
        version: "1".into(),
        sha256: "hash".into(),
        operations: vec![OwnedOperation {
            operation_id: "createOrder".into(),
            method: HttpMethod::Post,
            path: "/orders".into(),
            authenticated: true,
            idempotent: true,
        }],
        schema_names: Vec::new(),
        raw: serde_json::json!({}),
    }
}

fn topology() -> DurableWorkTopology {
    DurableWorkTopology {
        enabled: true,
        profiles: vec![WorkerProfilePlan {
            id: "orders-notifications".into(),
            queue_id: "jobs-orders-notifications".into(),
            function_id: "orders-jobs-worker".into(),
            artifact_path: "target/lambda/orders-jobs-worker.zip".into(),
            fifo: false,
            batch_size: 10,
            batching_window_seconds: 1,
            maximum_concurrency: 2,
            memory_mb: 512,
            timeout_seconds: 30,
            reserved_concurrency: 2,
            max_payload_bytes: 262_143,
            database_connections_per_instance: 1,
            dead_letter_queue_id: None,
            max_receive_count: None,
            data_classes: vec!["internal".into()],
            required_capabilities: vec!["notifications.send".into()],
        }],
        routes: vec![JobRoutePlan {
            job_name: "orders.send-confirmation".into(),
            job_version: 1,
            worker_profile: "orders-notifications".into(),
            ordering_source: None,
        }],
        schedules: vec![JobSchedulePlan {
            id: "orders-expiry".into(),
            job_name: "orders.expire-unpaid".into(),
            job_version: 1,
            worker_profile: "orders-notifications".into(),
            payload: serde_json::json!({ "older_than_hours": 24 }),
            expression: "rate(1 hours)".into(),
            enabled: true,
            purpose: "Expire unpaid orders".into(),
            timezone: Some("Pacific/Auckland".into()),
            flexible_window_minutes: Some(15),
            maximum_retry_attempts: Some(3),
            dead_letter_queue_id: None,
        }],
    }
}

#[test]
fn render_durable_work_template_artifact() {
    let topology = topology();
    let plan: DeploymentPlan =
        config().into_plan_with_graph(&contract(), minco_core::ApplicationGraph::default());
    let applied = apply_durable_work(&plan, &topology);
    let mut code_uris = std::collections::BTreeMap::new();
    code_uris.insert("api".to_owned(), "./api.zip".to_owned());
    code_uris.insert(
        "orders-jobs-worker".to_owned(),
        "./orders-jobs-worker.zip".to_owned(),
    );
    let template = render_sam_with_durable_work(&applied, &topology, &code_uris).expect("render");
    let output = std::env::temp_dir().join(format!(
        "minco-durable-work-template-{}.yaml",
        std::process::id()
    ));
    std::fs::write(&output, &template).expect("write template");
}
