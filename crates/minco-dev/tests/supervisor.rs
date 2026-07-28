#![cfg(unix)]

use minco_dev::{
    CommandSpec, DevEvent, DevPlan, DevStream, LifecycleKind, LifecyclePlan, ProcessPlan,
    ProcessRole, ReadinessProbe, ServiceKind, ServicePlan, Supervisor,
};
use std::{collections::BTreeMap, fs, future::pending, process::Command, time::Duration};
use tempfile::tempdir;
use tokio::sync::mpsc;

fn shell(script: &str, path: &str) -> CommandSpec {
    CommandSpec {
        program: "/bin/sh".into(),
        arguments: vec!["-c".into(), script.into(), "minco-test".into(), path.into()],
        environment: BTreeMap::new(),
    }
}

#[tokio::test]
async fn child_failure_runs_declared_cleanup_after_services_lifecycle_and_process_start() {
    let root = tempdir().expect("temporary root");
    let journal = root.path().join("journal");
    let journal_path = journal.to_str().expect("UTF-8 temporary path");
    let plan = DevPlan {
        schema_version: 1,
        application: "test".into(),
        environment: "local".into(),
        profile: "test".into(),
        external_aws_contact: false,
        services: vec![ServicePlan {
            id: "service".into(),
            kind: ServiceKind::Postgres,
            port: None,
            local_only: true,
            aws_services: Vec::new(),
            start: Some(shell("printf 'service-start\\n' >> \"$1\"", journal_path)),
            stop: Some(shell("printf 'service-stop\\n' >> \"$1\"", journal_path)),
        }],
        lifecycle: vec![LifecyclePlan {
            id: "migrate".into(),
            kind: LifecycleKind::Migrate,
            command: shell("printf 'migrate\\n' >> \"$1\"", journal_path),
        }],
        processes: vec![ProcessPlan {
            id: "api".into(),
            role: ProcessRole::Api,
            command: shell("printf 'api\\n' >> \"$1\"; exit 17", journal_path),
            readiness: ReadinessProbe::Process,
        }],
        omitted_schedule_ids: Vec::new(),
    };
    let (events, _receiver) = mpsc::unbounded_channel();
    let supervisor = Supervisor::new(root.path())
        .with_poll_interval(Duration::from_millis(5))
        .with_shutdown_grace(Duration::from_millis(50));

    let error = supervisor
        .run_until(&plan, &BTreeMap::new(), pending(), events)
        .await
        .expect_err("process failure must stop the topology");

    assert!(error.to_string().contains("api"));
    assert!(error.to_string().contains("17"));
    assert_eq!(
        fs::read_to_string(journal).expect("supervisor journal"),
        "service-start\nmigrate\napi\nservice-stop\n"
    );
}

#[tokio::test]
async fn coordinated_shutdown_terminates_process_descendants() {
    let root = tempdir().expect("temporary root");
    let pid_file = root.path().join("descendant.pid");
    let pid_path = pid_file.to_str().expect("UTF-8 temporary path");
    let plan = DevPlan {
        schema_version: 1,
        application: "test".into(),
        environment: "local".into(),
        profile: "test".into(),
        external_aws_contact: false,
        services: Vec::new(),
        lifecycle: Vec::new(),
        processes: vec![ProcessPlan {
            id: "worker".into(),
            role: ProcessRole::Worker,
            command: shell(
                "sleep 30 & child=$!; printf '%s' \"$child\" > \"$1\"; wait",
                pid_path,
            ),
            readiness: ReadinessProbe::Process,
        }],
        omitted_schedule_ids: Vec::new(),
    };
    let wait_for_descendant = {
        let pid_file = pid_file.clone();
        async move {
            loop {
                if tokio::fs::metadata(&pid_file).await.is_ok() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        }
    };
    let (events, _receiver) = mpsc::unbounded_channel();
    let supervisor = Supervisor::new(root.path())
        .with_poll_interval(Duration::from_millis(5))
        .with_shutdown_grace(Duration::from_millis(100));

    supervisor
        .run_until(&plan, &BTreeMap::new(), wait_for_descendant, events)
        .await
        .expect("coordinated shutdown");

    let pid = fs::read_to_string(pid_file)
        .expect("descendant PID")
        .parse::<u32>()
        .expect("numeric descendant PID");
    let alive = Command::new("/bin/kill")
        .args(["-0", &pid.to_string()])
        .output()
        .expect("inspect descendant")
        .status
        .success();
    if alive {
        let _ = Command::new("/bin/kill")
            .args(["-KILL", &pid.to_string()])
            .output();
    }
    assert!(!alive, "descendant process {pid} survived shutdown");
}

#[tokio::test]
async fn coordinated_shutdown_interrupts_lifecycle_commands_and_their_descendants() {
    let root = tempdir().expect("temporary root");
    let pid_file = root.path().join("lifecycle-descendant.pid");
    let pid_path = pid_file.to_str().expect("UTF-8 temporary path");
    let journal = root.path().join("journal");
    let journal_path = journal.to_str().expect("UTF-8 temporary path");
    let plan = DevPlan {
        schema_version: 1,
        application: "test".into(),
        environment: "local".into(),
        profile: "test".into(),
        external_aws_contact: false,
        services: vec![ServicePlan {
            id: "service".into(),
            kind: ServiceKind::Postgres,
            port: None,
            local_only: true,
            aws_services: Vec::new(),
            start: Some(shell("printf 'service-start\\n' >> \"$1\"", journal_path)),
            stop: Some(shell("printf 'service-stop\\n' >> \"$1\"", journal_path)),
        }],
        lifecycle: vec![LifecyclePlan {
            id: "migrate".into(),
            kind: LifecycleKind::Migrate,
            command: shell(
                "sleep 30 & child=$!; printf '%s' \"$child\" > \"$1\"; wait",
                pid_path,
            ),
        }],
        processes: Vec::new(),
        omitted_schedule_ids: Vec::new(),
    };
    let wait_for_descendant = {
        let pid_file = pid_file.clone();
        async move {
            loop {
                if tokio::fs::metadata(&pid_file).await.is_ok() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        }
    };
    let (events, _receiver) = mpsc::unbounded_channel();
    let supervisor = Supervisor::new(root.path()).with_shutdown_grace(Duration::from_millis(100));

    let result = tokio::time::timeout(
        Duration::from_secs(1),
        supervisor.run_until(&plan, &BTreeMap::new(), wait_for_descendant, events),
    )
    .await;

    let pid = fs::read_to_string(pid_file)
        .expect("descendant PID")
        .parse::<u32>()
        .expect("numeric descendant PID");
    let alive = Command::new("/bin/kill")
        .args(["-0", &pid.to_string()])
        .output()
        .expect("inspect descendant")
        .status
        .success();
    if alive {
        let _ = Command::new("/bin/kill")
            .args(["-KILL", &pid.to_string()])
            .output();
    }

    result
        .expect("lifecycle command ignored coordinated shutdown")
        .expect("lifecycle shutdown should be clean");
    assert!(!alive, "lifecycle descendant {pid} survived shutdown");
    assert_eq!(
        fs::read_to_string(journal).expect("supervisor journal"),
        "service-start\nservice-stop\n"
    );
}

#[tokio::test]
async fn process_logs_are_labeled_and_sensitive_runtime_values_are_redacted() {
    let root = tempdir().expect("temporary root");
    let secret = "postgres://minco:do-not-log@127.0.0.1/orders";
    let plan = DevPlan {
        schema_version: 1,
        application: "test".into(),
        environment: "local".into(),
        profile: "test".into(),
        external_aws_contact: false,
        services: Vec::new(),
        lifecycle: Vec::new(),
        processes: vec![ProcessPlan {
            id: "api".into(),
            role: ProcessRole::Api,
            command: CommandSpec {
                program: "/bin/sh".into(),
                arguments: vec![
                    "-c".into(),
                    "printf 'hello\\n'; printf '%s\\n' \"$DATABASE_URL\" >&2; exit 9".into(),
                ],
                environment: BTreeMap::new(),
            },
            readiness: ReadinessProbe::Process,
        }],
        omitted_schedule_ids: Vec::new(),
    };
    let runtime_environment = BTreeMap::from([("DATABASE_URL".into(), secret.into())]);
    let (events, mut receiver) = mpsc::unbounded_channel();
    let supervisor = Supervisor::new(root.path()).with_poll_interval(Duration::from_millis(5));

    let _ = supervisor
        .run_until(&plan, &runtime_environment, pending(), events)
        .await
        .expect_err("process exit should end supervision");

    let mut observed = Vec::new();
    while let Ok(event) = receiver.try_recv() {
        observed.push(event);
    }
    assert!(observed.contains(&DevEvent::Log {
        id: "api".into(),
        stream: DevStream::Stdout,
        line: "hello".into(),
    }));
    assert!(observed.contains(&DevEvent::Log {
        id: "api".into(),
        stream: DevStream::Stderr,
        line: "<redacted>".into(),
    }));
    assert!(!format!("{observed:?}").contains(secret));
}

#[tokio::test]
async fn http_process_is_reported_ready_only_after_its_local_probe_succeeds() {
    let root = tempdir().expect("temporary root");
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("reserve local port");
    let port = listener.local_addr().expect("local address").port();
    drop(listener);
    let plan = DevPlan {
        schema_version: 1,
        application: "test".into(),
        environment: "local".into(),
        profile: "test".into(),
        external_aws_contact: false,
        services: Vec::new(),
        lifecycle: Vec::new(),
        processes: vec![ProcessPlan {
            id: "api".into(),
            role: ProcessRole::Api,
            command: CommandSpec {
                program: "python3".into(),
                arguments: vec![
                    "-m".into(),
                    "http.server".into(),
                    port.to_string(),
                    "--bind".into(),
                    "127.0.0.1".into(),
                ],
                environment: BTreeMap::new(),
            },
            readiness: ReadinessProbe::Http {
                url: format!("http://127.0.0.1:{port}/"),
            },
        }],
        omitted_schedule_ids: Vec::new(),
    };
    let (events, mut receiver) = mpsc::unbounded_channel();
    let shutdown = async move {
        while let Some(event) = receiver.recv().await {
            if event == (DevEvent::Ready { id: "api".into() }) {
                return;
            }
        }
    };
    let supervisor = Supervisor::new(root.path())
        .with_poll_interval(Duration::from_millis(10))
        .with_readiness_timeout(Duration::from_secs(2))
        .with_shutdown_grace(Duration::from_millis(100));

    tokio::time::timeout(
        Duration::from_secs(3),
        supervisor.run_until(&plan, &BTreeMap::new(), shutdown, events),
    )
    .await
    .expect("readiness did not complete")
    .expect("supervision should stop cleanly after readiness");
}

#[tokio::test]
async fn readiness_probe_rejects_non_local_urls_without_contacting_them() {
    let root = tempdir().expect("temporary root");
    let plan = DevPlan {
        schema_version: 1,
        application: "test".into(),
        environment: "local".into(),
        profile: "test".into(),
        external_aws_contact: false,
        services: Vec::new(),
        lifecycle: Vec::new(),
        processes: vec![ProcessPlan {
            id: "api".into(),
            role: ProcessRole::Api,
            command: CommandSpec {
                program: "/bin/sh".into(),
                arguments: vec!["-c".into(), "sleep 30".into()],
                environment: BTreeMap::new(),
            },
            readiness: ReadinessProbe::Http {
                url: "http://example.com/health".into(),
            },
        }],
        omitted_schedule_ids: Vec::new(),
    };
    let (events, _receiver) = mpsc::unbounded_channel();
    let supervisor = Supervisor::new(root.path()).with_shutdown_grace(Duration::from_millis(100));

    let error = tokio::time::timeout(
        Duration::from_secs(1),
        supervisor.run_until(&plan, &BTreeMap::new(), pending(), events),
    )
    .await
    .expect("invalid readiness URL should fail without network delay")
    .expect_err("non-local readiness URL must be rejected");

    assert!(error.to_string().contains("non-local or invalid"));
}

#[tokio::test]
async fn readiness_probe_rejects_query_credentials_before_contacting_loopback() {
    let root = tempdir().expect("temporary root");
    let plan = DevPlan {
        schema_version: 1,
        application: "test".into(),
        environment: "local".into(),
        profile: "test".into(),
        external_aws_contact: false,
        services: Vec::new(),
        lifecycle: Vec::new(),
        processes: vec![ProcessPlan {
            id: "api".into(),
            role: ProcessRole::Api,
            command: CommandSpec {
                program: "/bin/sh".into(),
                arguments: vec!["-c".into(), "sleep 30".into()],
                environment: BTreeMap::new(),
            },
            readiness: ReadinessProbe::Http {
                url: "http://127.0.0.1:9/health?token=do-not-serialize".into(),
            },
        }],
        omitted_schedule_ids: Vec::new(),
    };
    let (events, _receiver) = mpsc::unbounded_channel();
    let supervisor = Supervisor::new(root.path())
        .with_readiness_timeout(Duration::from_millis(50))
        .with_shutdown_grace(Duration::from_millis(100));

    let error = supervisor
        .run_until(&plan, &BTreeMap::new(), pending(), events)
        .await
        .expect_err("credential-bearing readiness URL must be rejected");

    assert!(error.to_string().contains("non-local or invalid"));
    assert!(!error.to_string().contains("do-not-serialize"));
}

#[tokio::test]
async fn lifecycle_output_uses_the_same_labeled_log_stream_as_long_running_processes() {
    let root = tempdir().expect("temporary root");
    let plan = DevPlan {
        schema_version: 1,
        application: "test".into(),
        environment: "local".into(),
        profile: "test".into(),
        external_aws_contact: false,
        services: Vec::new(),
        lifecycle: vec![LifecyclePlan {
            id: "migrate".into(),
            kind: LifecycleKind::Migrate,
            command: CommandSpec {
                program: "/bin/sh".into(),
                arguments: vec!["-c".into(), "printf 'migration-output\\n'".into()],
                environment: BTreeMap::new(),
            },
        }],
        processes: vec![ProcessPlan {
            id: "api".into(),
            role: ProcessRole::Api,
            command: CommandSpec {
                program: "/bin/sh".into(),
                arguments: vec!["-c".into(), "exit 8".into()],
                environment: BTreeMap::new(),
            },
            readiness: ReadinessProbe::Process,
        }],
        omitted_schedule_ids: Vec::new(),
    };
    let (events, mut receiver) = mpsc::unbounded_channel();
    let supervisor = Supervisor::new(root.path()).with_poll_interval(Duration::from_millis(5));

    let _ = supervisor
        .run_until(&plan, &BTreeMap::new(), pending(), events)
        .await
        .expect_err("process exit should end supervision");

    let mut observed = Vec::new();
    while let Ok(event) = receiver.try_recv() {
        observed.push(event);
    }
    assert!(observed.contains(&DevEvent::Log {
        id: "migrate".into(),
        stream: DevStream::Stdout,
        line: "migration-output".into(),
    }));
}

#[tokio::test]
async fn failed_service_cleanup_is_reported_instead_of_claiming_clean_shutdown() {
    let root = tempdir().expect("temporary root");
    let plan = DevPlan {
        schema_version: 1,
        application: "test".into(),
        environment: "local".into(),
        profile: "test".into(),
        external_aws_contact: false,
        services: vec![ServicePlan {
            id: "postgres".into(),
            kind: ServiceKind::Postgres,
            port: None,
            local_only: true,
            aws_services: Vec::new(),
            start: Some(CommandSpec {
                program: "/usr/bin/true".into(),
                arguments: Vec::new(),
                environment: BTreeMap::new(),
            }),
            stop: Some(CommandSpec {
                program: "/bin/sh".into(),
                arguments: vec!["-c".into(), "exit 23".into()],
                environment: BTreeMap::new(),
            }),
        }],
        lifecycle: Vec::new(),
        processes: vec![ProcessPlan {
            id: "api".into(),
            role: ProcessRole::Api,
            command: CommandSpec {
                program: "/bin/sh".into(),
                arguments: vec!["-c".into(), "sleep 30".into()],
                environment: BTreeMap::new(),
            },
            readiness: ReadinessProbe::Process,
        }],
        omitted_schedule_ids: Vec::new(),
    };
    let (events, mut receiver) = mpsc::unbounded_channel();
    let supervisor = Supervisor::new(root.path()).with_shutdown_grace(Duration::from_millis(100));

    let error = supervisor
        .run_until(&plan, &BTreeMap::new(), async {}, events)
        .await
        .expect_err("cleanup failure must fail supervision");
    assert!(error.to_string().contains("postgres"));
    assert!(error.to_string().contains("23"));
    let mut observed = Vec::new();
    while let Ok(event) = receiver.try_recv() {
        observed.push(event);
    }
    assert!(observed.contains(&DevEvent::Failed {
        id: "postgres".into(),
    }));
    assert!(!observed.contains(&DevEvent::Stopped {
        id: "postgres".into(),
    }));
}
