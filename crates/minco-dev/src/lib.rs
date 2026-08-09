//! Deterministic local development plans and coordinated process supervision.
#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize, ser::SerializeStruct};
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;

mod supervisor;

pub use supervisor::{DevEvent, DevStream, Supervisor, SupervisorError};

const SUPPORTED_LOCAL_AWS_SERVICES: &[&str] = &[
    "apigatewayv2",
    "cloudfront",
    "cloudwatch",
    "dynamodb",
    "dynamodbstreams",
    "events",
    "iam",
    "kinesis",
    "kms",
    "lambda",
    "logs",
    "s3",
    "secretsmanager",
    "ses",
    "sns",
    "sqs",
    "ssm",
    "sts",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DevDatabase {
    Postgres,
    Sqlite,
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct CommandSpec {
    pub program: String,
    #[serde(default)]
    pub arguments: Vec<String>,
    #[serde(default)]
    pub environment: BTreeMap<String, String>,
}

impl Serialize for CommandSpec {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let environment = self
            .environment
            .iter()
            .map(|(name, value)| {
                (
                    name.as_str(),
                    if is_sensitive_environment_name(name) {
                        "<redacted>"
                    } else {
                        value.as_str()
                    },
                )
            })
            .collect::<BTreeMap<_, _>>();
        let mut state = serializer.serialize_struct("CommandSpec", 3)?;
        state.serialize_field("program", &self.program)?;
        state.serialize_field("arguments", &self.arguments)?;
        state.serialize_field("environment", &environment)?;
        state.end()
    }
}

pub(crate) fn is_sensitive_environment_name(name: &str) -> bool {
    let name = name.to_ascii_uppercase();
    name.ends_with("_URL")
        || name.ends_with("_DSN")
        || name.ends_with("_KEY")
        || name.contains("_KEY_")
        || [
            "AUTHORIZATION",
            "COOKIE",
            "CREDENTIAL",
            "PASSPHRASE",
            "PASSWORD",
            "SECRET",
            "TOKEN",
        ]
        .iter()
        .any(|marker| name.contains(marker))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ReadinessProbe {
    Process,
    Http { url: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessConfig {
    pub id: String,
    pub command: CommandSpec,
    pub readiness: ReadinessProbe,
    #[serde(default)]
    pub default_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DevGraph {
    pub application: String,
    pub environment: String,
    pub compose_file: String,
    pub database: DevDatabase,
    #[serde(default)]
    pub local_aws_services: Vec<String>,
    pub api: ProcessConfig,
    #[serde(default)]
    pub workers: Vec<ProcessConfig>,
    pub frontend: Option<ProcessConfig>,
    pub migration: Option<CommandSpec>,
    #[serde(default)]
    pub seeds: BTreeMap<String, CommandSpec>,
    #[serde(default)]
    pub schedules: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DevOptions {
    pub profile: String,
    pub migrate: bool,
    pub seed: Option<String>,
    pub with_workers: BTreeSet<String>,
    pub without_workers: BTreeSet<String>,
    pub frontend: Option<bool>,
    pub port: Option<u16>,
    pub rustack_port: Option<u16>,
}

impl Default for DevOptions {
    fn default() -> Self {
        Self {
            profile: "default".into(),
            migrate: true,
            seed: None,
            with_workers: BTreeSet::new(),
            without_workers: BTreeSet::new(),
            frontend: None,
            port: None,
            rustack_port: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceKind {
    Postgres,
    Sqlite,
    Rustack,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServicePlan {
    pub id: String,
    pub kind: ServiceKind,
    pub port: Option<u16>,
    pub local_only: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aws_services: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<CommandSpec>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<CommandSpec>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleKind {
    Migrate,
    Seed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LifecyclePlan {
    pub id: String,
    pub kind: LifecycleKind,
    pub command: CommandSpec,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessRole {
    Api,
    Worker,
    Frontend,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessPlan {
    pub id: String,
    pub role: ProcessRole,
    pub command: CommandSpec,
    pub readiness: ReadinessProbe,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DevPlan {
    pub schema_version: u32,
    pub application: String,
    pub environment: String,
    pub profile: String,
    pub external_aws_contact: bool,
    pub services: Vec<ServicePlan>,
    pub lifecycle: Vec<LifecyclePlan>,
    pub processes: Vec<ProcessPlan>,
    pub omitted_schedule_ids: Vec<String>,
}

impl DevPlan {
    pub fn derive(graph: &DevGraph, options: &DevOptions) -> Result<Self, DevPlanError> {
        let mut process_ids = BTreeSet::new();
        for process in std::iter::once(&graph.api)
            .chain(graph.workers.iter())
            .chain(graph.frontend.iter())
        {
            if !process_ids.insert(process.id.as_str()) {
                return Err(DevPlanError::Invalid(format!(
                    "duplicate development process identifier `{}`",
                    process.id
                )));
            }
        }
        if let Some(worker) = options
            .with_workers
            .intersection(&options.without_workers)
            .next()
        {
            return Err(DevPlanError::Invalid(format!(
                "worker `{worker}` cannot be both included and omitted"
            )));
        }
        let declared_workers = graph
            .workers
            .iter()
            .map(|worker| worker.id.as_str())
            .collect::<BTreeSet<_>>();
        for worker in options
            .with_workers
            .iter()
            .chain(options.without_workers.iter())
        {
            if !declared_workers.contains(worker.as_str()) {
                return Err(DevPlanError::Invalid(format!(
                    "worker `{worker}` is not declared"
                )));
            }
        }

        for service in &graph.local_aws_services {
            if !SUPPORTED_LOCAL_AWS_SERVICES.contains(&service.as_str()) {
                return Err(DevPlanError::Invalid(format!(
                    "local AWS service `{service}` is not supported"
                )));
            }
        }
        if options.frontend == Some(true) && graph.frontend.is_none() {
            return Err(DevPlanError::Invalid(
                "frontend was requested but development.frontend is not declared".into(),
            ));
        }

        let mut services = Vec::new();
        match graph.database {
            DevDatabase::Postgres => {
                let postgres_port = 55_432;
                let environment =
                    BTreeMap::from([("MINCO_POSTGRES_PORT".into(), postgres_port.to_string())]);
                services.push(ServicePlan {
                    id: "postgres".into(),
                    kind: ServiceKind::Postgres,
                    port: Some(postgres_port),
                    local_only: true,
                    aws_services: Vec::new(),
                    start: Some(service_runtime_command(
                        "start",
                        &graph.application,
                        &graph.compose_file,
                        "postgres",
                        postgres_port,
                        &[],
                        environment,
                    )),
                    stop: Some(service_runtime_command(
                        "stop",
                        &graph.application,
                        &graph.compose_file,
                        "postgres",
                        postgres_port,
                        &[],
                        BTreeMap::new(),
                    )),
                });
            }
            DevDatabase::Sqlite => services.push(ServicePlan {
                id: "sqlite".into(),
                kind: ServiceKind::Sqlite,
                port: None,
                local_only: true,
                aws_services: Vec::new(),
                start: None,
                stop: None,
            }),
            DevDatabase::None => {}
        }

        if !graph.local_aws_services.is_empty() {
            let mut aws_services = graph.local_aws_services.clone();
            aws_services.sort();
            aws_services.dedup();
            let rustack_port = options.rustack_port.unwrap_or(4_566);
            let environment = BTreeMap::from([
                ("MINCO_RUSTACK_PORT".into(), rustack_port.to_string()),
                ("MINCO_RUSTACK_SERVICES".into(), aws_services.join(",")),
            ]);
            services.push(ServicePlan {
                id: "rustack".into(),
                kind: ServiceKind::Rustack,
                port: Some(rustack_port),
                local_only: true,
                aws_services: aws_services.clone(),
                start: Some(service_runtime_command(
                    "start",
                    &graph.application,
                    &graph.compose_file,
                    "rustack",
                    rustack_port,
                    &aws_services,
                    environment,
                )),
                stop: Some(service_runtime_command(
                    "stop",
                    &graph.application,
                    &graph.compose_file,
                    "rustack",
                    rustack_port,
                    &aws_services,
                    BTreeMap::new(),
                )),
            });
        }

        let mut lifecycle = Vec::new();
        if options.migrate
            && let Some(command) = &graph.migration
        {
            lifecycle.push(LifecyclePlan {
                id: "migrate".into(),
                kind: LifecycleKind::Migrate,
                command: command.clone(),
            });
        }
        if let Some(seed) = &options.seed {
            let command = graph.seeds.get(seed).ok_or_else(|| {
                DevPlanError::Invalid(format!("seed profile `{seed}` is not declared"))
            })?;
            lifecycle.push(LifecyclePlan {
                id: format!("seed:{seed}"),
                kind: LifecycleKind::Seed,
                command: command.clone(),
            });
        }

        let mut api_command = graph.api.command.clone();
        if let Some(port) = options.port {
            api_command
                .environment
                .insert("PORT".into(), port.to_string());
        }
        let api_readiness = override_readiness_port(&graph.api.readiness, options.port)?;
        let mut processes = vec![ProcessPlan {
            id: graph.api.id.clone(),
            role: ProcessRole::Api,
            command: api_command,
            readiness: api_readiness,
        }];
        let mut workers = graph
            .workers
            .iter()
            .filter(|worker| {
                (worker.default_enabled || options.with_workers.contains(&worker.id))
                    && !options.without_workers.contains(&worker.id)
            })
            .map(|worker| ProcessPlan {
                id: worker.id.clone(),
                role: ProcessRole::Worker,
                command: worker.command.clone(),
                readiness: worker.readiness.clone(),
            })
            .collect::<Vec<_>>();
        workers.sort_by(|left, right| left.id.cmp(&right.id));
        processes.extend(workers);

        if let Some(frontend) = graph
            .frontend
            .as_ref()
            .filter(|frontend| options.frontend.unwrap_or(frontend.default_enabled))
        {
            processes.push(ProcessPlan {
                id: frontend.id.clone(),
                role: ProcessRole::Frontend,
                command: frontend.command.clone(),
                readiness: frontend.readiness.clone(),
            });
        }

        let mut omitted_schedule_ids = graph.schedules.clone();
        omitted_schedule_ids.sort();
        omitted_schedule_ids.dedup();

        Ok(Self {
            schema_version: 1,
            application: graph.application.clone(),
            environment: graph.environment.clone(),
            profile: options.profile.clone(),
            external_aws_contact: false,
            services,
            lifecycle,
            processes,
            omitted_schedule_ids,
        })
    }
}

fn override_readiness_port(
    readiness: &ReadinessProbe,
    port: Option<u16>,
) -> Result<ReadinessProbe, DevPlanError> {
    let (ReadinessProbe::Http { url }, Some(port)) = (readiness, port) else {
        return Ok(readiness.clone());
    };
    let mut url = reqwest::Url::parse(url)
        .map_err(|_| DevPlanError::Invalid("API readiness URL is invalid".into()))?;
    url.set_port(Some(port))
        .map_err(|()| DevPlanError::Invalid("API readiness URL cannot accept a port".into()))?;
    Ok(ReadinessProbe::Http { url: url.into() })
}

#[allow(clippy::too_many_arguments)]
fn service_runtime_command(
    action: &str,
    application: &str,
    compose_file: &str,
    service: &str,
    port: u16,
    aws_services: &[String],
    environment: BTreeMap<String, String>,
) -> CommandSpec {
    let mut arguments = vec![
        "__local-service".into(),
        action.into(),
        service.into(),
        "--application".into(),
        application.into(),
        "--compose-file".into(),
        compose_file.into(),
        "--port".into(),
        port.to_string(),
    ];
    if !aws_services.is_empty() {
        arguments.extend(["--aws-services".into(), aws_services.join(",")]);
    }
    CommandSpec {
        program: "cargo-minco".into(),
        arguments,
        environment,
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum DevPlanError {
    #[error("invalid development plan: {0}")]
    Invalid(String),
}
