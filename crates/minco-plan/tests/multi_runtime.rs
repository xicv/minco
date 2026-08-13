use minco_contract::{ContractDocument, HttpMethod, OwnedOperation};
use minco_plan::{
    CostClass, DeploymentConfig, DeploymentPlan, IamResource, IngressPlan, PreviewCleanupSchedule,
    PreviewLifecyclePlan, PreviewResource, PreviewResourceRetention, QueuePlan, RealtimeDeployment,
    RuntimePlan, ScheduleCleanupPlan, ScheduleCompletionAction, Severity, StaticSiteDeployment,
    TriggerPlan, estimate_runtime_cost, render_sam, render_sam_with_code_uris,
};
use std::collections::BTreeMap;

fn standard_worker_plan() -> DeploymentPlan {
    plan_from_config(include_str!("fixtures/api_worker_standard_v2.toml"))
}

#[test]
fn realtime_cost_exposes_connection_minutes_and_five_kib_operation_units() {
    let mut plan = standard_worker_plan();
    plan.realtime = Some(RealtimeDeployment {
        namespace: "orders".into(),
        max_event_bytes: 5 * 1024,
        subscriber_claim: "sub".into(),
    });

    let estimate = estimate_runtime_cost(&plan);
    let realtime = estimate.realtime.expect("realtime cost dimension");

    assert_eq!(realtime.operation_unit_bytes, 5 * 1024);
    assert_eq!(realtime.maximum_units_per_event, 1);
    assert_eq!(realtime.event_operations_usd_per_million, 1);
    assert_eq!(realtime.connection_minutes_cents_per_million, 8);
    assert_eq!(realtime.fixed_monthly_usd, 0);
    assert!(!realtime.sends_client_pings);
    assert!((realtime.estimate_monthly_usd(600_000, 100_000) - 0.148).abs() < 0.000_001);
}

fn preview_lifecycle() -> PreviewLifecyclePlan {
    PreviewLifecyclePlan {
        owner: "team-orders".into(),
        ttl_seconds: 86_400,
        expected_account_id: "111122223333".into(),
        expected_region: "ap-southeast-2".into(),
        resources: vec![
            PreviewResource {
                logical_id: "OrdersApi".into(),
                resource_type: "AWS::ApiGatewayV2::Api".into(),
                retention: PreviewResourceRetention::Delete,
                idle_cost_class: CostClass::RequestOnly,
            },
            PreviewResource {
                logical_id: "StaticSiteBucket".into(),
                resource_type: "AWS::S3::Bucket".into(),
                retention: PreviewResourceRetention::Retain,
                idle_cost_class: CostClass::StorageOnly,
            },
        ],
        pricing_complete: false,
        cleanup_schedule: None,
    }
}

#[test]
fn preview_plan_exposes_bounded_lifecycle_retention_and_incomplete_pricing() {
    let mut plan = standard_worker_plan();
    plan.environment = "preview".into();
    plan.preview = Some(preview_lifecycle());

    let diagnostics = plan.validate();
    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.severity != Severity::Error),
        "{diagnostics:#?}"
    );
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "MINCO-PREVIEW-006" && diagnostic.severity == Severity::Warning
    }));
    assert!(estimate_runtime_cost(&plan).schedules.is_empty());

    let value = serde_json::to_value(&plan).expect("serialize preview plan");
    assert_eq!(value["preview"]["owner"], "team-orders");
    assert_eq!(value["preview"]["ttl_seconds"], 86_400);
    assert_eq!(value["preview"]["resources"][1]["retention"], "retain");
    assert_eq!(value["preview"]["pricing_complete"], false);
}

#[test]
fn opt_in_preview_cleanup_is_a_visible_one_time_scheduled_wakeup() {
    let mut plan = standard_worker_plan();
    plan.environment = "preview".into();
    plan.cost_policy.deny_scheduled_wakeups = false;
    let mut preview = preview_lifecycle();
    preview.cleanup_schedule = Some(PreviewCleanupSchedule {
        expression: "at(2026-08-04T00:00:00)".into(),
        action_after_completion: ScheduleCompletionAction::Delete,
        residual_resources: vec!["StaticSiteBucket".into()],
        manual_fallback: "cargo minco destroy --environment preview --dry-run".into(),
    });
    plan.preview = Some(preview);

    let diagnostics = plan.validate();
    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.severity != Severity::Error),
        "{diagnostics:#?}"
    );
    let estimate = estimate_runtime_cost(&plan);
    assert_eq!(estimate.schedules.len(), 1);
    let cleanup = &estimate.schedules[0];
    assert_eq!(cleanup.trigger_id, "preview-cleanup");
    assert_eq!(cleanup.estimated_monthly_invocations, Some(1));
    assert_eq!(
        cleanup.action_after_completion,
        Some(ScheduleCompletionAction::Delete)
    );
    assert_eq!(cleanup.residual_resources, ["StaticSiteBucket"]);
    assert_eq!(
        cleanup.manual_fallback.as_deref(),
        Some("cargo minco destroy --environment preview --dry-run")
    );
    assert!(estimate.evidence.iter().any(|evidence| {
        evidence.name == "schedule:preview-cleanup"
            && evidence.cost_class == CostClass::ScheduledWakeup
    }));
}

#[test]
fn preview_cleanup_rejects_recurring_or_incomplete_schedule_authority() {
    let mut plan = standard_worker_plan();
    plan.environment = "preview".into();
    plan.cost_policy.deny_scheduled_wakeups = false;
    let mut preview = preview_lifecycle();
    preview.cleanup_schedule = Some(PreviewCleanupSchedule {
        expression: "rate(1 day)".into(),
        action_after_completion: ScheduleCompletionAction::Delete,
        residual_resources: vec!["UnknownBucket".into()],
        manual_fallback: String::new(),
    });
    plan.preview = Some(preview);

    assert!(
        plan.validate()
            .iter()
            .any(|diagnostic| diagnostic.code == "MINCO-PREVIEW-003")
    );
}

fn plan_from_config(source: &str) -> DeploymentPlan {
    let config: DeploymentConfig = toml::from_str(source).expect("deployment config");
    let contract = ContractDocument {
        source: "inline".into(),
        openapi_version: "3.1.0".into(),
        title: "orders".into(),
        version: "1".into(),
        sha256: "hash".into(),
        operations: Vec::new(),
        schema_names: Vec::new(),
        raw: serde_json::json!({}),
    };

    config.into_plan(&contract)
}

#[test]
fn schema_v2_plans_one_api_and_one_explicit_sqs_worker() {
    let plan = standard_worker_plan();

    assert_eq!(plan.local_aws_services, ["sqs", "ssm", "sts"]);
    assert!(
        plan.validate()
            .iter()
            .all(|diagnostic| diagnostic.severity != Severity::Error)
    );
}

#[test]
fn sam_renders_private_static_site_resources_from_explicit_plan() {
    let mut plan = standard_worker_plan();
    plan.static_site = Some(StaticSiteDeployment {
        source_directory: "dist".into(),
        index_document: "index.html".into(),
        spa_fallback: true,
        immutable_cache_seconds: 31_536_000,
        html_cache_seconds: 0,
        price_class: "PriceClass_100".into(),
        ipv6_enabled: true,
        custom_domain: Some("app.example.com".into()),
        manage_dns_alias: true,
    });

    let yaml = render_sam(&plan).expect("static-site SAM");
    assert!(yaml.contains("StaticSiteOriginAccessControl:"));
    assert!(yaml.contains("SigningBehavior: always"));
    assert!(yaml.contains("StaticSiteCachePolicy:"));
    assert!(yaml.contains("CachePolicyId: !Ref StaticSiteCachePolicy"));
    assert!(!yaml.contains("ForwardedValues:"));
    assert!(yaml.contains("StaticSiteCertificateArn:"));
    assert!(yaml.contains("StaticSiteHostedZoneId:"));
    assert!(yaml.contains("StaticSiteDnsIpv6Alias:"));
    assert!(yaml.contains("StaticSiteDistributionId:"));
    let parsed: serde_yaml_ng::Value =
        serde_yaml_ng::from_str(&yaml).expect("syntactically valid static-site SAM YAML");
    assert_eq!(
        parsed["Resources"]["StaticSiteDistribution"]["Type"],
        "AWS::CloudFront::Distribution"
    );
}

#[test]
fn every_openapi_operation_resolves_to_the_single_api_function() {
    let config: DeploymentConfig =
        toml::from_str(include_str!("fixtures/api_worker_standard_v2.toml"))
            .expect("deployment config");
    let contract = ContractDocument {
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
    };

    let plan = config.into_plan(&contract);

    assert_eq!(plan.operation_function_id("createOrder"), Some("api"));
    assert_eq!(plan.operation_function_id("missing"), None);
}

#[test]
fn minco_sqs_workers_require_partial_batch_responses() {
    let mut plan = standard_worker_plan();
    let TriggerPlan::Sqs {
        report_batch_item_failures,
        ..
    } = &mut plan.triggers[1]
    else {
        panic!("SQS trigger");
    };
    *report_batch_item_failures = false;

    assert!(
        plan.validate()
            .iter()
            .any(|diagnostic| diagnostic.code == "MINCO-SQS-001")
    );
}

#[test]
fn queue_visibility_covers_six_function_timeouts_and_the_batch_window() {
    let mut plan = standard_worker_plan();
    plan.queues[0].visibility_timeout_seconds = 179;

    assert!(
        plan.validate()
            .iter()
            .any(|diagnostic| diagnostic.code == "MINCO-SQS-002")
    );
}

#[test]
fn fifo_sources_require_fifo_dead_letter_queues() {
    let mut plan = standard_worker_plan();
    plan.queues[0].fifo = true;
    plan.queues[0].dead_letter_queue_id = Some("orders-dlq".into());
    plan.queues[0].max_receive_count = Some(5);
    plan.queues.push(QueuePlan {
        id: "orders-dlq".into(),
        fifo: false,
        visibility_timeout_seconds: 180,
        retention_seconds: 1_209_600,
        dead_letter_queue_id: None,
        max_receive_count: None,
    });

    assert!(
        plan.validate()
            .iter()
            .any(|diagnostic| diagnostic.code == "MINCO-SQS-003")
    );
}

#[test]
fn dead_letter_queue_graph_rejects_cycles() {
    let mut plan = standard_worker_plan();
    plan.queues[0].dead_letter_queue_id = Some("orders-dlq".into());
    plan.queues[0].max_receive_count = Some(5);
    plan.queues.push(QueuePlan {
        id: "orders-dlq".into(),
        fifo: false,
        visibility_timeout_seconds: 180,
        retention_seconds: 1_209_600,
        dead_letter_queue_id: Some("orders".into()),
        max_receive_count: Some(5),
    });

    assert!(
        plan.validate()
            .iter()
            .any(|diagnostic| diagnostic.code == "MINCO-SQS-006")
    );
}

#[test]
fn sqs_mapping_concurrency_is_bounded_by_worker_and_cost_policy() {
    let mut plan = standard_worker_plan();
    let TriggerPlan::Sqs {
        maximum_concurrency,
        ..
    } = &mut plan.triggers[1]
    else {
        panic!("SQS trigger");
    };
    *maximum_concurrency = 6;

    assert!(
        plan.validate()
            .iter()
            .any(|diagnostic| diagnostic.code == "MINCO-SQS-007")
    );
}

#[test]
fn aggregate_sqs_mapping_concurrency_cannot_exceed_worker_reservation() {
    let mut plan = standard_worker_plan();
    let mut second_queue = plan.queues[0].clone();
    second_queue.id = "audit".into();
    plan.queues.push(second_queue);
    plan.triggers.push(TriggerPlan::Sqs {
        id: "audit".into(),
        function_id: "orders-worker".into(),
        queue_id: "audit".into(),
        batch_size: 10,
        batching_window_seconds: 0,
        report_batch_item_failures: true,
        maximum_concurrency: 2,
    });

    assert!(
        plan.validate()
            .iter()
            .any(|diagnostic| diagnostic.code == "MINCO-SQS-012")
    );
}

#[test]
fn fifo_mapping_batch_size_is_limited_to_ten() {
    let mut plan = standard_worker_plan();
    plan.queues[0].fifo = true;
    let TriggerPlan::Sqs { batch_size, .. } = &mut plan.triggers[1] else {
        panic!("SQS trigger");
    };
    *batch_size = 11;

    assert!(
        plan.validate()
            .iter()
            .any(|diagnostic| diagnostic.code == "MINCO-SQS-008")
    );
}

#[test]
fn minimal_idle_policy_rejects_enabled_explicit_schedules() {
    let mut plan = standard_worker_plan();
    plan.triggers.push(TriggerPlan::Schedule {
        id: "outbox-recovery".into(),
        function_id: "orders-worker".into(),
        expression: "rate(15 minutes)".into(),
        enabled: true,
        purpose: "recover stranded outbox records".into(),
        cleanup: None,
    });

    assert!(
        plan.validate()
            .iter()
            .any(|diagnostic| diagnostic.code == "MINCO-COST-002")
    );
}

#[test]
fn permitted_schedules_expose_wake_and_cost_diagnostics() {
    let mut plan = standard_worker_plan();
    plan.cost_policy.deny_scheduled_wakeups = false;
    plan.triggers.push(TriggerPlan::Schedule {
        id: "outbox-recovery".into(),
        function_id: "orders-worker".into(),
        expression: "rate(15 minutes)".into(),
        enabled: true,
        purpose: "recover stranded outbox records".into(),
        cleanup: None,
    });

    let diagnostic = plan
        .validate()
        .into_iter()
        .find(|diagnostic| diagnostic.code == "MINCO-COST-009")
        .expect("schedule diagnostic");
    assert_eq!(diagnostic.severity, Severity::Information);
    assert!(diagnostic.message.contains("outbox-recovery"));
    assert!(diagnostic.message.contains("rate(15 minutes)"));
    assert!(diagnostic.message.contains("scale-to-zero"));
}

#[test]
fn legacy_api_only_plan_migrates_deterministically_to_schema_v2() {
    let config: DeploymentConfig = toml::from_str(include_str!(
        "../../../examples/orders/config/minco.dev.toml"
    ))
    .expect("legacy config");
    let contract = ContractDocument {
        source: "inline".into(),
        openapi_version: "3.1.0".into(),
        title: "orders".into(),
        version: "1".into(),
        sha256: "hash".into(),
        operations: Vec::new(),
        schema_names: Vec::new(),
        raw: serde_json::json!({}),
    };

    let legacy = config.into_plan(&contract);
    let migrated = legacy.migrate_to_latest().expect("schema migration");

    assert_eq!(migrated.schema_version, 2);
    assert_eq!(
        migrated.triggers,
        [TriggerPlan::HttpApi {
            id: "http-api".into(),
            function_id: "api".into(),
        }]
    );
    assert!(
        migrated
            .validate()
            .iter()
            .all(|diagnostic| diagnostic.severity != Severity::Error)
    );
}

#[test]
fn sam_renders_only_the_explicit_worker_queue_and_mapping() {
    let yaml = render_sam(&standard_worker_plan()).expect("SAM");

    assert!(yaml.contains("  OrdersQueue:\n    Type: AWS::SQS::Queue"));
    assert!(yaml.contains("  OrdersWorkerFunction:\n    Type: AWS::Serverless::Function"));
    assert!(yaml.contains("Queue: !GetAtt OrdersQueue.Arn"));
    assert!(yaml.contains("FunctionResponseTypes:\n              - ReportBatchItemFailures"));
    assert!(yaml.contains("ScalingConfig:\n              MaximumConcurrency: 2"));
    assert!(yaml.contains("Resource: !GetAtt OrdersQueue.Arn"));
    assert!(!yaml.contains("Type: ScheduleV2"));
}

#[test]
fn database_free_functions_receive_no_database_parameter_or_vpc_policy() {
    let config: DeploymentConfig =
        toml::from_str(include_str!("fixtures/api_worker_database_free_v2.toml"))
            .expect("deployment config");
    let contract = ContractDocument {
        source: "inline".into(),
        openapi_version: "3.1.0".into(),
        title: "orders".into(),
        version: "1".into(),
        sha256: "hash".into(),
        operations: Vec::new(),
        schema_names: Vec::new(),
        raw: serde_json::json!({}),
    };
    let plan = config.into_plan(&contract);

    let yaml = render_sam(&plan).expect("SAM");

    assert!(!yaml.contains("DatabaseUrlParameterName"));
    assert!(!yaml.contains("ssm:GetParameter"));
    assert!(!yaml.contains("VpcConfig"));
    assert!(yaml.contains("sqs:ReceiveMessage"));
}

#[test]
fn plan_derives_exact_queue_consumer_iam_intent() {
    let plan = standard_worker_plan();
    let intent = plan
        .iam_intents
        .iter()
        .find(|intent| {
            intent.function_id == "orders-worker"
                && intent.resource
                    == IamResource::Queue {
                        queue_id: "orders".into(),
                    }
        })
        .expect("worker queue IAM intent");

    assert_eq!(
        intent.actions,
        [
            "sqs:ChangeMessageVisibility",
            "sqs:DeleteMessage",
            "sqs:GetQueueAttributes",
            "sqs:ReceiveMessage",
        ]
    );
}

#[test]
fn runtime_cost_report_exposes_schedule_wakes_and_worker_connection_pressure() {
    let mut plan = standard_worker_plan();
    plan.cost_policy.deny_scheduled_wakeups = false;
    plan.triggers.push(TriggerPlan::Schedule {
        id: "outbox-recovery".into(),
        function_id: "orders-worker".into(),
        expression: "rate(15 minutes)".into(),
        enabled: true,
        purpose: "recover stranded outbox records".into(),
        cleanup: None,
    });

    let report = estimate_runtime_cost(&plan);

    assert_eq!(
        report.schedules[0].estimated_monthly_invocations,
        Some(2_922)
    );
    assert!(report.schedules[0].can_wake_scale_to_zero_database);
    assert_eq!(report.workers[0].maximum_database_connections, 2);
    assert_eq!(report.queues[0].queue_id, "orders");
    assert_eq!(report.queues[0].mappings[0].trigger_id, "orders");
    assert!(report.queues[0].regional_request_rate_required);
    assert!(!report.complete);
}

#[test]
fn runtime_cost_report_preserves_every_mapping_for_a_queue() {
    let mut plan = standard_worker_plan();
    let mut second_worker = plan.functions[1].clone();
    second_worker.name = "audit-worker".into();
    plan.functions.push(second_worker);
    plan.triggers.push(TriggerPlan::Sqs {
        id: "audit".into(),
        function_id: "audit-worker".into(),
        queue_id: "orders".into(),
        batch_size: 5,
        batching_window_seconds: 0,
        report_batch_item_failures: true,
        maximum_concurrency: 2,
    });

    let report = estimate_runtime_cost(&plan);

    assert_eq!(report.queues[0].mappings.len(), 2);
    assert_eq!(report.queues[0].mappings[1].trigger_id, "audit");
    assert_eq!(report.queues[0].mappings[1].function_id, "audit-worker");
}

#[test]
fn sam_accepts_an_exact_artifact_uri_for_every_function() {
    let code_uris = BTreeMap::from([
        ("api".into(), "../../../artifacts/api.zip".into()),
        (
            "orders-worker".into(),
            "../../../artifacts/orders-worker.zip".into(),
        ),
    ]);

    let yaml =
        render_sam_with_code_uris(&standard_worker_plan(), &code_uris).expect("SAM artifacts");

    assert!(yaml.contains("CodeUri: '../../../artifacts/api.zip'"));
    assert!(yaml.contains("CodeUri: '../../../artifacts/orders-worker.zip'"));
}

#[test]
fn sam_renders_only_an_explicitly_declared_schedule() {
    let mut plan = standard_worker_plan();
    plan.cost_policy.deny_scheduled_wakeups = false;
    plan.triggers.push(TriggerPlan::Schedule {
        id: "outbox-recovery".into(),
        function_id: "orders-worker".into(),
        expression: "rate(15 minutes)".into(),
        enabled: true,
        purpose: "recover stranded outbox records".into(),
        cleanup: None,
    });

    let yaml = render_sam(&plan).expect("scheduled SAM");

    assert!(yaml.contains("Type: ScheduleV2"));
    assert!(yaml.contains("ScheduleExpression: 'rate(15 minutes)'"));
    assert!(yaml.contains("State: ENABLED"));
    assert!(yaml.contains("Description: 'recover stranded outbox records'"));
    assert_eq!(yaml.matches("Type: ScheduleV2").count(), 1);
}

#[test]
fn schedules_require_a_reviewable_purpose() {
    let mut plan = standard_worker_plan();
    plan.triggers.push(TriggerPlan::Schedule {
        id: "outbox-recovery".into(),
        function_id: "orders-worker".into(),
        expression: "rate(15 minutes)".into(),
        enabled: false,
        purpose: " ".into(),
        cleanup: None,
    });

    assert!(
        plan.validate()
            .iter()
            .any(|diagnostic| diagnostic.code == "MINCO-SCHEDULE-001")
    );
}

#[test]
fn schedules_accept_only_eventbridge_expression_forms() {
    let mut plan = standard_worker_plan();
    plan.triggers.push(TriggerPlan::Schedule {
        id: "outbox-recovery".into(),
        function_id: "orders-worker".into(),
        expression: "every 15 minutes".into(),
        enabled: false,
        purpose: "recover stranded outbox records".into(),
        cleanup: None,
    });

    assert!(
        plan.validate()
            .iter()
            .any(|diagnostic| diagnostic.code == "MINCO-SCHEDULE-002")
    );
}

#[test]
fn function_url_is_declared_but_rejected_before_provider_rendering() {
    let mut plan = plan_from_config(include_str!("fixtures/api_only_v1.toml"));
    plan.ingress = IngressPlan::LambdaFunctionUrl;

    assert!(plan.validate().iter().any(|diagnostic| {
        diagnostic.code == "MINCO-PLAN-INGRESS-001" && diagnostic.severity == Severity::Error
    }));
    let cost = estimate_runtime_cost(&plan);
    assert!(
        cost.request_based_resources
            .contains(&"lambda_function_url".to_owned())
    );
    assert!(
        cost.missing_rates
            .iter()
            .all(|rate| !rate.contains("api_gateway"))
    );
}

#[test]
fn runtime_and_ingress_topology_fails_closed_before_rendering() {
    let mut plan = plan_from_config(include_str!("fixtures/api_only_v1.toml"));
    plan.runtime = RuntimePlan::LocalNative;

    assert!(plan.validate().iter().any(|diagnostic| {
        diagnostic.code == "MINCO-PLAN-INGRESS-002" && diagnostic.severity == Severity::Error
    }));
}

#[test]
fn every_runtime_and_ingress_pair_has_a_stable_validation_result() {
    let cases = [
        (
            RuntimePlan::LambdaZipArm64,
            IngressPlan::ApiGatewayHttpApi,
            None,
        ),
        (
            RuntimePlan::LambdaZipArm64,
            IngressPlan::LambdaFunctionUrl,
            Some("MINCO-PLAN-INGRESS-001"),
        ),
        (
            RuntimePlan::LambdaZipArm64,
            IngressPlan::LocalTcp,
            Some("MINCO-PLAN-INGRESS-002"),
        ),
        (
            RuntimePlan::LocalNative,
            IngressPlan::ApiGatewayHttpApi,
            Some("MINCO-PLAN-INGRESS-002"),
        ),
        (
            RuntimePlan::LocalNative,
            IngressPlan::LambdaFunctionUrl,
            Some("MINCO-PLAN-INGRESS-002"),
        ),
        (RuntimePlan::LocalNative, IngressPlan::LocalTcp, None),
    ];

    for (runtime, ingress, expected_code) in cases {
        let mut plan = plan_from_config(include_str!("fixtures/api_only_v1.toml"));
        plan.runtime = runtime;
        plan.ingress = ingress;
        let ingress_errors = plan
            .validate()
            .into_iter()
            .filter(|diagnostic| diagnostic.code.starts_with("MINCO-PLAN-INGRESS-"))
            .collect::<Vec<_>>();

        match expected_code {
            Some(code) => {
                assert_eq!(ingress_errors.len(), 1, "{ingress_errors:#?}");
                assert_eq!(ingress_errors[0].code, code);
                assert_eq!(ingress_errors[0].severity, Severity::Error);
            }
            None => assert!(ingress_errors.is_empty(), "{ingress_errors:#?}"),
        }
    }
}

#[test]
fn local_native_cost_projection_has_no_aws_runtime_rates() {
    let mut plan = plan_from_config(include_str!("fixtures/api_only_v1.toml"));
    plan.runtime = RuntimePlan::LocalNative;
    plan.ingress = IngressPlan::LocalTcp;

    let cost = estimate_runtime_cost(&plan);

    assert!(cost.complete);
    assert!(cost.request_based_resources.is_empty());
    assert!(cost.missing_rates.is_empty());
    assert!(cost.evidence.is_empty());
}

#[test]
fn local_native_cost_projection_keeps_shape_without_aws_provider_charges() {
    let mut plan = standard_worker_plan();
    plan.runtime = RuntimePlan::LocalNative;
    plan.ingress = IngressPlan::LocalTcp;
    plan.functions[0].provisioned_concurrency = 2;
    plan.cost_policy.deny_scheduled_wakeups = false;
    plan.triggers.push(TriggerPlan::Schedule {
        id: "local-reconciliation".into(),
        function_id: "orders-worker".into(),
        expression: "rate(1 hour)".into(),
        enabled: true,
        purpose: "local parity only".into(),
        cleanup: None,
    });
    plan.realtime = Some(RealtimeDeployment {
        namespace: "orders".into(),
        max_event_bytes: 5 * 1024,
        subscriber_claim: "sub".into(),
    });

    let cost = estimate_runtime_cost(&plan);

    assert!(cost.complete);
    assert!(!cost.workers.is_empty());
    assert!(!cost.queues.is_empty());
    assert!(!cost.schedules.is_empty());
    assert!(
        cost.queues
            .iter()
            .all(|queue| !queue.regional_request_rate_required)
    );
    assert!(cost.realtime.is_none());
    assert!(cost.fixed_cost_resources.is_empty());
    assert!(cost.request_based_resources.is_empty());
    assert!(cost.missing_rates.is_empty());
    assert!(cost.evidence.is_empty());
    assert!(
        cost.schedules
            .iter()
            .all(|schedule| !schedule.can_wake_scale_to_zero_database)
    );
}

#[test]
fn generic_api_only_v1_fixture_remains_supported() {
    let plan = plan_from_config(include_str!("fixtures/api_only_v1.toml"));

    assert_eq!(plan.schema_version, 1);
    assert!(plan.queues.is_empty());
    assert!(plan.triggers.is_empty());
    assert!(
        plan.validate()
            .iter()
            .all(|diagnostic| diagnostic.severity != Severity::Error)
    );
}

#[test]
fn schema_v1_cannot_relabel_its_only_function_as_a_worker() {
    let mut plan = plan_from_config(include_str!("fixtures/api_only_v1.toml"));
    plan.functions[0].role = minco_plan::FunctionRole::Worker;

    assert!(
        plan.validate()
            .iter()
            .any(|diagnostic| diagnostic.code == "MINCO-PLAN-005")
    );
    assert!(
        plan.migrate_to_latest()
            .expect_err("worker cannot migrate as an API")
            .to_string()
            .contains("MINCO-PLAN-MIGRATE-002")
    );
}

#[test]
fn fifo_dlq_fixture_renders_compatible_redrive_resources() {
    let plan = plan_from_config(include_str!("fixtures/api_worker_fifo_dlq_v2.toml"));
    assert!(
        plan.validate()
            .iter()
            .all(|diagnostic| diagnostic.severity != Severity::Error)
    );

    let yaml = render_sam(&plan).expect("FIFO SAM");
    assert_eq!(yaml.matches("FifoQueue: true").count(), 2);
    assert!(yaml.contains("deadLetterTargetArn: !GetAtt OrdersDlqQueue.Arn"));
    assert!(yaml.contains("maxReceiveCount: 5"));
    assert!(yaml.contains("QueueName: 'orders-test-orders.fifo'"));
}

#[test]
fn explicit_cleanup_schedule_fixture_is_reviewable_and_rendering_fails_closed() {
    let plan = plan_from_config(include_str!("fixtures/api_worker_schedule_v2.toml"));

    assert!(
        plan.validate()
            .iter()
            .any(|diagnostic| diagnostic.code == "MINCO-COST-009")
    );
    assert!(plan.iam_intents.iter().any(|intent| {
        intent.actions == ["lambda:InvokeFunction"]
            && intent.resource
                == IamResource::Function {
                    function_id: "recovery-worker".into(),
                }
    }));
    assert!(
        render_sam(&plan)
            .expect_err("cleanup requires a guarded Scheduler API apply")
            .to_string()
            .contains("ActionAfterCompletion")
    );
}

#[test]
fn one_time_schedule_cleanup_is_explicit_and_sam_fails_closed() {
    let plan = plan_from_config(include_str!("fixtures/api_worker_schedule_v2.toml"));

    assert!(
        plan.validate()
            .iter()
            .all(|diagnostic| diagnostic.severity != Severity::Error)
    );
    let schedule = &estimate_runtime_cost(&plan).schedules[0];
    assert_eq!(
        schedule.action_after_completion,
        Some(ScheduleCompletionAction::Delete)
    );
    assert_eq!(schedule.residual_resources.len(), 3);
    assert!(schedule.manual_fallback.is_some());

    assert!(
        render_sam(&plan)
            .expect_err("cleanup requires a guarded Scheduler API apply")
            .to_string()
            .contains("ActionAfterCompletion")
    );
}

#[test]
fn completion_deletion_is_rejected_for_recurring_schedules() {
    let mut plan = standard_worker_plan();
    plan.triggers.push(TriggerPlan::Schedule {
        id: "unsafe-cleanup".into(),
        function_id: "orders-worker".into(),
        expression: "rate(1 day)".into(),
        enabled: false,
        purpose: "must remain recurring".into(),
        cleanup: Some(ScheduleCleanupPlan {
            action_after_completion: ScheduleCompletionAction::Delete,
            residual_resources: vec!["target outputs".into()],
            manual_fallback: "run guarded cleanup".into(),
        }),
    });

    assert!(
        plan.validate()
            .iter()
            .any(|diagnostic| diagnostic.code == "MINCO-SCHEDULE-004")
    );
}

#[test]
fn schema_v2_schedule_without_cleanup_remains_deserializable() {
    let source = include_str!("fixtures/api_worker_schedule_v2.toml")
        .split_once("[triggers.cleanup]")
        .expect("cleanup section")
        .0;
    let plan = plan_from_config(source);
    let cleanup = plan.triggers.iter().find_map(|trigger| {
        let TriggerPlan::Schedule { cleanup, .. } = trigger else {
            return None;
        };
        Some(cleanup)
    });

    assert_eq!(cleanup, Some(&None));
    assert!(
        serde_json::to_value(&plan)
            .expect("serialized plan")
            .to_string()
            .contains("\"kind\":\"schedule\"")
    );
    assert!(
        !serde_json::to_value(&plan)
            .expect("serialized plan")
            .to_string()
            .contains("\"cleanup\"")
    );
    assert!(
        render_sam(&plan)
            .expect("legacy schema 2 schedule remains renderable")
            .contains("Type: ScheduleV2")
    );
}

#[test]
fn dynamodb_worker_fixture_has_no_relational_connection_or_iam_projection() {
    let plan = plan_from_config(include_str!("fixtures/api_worker_dynamodb_v2.toml"));

    assert_eq!(plan.local_aws_services, ["dynamodb", "sqs", "sts"]);
    assert!(
        plan.validate()
            .iter()
            .all(|diagnostic| diagnostic.severity != Severity::Error)
    );
    assert!(plan.iam_intents.iter().all(|intent| {
        !matches!(
            intent.resource,
            IamResource::DatabaseUrlParameter | IamResource::DatabaseUrlKmsKey
        )
    }));
    assert!(
        render_sam(&plan)
            .expect_err("generic DynamoDB SAM must fail closed")
            .to_string()
            .contains("DynamoDB needs a dedicated adapter/rendering plugin")
    );
}

#[test]
fn explicit_dynamodb_table_renders_on_demand_indexes_environment_and_exact_iam() {
    let plan = plan_from_config(include_str!("fixtures/api_dynamodb_explicit_v2.toml"));

    let diagnostics = plan.validate();
    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.severity != Severity::Error),
        "{diagnostics:#?}"
    );
    assert!(plan.iam_intents.iter().any(|intent| {
        intent.function_id == "api"
            && intent.actions
                == [
                    "dynamodb:DescribeTable",
                    "dynamodb:GetItem",
                    "dynamodb:Query",
                    "dynamodb:TransactWriteItems",
                    "dynamodb:UpdateItem",
                ]
            && matches!(
                &intent.resource,
                IamResource::DynamoDbTable { logical_id } if logical_id == "OrdersTable"
            )
    }));
    assert!(plan.iam_intents.iter().any(|intent| {
        intent.function_id == "api"
            && intent.actions
                == [
                    "dynamodb:BatchGetItem",
                    "dynamodb:DescribeTable",
                    "dynamodb:Query",
                    "dynamodb:TransactWriteItems",
                ]
            && matches!(
                &intent.resource,
                IamResource::DynamoDbTable { logical_id } if logical_id == "AuditLedgerTable"
            )
    }));

    let yaml = render_sam(&plan).expect("explicit DynamoDB SAM");
    let _: serde_yaml_ng::Value = serde_yaml_ng::from_str(&yaml).expect("syntactically valid SAM");
    for required in [
        "  OrdersTable:\n",
        "  AuditLedgerTable:\n",
        "    Type: AWS::DynamoDB::Table\n",
        "      BillingMode: PAY_PER_REQUEST\n",
        "      DeletionProtectionEnabled: true\n",
        "      PointInTimeRecoverySpecification:\n        PointInTimeRecoveryEnabled: true\n",
        "    DeletionPolicy: Retain\n",
        "    UpdateReplacePolicy: Retain\n",
        "          DYNAMODB_TABLE_NAME: !Ref OrdersTable\n",
        "          AUDIT_DYNAMODB_TABLE_NAME: !Ref AuditLedgerTable\n",
        "                - dynamodb:DescribeTable\n",
        "                - dynamodb:GetItem\n",
        "                - dynamodb:TransactWriteItems\n",
        "                - dynamodb:UpdateItem\n",
        "              Resource: !GetAtt OrdersTable.Arn\n",
        "              Action: dynamodb:Query\n",
        "                - !Sub '${OrdersTable.Arn}/index/orders-by-created-at'\n",
        "                - !Sub '${OrdersTable.Arn}/index/orders-by-created-at-inverted-id'\n",
        "                - !Sub '${OrdersTable.Arn}/index/orders-by-id'\n",
        "                - dynamodb:BatchGetItem\n",
        "              Resource: !GetAtt AuditLedgerTable.Arn\n",
    ] {
        assert!(yaml.contains(required), "missing {required:?} in:\n{yaml}");
    }
    assert!(!yaml.contains("dynamodb:*"));
    assert!(!yaml.contains("dynamodb:PutItem"));
    assert!(!yaml.contains("Resource: '*'"));
}

#[test]
fn explicit_dynamodb_table_contract_is_schema_closed_and_validates_provider_identity() {
    let source = include_str!("fixtures/api_dynamodb_explicit_v2.toml");
    let unknown = source.replace(
        "deletion_policy = \"retain\"",
        "deletion_policy = \"retain\"\nunexpected_policy = true",
    );
    assert!(toml::from_str::<DeploymentConfig>(&unknown).is_err());

    let mut plan = plan_from_config(source);
    let minco_plan::DatabaseDeployment::DynamoDbOnDemand {
        table: Some(table),
        audit_table: Some(audit_table),
        ..
    } = &mut plan.database
    else {
        panic!("explicit DynamoDB table");
    };
    table.logical_id = "not-a-logical-id".into();
    table.function_id = "missing-function".into();
    table.global_secondary_indexes[1].name = table.global_secondary_indexes[0].name.clone();
    audit_table.logical_id = table.logical_id.clone();
    audit_table.partition_key.name = "wrong".into();
    audit_table.point_in_time_recovery = false;
    let diagnostics = plan.validate();
    for code in [
        "MINCO-DYNAMODB-002",
        "MINCO-DYNAMODB-003",
        "MINCO-DYNAMODB-008",
        "MINCO-DYNAMODB-009",
        "MINCO-DYNAMODB-011",
        "MINCO-DYNAMODB-012",
    ] {
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == code && diagnostic.severity == Severity::Error),
            "missing {code} in {diagnostics:#?}"
        );
    }
}

#[test]
fn local_native_topology_has_no_aws_database_parameter_iam() {
    let mut config: DeploymentConfig =
        toml::from_str(include_str!("fixtures/api_worker_standard_v2.toml"))
            .expect("deployment config");
    config.runtime = RuntimePlan::LocalNative;
    config.ingress = minco_plan::IngressPlan::LocalTcp;
    let contract = ContractDocument {
        source: "inline".into(),
        openapi_version: "3.1.0".into(),
        title: "orders".into(),
        version: "1".into(),
        sha256: "hash".into(),
        operations: Vec::new(),
        schema_names: Vec::new(),
        raw: serde_json::json!({}),
    };

    let plan = config.into_plan(&contract);

    assert_eq!(plan.local_aws_services, ["sqs"]);
    assert!(plan.iam_intents.iter().all(|intent| {
        !matches!(
            intent.resource,
            IamResource::DatabaseUrlParameter | IamResource::DatabaseUrlKmsKey
        )
    }));
}

#[test]
fn missing_trigger_references_have_a_stable_diagnostic() {
    let mut plan = standard_worker_plan();
    let TriggerPlan::Sqs { queue_id, .. } = &mut plan.triggers[1] else {
        panic!("SQS trigger");
    };
    *queue_id = "missing".into();

    assert!(
        plan.validate()
            .iter()
            .any(|diagnostic| diagnostic.code == "MINCO-PLAN-015")
    );
}

#[test]
fn duplicate_function_queue_and_trigger_ids_are_rejected() {
    let mut plan = standard_worker_plan();
    plan.functions.push(plan.functions[1].clone());
    plan.queues.push(plan.queues[0].clone());
    plan.triggers.push(plan.triggers[1].clone());
    let codes = plan
        .validate()
        .into_iter()
        .map(|diagnostic| diagnostic.code)
        .collect::<Vec<_>>();

    assert!(codes.contains(&"MINCO-PLAN-011".into()));
    assert!(codes.contains(&"MINCO-PLAN-013".into()));
    assert!(codes.contains(&"MINCO-PLAN-014".into()));
}

#[test]
fn distinct_ids_cannot_collapse_to_the_same_sam_logical_id() {
    let mut plan = standard_worker_plan();
    let mut first = plan.functions[1].clone();
    first.name = "audit-1".into();
    let mut second = first.clone();
    second.name = "audit1".into();
    plan.functions.extend([first, second]);

    assert!(
        plan.validate()
            .iter()
            .any(|diagnostic| diagnostic.code == "MINCO-PLAN-018")
    );
}

#[test]
fn openapi_operation_ids_cannot_collapse_to_one_sam_event() {
    let mut plan = standard_worker_plan();
    plan.routes = vec![
        minco_plan::RoutePlan {
            operation_id: "order-1".into(),
            method: HttpMethod::Get,
            path: "/orders/1".into(),
            authenticated: false,
        },
        minco_plan::RoutePlan {
            operation_id: "order1".into(),
            method: HttpMethod::Get,
            path: "/orders/2".into(),
            authenticated: false,
        },
    ];

    assert!(
        plan.validate()
            .iter()
            .any(|diagnostic| diagnostic.code == "MINCO-PLAN-018")
    );
}

#[test]
fn schema_v2_rejects_invalid_derived_aws_resource_names() {
    let mut plan = standard_worker_plan();
    plan.application = "orders".repeat(12);

    assert!(
        plan.validate()
            .iter()
            .any(|diagnostic| diagnostic.code == "MINCO-AWS-001")
    );
    assert!(
        plan.validate()
            .iter()
            .any(|diagnostic| diagnostic.code == "MINCO-AWS-002")
    );
}

#[test]
fn worker_functions_cannot_own_http_operations() {
    let mut plan = standard_worker_plan();
    let TriggerPlan::HttpApi { function_id, .. } = &mut plan.triggers[0] else {
        panic!("HTTP trigger");
    };
    *function_id = "orders-worker".into();

    assert!(
        plan.validate()
            .iter()
            .any(|diagnostic| diagnostic.code == "MINCO-PLAN-016")
    );
}

#[test]
fn aggregate_connection_budget_includes_api_and_workers() {
    let mut plan = standard_worker_plan();
    plan.cost_policy.max_database_connections = 5;

    assert!(
        plan.validate()
            .iter()
            .any(|diagnostic| diagnostic.code == "MINCO-COST-005")
    );
}

#[test]
fn schema_v2_rejects_legacy_schedule_strings() {
    let mut plan = standard_worker_plan();
    plan.scheduled_wakeups.push("rate(15 minutes)".into());

    assert!(
        plan.validate()
            .iter()
            .any(|diagnostic| diagnostic.code == "MINCO-SCHEDULE-003")
    );
}

#[test]
fn tampered_iam_projection_is_rejected() {
    let mut plan = standard_worker_plan();
    plan.iam_intents.clear();

    assert!(
        plan.validate()
            .iter()
            .any(|diagnostic| diagnostic.code == "MINCO-IAM-001")
    );
}

#[test]
fn multi_runtime_sam_is_byte_deterministic() {
    let plan = plan_from_config(include_str!("fixtures/api_worker_fifo_dlq_v2.toml"));

    assert_eq!(
        render_sam(&plan).expect("first SAM"),
        render_sam(&plan).expect("second SAM")
    );
}

#[test]
fn legacy_unstructured_schedules_receive_a_stable_migration_rejection() {
    let mut legacy = plan_from_config(include_str!("fixtures/api_only_v1.toml"));
    legacy
        .scheduled_wakeups
        .push("legacy nightly recovery".into());

    let error = legacy
        .migrate_to_latest()
        .expect_err("unstructured schedule must not be guessed");

    assert!(error.to_string().contains("MINCO-PLAN-MIGRATE-001"));
}
