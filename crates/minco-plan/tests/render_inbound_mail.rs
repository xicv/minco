//! Inbound-mail sidecar: synthesis, validation and SAM rendering tests
//! (ADR-0065). The rendered template is also written to a temporary file
//! outside the repository so `sam validate --lint` can check the exact
//! artifact callers produce.

use minco_plan::inbound_mail::{
    InboundMailTopology, apply_inbound_mail, estimate_inbound_mail_cost,
    render_sam_with_inbound_mail, validate_inbound_mail,
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

fn binding() -> InboundMailBinding {
    InboundMailBinding {
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
    }
}

fn topology() -> InboundMailTopology {
    InboundMailTopology {
        enabled: true,
        bindings: vec![binding()],
    }
}

fn plan() -> DeploymentPlan {
    config().into_plan_with_graph(&contract(), minco_core::ApplicationGraph::default())
}

#[test]
fn synthesis_adds_wake_queue_trigger_and_binding() {
    let applied = apply_inbound_mail(&plan(), &topology());
    assert_eq!(applied.inbound_mail.len(), 1);
    assert!(
        applied
            .queues
            .iter()
            .any(|queue| queue.id == "mail-ticketing")
    );
    // Review finding 5: the wake queue carries a dead-letter queue and a
    // bounded max-receive count, so exhausted notifications are
    // inspectable instead of silently lost.
    let wake = applied
        .queues
        .iter()
        .find(|queue| queue.id == "mail-ticketing")
        .expect("wake queue");
    assert_eq!(
        wake.dead_letter_queue_id.as_deref(),
        Some("mail-ticketing-dlq")
    );
    assert_eq!(
        wake.max_receive_count,
        Some(minco_plan::inbound_mail::WAKE_MAX_RECEIVE_COUNT)
    );
    assert!(
        applied
            .queues
            .iter()
            .any(|queue| queue.id == "mail-ticketing-dlq")
    );
    assert!(applied.triggers.iter().any(|trigger| matches!(
        trigger,
        minco_plan::TriggerPlan::Sqs { id, function_id, queue_id, report_batch_item_failures, .. }
            if id == "ticketing-mail"
                && function_id == "mail-worker"
                && queue_id == "mail-ticketing"
                && *report_batch_item_failures
    )));
    // Applying twice is stable: no duplicate queues or triggers.
    let twice = apply_inbound_mail(&applied, &topology());
    assert_eq!(twice.queues, applied.queues);
    assert_eq!(twice.triggers, applied.triggers);
}

#[test]
fn validation_accepts_a_sound_binding_and_rejects_broken_ones() {
    let applied = apply_inbound_mail(&plan(), &topology());
    assert!(validate_inbound_mail(&applied, &topology()).is_empty());

    let unknown_worker = InboundMailTopology {
        enabled: true,
        bindings: vec![InboundMailBinding {
            worker_function_id: "ghost".into(),
            ..binding()
        }],
    };
    assert!(
        validate_inbound_mail(&applied, &unknown_worker)
            .into_iter()
            .any(|finding| finding.code == "MINCO-MAIL-004")
    );

    let mut shared_queue = binding();
    shared_queue.id = "ticketing-2".into();
    shared_queue.queue_id = binding().queue_id;
    let other = InboundMailTopology {
        enabled: true,
        bindings: vec![binding(), shared_queue],
    };
    assert!(
        validate_inbound_mail(&applied, &other)
            .into_iter()
            .any(|finding| finding.code == "MINCO-MAIL-003")
    );

    let disabled_with_bindings = InboundMailTopology {
        enabled: false,
        bindings: vec![binding()],
    };
    assert_eq!(
        validate_inbound_mail(&applied, &disabled_with_bindings).len(),
        1
    );
}

#[test]
fn duplicate_binding_ids_fail_closed() {
    let applied = apply_inbound_mail(&plan(), &topology());
    let duplicated = InboundMailTopology {
        enabled: true,
        bindings: vec![binding(), binding()],
    };
    assert!(
        validate_inbound_mail(&applied, &duplicated)
            .into_iter()
            .any(|finding| finding.code == "MINCO-MAIL-002")
    );
}

#[test]
fn invalid_fields_fail_closed() {
    let applied = apply_inbound_mail(&plan(), &topology());
    for broken in [
        InboundMailBinding {
            mailbox_scope: "not-an-address".into(),
            ..binding()
        },
        InboundMailBinding {
            bucket_name: "Bad_Name".into(),
            ..binding()
        },
        InboundMailBinding {
            key_prefix: "mail".into(),
            ..binding()
        },
        InboundMailBinding {
            retention_days: 0,
            ..binding()
        },
    ] {
        let broken_topology = InboundMailTopology {
            enabled: true,
            bindings: vec![broken],
        };
        assert!(
            !validate_inbound_mail(&applied, &broken_topology).is_empty(),
            "broken binding must produce a diagnostic"
        );
    }
}

#[test]
fn cost_assumptions_are_explicit() {
    let assumptions = estimate_inbound_mail_cost(&topology());
    assert_eq!(assumptions.len(), 1);
    assert_eq!(assumptions[0].binding_id, "ticketing");
    assert_eq!(assumptions[0].s3_puts_per_mail, 1);
    assert_eq!(assumptions[0].s3_gets_per_wake, 1);
    assert_eq!(assumptions[0].sqs_sends_per_mail, 1);
    assert_eq!(assumptions[0].retention_days, 30);
    assert!(estimate_inbound_mail_cost(&InboundMailTopology::default()).is_empty());
}

#[test]
fn renders_the_full_provider_chain_into_sam() {
    let topology = topology();
    let applied = apply_inbound_mail(&plan(), &topology);
    let mut code_uris = std::collections::BTreeMap::new();
    code_uris.insert("api".to_owned(), "./api.zip".to_owned());
    code_uris.insert("mail-worker".to_owned(), "./mail-worker.zip".to_owned());
    let template = render_sam_with_inbound_mail(&applied, &topology, &code_uris).expect("render");
    for expected in [
        "TicketingRawMailBucket:\n    Type: AWS::S3::Bucket",
        "Event: s3:ObjectCreated:*",
        "Name: prefix\n                    Value: 'mail/'",
        "ExpirationInDays: 30",
        "AllowSeSInboundWrite",
        "Service: ses.amazonaws.com",
        "Action: s3:PutObject",
        "TicketingMailQueuePolicy:\n    Type: AWS::SQS::QueuePolicy",
        "Service: s3.amazonaws.com",
        "Action: sqs:SendMessage",
        "aws:SourceArn: !GetAtt TicketingRawMailBucket.Arn",
        "InboundMailReceiptRuleSet:\n    Type: AWS::SES::ReceiptRuleSet",
        "TicketingReceiptRule:\n    Type: AWS::SES::ReceiptRule",
        "ScanEnabled: true",
        "ObjectKeyPrefix: 'mail/'",
        // Review finding 5: full mailbox recipient, source-account-bound
        // SES writes, one shared rule set name on every rule.
        "Recipients:\n          - 'support@example.test'",
        "aws:SourceAccount: !Sub '${AWS::AccountId}'",
        "aws:SourceArn: !Sub 'arn:aws:ses:${AWS::Region}:${AWS::AccountId}:receipt-rule-set/Ticketing-inbound-mail-ruleset:receipt-rule/'",
        // Exact-head review R9: TLS required, prefix-scoped writes,
        // deployment-order dependency.
        "TlsPolicy: Require",
        "Resource: !Sub '${TicketingRawMailBucket.Arn}/'mail/'*'",
        "DependsOn: [TicketingMailQueuePolicy]",
        "RuleSetName: 'Ticketing-inbound-mail-ruleset'",
        // Worker wake policy: SQS receive/delete plus raw-object reads.
        "sqs:ReceiveMessage",
        "- s3:GetObject",
        "Resource: !Sub '${TicketingRawMailBucket.Arn}/*'",
    ] {
        assert!(
            template.contains(expected),
            "template is missing: {expected}"
        );
    }
    // The worker never gains write access to the raw bucket: the only
    // s3:PutObject grant names the SES service principal.
    let put_position = template
        .find("Action: s3:PutObject")
        .expect("ses put grant");
    assert!(
        template[..put_position]
            .rsplit_once("Principal:")
            .is_some_and(|(_, tail)| tail.contains("Service: ses.amazonaws.com"))
    );
    let output = std::env::temp_dir().join(format!(
        "minco-inbound-mail-template-{}.yaml",
        std::process::id()
    ));
    std::fs::write(&output, &template).expect("write template");
}

#[test]
fn disabled_topology_leaves_the_template_unchanged() {
    let mut code_uris = std::collections::BTreeMap::new();
    code_uris.insert("api".to_owned(), "./api.zip".to_owned());
    code_uris.insert("mail-worker".to_owned(), "./mail-worker.zip".to_owned());
    let base = minco_plan::render_sam_with_code_uris(&plan(), &code_uris).expect("base");
    let unchanged =
        render_sam_with_inbound_mail(&plan(), &InboundMailTopology::default(), &code_uris)
            .expect("render");
    assert_eq!(base, unchanged);
}
