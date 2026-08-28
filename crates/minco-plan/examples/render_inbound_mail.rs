//! Renders the inbound-mail SAM template for the structural-parse gate
//! (exact-head review R14): prints the exact template
//! `render_sam_with_inbound_mail` produces for a representative plan so
//! `scripts/test/inbound_mail_template_parse.py` can parse the complete
//! document as YAML.
use minco_plan::inbound_mail::{
    InboundMailTopology, apply_inbound_mail, render_sam_with_inbound_mail,
};
use minco_plan::{DeploymentConfig, DeploymentPlan, InboundMailBinding};

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

[[functions]]
name = "mail-worker"
role = "worker"
artifact_path = "target/lambda/mail-worker.zip"
memory_mb = 256
timeout_seconds = 60
reserved_concurrency = 2
provisioned_concurrency = 0
database_connections_per_instance = 1
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

fn main() {
    let plan: DeploymentPlan =
        config().into_plan_with_graph(&contract(), minco_core::ApplicationGraph::default());
    let topology = InboundMailTopology {
        enabled: true,
        bindings: vec![InboundMailBinding {
            id: "ticketing".into(),
            mailbox_scope: "support@example.test".into(),
            bucket_name: "orders-dev-raw-mail".into(),
            key_prefix: "mail/".into(),
            retention_days: 30,
            worker_function_id: "mail-worker".into(),
            queue_id: "mail-ticketing".into(),
            batch_size: 10,
            batching_window_seconds: 1,
            maximum_concurrency: 2,
        }],
    };
    let applied = apply_inbound_mail(&plan, &topology);
    let mut code_uris = std::collections::BTreeMap::new();
    code_uris.insert("api".to_owned(), "./api.zip".to_owned());
    code_uris.insert("mail-worker".to_owned(), "./mail-worker.zip".to_owned());
    let template = render_sam_with_inbound_mail(&applied, &topology, &code_uris)
        .expect("render the inbound-mail template");
    print!("{template}");
}
