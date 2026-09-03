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
timeout_seconds = 30
reserved_concurrency = 2
provisioned_concurrency = 0
database_connections_per_instance = 1

[[functions]]
name = "jobs-worker"
role = "worker"
artifact_path = "target/lambda/jobs-worker.zip"
memory_mb = 512
timeout_seconds = 30
reserved_concurrency = 2
provisioned_concurrency = 0
database_connections_per_instance = 1

[[triggers]]
kind = "http_api"
id = "api"
function_id = "api"
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
            // The fixture's deployment has no authorizer; the operation
            // must match or even the BASE plan fails MINCO-AUTH-001 and
            // the applied-plan validation regression could never pass
            // (exact-head review 5064401898).
            authenticated: false,
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
    // Exact-head review 5060065907: the topology is an explicit sidecar
    // — the plan never carries it as a field, so the witness here is
    // the projection into the EXISTING queues/triggers collections.
    let topology = topology();
    assert_eq!(topology.bindings.len(), 1);
    let applied = apply_inbound_mail(&plan(), &topology);
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
    let twice = apply_inbound_mail(&applied, &topology);
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
        // Exact-head review 5083559431 P0-1: the SourceArn is built from
        // the EXPLICIT bucket name so the queue policy can be created
        // before the bucket (S3 validates the notification destination
        // permission at bucket-creation time).
        "aws:SourceArn: !Sub 'arn:${AWS::Partition}:s3:::orders-dev-raw-mail'",
        "aws:SourceAccount: !Ref AWS::AccountId",
        "InboundMailReceiptRuleSet:\n    Type: AWS::SES::ReceiptRuleSet",
        // P0-1: the bucket waits for the queue policy.
        "TicketingRawMailBucket:\n    Type: AWS::S3::Bucket\n    DependsOn: [TicketingMailQueuePolicy]",
        // P0-2: the rule carries a REAL dependency on the rule set
        // (!Ref) and waits for the SES-write bucket policy.
        "TicketingReceiptRule:\n    Type: AWS::SES::ReceiptRule\n    DependsOn: [TicketingRawMailBucketPolicy, TicketingMailQueuePolicy]",
        "RuleSetName: !Ref InboundMailReceiptRuleSet",
        "ScanEnabled: true",
        "ObjectKeyPrefix: 'mail/'",
        // Review finding 5: full mailbox recipient, source-account-bound
        // SES writes, one shared rule set name on every rule.
        "Recipients:\n          - 'support@example.test'",
        "aws:SourceAccount: !Sub '${AWS::AccountId}'",
        // Exact-head review R9: TLS required, prefix-scoped writes,
        // deployment-order dependency.
        "TlsPolicy: Require",
        "Resource: !Sub '${TicketingRawMailBucket.Arn}/mail/*'",
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
    // Stable rule-set identity (exact-head review 5072859042): the name
    // is derived from application + environment + an order-independent
    // topology digest — never from the first binding.
    let rule_set_line = template
        .lines()
        .find(|line| line.trim_start().starts_with("RuleSetName: "))
        .expect("rule set name line");
    let rule_set_name = rule_set_line
        .trim()
        .trim_start_matches("RuleSetName: ")
        .trim_matches('\'');
    assert!(
        rule_set_name.starts_with("orders-dev-inbound-mail-"),
        "rule set name carries the deployment identity: {rule_set_name}"
    );
    assert!(rule_set_name.len() <= 64, "SES RuleSetName limit");
    let digest = rule_set_name.rsplit('-').next().expect("digest suffix");
    assert_eq!(digest.len(), 12, "bounded hex digest suffix");
    assert!(digest.chars().all(|c| c.is_ascii_hexdigit()));
    assert!(
        template.contains(&format!(
            "arn:aws:ses:${{AWS::Region}}:${{AWS::AccountId}}:receipt-rule-set/{rule_set_name}:receipt-rule/ticketing-inbound-mail"
        )),
        "the receipt-rule ARN embeds the stable rule-set name"
    );
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

#[test]
fn applied_inbound_mail_plan_passes_ordinary_plan_validation() {
    // Exact-head review 5064401898: the synthesized queues/triggers
    // change the derived fields; the applied plan must still satisfy
    // the ordinary DeploymentPlan validators.
    let applied = apply_inbound_mail(&plan(), &topology());
    let diagnostics = applied.validate();
    assert!(
        diagnostics.is_empty(),
        "applied plan must remain internally valid: {diagnostics:#?}"
    );
}

#[test]
fn applying_inbound_mail_refreshes_derived_services_and_iam() {
    let base = plan();
    assert!(
        !base
            .local_aws_services
            .iter()
            .any(|service| service == "sqs"),
        "fixture sanity: the queue-free base plan has no sqs service"
    );
    let applied = apply_inbound_mail(&base, &topology());
    assert!(
        applied
            .local_aws_services
            .iter()
            .any(|service| service == "sqs"),
        "the synthesized wake queue must add the sqs local service"
    );
    assert_eq!(
        applied.local_aws_services,
        minco_plan::local_aws_services(
            &applied.runtime,
            &applied.database,
            &applied.application_graph,
            &applied.queues,
        )
    );
    assert_eq!(
        applied.iam_intents,
        minco_plan::derive_iam_intents(
            applied.schema_version,
            &applied.runtime,
            &applied.database,
            &applied.application_graph,
            &applied.functions,
            &applied.triggers,
        )
    );
}

#[test]
fn applying_inbound_mail_twice_keeps_derived_state_stable() {
    let once = apply_inbound_mail(&plan(), &topology());
    let twice = apply_inbound_mail(&once, &topology());
    assert_eq!(once, twice);
    assert!(twice.validate().is_empty());
}

#[test]
fn disabled_topology_with_bindings_never_affects_rendering() {
    // Exact-head review 5064401898: disabled-with-bindings is
    // internally inconsistent — validation rejects it and rendering
    // fails closed instead of half-producing a template.
    let invalid = InboundMailTopology {
        enabled: false,
        bindings: vec![binding()],
    };
    assert!(!validate_inbound_mail(&plan(), &invalid).is_empty());
    let mut code_uris = std::collections::BTreeMap::new();
    code_uris.insert("api".to_owned(), "./api.zip".to_owned());
    code_uris.insert("mail-worker".to_owned(), "./mail-worker.zip".to_owned());
    let result = render_sam_with_inbound_mail(&plan(), &invalid, &code_uris);
    assert!(
        result.is_err(),
        "disabled topology with bindings must fail closed"
    );
}

#[test]
fn durable_work_and_inbound_mail_sidecars_compose_in_both_orders() {
    // Exact-head review 5064401898: two sidecars composing in either
    // order must leave derived state consistent and both sidecar
    // validators plus the ordinary plan validator green.
    use minco_plan::durable_work::{
        DurableWorkTopology, JobRoutePlan, WorkerProfilePlan, apply_durable_work,
        validate_durable_work,
    };
    let durable = DurableWorkTopology {
        enabled: true,
        profiles: vec![WorkerProfilePlan {
            id: "orders-notifications".into(),
            queue_id: "jobs-orders-notifications".into(),
            function_id: "jobs-worker".into(),
            artifact_path: "target/lambda/jobs-worker.zip".into(),
            fifo: false,
            batch_size: 10,
            batching_window_seconds: 1,
            maximum_concurrency: 2,
            memory_mb: 512,
            timeout_seconds: 30,
            reserved_concurrency: 2,
            max_payload_bytes: 262_144,
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
        schedules: vec![],
    };
    // Both sidecar triggers bind mail-worker; the validator requires
    // per-trigger concurrency >= 2 AND aggregate <= reserved, so the
    // fixture reserves 4 for the two triggers' 2 each.
    let mail = topology();
    let base = plan();
    let mail_first = apply_inbound_mail(&apply_durable_work(&base, &durable), &mail);
    let durable_first = apply_durable_work(&apply_inbound_mail(&base, &mail), &durable);
    for composed in [&mail_first, &durable_first] {
        let diagnostics = composed.validate();
        assert!(
            diagnostics.is_empty(),
            "composed plan must pass ordinary validation: {diagnostics:#?}"
        );
        assert!(validate_durable_work(composed, &durable).is_empty());
        assert!(validate_inbound_mail(composed, &mail).is_empty());
        assert_eq!(
            composed.local_aws_services,
            minco_plan::local_aws_services(
                &composed.runtime,
                &composed.database,
                &composed.application_graph,
                &composed.queues,
            )
        );
        assert_eq!(
            composed.iam_intents,
            minco_plan::derive_iam_intents(
                composed.schema_version,
                &composed.runtime,
                &composed.database,
                &composed.application_graph,
                &composed.functions,
                &composed.triggers,
            )
        );
    }
    // Both orders converge on the same synthesized collections as SETS
    // (the order-independence property of projection-only sidecars;
    // insertion order legitimately differs by application order).
    let sorted_ids = {
        fn sorted(queues: &[minco_plan::QueuePlan]) -> Vec<&str> {
            let mut ids: Vec<&str> = queues.iter().map(|queue| queue.id.as_str()).collect();
            ids.sort_unstable();
            ids
        }
        sorted
    };
    let sorted_triggers = {
        fn sorted(triggers: &[minco_plan::TriggerPlan]) -> Vec<String> {
            let mut keys: Vec<String> = triggers
                .iter()
                .map(|trigger| match trigger {
                    minco_plan::TriggerPlan::Sqs { id, .. }
                    | minco_plan::TriggerPlan::HttpApi { id, .. }
                    | minco_plan::TriggerPlan::Schedule { id, .. } => id.clone(),
                })
                .collect();
            keys.sort_unstable();
            keys
        }
        sorted
    };
    let sorted_functions = {
        fn sorted(functions: &[minco_plan::FunctionPlan]) -> Vec<String> {
            let mut names: Vec<String> = functions.iter().map(|f| f.name.clone()).collect();
            names.sort_unstable();
            names
        }
        sorted
    };
    assert_eq!(
        sorted_ids(&mail_first.queues),
        sorted_ids(&durable_first.queues)
    );
    assert_eq!(
        sorted_triggers(&mail_first.triggers),
        sorted_triggers(&durable_first.triggers)
    );
    assert_eq!(
        sorted_functions(&mail_first.functions),
        sorted_functions(&durable_first.functions)
    );

    // The composed template renders; the external `sam validate
    // --lint` gate over the exact artifact runs in
    // scripts/test/inbound_mail_template_parse.py.
    let mut code_uris = std::collections::BTreeMap::new();
    code_uris.insert("api".to_owned(), "./api.zip".to_owned());
    code_uris.insert("mail-worker".to_owned(), "./mail-worker.zip".to_owned());
    let template =
        render_sam_with_inbound_mail(&mail_first, &mail, &code_uris).expect("composed render");
    assert!(template.contains("AWS::SES::ReceiptRule"));
}

// ---- Exact-shape resource ownership regressions (exact-head review
// 5072859042): a same-ID queue/trigger/DLQ with a different shape is a
// collision, never something the sidecar adopts silently. ----

fn code_uris() -> std::collections::BTreeMap<String, String> {
    let mut uris = std::collections::BTreeMap::new();
    uris.insert("api".to_owned(), "./api.zip".to_owned());
    uris.insert("mail-worker".to_owned(), "./mail-worker.zip".to_owned());
    uris.insert("jobs-worker".to_owned(), "./jobs-worker.zip".to_owned());
    uris
}

fn codes(diagnostic: &[minco_plan::PlanDiagnostic]) -> Vec<&str> {
    diagnostic.iter().map(|d| d.code.as_str()).collect()
}

#[test]
fn existing_queue_with_wrong_dlq_or_shape_is_a_collision() {
    let topology = topology();
    let applied = apply_inbound_mail(&plan(), &topology);
    // Wrong DLQ on the wake queue.
    let mut wrong_dlq = applied.clone();
    wrong_dlq
        .queues
        .iter_mut()
        .find(|q| q.id == "mail-ticketing")
        .unwrap()
        .dead_letter_queue_id = Some("some-foreign-dlq".into());
    let found = validate_inbound_mail(&wrong_dlq, &topology);
    assert!(codes(&found).contains(&"MINCO-MAIL-014"));
    // No DLQ at all.
    let mut no_dlq = applied.clone();
    no_dlq
        .queues
        .iter_mut()
        .find(|q| q.id == "mail-ticketing")
        .unwrap()
        .dead_letter_queue_id = None;
    assert!(codes(&validate_inbound_mail(&no_dlq, &topology)).contains(&"MINCO-MAIL-014"));
    // FIFO wake queue: S3 direct notifications cannot target FIFO.
    let mut fifo = applied;
    fifo.queues
        .iter_mut()
        .find(|q| q.id == "mail-ticketing")
        .unwrap()
        .fifo = true;
    let found = validate_inbound_mail(&fifo, &topology);
    assert!(codes(&found).contains(&"MINCO-MAIL-015"));
    // Rendering refuses every one of these.
    for mismatched in [wrong_dlq, no_dlq, fifo] {
        assert!(
            render_sam_with_inbound_mail(&mismatched, &topology, &code_uris()).is_err(),
            "the renderer must refuse a shape collision"
        );
    }
}

/// Mutates the Sqs trigger carrying the wake id (helper hoisted before
/// the statements that use it).
fn mutate_ticketing_trigger(
    plan: &mut minco_plan::DeploymentPlan,
    mutation: fn(&mut minco_plan::TriggerPlan),
) {
    if let Some(trigger) = plan
        .triggers
        .iter_mut()
        .find(|t| matches!(t, minco_plan::TriggerPlan::Sqs { id, .. } if id == "ticketing-mail"))
    {
        mutation(trigger);
    }
}

#[test]
fn existing_trigger_with_expected_id_but_wrong_shape_is_a_collision() {
    let topology = topology();
    let applied = apply_inbound_mail(&plan(), &topology);
    // Wrong worker under the expected trigger id (the exact attack from
    // the review: the wake lands in the wrong consumer).
    let mut wrong_worker = applied.clone();
    mutate_ticketing_trigger(&mut wrong_worker, |trigger| {
        if let minco_plan::TriggerPlan::Sqs { function_id, .. } = trigger {
            *function_id = "jobs-worker".into();
        }
    });
    assert!(codes(&validate_inbound_mail(&wrong_worker, &topology)).contains(&"MINCO-MAIL-016"));
    // Wrong batching settings under the expected id.
    let mut wrong_window = applied.clone();
    mutate_ticketing_trigger(&mut wrong_window, |trigger| {
        if let minco_plan::TriggerPlan::Sqs {
            batching_window_seconds,
            ..
        } = trigger
        {
            *batching_window_seconds = 42;
        }
    });
    assert!(codes(&validate_inbound_mail(&wrong_window, &topology)).contains(&"MINCO-MAIL-016"));
    // Partial-batch reporting disabled under the expected id.
    let mut no_partial = applied;
    mutate_ticketing_trigger(&mut no_partial, |trigger| {
        if let minco_plan::TriggerPlan::Sqs {
            report_batch_item_failures,
            ..
        } = trigger
        {
            *report_batch_item_failures = false;
        }
    });
    assert!(codes(&validate_inbound_mail(&no_partial, &topology)).contains(&"MINCO-MAIL-016"));
    // Rendering refuses each mismatch.
    for mismatched in [wrong_worker, wrong_window, no_partial] {
        assert!(render_sam_with_inbound_mail(&mismatched, &topology, &code_uris()).is_err());
    }
}

#[test]
fn a_second_consumer_on_the_wake_queue_is_rejected() {
    // Competing Lambda consumers on one SQS queue steal messages; they
    // do not fan out.
    let mut shared = apply_inbound_mail(&plan(), &topology());
    shared.triggers.push(minco_plan::TriggerPlan::Sqs {
        id: "foreign-consumer".into(),
        function_id: "jobs-worker".into(),
        queue_id: "mail-ticketing".into(),
        batch_size: 10,
        batching_window_seconds: 1,
        report_batch_item_failures: true,
        maximum_concurrency: 2,
    });
    assert!(codes(&validate_inbound_mail(&shared, &topology())).contains(&"MINCO-MAIL-017"));
    assert!(render_sam_with_inbound_mail(&shared, &topology(), &code_uris()).is_err());
}

#[test]
fn durable_work_claiming_the_mail_queue_collides_in_both_orders() {
    use minco_plan::durable_work::{
        DurableWorkTopology, JobRoutePlan, WorkerProfilePlan, apply_durable_work,
    };
    let mut durable = DurableWorkTopology {
        enabled: true,
        profiles: vec![WorkerProfilePlan {
            id: "claims-mail".into(),
            // The SAME queue id the inbound binding owns.
            queue_id: "mail-ticketing".into(),
            function_id: "jobs-worker".into(),
            artifact_path: "target/lambda/jobs-worker.zip".into(),
            fifo: false,
            batch_size: 10,
            batching_window_seconds: 1,
            maximum_concurrency: 2,
            memory_mb: 512,
            timeout_seconds: 30,
            reserved_concurrency: 2,
            max_payload_bytes: 262_144,
            database_connections_per_instance: 1,
            dead_letter_queue_id: None,
            max_receive_count: None,
            data_classes: vec!["internal".into()],
            required_capabilities: vec!["notifications.send".into()],
        }],
        routes: vec![JobRoutePlan {
            job_name: "orders.send-confirmation".into(),
            job_version: 1,
            worker_profile: "claims-mail".into(),
            ordering_source: None,
        }],
        schedules: vec![],
    };
    let mail = topology();
    let base = plan();
    let mail_first = apply_inbound_mail(&apply_durable_work(&base, &durable), &mail);
    let durable_first = apply_durable_work(&apply_inbound_mail(&base, &mail), &durable);
    // Durable-first order: the durable sidecar's queue-key dedup
    // silently skips BOTH its queue and its mapping — the inbound
    // resources are intact but the durable profile lost its mapping,
    // which the durable validator now fails closed on.
    let diagnostics = validate_inbound_mail(&mail_first, &mail);
    let found = codes(&diagnostics);
    assert!(
        found.contains(&"MINCO-MAIL-014") || found.contains(&"MINCO-MAIL-017"),
        "a durable-work claim on the mail queue must collide: {found:?}"
    );
    assert!(render_sam_with_inbound_mail(&mail_first, &mail, &code_uris()).is_err());
    let durable_diagnostics =
        minco_plan::durable_work::validate_durable_work(&durable_first, &durable);
    let durable_codes = codes(&durable_diagnostics);
    assert!(
        durable_codes.contains(&"MINCO-JOBS-020")
            || durable_codes.contains(&"MINCO-JOBS-021")
            || durable_codes.contains(&"MINCO-JOBS-023"),
        "the durable profile that lost its mapping to the wake queue must fail closed: {durable_codes:?}"
    );
    // The reverse direction: inbound claiming the DURABLE queue also
    // collides (the wake shape differs from the durable shape).
    durable.profiles[0].queue_id = "jobs-orders-notifications".into();
    let mut claimer = binding();
    claimer.queue_id = "jobs-orders-notifications".into();
    let claim_topology = InboundMailTopology {
        enabled: true,
        bindings: vec![claimer],
    };
    let composed = apply_inbound_mail(&apply_durable_work(&base, &durable), &claim_topology);
    assert!(codes(&validate_inbound_mail(&composed, &claim_topology)).contains(&"MINCO-MAIL-014"));
}

#[test]
fn binding_ids_collapsing_to_one_sam_logical_id_are_rejected() {
    // `ticket-ing` and `ticket--ing` both normalize to the CloudFormation
    // logical id `TicketIng`; two provider chains would render onto one
    // resource.
    let mut first = binding();
    first.id = "ticket-ing".into();
    let mut second = binding();
    second.id = "ticket--ing".into();
    second.queue_id = "mail-other".into();
    second.bucket_name = "orders-dev-raw-other".into();
    second.mailbox_scope = "other@example.test".into();
    let collapsing = InboundMailTopology {
        enabled: true,
        bindings: vec![first, second],
    };
    assert!(codes(&validate_inbound_mail(&plan(), &collapsing)).contains(&"MINCO-MAIL-018"));
    assert!(render_sam_with_inbound_mail(&plan(), &collapsing, &code_uris()).is_err());
}

#[test]
fn exactly_matching_preexisting_resources_remain_an_idempotent_reuse() {
    // Applying twice creates the expected resources once; the second
    // pass sees semantically identical same-ID resources and neither
    // creates duplicates nor reports collisions.
    let once = apply_inbound_mail(&plan(), &topology());
    let twice = apply_inbound_mail(&once, &topology());
    assert_eq!(once, twice);
    assert!(validate_inbound_mail(&twice, &topology()).is_empty());
    assert!(render_sam_with_inbound_mail(&twice, &topology(), &code_uris()).is_ok());
}

#[test]
fn reordering_bindings_does_not_change_the_rule_set_identity() {
    let mut second = binding();
    second.id = "billing".into();
    second.queue_id = "mail-billing".into();
    second.bucket_name = "orders-dev-raw-billing".into();
    // One wake discipline per worker AND one mailbox per binding
    // (exact-head review 5083559431 P1): duplicate recipients would
    // silently fan one mail into both bindings.
    second.mailbox_scope = "billing@example.test".into();
    second.worker_function_id = "jobs-worker".into();
    let ordered = InboundMailTopology {
        enabled: true,
        bindings: vec![binding(), second.clone()],
    };
    let reversed = InboundMailTopology {
        enabled: true,
        bindings: vec![second, binding()],
    };
    let base = plan();
    let applied_ordered = apply_inbound_mail(&base, &ordered);
    let applied_reversed = apply_inbound_mail(&base, &reversed);
    let first_template =
        render_sam_with_inbound_mail(&applied_ordered, &ordered, &code_uris()).expect("render");
    let second_template =
        render_sam_with_inbound_mail(&applied_reversed, &reversed, &code_uris()).expect("render");
    let name_of = |template: &str| {
        template
            .lines()
            .find(|line| line.trim_start().starts_with("RuleSetName: "))
            .expect("rule set name")
            .trim()
            .trim_start_matches("RuleSetName: ")
            .trim_matches('\'')
            .to_owned()
    };
    let first_name = name_of(&first_template);
    let second_name = name_of(&second_template);
    assert_eq!(
        first_name, second_name,
        "binding order must not change the provider deployment identity"
    );
    assert!(first_name.starts_with("orders-dev-inbound-mail-"));
    // A different application/environment identity produces a different
    // name: prove it on a renamed plan.
    let mut other_app = base;
    other_app.application = "billing".into();
    let other_applied = apply_inbound_mail(&other_app, &ordered);
    let other_template =
        render_sam_with_inbound_mail(&other_applied, &ordered, &code_uris()).expect("render");
    let other_name = name_of(&other_template);
    assert_ne!(first_name, other_name);
    assert!(other_name.starts_with("billing-dev-inbound-mail-"));
}

// ---- Round-9 regressions (exact-head review 5083559431) ----

#[test]
fn rule_set_name_respects_the_ses_limits_at_every_boundary() {
    // Both prefixes at their maximum allocation, punctuation-only
    // input, and same-first-20-chars collisions: the name must stay
    // within 64 characters, be [a-z0-9-] with alphanumeric ends, and
    // differ for different FULL identities.
    let topology = topology();
    let base = plan();
    let maxed = {
        let mut p = base.clone();
        p.application = "abcdefghijklmnopqrstuvwxyz0123456789".repeat(3);
        p.environment = "zyxwvutsrqponmlkjihgfedcba".repeat(3);
        p
    };
    let applied = apply_inbound_mail(&maxed, &topology);
    let template = render_sam_with_inbound_mail(&applied, &topology, &code_uris()).expect("render");
    let name = template
        .lines()
        .find(|line| line.trim_start().starts_with("RuleSetName: "))
        .expect("rule set name")
        .trim()
        .trim_start_matches("RuleSetName: ")
        .trim_matches('\'');
    assert!(name.len() <= 64, "SES limit: {} ({name})", name.len());
    assert!(
        name.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    );
    assert!(
        name.chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphanumeric())
    );
    assert!(
        name.chars()
            .last()
            .is_some_and(|c| c.is_ascii_alphanumeric())
    );

    // Punctuation-only application sanitizes to a placeholder, never a
    // leading separator.
    let mut punct = base.clone();
    punct.application = "---...---".into();
    let applied = apply_inbound_mail(&punct, &topology);
    let template = render_sam_with_inbound_mail(&applied, &topology, &code_uris()).expect("render");
    let name = template
        .lines()
        .find(|line| line.trim_start().starts_with("RuleSetName: "))
        .expect("rule set name")
        .trim()
        .trim_start_matches("RuleSetName: ")
        .trim_matches('\'');
    assert!(name.starts_with(|c: char| c.is_ascii_alphanumeric()));

    // Long-prefix collision: two applications sharing their first 20
    // characters must NOT share a rule set — the digest covers the
    // full identity.
    let name_of_plan = |application: &str| {
        let mut p = base.clone();
        p.application = application.into();
        let applied = apply_inbound_mail(&p, &topology);
        let template =
            render_sam_with_inbound_mail(&applied, &topology, &code_uris()).expect("render");
        template
            .lines()
            .find(|line| line.trim_start().starts_with("RuleSetName: "))
            .expect("rule set name")
            .trim()
            .trim_start_matches("RuleSetName: ")
            .trim_matches('\'')
            .to_owned()
    };
    let shared_prefix = "abcdefghijklmnopqrs";
    let left = name_of_plan(&format!("{shared_prefix}-product-a-with-a-long-name"));
    let right = name_of_plan(&format!("{shared_prefix}-product-b-with-a-long-name"));
    assert_ne!(
        left, right,
        "the full identity digest must distinguish long prefixes"
    );

    // Same application, different full environment.
    let mut other_env = base.clone();
    other_env.environment = "production-with-a-long-name".into();
    let applied_env = apply_inbound_mail(&other_env, &topology);
    let env_template =
        render_sam_with_inbound_mail(&applied_env, &topology, &code_uris()).expect("render");
    let env_name = env_template
        .lines()
        .find(|line| line.trim_start().starts_with("RuleSetName: "))
        .expect("rule set name")
        .trim()
        .trim_start_matches("RuleSetName: ")
        .trim_matches('\'');
    let applied_dev = apply_inbound_mail(&base, &topology);
    let dev_template =
        render_sam_with_inbound_mail(&applied_dev, &topology, &code_uris()).expect("render");
    let dev_name = dev_template
        .lines()
        .find(|line| line.trim_start().starts_with("RuleSetName: "))
        .expect("rule set name")
        .trim()
        .trim_start_matches("RuleSetName: ")
        .trim_matches('\'');
    assert_ne!(env_name, dev_name);

    // The same full input is stable.
    let again = name_of_plan(&format!("{shared_prefix}-product-a-with-a-long-name"));
    assert_eq!(left, again);
}

#[test]
fn duplicate_mailboxes_and_buckets_are_rejected() {
    // Duplicate recipient: SES evaluates every matching rule, so two
    // bindings routing one mailbox is an accidental fan-out.
    let mut duplicate_mailbox = binding();
    duplicate_mailbox.id = "second-route".into();
    duplicate_mailbox.queue_id = "mail-second".into();
    duplicate_mailbox.bucket_name = "orders-dev-raw-second".into();
    let topology = InboundMailTopology {
        enabled: true,
        bindings: vec![binding(), duplicate_mailbox],
    };
    let diagnostics = validate_inbound_mail(&plan(), &topology);
    let found = codes(&diagnostics);
    assert!(found.contains(&"MINCO-MAIL-019"), "{found:?}");
    assert!(render_sam_with_inbound_mail(&plan(), &topology, &code_uris()).is_err());

    // Duplicate physical bucket: two logical resources cannot own one
    // provider bucket name.
    let mut duplicate_bucket = binding();
    duplicate_bucket.id = "second-bucket".into();
    duplicate_bucket.queue_id = "mail-second".into();
    duplicate_bucket.mailbox_scope = "billing@example.test".into();
    let topology = InboundMailTopology {
        enabled: true,
        bindings: vec![binding(), duplicate_bucket],
    };
    let diagnostics = validate_inbound_mail(&plan(), &topology);
    let found = codes(&diagnostics);
    assert!(found.contains(&"MINCO-MAIL-020"), "{found:?}");
    assert!(render_sam_with_inbound_mail(&plan(), &topology, &code_uris()).is_err());
}

#[test]
fn clean_create_dependency_graph_is_acyclic_and_provider_ordered() {
    // Exact-head review 5083559431 P0-1/P0-2: prove the rendered
    // graph orders Queue -> QueuePolicy -> Bucket -> BucketPolicy ->
    // ReceiptRule, the ReceiptRule has a REAL dependency on the rule
    // set (!Ref, not a literal), and the queue policy never references
    // the bucket resource.
    use std::collections::BTreeMap;
    let topology = topology();
    let applied = apply_inbound_mail(&plan(), &topology);
    let template = render_sam_with_inbound_mail(&applied, &topology, &code_uris()).expect("render");
    // The queue policy's SourceArn uses the explicit bucket name.
    assert!(
        template.contains("aws:SourceArn: !Sub 'arn:${AWS::Partition}:s3:::orders-dev-raw-mail'")
    );
    assert!(
        !template
            .matches("aws:SourceArn: !GetAtt TicketingRawMailBucket.Arn")
            .count()
            > 0
    );
    // The bucket waits for the queue policy.
    assert!(
        template
            .contains("TicketingRawMailBucket:\n    Type: AWS::S3::Bucket\n    DependsOn: [TicketingMailQueuePolicy]")
    );
    // The rule waits for the bucket policy AND references the rule set.
    assert!(
        template.contains("DependsOn: [TicketingRawMailBucketPolicy, TicketingMailQueuePolicy]")
    );
    assert!(template.contains("RuleSetName: !Ref InboundMailReceiptRuleSet"));

    // Structural acyclicity over DependsOn edges PLUS intrinsic
    // references (ego-chat cycle-1 non-blocking strengthening): `!Ref X`,
    // `!GetAtt X.Y` and `!Sub '…${X.Y}…'` all resolve to a real edge
    // X -> resource, so the graph check no longer leans on source-text
    // order for the queue-policy-to-queue, bucket-policy-to-bucket and
    // rule-to-rule-set relationships.
    let document = serde_yaml_ng::from_str::<serde_yaml_ng::Value>(&template).expect("yaml");
    let resources = document
        .get("Resources")
        .and_then(|value| value.as_mapping())
        .expect("resources");
    let mut graph: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (key, value) in resources {
        let name = key.as_str().expect("resource name").to_owned();
        let mut depends = value
            .get("DependsOn")
            .and_then(|list| list.as_sequence())
            .map(|list| {
                list.iter()
                    .filter_map(|item| item.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();
        intrinsic_edges(value, &mut depends);
        depends.sort();
        depends.dedup();
        graph.insert(name, depends);
    }
    // The intrinsic edges the review called out are present.
    assert!(graph["TicketingRawMailBucketPolicy"].contains(&"TicketingRawMailBucket".to_owned()));
    assert!(graph["TicketingMailQueuePolicy"].contains(&"MailTicketingQueue".to_owned()));
    assert!(graph["TicketingReceiptRule"].contains(&"InboundMailReceiptRuleSet".to_owned()));
    // Kahn's algorithm: the graph must be a DAG.
    let mut pending: Vec<String> = graph.keys().cloned().collect();
    let mut resolved: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut progressed = true;
    while progressed {
        progressed = false;
        pending.retain(|name| {
            let ready = graph[name]
                .iter()
                .all(|dep| resolved.contains(dep) || !graph.contains_key(dep));
            if ready {
                resolved.insert(name.clone());
                progressed = true;
                false
            } else {
                true
            }
        });
    }
    assert!(
        pending.is_empty(),
        "the rendered dependency graph must be acyclic; unresolved: {pending:?}"
    );
    // Provider ordering: rule set and queue precede the rule; queue
    // policy precedes the bucket.
    let position = |needle: &str| template.find(needle).expect(needle);
    assert!(position("InboundMailReceiptRuleSet:\n") < position("TicketingReceiptRule:\n"));
    assert!(position("MailTicketingQueue:\n") < position("TicketingMailQueuePolicy:\n"));
}

// ---- Ego-chat cycle-1 regressions (AC-4 canonical identity / AC-5
// canonical provider order) ----

/// One binding-field mutation used by the identity regressions.
type BindingMutation = fn(&mut InboundMailBinding);

/// Resolves `CloudFormation` intrinsic references (ego-chat cycle-1
/// non-blocking strengthening): `!Ref X`, `!GetAtt X.Y` and
/// `!Sub '…${X.Y}…'` all add the edge `X -> resource`.
fn intrinsic_edges(value: &serde_yaml_ng::Value, edges: &mut Vec<String>) {
    use serde_yaml_ng::Value;
    match value {
        Value::Tagged(tagged) => {
            let tag = tagged.tag.to_string();
            if tag.ends_with("Ref") || tag.ends_with("GetAtt") {
                if let Some(text) = tagged.value.as_str() {
                    let target = text.split('.').next().unwrap_or(text);
                    edges.push(target.to_owned());
                } else if let Some(Value::String(target)) =
                    tagged.value.as_sequence().and_then(|s| s.first())
                {
                    edges.push(target.clone());
                }
            } else if tag.ends_with("Sub")
                && let Some(text) = tagged.value.as_str()
            {
                for piece in text.split("${").skip(1) {
                    let name = piece.split('}').next().unwrap_or(piece);
                    let target = name.split('.').next().unwrap_or(name);
                    if !target.starts_with("AWS::") && !target.is_empty() {
                        edges.push(target.to_owned());
                    }
                }
            }
            intrinsic_edges(&tagged.value, edges);
        }
        Value::Sequence(sequence) => {
            for item in sequence {
                intrinsic_edges(item, edges);
            }
        }
        Value::Mapping(mapping) => {
            for item in mapping.values() {
                intrinsic_edges(item, edges);
            }
        }
        _ => {}
    }
}

#[test]
fn receipt_rules_form_a_canonical_after_chain() {
    // The ordered provider rule sequence — logical id, rule name and
    // predecessor — compared across renders to prove order invariance.
    fn receipt_rule_sequence(template: &str) -> Vec<(String, String, Option<String>)> {
        let document = serde_yaml_ng::from_str::<serde_yaml_ng::Value>(template).expect("yaml");
        let resources = document
            .get("Resources")
            .and_then(|value| value.as_mapping())
            .expect("resources");
        resources
            .iter()
            .filter(|(key, _)| {
                key.as_str()
                    .is_some_and(|name| name.ends_with("ReceiptRule"))
            })
            .map(|(key, value)| {
                let rule = value
                    .get("Properties")
                    .and_then(|properties| properties.get("Rule"))
                    .expect("Rule properties");
                (
                    key.as_str().expect("rule logical id").to_owned(),
                    rule.get("Name")
                        .and_then(|name| name.as_str())
                        .expect("rule name")
                        .to_owned(),
                    rule.get("After")
                        .and_then(|after| after.as_str())
                        .map(str::to_owned),
                )
            })
            .collect()
    }
    // Ego-chat cycle-1 review, AC-5: the canonical binding order drives
    // receipt-rule rendering — the first rule has no predecessor, every
    // later rule names the previous rule with SES `After` AND carries a
    // real DependsOn edge on the previous rule resource, and reversing
    // the input bindings produces a byte-identical template.
    let mut billing = binding();
    billing.id = "billing".into();
    billing.queue_id = "mail-billing".into();
    billing.bucket_name = "orders-dev-raw-billing".into();
    billing.mailbox_scope = "billing@example.test".into();
    billing.worker_function_id = "jobs-worker".into();
    let ordered = InboundMailTopology {
        enabled: true,
        bindings: vec![binding(), billing.clone()],
    };
    let reversed = InboundMailTopology {
        enabled: true,
        bindings: vec![billing, binding()],
    };
    let base = plan();
    let first_template =
        render_sam_with_inbound_mail(&apply_inbound_mail(&base, &ordered), &ordered, &code_uris())
            .expect("render");
    let second_template = render_sam_with_inbound_mail(
        &apply_inbound_mail(&base, &reversed),
        &reversed,
        &code_uris(),
    )
    .expect("render");
    assert_eq!(
        receipt_rule_sequence(&first_template),
        receipt_rule_sequence(&second_template),
        "input order must never change the rendered provider rule order"
    );

    let document = serde_yaml_ng::from_str::<serde_yaml_ng::Value>(&first_template).expect("yaml");
    let resources = document
        .get("Resources")
        .and_then(|value| value.as_mapping())
        .expect("resources");
    let receipt_rules: Vec<(String, serde_yaml_ng::Value)> = resources
        .iter()
        .filter(|(key, _)| {
            key.as_str()
                .is_some_and(|name| name.ends_with("ReceiptRule"))
        })
        .map(|(key, value)| {
            (
                key.as_str().expect("rule logical id").to_owned(),
                value.clone(),
            )
        })
        .collect();
    assert_eq!(receipt_rules.len(), 2, "two bindings render two rules");
    // Canonical order sorts `billing` before `ticketing`.
    assert_eq!(receipt_rules[0].0, "BillingReceiptRule");
    assert_eq!(receipt_rules[1].0, "TicketingReceiptRule");
    let rule_of = |resource: &serde_yaml_ng::Value| {
        resource
            .get("Properties")
            .and_then(|properties| properties.get("Rule"))
            .cloned()
            .expect("Rule properties")
    };
    let first_rule = rule_of(&receipt_rules[0].1);
    assert!(
        first_rule.get("After").is_none(),
        "the first canonical rule has no predecessor"
    );
    let second_rule = rule_of(&receipt_rules[1].1);
    assert_eq!(
        second_rule.get("After").and_then(|after| after.as_str()),
        Some("billing-inbound-mail"),
        "the second rule chains to the canonical predecessor by name"
    );
    let second_depends = receipt_rules[1]
        .1
        .get("DependsOn")
        .and_then(|list| list.as_sequence())
        .expect("second rule DependsOn")
        .iter()
        .filter_map(|item| item.as_str())
        .collect::<Vec<_>>();
    assert!(
        second_depends.contains(&"BillingReceiptRule"),
        "the second rule carries a real dependency on the predecessor resource: {second_depends:?}"
    );
}

#[test]
fn binding_shape_fields_change_the_rule_set_identity() {
    // Ego-chat cycle-1 review, AC-4: EVERY material binding field is
    // part of the canonical framed identity — key prefix, retention,
    // batching and concurrency changes must all change the rule-set
    // name — while mailbox casing/whitespace follow the documented
    // identity rule (trim + ASCII lowercase) and stay the SAME name.
    let name_for = |mutated: &InboundMailBinding| {
        let topology = InboundMailTopology {
            enabled: true,
            bindings: vec![mutated.clone()],
        };
        let template = render_sam_with_inbound_mail(
            &apply_inbound_mail(&plan(), &topology),
            &topology,
            &code_uris(),
        )
        .expect("render");
        template
            .lines()
            .find(|line| line.trim_start().starts_with("RuleSetName: "))
            .expect("rule set name")
            .trim()
            .trim_start_matches("RuleSetName: ")
            .trim_matches('\'')
            .to_owned()
    };
    let base = name_for(&binding());
    let mutations: [(&str, BindingMutation); 6] = [
        ("key_prefix", |b: &mut InboundMailBinding| {
            b.key_prefix = "inbox/".into();
        }),
        ("retention", |b: &mut InboundMailBinding| {
            b.retention_days += 1;
        }),
        ("batch_size", |b: &mut InboundMailBinding| {
            b.batch_size += 1;
        }),
        ("batching_window", |b: &mut InboundMailBinding| {
            b.batching_window_seconds += 1;
        }),
        ("maximum_concurrency", |b: &mut InboundMailBinding| {
            b.maximum_concurrency += 1;
        }),
        ("queue", |b: &mut InboundMailBinding| {
            b.queue_id = "mail-other".into();
        }),
    ];
    for (label, mutate) in mutations {
        let mut other = binding();
        mutate(&mut other);
        assert_ne!(
            base,
            name_for(&other),
            "changing {label} must change the deployment identity"
        );
    }
    // Documented mailbox identity rule: trim + ASCII lowercase.
    let mut variant = binding();
    variant.mailbox_scope = "  Support@Example.TEST ".into();
    assert_eq!(
        base,
        name_for(&variant),
        "mailbox casing and surrounding whitespace are one identity"
    );
}

#[test]
fn control_characters_and_delimiters_cannot_ambiguate_the_identity() {
    // Ego-chat cycle-1 review, AC-4: control characters in a mailbox
    // are rejected BEFORE rendering (the crafted one-binding-versus-two
    // collision from the review embeds a newline), and delimiter-heavy
    // but legal mailboxes cannot make two distinct binding sets encode
    // identically because every field is framed individually.
    let mut crafted = binding();
    crafted.mailbox_scope = "u@example.test|q|w|bucket\nb|v@example.test".into();
    let topology = InboundMailTopology {
        enabled: true,
        bindings: vec![crafted],
    };
    let diagnostics = validate_inbound_mail(&plan(), &topology);
    assert!(
        codes(&diagnostics).contains(&"MINCO-MAIL-008"),
        "control characters must fail validation: {:?}",
        codes(&diagnostics)
    );
    let rendered = render_sam_with_inbound_mail(
        &apply_inbound_mail(&plan(), &topology),
        &topology,
        &code_uris(),
    );
    assert!(
        rendered.is_err(),
        "the renderer must refuse a non-empty validation"
    );

    let name_for = |bindings: &[InboundMailBinding]| {
        let topology = InboundMailTopology {
            enabled: true,
            bindings: bindings.to_vec(),
        };
        let template = render_sam_with_inbound_mail(
            &apply_inbound_mail(&plan(), &topology),
            &topology,
            &code_uris(),
        )
        .expect("render");
        template
            .lines()
            .find(|line| line.trim_start().starts_with("RuleSetName: "))
            .expect("rule set name")
            .trim()
            .trim_start_matches("RuleSetName: ")
            .trim_matches('\'')
            .to_owned()
    };
    let mut delimiters = binding();
    delimiters.mailbox_scope = "u@example.test|q|w|bucket".into();
    let single = name_for(&[delimiters.clone()]);
    let mut second = binding();
    second.id = "billing".into();
    second.queue_id = "mail-billing".into();
    second.bucket_name = "orders-dev-raw-billing".into();
    second.mailbox_scope = "b|v@example.test".into();
    second.worker_function_id = "jobs-worker".into();
    let pair = name_for(&[delimiters, second]);
    assert_ne!(
        single, pair,
        "framed encoding cannot let one binding imitate two"
    );
}

#[test]
fn durable_work_rejects_same_id_resources_with_wrong_shapes() {
    // Exact-head review 5083559431 P0-3: a base plan pre-providing a
    // wrong-shape function, queue or mapping under the profile's ids
    // must fail closed, never be silently adopted.
    use minco_plan::durable_work::{
        DurableWorkTopology, JobRoutePlan, WorkerProfilePlan, apply_durable_work,
        validate_durable_work,
    };
    let durable = DurableWorkTopology {
        enabled: true,
        profiles: vec![WorkerProfilePlan {
            id: "orders-notifications".into(),
            queue_id: "jobs-orders-notifications".into(),
            function_id: "jobs-worker".into(),
            artifact_path: "target/lambda/jobs-worker.zip".into(),
            fifo: false,
            batch_size: 10,
            batching_window_seconds: 1,
            maximum_concurrency: 2,
            memory_mb: 512,
            timeout_seconds: 30,
            reserved_concurrency: 2,
            max_payload_bytes: 262_144,
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
        schedules: vec![],
    };
    let base = plan();
    let applied = apply_durable_work(&base, &durable);
    assert!(validate_durable_work(&applied, &durable).is_empty());

    // Wrong artifact under the expected function name.
    let mut wrong_artifact = applied.clone();
    wrong_artifact
        .functions
        .iter_mut()
        .find(|f| f.name == "jobs-worker")
        .unwrap()
        .artifact_path = "wrong-worker.zip".into();
    assert!(codes(&validate_durable_work(&wrong_artifact, &durable)).contains(&"MINCO-JOBS-022"));

    // Wrong timeout under the expected function name.
    let mut wrong_timeout = applied.clone();
    wrong_timeout
        .functions
        .iter_mut()
        .find(|f| f.name == "jobs-worker")
        .unwrap()
        .timeout_seconds = 10;
    assert!(codes(&validate_durable_work(&wrong_timeout, &durable)).contains(&"MINCO-JOBS-022"));

    // Wrong queue redrive under the expected queue id.
    let mut wrong_redrive = applied.clone();
    wrong_redrive
        .queues
        .iter_mut()
        .find(|q| q.id == "jobs-orders-notifications")
        .unwrap()
        .dead_letter_queue_id = Some("foreign-dlq".into());
    assert!(codes(&validate_durable_work(&wrong_redrive, &durable)).contains(&"MINCO-JOBS-021"));

    // Wrong batching under the expected mapping id.
    let mut wrong_batching = applied.clone();
    if let Some(minco_plan::TriggerPlan::Sqs {
        batching_window_seconds,
        ..
    }) = wrong_batching.triggers.iter_mut().find(|t| {
        matches!(t, minco_plan::TriggerPlan::Sqs { id, .. } if id == "orders-notifications-mapping")
    }) {
        *batching_window_seconds = 42;
    }
    assert!(codes(&validate_durable_work(&wrong_batching, &durable)).contains(&"MINCO-JOBS-023"));

    // A second consumer on the profile queue.
    let mut second_consumer = applied.clone();
    second_consumer.triggers.push(minco_plan::TriggerPlan::Sqs {
        id: "foreign-worker-mapping".into(),
        function_id: "mail-worker".into(),
        queue_id: "jobs-orders-notifications".into(),
        batch_size: 5,
        batching_window_seconds: 1,
        report_batch_item_failures: true,
        maximum_concurrency: 1,
    });
    assert!(codes(&validate_durable_work(&second_consumer, &durable)).contains(&"MINCO-JOBS-024"));

    // Exact-shape repeat application remains idempotent.
    let twice = apply_durable_work(&applied, &durable);
    assert_eq!(applied, twice);
    assert!(validate_durable_work(&twice, &durable).is_empty());
}
