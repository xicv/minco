use minco_dev::{
    CommandSpec, DevDatabase, DevGraph, DevOptions, DevPlan, LifecycleKind, ProcessConfig,
    ProcessRole, ReadinessProbe, ServiceKind,
};
use std::collections::{BTreeMap, BTreeSet};

fn command(program: &str, arguments: &[&str]) -> CommandSpec {
    CommandSpec {
        program: program.into(),
        arguments: arguments.iter().map(ToString::to_string).collect(),
        environment: BTreeMap::new(),
    }
}

fn process(id: &str, default_enabled: bool) -> ProcessConfig {
    ProcessConfig {
        id: id.into(),
        command: command("cargo", &["run", "--bin", id]),
        readiness: ReadinessProbe::Process,
        default_enabled,
    }
}

fn graph() -> DevGraph {
    DevGraph {
        application: "orders".into(),
        environment: "local".into(),
        compose_file: "infra/local/compose.yaml".into(),
        database: DevDatabase::Postgres,
        local_aws_services: vec!["sts".into(), "ssm".into()],
        api: ProcessConfig {
            id: "api".into(),
            command: command("cargo", &["run", "--bin", "orders-local"]),
            readiness: ReadinessProbe::Http {
                url: "http://127.0.0.1:3000/health/ready".into(),
            },
            default_enabled: true,
        },
        workers: vec![process("worker-z", false), process("worker-a", true)],
        frontend: Some(process("frontend", false)),
        migration: Some(command("cargo", &["minco", "db", "migrate"])),
        seeds: BTreeMap::from([(
            "demo".into(),
            command("cargo", &["minco", "db", "seed", "--profile", "demo"]),
        )]),
        schedules: vec!["nightly-reconciliation".into()],
    }
}

#[test]
fn safe_defaults_derive_only_declared_local_services_and_never_seed_or_schedule() {
    let plan = DevPlan::derive(&graph(), &DevOptions::default()).expect("valid development plan");

    assert_eq!(plan.schema_version, 1);
    assert_eq!(plan.environment, "local");
    assert!(!plan.external_aws_contact);
    assert_eq!(
        plan.services
            .iter()
            .map(|service| (&service.kind, service.port))
            .collect::<Vec<_>>(),
        [
            (&ServiceKind::Postgres, Some(55_432)),
            (&ServiceKind::Rustack, Some(4_566)),
        ]
    );
    assert_eq!(plan.services[1].aws_services, ["ssm", "sts"]);
    assert_eq!(
        plan.lifecycle
            .iter()
            .map(|step| step.kind)
            .collect::<Vec<_>>(),
        [LifecycleKind::Migrate]
    );
    assert_eq!(
        plan.processes
            .iter()
            .map(|process| (process.id.as_str(), process.role))
            .collect::<Vec<_>>(),
        [("api", ProcessRole::Api), ("worker-a", ProcessRole::Worker),]
    );
    assert_eq!(plan.omitted_schedule_ids, ["nightly-reconciliation"]);
}

#[test]
fn explicit_worker_seed_frontend_and_ports_are_deterministic() {
    let options = DevOptions {
        seed: Some("demo".into()),
        with_workers: BTreeSet::from(["worker-z".into()]),
        without_workers: BTreeSet::from(["worker-a".into()]),
        frontend: Some(true),
        port: Some(31_000),
        rustack_port: Some(45_666),
        ..DevOptions::default()
    };

    let first = DevPlan::derive(&graph(), &options).expect("valid development plan");
    let second = DevPlan::derive(&graph(), &options).expect("repeatable development plan");

    assert_eq!(first, second);
    assert_eq!(first.services[1].port, Some(45_666));
    assert_eq!(
        first
            .lifecycle
            .iter()
            .map(|step| step.kind)
            .collect::<Vec<_>>(),
        [LifecycleKind::Migrate, LifecycleKind::Seed]
    );
    assert_eq!(
        first
            .processes
            .iter()
            .map(|process| (process.id.as_str(), process.role))
            .collect::<Vec<_>>(),
        [
            ("api", ProcessRole::Api),
            ("worker-z", ProcessRole::Worker),
            ("frontend", ProcessRole::Frontend),
        ]
    );
    assert_eq!(
        first.processes[0].command.environment.get("PORT"),
        Some(&"31000".to_owned())
    );
    assert_eq!(
        first.processes[0].readiness,
        ReadinessProbe::Http {
            url: "http://127.0.0.1:31000/health/ready".into(),
        }
    );
}

#[test]
fn unknown_workers_and_unsupported_local_aws_services_fail_closed() {
    let options = DevOptions {
        with_workers: BTreeSet::from(["missing-worker".into()]),
        ..DevOptions::default()
    };
    let error = DevPlan::derive(&graph(), &options).expect_err("unknown worker must fail");
    assert!(error.to_string().contains("missing-worker"));

    let mut unsupported = graph();
    unsupported.local_aws_services.push("lambda".into());
    let error =
        DevPlan::derive(&unsupported, &DevOptions::default()).expect_err("unsupported AWS seam");
    assert!(error.to_string().contains("lambda"));
}

#[test]
fn service_activation_and_cleanup_commands_are_explicit_and_deterministic() {
    let plan = DevPlan::derive(&graph(), &DevOptions::default()).expect("valid development plan");

    assert_eq!(
        plan.services[0].start,
        Some(command(
            "docker",
            &[
                "compose",
                "-f",
                "infra/local/compose.yaml",
                "up",
                "-d",
                "--wait",
                "postgres",
            ],
        ))
    );
    assert_eq!(
        plan.services[0].stop,
        Some(command(
            "docker",
            &[
                "compose",
                "-f",
                "infra/local/compose.yaml",
                "stop",
                "postgres",
            ],
        ))
    );
    let rustack_start = plan.services[1].start.as_ref().expect("Rustack start");
    assert_eq!(
        rustack_start.environment.get("MINCO_RUSTACK_SERVICES"),
        Some(&"ssm,sts".to_owned())
    );
    assert_eq!(
        rustack_start.environment.get("MINCO_RUSTACK_PORT"),
        Some(&"4566".to_owned())
    );
}

#[test]
fn explicitly_requested_undeclared_frontend_fails_closed() {
    let mut graph = graph();
    graph.frontend = None;
    let options = DevOptions {
        frontend: Some(true),
        ..DevOptions::default()
    };

    let error = DevPlan::derive(&graph, &options).expect_err("frontend must be declared");
    assert!(error.to_string().contains("frontend"));
}

#[test]
fn serialized_commands_retain_secret_names_but_never_secret_values() {
    let command = CommandSpec {
        program: "server".into(),
        arguments: Vec::new(),
        environment: BTreeMap::from([
            (
                "DATABASE_URL".into(),
                "postgres://user:secret@db/app".into(),
            ),
            ("API_KEY".into(), "minco-api-key-value".into()),
            ("AWS_ACCESS_KEY_ID".into(), "minco-access-key-id".into()),
            ("PORT".into(), "3000".into()),
        ]),
    };

    let serialized = serde_json::to_value(command).expect("serialized command");
    assert_eq!(serialized["environment"]["DATABASE_URL"], "<redacted>");
    assert_eq!(serialized["environment"]["API_KEY"], "<redacted>");
    assert_eq!(serialized["environment"]["AWS_ACCESS_KEY_ID"], "<redacted>");
    assert_eq!(serialized["environment"]["PORT"], "3000");
    assert!(!serialized.to_string().contains("user:secret"));
    assert!(!serialized.to_string().contains("minco-api-key-value"));
    assert!(!serialized.to_string().contains("minco-access-key-id"));
}

#[test]
fn conflicting_worker_overrides_fail_instead_of_silently_choosing_precedence() {
    let options = DevOptions {
        with_workers: BTreeSet::from(["worker-a".into()]),
        without_workers: BTreeSet::from(["worker-a".into()]),
        ..DevOptions::default()
    };

    let error = DevPlan::derive(&graph(), &options).expect_err("conflicting worker selection");
    assert!(error.to_string().contains("worker-a"));
    assert!(error.to_string().contains("both"));
}

#[test]
fn duplicate_process_identifiers_fail_before_any_process_plan_is_emitted() {
    let mut graph = graph();
    graph.workers.push(process("worker-a", true));

    let error = DevPlan::derive(&graph, &DevOptions::default()).expect_err("duplicate worker");
    assert!(error.to_string().contains("worker-a"));
    assert!(error.to_string().contains("duplicate"));
}
