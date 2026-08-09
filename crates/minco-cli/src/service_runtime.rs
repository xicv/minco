#![forbid(unsafe_code)]

use anyhow::{Context, Result, bail, ensure};
use clap::{Args, Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use sqlx::{
    Connection as _, Row as _,
    postgres::{PgConnectOptions, PgSslMode},
};
use std::{
    collections::BTreeMap,
    env,
    ffi::OsString,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

const POSTGRES_CONTAINER_PORT: u16 = 5_432;
const POSTGRES_DATABASE: &str = "minco_orders";
const POSTGRES_IMAGE: &str = concat!(
    "docker.io/library/postgres:18.4-alpine3.24@",
    "sha256:9a8afca54e7861fd90fab5fdf4c42477a6b1cb7d293595148e674e0a3181de15"
);
const POSTGRES_PASSWORD: &str = "minco";
const POSTGRES_USER: &str = "minco";
const RUSTACK_CONTAINER_PORT: u16 = 4_566;
const RUSTACK_IMAGE: &str = concat!(
    "ghcr.io/tyrchen/rustack:0.9.1@",
    "sha256:18cd91395e17453e2c34b299e45f4679dc2427473dc1db6541bbe212fd70a104"
);
const RUSTACK_SUPPORTED_SERVICES: &[&str] = &[
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
const COMMAND_TIMEOUT: Duration = Duration::from_secs(5);
const READY_TIMEOUT: Duration = Duration::from_mins(1);
const RETRY_INTERVAL: Duration = Duration::from_millis(250);
const OWNERSHIP_MANAGED: &str = "dev.minco.managed";
const OWNERSHIP_SCHEMA: &str = "dev.minco.schema";
const OWNERSHIP_APPLICATION: &str = "dev.minco.application";
const OWNERSHIP_WORKSPACE: &str = "dev.minco.workspace";
const OWNERSHIP_SERVICE: &str = "dev.minco.service";
const OWNERSHIP_CONFIGURATION: &str = "dev.minco.configuration";

pub fn supports_local_aws_service(service: &str) -> bool {
    RUSTACK_SUPPORTED_SERVICES.contains(&service)
}

pub fn normalize_dev_plan_services(
    plan: &mut minco_dev::DevPlan,
    application: &str,
    compose_file: &str,
    requested_aws_services: &[String],
    rustack_port: u16,
) {
    if let Some(postgres) = plan
        .services
        .iter_mut()
        .find(|service| service.kind == minco_dev::ServiceKind::Postgres)
    {
        postgres.start = Some(local_service_plan_command(
            "start",
            application,
            compose_file,
            "postgres",
            postgres.port.unwrap_or(55_432),
            &[],
        ));
        postgres.stop = Some(local_service_plan_command(
            "stop",
            application,
            compose_file,
            "postgres",
            postgres.port.unwrap_or(55_432),
            &[],
        ));
    }
    if requested_aws_services.is_empty() {
        return;
    }
    let mut requested = requested_aws_services.to_vec();
    requested.sort();
    requested.dedup();
    let rustack = if let Some(index) = plan
        .services
        .iter()
        .position(|service| service.kind == minco_dev::ServiceKind::Rustack)
    {
        &mut plan.services[index]
    } else {
        plan.services.push(minco_dev::ServicePlan {
            id: "rustack".into(),
            kind: minco_dev::ServiceKind::Rustack,
            port: Some(rustack_port),
            local_only: true,
            aws_services: Vec::new(),
            start: None,
            stop: None,
        });
        plan.services
            .last_mut()
            .expect("Rustack service was inserted")
    };
    rustack.port = Some(rustack_port);
    rustack.aws_services.clone_from(&requested);
    rustack.start = Some(local_service_plan_command(
        "start",
        application,
        compose_file,
        "rustack",
        rustack_port,
        &requested,
    ));
    rustack.stop = Some(local_service_plan_command(
        "stop",
        application,
        compose_file,
        "rustack",
        rustack_port,
        &requested,
    ));
}

fn local_service_plan_command(
    action: &str,
    application: &str,
    compose_file: &str,
    service: &str,
    port: u16,
    aws_services: &[String],
) -> minco_dev::CommandSpec {
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
    let environment = if action == "start" {
        match service {
            "postgres" => BTreeMap::from([("MINCO_POSTGRES_PORT".into(), port.to_string())]),
            "rustack" => BTreeMap::from([
                ("MINCO_RUSTACK_PORT".into(), port.to_string()),
                ("MINCO_RUSTACK_SERVICES".into(), aws_services.join(",")),
            ]),
            _ => BTreeMap::new(),
        }
    } else {
        BTreeMap::new()
    };
    minco_dev::CommandSpec {
        program: "cargo-minco".into(),
        arguments,
        environment,
    }
}

#[derive(Debug, Args)]
pub struct LocalServiceArgs {
    #[command(subcommand)]
    pub(crate) action: Action,
}

#[derive(Debug, Subcommand)]
pub enum Action {
    Start(ServiceArgs),
    Stop(ServiceArgs),
}

#[derive(Debug, Clone, Args)]
pub struct ServiceArgs {
    #[arg(value_enum)]
    service: Service,
    #[arg(long)]
    application: String,
    #[arg(long)]
    compose_file: PathBuf,
    #[arg(long, value_parser = clap::value_parser!(u16).range(1..))]
    port: u16,
    #[arg(long, value_delimiter = ',')]
    aws_services: Vec<String>,
    #[arg(
        long,
        env = "MINCO_CONTAINER_RUNTIME",
        value_enum,
        default_value = "auto"
    )]
    runtime: RuntimePreference,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, ValueEnum)]
#[serde(rename_all = "snake_case")]
enum Service {
    Postgres,
    Rustack,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct VolumeContract {
    name: String,
    container_path: String,
    persistent: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum ServiceReadiness {
    Postgres {
        expected_user: String,
        expected_database: String,
        query: String,
    },
    Rustack {
        path: String,
        requested_services: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct LocalServiceSpec {
    service: Service,
    image: String,
    container_port: u16,
    host_port: u16,
    bind_address: String,
    environment: BTreeMap<String, String>,
    secret_environment: Vec<String>,
    volume: Option<VolumeContract>,
    readiness: ServiceReadiness,
    aws_services: Vec<String>,
    ownership: BTreeMap<String, String>,
}

impl LocalServiceSpec {
    fn from_arguments(
        arguments: &ServiceArgs,
        source_environment: &BTreeMap<String, String>,
    ) -> Result<Self> {
        let canonical_compose =
            std::fs::canonicalize(&arguments.compose_file).with_context(|| {
                format!(
                    "resolve Compose file {} for local service identity",
                    arguments.compose_file.display()
                )
            })?;
        let application = normalized(&arguments.application);
        let workspace = digest_prefix(canonical_compose.to_string_lossy().as_bytes(), 16);
        let mut aws_services = arguments.aws_services.clone();
        aws_services.sort();
        aws_services.dedup();
        let (image, container_port, environment, secret_environment, volume, readiness) =
            match arguments.service {
                Service::Postgres => {
                    let database =
                        environment_or(source_environment, "MINCO_POSTGRES_DB", POSTGRES_DATABASE);
                    let user =
                        environment_or(source_environment, "MINCO_POSTGRES_USER", POSTGRES_USER);
                    (
                        environment_or(source_environment, "MINCO_POSTGRES_IMAGE", POSTGRES_IMAGE),
                        POSTGRES_CONTAINER_PORT,
                        BTreeMap::from([
                            ("POSTGRES_DB".into(), database.clone()),
                            ("POSTGRES_USER".into(), user.clone()),
                        ]),
                        vec!["POSTGRES_PASSWORD".into()],
                        Some(VolumeContract {
                            name: scoped_resource_name(&application, &workspace, "-postgres-data"),
                            container_path: "/var/lib/postgresql".into(),
                            persistent: true,
                        }),
                        ServiceReadiness::Postgres {
                            expected_user: user,
                            expected_database: database,
                            query: "SELECT current_user, current_database(), 1".into(),
                        },
                    )
                }
                Service::Rustack => (
                    environment_or(source_environment, "MINCO_RUSTACK_IMAGE", RUSTACK_IMAGE),
                    RUSTACK_CONTAINER_PORT,
                    BTreeMap::from([
                        ("DEFAULT_REGION".into(), region_from(source_environment)),
                        (
                            "LOG_LEVEL".into(),
                            environment_or(source_environment, "MINCO_RUSTACK_LOG_LEVEL", "info"),
                        ),
                        ("SERVICES".into(), aws_services.join(",")),
                    ]),
                    Vec::new(),
                    None,
                    ServiceReadiness::Rustack {
                        path: "/_localstack/health".into(),
                        requested_services: aws_services.clone(),
                    },
                ),
            };
        validate_immutable_image_reference(&image)?;
        let mut spec = Self {
            service: arguments.service,
            image,
            container_port,
            host_port: arguments.port,
            bind_address: "127.0.0.1".into(),
            environment,
            secret_environment,
            volume,
            readiness,
            aws_services,
            ownership: BTreeMap::new(),
        };
        let configuration = spec.compute_configuration_digest()?;
        spec.ownership = BTreeMap::from([
            (OWNERSHIP_APPLICATION.into(), application),
            (OWNERSHIP_CONFIGURATION.into(), configuration),
            (OWNERSHIP_MANAGED.into(), "true".into()),
            (OWNERSHIP_SCHEMA.into(), "1".into()),
            (OWNERSHIP_SERVICE.into(), arguments.service.label().into()),
            (OWNERSHIP_WORKSPACE.into(), workspace),
        ]);
        Ok(spec)
    }

    fn compute_configuration_digest(&self) -> Result<String> {
        let mut digestible = self.clone();
        digestible.ownership.clear();
        Ok(format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&digestible)?)
        ))
    }

    fn configuration_digest(&self) -> &str {
        self.ownership
            .get(OWNERSHIP_CONFIGURATION)
            .map(String::as_str)
            .unwrap_or_default()
    }
}

fn validate_immutable_image_reference(image: &str) -> Result<()> {
    let valid = image
        .rsplit_once("@sha256:")
        .is_some_and(|(repository, digest)| {
            !repository.is_empty()
                && !repository.chars().any(char::is_whitespace)
                && digest.len() == 64
                && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
        });
    ensure!(
        valid,
        "local service image must use an immutable sha256 digest reference"
    );
    Ok(())
}

fn environment_or(environment: &BTreeMap<String, String>, name: &str, default: &str) -> String {
    environment
        .get(name)
        .filter(|value| !value.trim().is_empty())
        .cloned()
        .unwrap_or_else(|| default.to_owned())
}

fn region_from(environment: &BTreeMap<String, String>) -> String {
    environment
        .get("AWS_REGION")
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            environment
                .get("AWS_DEFAULT_REGION")
                .filter(|value| !value.trim().is_empty())
        })
        .cloned()
        .unwrap_or_else(|| "ap-southeast-2".into())
}

fn digest_prefix(value: &[u8], length: usize) -> String {
    let digest = format!("{:x}", Sha256::digest(value));
    digest[..length.min(digest.len())].to_owned()
}

impl Service {
    const fn label(self) -> &'static str {
        match self {
            Self::Postgres => "postgres",
            Self::Rustack => "rustack",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum RuntimePreference {
    Auto,
    #[value(alias = "docker-compose")]
    Docker,
    #[value(alias = "apple-container")]
    Apple,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum Runtime {
    Docker,
    Apple,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
struct LifecycleReceipt {
    schema_version: u32,
    runtime: Runtime,
    resource: String,
    application: String,
    workspace: String,
    service: String,
    configuration: String,
}

impl LifecycleReceipt {
    fn for_spec(runtime: Runtime, spec: &LocalServiceSpec) -> Self {
        Self {
            schema_version: 1,
            runtime,
            resource: resource_name(spec),
            application: spec.ownership[OWNERSHIP_APPLICATION].clone(),
            workspace: spec.ownership[OWNERSHIP_WORKSPACE].clone(),
            service: spec.service.label().into(),
            configuration: spec.configuration_digest().into(),
        }
    }

    fn agrees_with(&self, spec: &LocalServiceSpec) -> bool {
        self.schema_version == 1
            && self.resource == resource_name(spec)
            && self.application == spec.ownership[OWNERSHIP_APPLICATION]
            && self.workspace == spec.ownership[OWNERSHIP_WORKSPACE]
            && self.service == spec.service.label()
            && self.configuration == spec.configuration_digest()
    }
}

#[derive(Debug)]
struct ServiceLock {
    _file: std::fs::File,
}

impl ServiceLock {
    fn acquire(base: &Path, spec: &LocalServiceSpec) -> Result<Self> {
        let directory = receipt_directory(base, spec);
        std::fs::create_dir_all(&directory).with_context(|| {
            format!(
                "create local service state directory {}",
                directory.display()
            )
        })?;
        let path = directory.join(format!(".{}.lock", spec.service.label()));
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .with_context(|| format!("open local service lock {}", path.display()))?;
        file.try_lock().map_err(|error| match error {
            std::fs::TryLockError::WouldBlock => anyhow::anyhow!(
                "another local service operation is already active for `{}`",
                spec.service.label()
            ),
            std::fs::TryLockError::Error(error) => {
                anyhow::anyhow!("lock local service operation: {error}")
            }
        })?;
        Ok(Self { _file: file })
    }
}

fn receipt_directory(base: &Path, spec: &LocalServiceSpec) -> PathBuf {
    base.join(&spec.ownership[OWNERSHIP_WORKSPACE])
        .join(&spec.ownership[OWNERSHIP_APPLICATION])
}

fn receipt_path(base: &Path, spec: &LocalServiceSpec) -> PathBuf {
    receipt_directory(base, spec).join(format!("{}.json", spec.service.label()))
}

fn write_receipt_atomic(base: &Path, receipt: &LifecycleReceipt) -> Result<()> {
    let directory = base.join(&receipt.workspace).join(&receipt.application);
    std::fs::create_dir_all(&directory).with_context(|| {
        format!(
            "create local service receipt directory {}",
            directory.display()
        )
    })?;
    let path = directory.join(format!("{}.json", receipt.service));
    let temporary = directory.join(format!(
        ".{}.{}.{}.tmp",
        receipt.service,
        std::process::id(),
        &receipt.configuration[..16]
    ));
    let mut rendered = serde_json::to_vec_pretty(receipt)?;
    rendered.push(b'\n');
    let result = (|| {
        let mut output = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .with_context(|| format!("create atomic receipt {}", temporary.display()))?;
        output.write_all(&rendered)?;
        output.sync_all()?;
        std::fs::rename(&temporary, &path)
            .with_context(|| format!("install lifecycle receipt {}", path.display()))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

fn read_receipt(base: &Path, spec: &LocalServiceSpec) -> Result<Option<LifecycleReceipt>> {
    let path = receipt_path(base, spec);
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).context("read local service lifecycle receipt"),
    };
    let receipt: LifecycleReceipt = serde_json::from_slice(&bytes).map_err(|_| {
        anyhow::anyhow!(
            "local service receipt is corrupt at {}; inspect it before recovery",
            path.display()
        )
    })?;
    ensure!(
        receipt.agrees_with(spec),
        "local service receipt disagrees with the requested configuration; inspect {} before recovery",
        path.display()
    );
    Ok(Some(receipt))
}

#[derive(Debug, Clone)]
struct CommandRequest {
    program: String,
    arguments: Vec<OsString>,
    environment: BTreeMap<String, String>,
    timeout: Duration,
}

impl CommandRequest {
    fn probe(program: &str, arguments: &[&str]) -> Self {
        Self {
            program: program.into(),
            arguments: arguments.iter().map(OsString::from).collect(),
            environment: BTreeMap::new(),
            timeout: COMMAND_TIMEOUT,
        }
    }
}

#[derive(Debug, Clone)]
struct CommandOutput {
    successful: bool,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

impl CommandOutput {
    #[cfg(test)]
    const fn success(stdout: Vec<u8>) -> Self {
        Self {
            successful: true,
            stdout,
            stderr: Vec::new(),
        }
    }

    fn failure() -> Self {
        Self {
            successful: false,
            stdout: Vec::new(),
            stderr: b"not found".to_vec(),
        }
    }
}

trait CommandRunner {
    fn run(&self, command: &CommandRequest) -> Result<CommandOutput>;
}

#[derive(Debug, Default)]
struct SystemCommandRunner;

impl CommandRunner for SystemCommandRunner {
    fn run(&self, request: &CommandRequest) -> Result<CommandOutput> {
        let mut command = Command::new(&request.program);
        command
            .args(&request.arguments)
            .envs(&request.environment)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command
            .spawn()
            .with_context(|| format!("start local runtime command `{}`", request.program))?;
        let mut stdout = child
            .stdout
            .take()
            .context("capture local runtime stdout")?;
        let mut stderr = child
            .stderr
            .take()
            .context("capture local runtime stderr")?;
        let stdout_reader = thread::spawn(move || {
            let mut value = Vec::new();
            stdout.read_to_end(&mut value).map(|_| value)
        });
        let stderr_reader = thread::spawn(move || {
            let mut value = Vec::new();
            stderr.read_to_end(&mut value).map(|_| value)
        });
        let deadline = Instant::now() + request.timeout;
        let successful = loop {
            match child.try_wait().context("inspect local runtime command")? {
                Some(status) => break status.success(),
                None if Instant::now() < deadline => {
                    thread::sleep(Duration::from_millis(25));
                }
                None => {
                    let _ = child.kill();
                    let _ = child.wait();
                    bail!(
                        "local runtime command `{}` exceeded its bounded timeout",
                        request.program
                    );
                }
            }
        };
        let stdout = stdout_reader
            .join()
            .map_err(|_| anyhow::anyhow!("local runtime stdout reader panicked"))??;
        let stderr = stderr_reader
            .join()
            .map_err(|_| anyhow::anyhow!("local runtime stderr reader panicked"))??;
        Ok(CommandOutput {
            successful,
            stdout,
            stderr,
        })
    }
}

impl Runtime {
    const fn label(self) -> &'static str {
        match self {
            Self::Docker => "Docker Compose",
            Self::Apple => "Apple Container",
        }
    }
}

pub async fn execute(arguments: LocalServiceArgs) -> Result<()> {
    let source_environment = env::vars().collect::<BTreeMap<_, _>>();
    let root = env::current_dir().context("resolve local service project directory")?;
    match arguments.action {
        Action::Start(arguments) => {
            start_with_runner(&SystemCommandRunner, &arguments, &source_environment, &root).await?;
            println!(
                "minco: {} is ready on 127.0.0.1:{}",
                arguments.service.label(),
                arguments.port
            );
            Ok(())
        }
        Action::Stop(arguments) => {
            stop_with_runner(&SystemCommandRunner, &arguments, &source_environment, &root)?;
            println!("minco: stopped {}", arguments.service.label());
            Ok(())
        }
    }
}

fn validate(arguments: &ServiceArgs, starting: bool) -> Result<()> {
    ensure!(
        !arguments.application.trim().is_empty(),
        "application name cannot be empty"
    );
    if starting {
        ensure!(
            arguments.compose_file.is_file(),
            "Compose file `{}` does not exist",
            arguments.compose_file.display()
        );
        ensure!(
            arguments.service != Service::Rustack || !arguments.aws_services.is_empty(),
            "Rustack requires at least one declared AWS service"
        );
        if arguments.service == Service::Rustack {
            for service in &arguments.aws_services {
                ensure!(
                    RUSTACK_SUPPORTED_SERVICES.contains(&service.as_str()),
                    "Rustack 0.9.1 does not support requested local AWS service `{service}`"
                );
            }
        }
    }
    Ok(())
}

#[derive(Debug)]
struct RuntimeAvailability {
    installed: bool,
    ready: bool,
    diagnostic: String,
}

fn probe(runner: &impl CommandRunner, program: &str, arguments: &[&str]) -> CommandOutput {
    runner
        .run(&CommandRequest::probe(program, arguments))
        .unwrap_or_else(|_| CommandOutput::failure())
}

fn parsed_version(output: &[u8]) -> Option<(u64, u64, u64)> {
    String::from_utf8_lossy(output)
        .split_whitespace()
        .map(|candidate| {
            candidate
                .trim_matches(|character: char| !(character.is_ascii_digit() || character == '.'))
        })
        .find_map(|candidate| {
            let mut parts = candidate.split('.');
            let major = parts.next()?.parse().ok()?;
            let minor = parts.next()?.parse().ok()?;
            let patch = parts.next()?.parse().ok()?;
            (parts.next().is_none()).then_some((major, minor, patch))
        })
}

fn docker_availability(runner: &impl CommandRunner) -> RuntimeAvailability {
    let cli = probe(runner, "docker", &["--version"]);
    if !cli.successful {
        return RuntimeAvailability {
            installed: false,
            ready: false,
            diagnostic: "Docker CLI is not available".into(),
        };
    }
    let compose = probe(runner, "docker", &["compose", "version", "--short"]);
    if !compose.successful || parsed_version(&compose.stdout).is_none_or(|(major, _, _)| major < 2)
    {
        return RuntimeAvailability {
            installed: true,
            ready: false,
            diagnostic: "Docker is installed but Docker Compose v2 or newer is not ready".into(),
        };
    }
    let daemon = probe(runner, "docker", &["info"]);
    RuntimeAvailability {
        installed: true,
        ready: daemon.successful,
        diagnostic: if daemon.successful {
            "Docker Compose is ready".into()
        } else {
            "Docker is installed but not ready; start the daemon".into()
        },
    }
}

fn apple_availability(runner: &impl CommandRunner) -> RuntimeAvailability {
    let cli = probe(runner, "container", &["--version"]);
    if !cli.successful {
        return RuntimeAvailability {
            installed: false,
            ready: false,
            diagnostic: "Apple Container CLI is not available".into(),
        };
    }
    if !parsed_version(&cli.stdout).is_some_and(|(major, minor, _)| major == 1 && minor == 2) {
        return RuntimeAvailability {
            installed: true,
            ready: false,
            diagnostic: "Apple Container version is unsupported; install a qualified 1.2.x release"
                .into(),
        };
    }
    let system = probe(runner, "container", &["system", "status"]);
    RuntimeAvailability {
        installed: true,
        ready: system.successful,
        diagnostic: if system.successful {
            "Apple Container is ready".into()
        } else {
            "Apple Container is installed but not ready; run `container system start`".into()
        },
    }
}

fn resolve_runtime_with(
    runner: &impl CommandRunner,
    preference: RuntimePreference,
) -> Result<Runtime> {
    let docker = docker_availability(runner);
    let apple = apple_availability(runner);
    match preference {
        RuntimePreference::Docker => {
            ensure!(docker.ready, "{}", docker.diagnostic);
            Ok(Runtime::Docker)
        }
        RuntimePreference::Apple => {
            ensure!(apple.ready, "{}", apple.diagnostic);
            Ok(Runtime::Apple)
        }
        RuntimePreference::Auto if docker.ready => Ok(Runtime::Docker),
        RuntimePreference::Auto if apple.ready => Ok(Runtime::Apple),
        RuntimePreference::Auto if docker.installed => bail!("{}", docker.diagnostic),
        RuntimePreference::Auto if apple.installed => bail!("{}", apple.diagnostic),
        RuntimePreference::Auto => bail!(
            "no supported container runtime is ready; install/start Docker, or on Apple silicon macOS 26 install Apple Container 1.2.x and run `container system start`"
        ),
    }
}

fn state_base(root: &Path) -> PathBuf {
    root.join("target/minco/dev")
}

fn runtime_request(program: &str, arguments: Vec<OsString>) -> CommandRequest {
    CommandRequest {
        program: program.into(),
        arguments,
        environment: BTreeMap::new(),
        timeout: COMMAND_TIMEOUT,
    }
}

fn output_says_missing(output: &CommandOutput) -> bool {
    let stderr = String::from_utf8_lossy(&output.stderr).to_ascii_lowercase();
    stderr.contains("not found")
        || stderr.contains("no such object")
        || stderr.contains("no such volume")
}

fn inspect_resource(
    runner: &impl CommandRunner,
    runtime: Runtime,
    spec: &LocalServiceSpec,
    expected_environment: &BTreeMap<String, String>,
) -> Result<Option<InspectedResource>> {
    let request = match runtime {
        Runtime::Docker => {
            runtime_request("docker", vec!["inspect".into(), resource_name(spec).into()])
        }
        Runtime::Apple => runtime_request(
            "container",
            vec!["inspect".into(), resource_name(spec).into()],
        ),
    };
    let output = runner.run(&request)?;
    if !output.successful {
        if output_says_missing(&output) {
            return Ok(None);
        }
        bail!(
            "failed to inspect the exact {} resource `{}`",
            runtime.label(),
            resource_name(spec)
        );
    }
    match runtime {
        Runtime::Docker => {
            verify_docker_inspect(&output.stdout, spec, expected_environment).map(Some)
        }
        Runtime::Apple => {
            verify_apple_inspect(&output.stdout, spec, expected_environment).map(Some)
        }
    }
}

trait ReadinessChecker {
    async fn wait_until_ready(
        &self,
        spec: &LocalServiceSpec,
        environment: &BTreeMap<String, String>,
    ) -> Result<()>;
}

#[derive(Debug, Default)]
struct SystemReadinessChecker;

impl ReadinessChecker for SystemReadinessChecker {
    async fn wait_until_ready(
        &self,
        spec: &LocalServiceSpec,
        environment: &BTreeMap<String, String>,
    ) -> Result<()> {
        let deadline = Instant::now() + READY_TIMEOUT;
        while Instant::now() < deadline {
            let ready = match &spec.readiness {
                ServiceReadiness::Postgres {
                    expected_user,
                    expected_database,
                    ..
                } => {
                    postgres_authenticated_ready(
                        spec.host_port,
                        expected_user,
                        expected_database,
                        &environment["POSTGRES_PASSWORD"],
                    )
                    .await
                }
                ServiceReadiness::Rustack {
                    path,
                    requested_services,
                } => {
                    rustack_structurally_ready(
                        spec.host_port,
                        path,
                        requested_services,
                        &environment["DEFAULT_REGION"],
                    )
                    .await
                }
            };
            if ready {
                return Ok(());
            }
            tokio::time::sleep(RETRY_INTERVAL).await;
        }
        bail!(
            "{} did not become ready on its loopback endpoint within {} seconds",
            spec.service.label(),
            READY_TIMEOUT.as_secs()
        )
    }
}

async fn postgres_authenticated_ready(
    port: u16,
    user: &str,
    database: &str,
    password: &str,
) -> bool {
    let options = PgConnectOptions::new()
        .host("127.0.0.1")
        .port(port)
        .username(user)
        .password(password)
        .database(database)
        .ssl_mode(PgSslMode::Disable);
    let Ok(Ok(mut connection)) = tokio::time::timeout(
        Duration::from_millis(750),
        sqlx::PgConnection::connect_with(&options),
    )
    .await
    else {
        return false;
    };
    let Ok(Ok(row)) = tokio::time::timeout(
        Duration::from_millis(750),
        sqlx::query("SELECT current_user, current_database(), 1::INT4").fetch_one(&mut connection),
    )
    .await
    else {
        return false;
    };
    row.try_get::<String, _>(0).ok().as_deref() == Some(user)
        && row.try_get::<String, _>(1).ok().as_deref() == Some(database)
        && row.try_get::<i32, _>(2).ok() == Some(1)
}

#[derive(Debug, Deserialize)]
struct RustackHealth {
    services: BTreeMap<String, String>,
}

async fn rustack_structurally_ready(
    port: u16,
    path: &str,
    requested_services: &[String],
    region: &str,
) -> bool {
    let Ok(client) = reqwest::Client::builder()
        .timeout(Duration::from_millis(750))
        .redirect(reqwest::redirect::Policy::none())
        .build()
    else {
        return false;
    };
    let endpoint = format!("http://127.0.0.1:{port}{path}");
    let Ok(response) = client.get(endpoint).send().await else {
        return false;
    };
    if !response.status().is_success() {
        return false;
    }
    let Ok(bytes) = response.bytes().await else {
        return false;
    };
    if verify_rustack_health(&bytes, requested_services).is_err() {
        return false;
    }
    !requested_services.iter().any(|service| service == "sts")
        || rustack_sts_identity_ready(port, region).await
}

async fn rustack_sts_identity_ready(port: u16, region: &str) -> bool {
    let endpoint = format!("http://127.0.0.1:{port}");
    let config = aws_sdk_sts::Config::builder()
        .behavior_version(aws_sdk_sts::config::BehaviorVersion::latest())
        .credentials_provider(aws_sdk_sts::config::Credentials::new(
            "test",
            "test",
            None,
            None,
            "minco-local-static",
        ))
        .region(aws_sdk_sts::config::Region::new(region.to_owned()))
        .endpoint_url(endpoint)
        .build();
    let client = aws_sdk_sts::Client::from_conf(config);
    matches!(
        tokio::time::timeout(Duration::from_millis(750), client.get_caller_identity().send()).await,
        Ok(Ok(identity)) if identity.account() == Some("000000000000")
    )
}

fn verify_rustack_health(output: &[u8], requested_services: &[String]) -> Result<()> {
    let health: RustackHealth =
        serde_json::from_slice(output).context("parse Rustack health JSON")?;
    for service in requested_services {
        ensure!(
            health
                .services
                .get(service)
                .is_some_and(|state| state == "running"),
            "Rustack health does not report every requested service as running"
        );
    }
    Ok(())
}

fn apple_volume_create_arguments(spec: &LocalServiceSpec) -> Vec<OsString> {
    let volume = spec
        .volume
        .as_ref()
        .expect("volume creation requires a typed volume contract");
    let mut arguments = vec!["volume".into(), "create".into()];
    for (name, value) in &spec.ownership {
        arguments.extend(["--label".into(), format!("{name}={value}").into()]);
    }
    arguments.push(volume.name.clone().into());
    arguments
}

fn inspect_apple_volume(runner: &impl CommandRunner, spec: &LocalServiceSpec) -> Result<bool> {
    let volume = spec
        .volume
        .as_ref()
        .context("service does not declare a persistent volume")?;
    let output = runner.run(&runtime_request(
        "container",
        vec![
            "volume".into(),
            "inspect".into(),
            volume.name.clone().into(),
        ],
    ))?;
    if !output.successful {
        if output_says_missing(&output) {
            return Ok(false);
        }
        bail!("failed to inspect the exact Apple Container persistent volume");
    }
    verify_apple_volume_inspect(&output.stdout, spec)?;
    Ok(true)
}

fn inspect_docker_volume(runner: &impl CommandRunner, spec: &LocalServiceSpec) -> Result<bool> {
    let volume = spec
        .volume
        .as_ref()
        .context("service does not declare a persistent volume")?;
    let output = runner.run(&runtime_request(
        "docker",
        vec![
            "volume".into(),
            "inspect".into(),
            volume.name.clone().into(),
        ],
    ))?;
    if !output.successful {
        if output_says_missing(&output) {
            return Ok(false);
        }
        bail!("failed to inspect the exact Docker persistent volume");
    }
    verify_docker_volume_inspect(&output.stdout, spec)?;
    Ok(true)
}

fn diagnose_legacy_compose_volume(runner: &impl CommandRunner, spec: &LocalServiceSpec) {
    if spec.service != Service::Postgres {
        return;
    }
    let output = runner.run(&runtime_request(
        "docker",
        vec![
            "volume".into(),
            "inspect".into(),
            "local_minco-postgres".into(),
        ],
    ));
    if output.is_ok_and(|output| output.successful) {
        eprintln!(
            "minco: legacy Compose volume `local_minco-postgres` exists; it is not adopted or deleted; inspect and migrate its data explicitly"
        );
    }
}

fn run_checked(
    runner: &impl CommandRunner,
    request: &CommandRequest,
    description: &str,
) -> Result<()> {
    let output = runner.run(request)?;
    ensure!(output.successful, "{description} failed");
    Ok(())
}

fn recent_runtime_logs(
    runner: &impl CommandRunner,
    runtime: Runtime,
    spec: &LocalServiceSpec,
    expected_environment: &BTreeMap<String, String>,
) -> String {
    let arguments = match runtime {
        Runtime::Docker => vec![
            "logs".into(),
            "--tail".into(),
            "50".into(),
            resource_name(spec).into(),
        ],
        Runtime::Apple => vec![
            "logs".into(),
            "-n".into(),
            "50".into(),
            resource_name(spec).into(),
        ],
    };
    let program = match runtime {
        Runtime::Docker => "docker",
        Runtime::Apple => "container",
    };
    let Ok(output) = runner.run(&runtime_request(program, arguments)) else {
        return "unavailable".into();
    };
    if !output.successful {
        return "unavailable".into();
    }
    let mut rendered = [output.stdout, output.stderr].concat();
    rendered.truncate(8_192);
    let mut rendered = String::from_utf8_lossy(&rendered).into_owned();
    for name in &spec.secret_environment {
        if let Some(secret) = expected_environment
            .get(name)
            .filter(|value| !value.is_empty())
        {
            rendered = rendered.replace(secret, "<redacted>");
        }
    }
    rendered
}

fn redact_service_secrets(
    mut value: String,
    spec: &LocalServiceSpec,
    expected_environment: &BTreeMap<String, String>,
) -> String {
    for name in &spec.secret_environment {
        if let Some(secret) = expected_environment
            .get(name)
            .filter(|value| !value.is_empty())
        {
            value = value.replace(secret, "<redacted>");
        }
    }
    value
}

fn cleanup_created_resource(
    runner: &impl CommandRunner,
    runtime: Runtime,
    spec: &LocalServiceSpec,
    expected_environment: &BTreeMap<String, String>,
) -> Result<()> {
    stop_exact_resource(runner, runtime, spec, expected_environment)?;
    let Some(resource) = inspect_resource(runner, runtime, spec, expected_environment)? else {
        return Ok(());
    };
    ensure!(
        resource.state == ResourceState::Stopped,
        "attempt-created owned resource is still running after cleanup stop"
    );
    let (program, arguments) = match runtime {
        Runtime::Docker => ("docker", vec!["rm".into(), resource.name.into()]),
        Runtime::Apple => ("container", vec!["delete".into(), resource.name.into()]),
    };
    run_checked(
        runner,
        &runtime_request(program, arguments),
        "remove attempt-created exact owned resource",
    )?;
    ensure!(
        inspect_resource(runner, runtime, spec, expected_environment)?.is_none(),
        "attempt-created owned resource remains after cleanup"
    );
    Ok(())
}

fn ensure_port_available(spec: &LocalServiceSpec) -> Result<()> {
    let listener = std::net::TcpListener::bind((spec.bind_address.as_str(), spec.host_port));
    ensure!(
        listener.is_ok(),
        "loopback port {} is occupied by a foreign or unverified process; no local service was changed",
        spec.host_port
    );
    Ok(())
}

fn select_start_runtime(
    runner: &impl CommandRunner,
    arguments: &ServiceArgs,
    receipt: Option<&LifecycleReceipt>,
    spec: &LocalServiceSpec,
    expected_environment: &BTreeMap<String, String>,
) -> Result<Runtime> {
    if let Some(receipt) = receipt {
        let explicit_runtime = match arguments.runtime {
            RuntimePreference::Auto => None,
            RuntimePreference::Docker => Some(Runtime::Docker),
            RuntimePreference::Apple => Some(Runtime::Apple),
        };
        ensure!(
            explicit_runtime.is_none_or(|runtime| runtime == receipt.runtime),
            "requested runtime disagrees with the exact lifecycle receipt for `{}`; stop the receipted resource before changing runtimes",
            receipt.resource
        );
        ensure_selected_runtime_ready(runner, receipt.runtime, &receipt.resource)?;
        return Ok(receipt.runtime);
    }

    if arguments.runtime != RuntimePreference::Auto {
        return resolve_runtime_with(runner, arguments.runtime);
    }

    let runtimes = startup_runtimes(runner)?;
    let mut owned = Vec::new();
    for runtime in &runtimes {
        if inspect_resource(runner, *runtime, spec, expected_environment)?.is_some() {
            owned.push(*runtime);
        }
    }
    ensure!(
        owned.len() <= 1,
        "both Docker Compose and Apple Container contain an exact owned `{}` resource; no runtime was changed; stop one runtime explicitly after inspecting both resources",
        resource_name(spec)
    );
    Ok(owned.first().copied().unwrap_or_else(|| runtimes[0]))
}

async fn start_with_dependencies(
    runner: &impl CommandRunner,
    readiness: &impl ReadinessChecker,
    arguments: &ServiceArgs,
    source_environment: &BTreeMap<String, String>,
    root: &Path,
) -> Result<()> {
    validate(arguments, true)?;
    let spec = LocalServiceSpec::from_arguments(arguments, source_environment)?;
    let base = state_base(root);
    let _lock = ServiceLock::acquire(&base, &spec)?;
    let receipt = read_receipt(&base, &spec)?;
    let expected_environment = service_environment(&spec, source_environment)?;
    let runtime = select_start_runtime(
        runner,
        arguments,
        receipt.as_ref(),
        &spec,
        &expected_environment,
    )?;
    let existing = inspect_resource(runner, runtime, &spec, &expected_environment)?;
    if existing
        .as_ref()
        .is_some_and(|resource| resource.state == ResourceState::Running)
    {
        readiness
            .wait_until_ready(&spec, &expected_environment)
            .await?;
        write_receipt_atomic(&base, &LifecycleReceipt::for_spec(runtime, &spec))?;
        return Ok(());
    }
    ensure_port_available(&spec)?;
    let created = existing.is_none();
    let startup = (|| -> Result<()> {
        match (runtime, existing.as_ref().map(|resource| resource.state)) {
            (Runtime::Apple, None) => {
                if spec.volume.is_some() && !inspect_apple_volume(runner, &spec)? {
                    run_checked(
                        runner,
                        &runtime_request("container", apple_volume_create_arguments(&spec)),
                        "create owned Apple Container persistent volume",
                    )?;
                    ensure!(
                        inspect_apple_volume(runner, &spec)?,
                        "created Apple Container volume could not be verified"
                    );
                }
                let mut request = runtime_request("container", apple_run_arguments(&spec));
                request.environment = expected_environment.clone();
                request.timeout = READY_TIMEOUT;
                run_checked(runner, &request, "start owned Apple Container service")?;
            }
            (Runtime::Apple, Some(ResourceState::Stopped)) => {
                run_checked(
                    runner,
                    &runtime_request(
                        "container",
                        vec!["start".into(), resource_name(&spec).into()],
                    ),
                    "restart owned Apple Container service",
                )?;
            }
            (Runtime::Docker, None) => {
                diagnose_legacy_compose_volume(runner, &spec);
                if spec.volume.is_some() {
                    let _ = inspect_docker_volume(runner, &spec)?;
                }
                let config_request = compose_request(
                    arguments,
                    &spec,
                    &expected_environment,
                    &["config", "--format", "json"],
                )?;
                let config = runner.run(&config_request)?;
                ensure!(
                    config.successful,
                    "Docker Compose configuration validation failed"
                );
                verify_compose_config(&config.stdout, &spec, &expected_environment)?;
                run_checked(
                    runner,
                    &compose_request(
                        arguments,
                        &spec,
                        &expected_environment,
                        &["up", "--detach", "--no-deps", spec.service.label()],
                    )?,
                    "start exact owned Docker Compose service",
                )?;
                if spec.volume.is_some() {
                    ensure!(
                        inspect_docker_volume(runner, &spec)?,
                        "Docker Compose reported startup success but the owned persistent volume is absent"
                    );
                }
            }
            (Runtime::Docker, Some(ResourceState::Stopped)) => {
                run_checked(
                    runner,
                    &runtime_request("docker", vec!["start".into(), resource_name(&spec).into()]),
                    "restart exact owned Docker service",
                )?;
            }
            (_, Some(ResourceState::Running)) => unreachable!("running resource returned early"),
        }
        let verified = inspect_resource(runner, runtime, &spec, &expected_environment)?
            .context("runtime reported startup success but the owned resource is absent")?;
        ensure!(
            verified.state == ResourceState::Running,
            "runtime reported startup success but the owned resource is stopped"
        );
        Ok(())
    })();
    if let Err(error) = startup {
        let diagnostics = recent_runtime_logs(runner, runtime, &spec, &expected_environment);
        let error = redact_service_secrets(format!("{error:#}"), &spec, &expected_environment);
        if created
            && let Err(cleanup_error) =
                cleanup_created_resource(runner, runtime, &spec, &expected_environment)
        {
            bail!(
                "{error}; recent runtime logs: {diagnostics}; exact owned startup cleanup failed: {cleanup_error:#}"
            );
        }
        bail!("{error}; recent runtime logs: {diagnostics}");
    }
    if let Err(error) = readiness
        .wait_until_ready(&spec, &expected_environment)
        .await
    {
        let diagnostics = recent_runtime_logs(runner, runtime, &spec, &expected_environment);
        let error = redact_service_secrets(format!("{error:#}"), &spec, &expected_environment);
        if created
            && let Err(cleanup_error) =
                cleanup_created_resource(runner, runtime, &spec, &expected_environment)
        {
            bail!(
                "{error}; recent runtime logs: {diagnostics}; exact owned startup cleanup failed: {cleanup_error:#}"
            );
        }
        bail!("{error}; recent runtime logs: {diagnostics}");
    }
    write_receipt_atomic(&base, &LifecycleReceipt::for_spec(runtime, &spec))?;
    Ok(())
}

fn stop_exact_resource(
    runner: &impl CommandRunner,
    runtime: Runtime,
    spec: &LocalServiceSpec,
    expected_environment: &BTreeMap<String, String>,
) -> Result<()> {
    let Some(resource) = inspect_resource(runner, runtime, spec, expected_environment)? else {
        return Ok(());
    };
    if resource.state == ResourceState::Stopped {
        return Ok(());
    }
    match runtime {
        Runtime::Apple => {
            let mut request = runtime_request(
                "container",
                vec![
                    "stop".into(),
                    "--time".into(),
                    "10".into(),
                    resource.name.into(),
                ],
            );
            request.timeout = Duration::from_secs(15);
            run_checked(runner, &request, "stop exact owned Apple Container service")
        }
        Runtime::Docker => {
            let mut request = runtime_request(
                "docker",
                vec![
                    "stop".into(),
                    "--timeout".into(),
                    "10".into(),
                    resource.name.into(),
                ],
            );
            request.timeout = Duration::from_secs(15);
            run_checked(runner, &request, "stop exact owned Docker service")
        }
    }
}

fn ensure_selected_runtime_ready(
    runner: &impl CommandRunner,
    runtime: Runtime,
    resource: &str,
) -> Result<()> {
    let availability = match runtime {
        Runtime::Docker => docker_availability(runner),
        Runtime::Apple => apple_availability(runner),
    };
    ensure!(
        availability.ready,
        "{}; residual owned resource `{resource}` remains on {}",
        availability.diagnostic,
        runtime.label()
    );
    Ok(())
}

fn remove_receipt(base: &Path, spec: &LocalServiceSpec) -> Result<()> {
    let path = receipt_path(base, spec);
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).context("remove stopped local service receipt"),
    }
}

fn discovery_runtimes(
    runner: &impl CommandRunner,
    preference: RuntimePreference,
) -> Result<Vec<Runtime>> {
    match preference {
        RuntimePreference::Docker => {
            let availability = docker_availability(runner);
            ensure!(availability.ready, "{}", availability.diagnostic);
            Ok(vec![Runtime::Docker])
        }
        RuntimePreference::Apple => {
            let availability = apple_availability(runner);
            ensure!(availability.ready, "{}", availability.diagnostic);
            Ok(vec![Runtime::Apple])
        }
        RuntimePreference::Auto => {
            let docker = docker_availability(runner);
            let apple = apple_availability(runner);
            ensure!(
                !docker.installed || docker.ready,
                "{}; start Docker to complete ownership discovery before stopping",
                docker.diagnostic
            );
            ensure!(
                !apple.installed || apple.ready,
                "{}; start Apple Container to complete ownership discovery before stopping",
                apple.diagnostic
            );
            let runtimes = [
                (Runtime::Docker, docker.ready),
                (Runtime::Apple, apple.ready),
            ]
            .into_iter()
            .filter_map(|(runtime, ready)| ready.then_some(runtime))
            .collect::<Vec<_>>();
            ensure!(
                !runtimes.is_empty(),
                "no supported container runtime is ready; ownership discovery cannot prove the service is stopped"
            );
            Ok(runtimes)
        }
    }
}

fn startup_runtimes(runner: &impl CommandRunner) -> Result<Vec<Runtime>> {
    let docker = docker_availability(runner);
    let apple = apple_availability(runner);
    let runtimes = [
        (Runtime::Docker, docker.ready),
        (Runtime::Apple, apple.ready),
    ]
    .into_iter()
    .filter_map(|(runtime, ready)| ready.then_some(runtime))
    .collect::<Vec<_>>();
    if !runtimes.is_empty() {
        return Ok(runtimes);
    }
    if docker.installed {
        bail!("{}", docker.diagnostic);
    }
    if apple.installed {
        bail!("{}", apple.diagnostic);
    }
    bail!(
        "no supported container runtime is ready; install/start Docker, or on Apple silicon macOS 26 install Apple Container 1.2.x and run `container system start`"
    )
}

fn stop_with_runner(
    runner: &impl CommandRunner,
    arguments: &ServiceArgs,
    source_environment: &BTreeMap<String, String>,
    root: &Path,
) -> Result<()> {
    validate(arguments, false)?;
    let spec = LocalServiceSpec::from_arguments(arguments, source_environment)?;
    let expected_environment = service_environment(&spec, source_environment)?;
    let base = state_base(root);
    let _lock = ServiceLock::acquire(&base, &spec)?;
    let receipt = read_receipt(&base, &spec)?;
    let runtime = if let Some(receipt) = &receipt {
        ensure_selected_runtime_ready(runner, receipt.runtime, &receipt.resource)?;
        receipt.runtime
    } else {
        let mut owned = Vec::new();
        for runtime in discovery_runtimes(runner, arguments.runtime)? {
            if inspect_resource(runner, runtime, &spec, &expected_environment)?.is_some() {
                owned.push(runtime);
            }
        }
        ensure!(
            owned.len() <= 1,
            "both Docker Compose and Apple Container contain an exact owned `{}` resource; no runtime was changed; stop one runtime explicitly after inspecting both resources",
            resource_name(&spec)
        );
        let Some(runtime) = owned.first().copied() else {
            remove_receipt(&base, &spec)?;
            return Ok(());
        };
        runtime
    };
    stop_exact_resource(runner, runtime, &spec, &expected_environment)?;
    if let Some(resource) = inspect_resource(runner, runtime, &spec, &expected_environment)? {
        ensure!(
            resource.state == ResourceState::Stopped,
            "owned local service `{}` is still running after stop",
            resource.name
        );
    }
    remove_receipt(&base, &spec)
}

async fn start_with_runner(
    runner: &impl CommandRunner,
    arguments: &ServiceArgs,
    source_environment: &BTreeMap<String, String>,
    root: &Path,
) -> Result<()> {
    start_with_dependencies(
        runner,
        &SystemReadinessChecker,
        arguments,
        source_environment,
        root,
    )
    .await
}

fn resource_name(spec: &LocalServiceSpec) -> String {
    scoped_resource_name(
        &spec.ownership[OWNERSHIP_APPLICATION],
        &spec.ownership[OWNERSHIP_WORKSPACE],
        &format!("-{}", spec.service.label()),
    )
}

fn compose_project_name(spec: &LocalServiceSpec) -> String {
    scoped_resource_name(
        &spec.ownership[OWNERSHIP_APPLICATION],
        &spec.ownership[OWNERSHIP_WORKSPACE],
        "",
    )
}

fn compose_arguments(
    arguments: &ServiceArgs,
    spec: &LocalServiceSpec,
    action: &[&str],
) -> Vec<OsString> {
    let mut result = vec![
        "compose".into(),
        "--project-name".into(),
        compose_project_name(spec).into(),
        "-f".into(),
        arguments.compose_file.as_os_str().to_owned(),
    ];
    result.extend(action.iter().map(OsString::from));
    result
}

fn compose_environment(
    spec: &LocalServiceSpec,
    expected_environment: &BTreeMap<String, String>,
) -> Result<BTreeMap<String, String>> {
    let prefix = match spec.service {
        Service::Postgres => "MINCO_POSTGRES",
        Service::Rustack => "MINCO_RUSTACK",
    };
    let mut environment = BTreeMap::from([
        (format!("{prefix}_RESOURCE_NAME"), resource_name(spec)),
        (format!("{prefix}_IMAGE"), spec.image.clone()),
        (format!("{prefix}_PORT"), spec.host_port.to_string()),
    ]);
    for (name, value) in &spec.ownership {
        let suffix = name
            .strip_prefix("dev.minco.")
            .context("unsupported local ownership label")?
            .replace('.', "_")
            .to_ascii_uppercase();
        environment.insert(format!("{prefix}_LABEL_{suffix}"), value.clone());
    }
    match spec.service {
        Service::Postgres => {
            environment.insert(
                "MINCO_POSTGRES_DB".into(),
                expected_environment["POSTGRES_DB"].clone(),
            );
            environment.insert(
                "MINCO_POSTGRES_USER".into(),
                expected_environment["POSTGRES_USER"].clone(),
            );
            environment.insert(
                "MINCO_POSTGRES_PASSWORD".into(),
                expected_environment["POSTGRES_PASSWORD"].clone(),
            );
            environment.insert(
                "MINCO_POSTGRES_VOLUME_NAME".into(),
                spec.volume
                    .as_ref()
                    .context("PostgreSQL requires a persistent volume")?
                    .name
                    .clone(),
            );
        }
        Service::Rustack => {
            environment.insert(
                "MINCO_RUSTACK_SERVICES".into(),
                expected_environment["SERVICES"].clone(),
            );
            environment.insert(
                "MINCO_RUSTACK_REGION".into(),
                expected_environment["DEFAULT_REGION"].clone(),
            );
            environment.insert(
                "MINCO_RUSTACK_LOG_LEVEL".into(),
                expected_environment["LOG_LEVEL"].clone(),
            );
        }
    }
    Ok(environment)
}

fn compose_request(
    arguments: &ServiceArgs,
    spec: &LocalServiceSpec,
    expected_environment: &BTreeMap<String, String>,
    action: &[&str],
) -> Result<CommandRequest> {
    Ok(CommandRequest {
        program: "docker".into(),
        arguments: compose_arguments(arguments, spec, action),
        environment: compose_environment(spec, expected_environment)?,
        timeout: READY_TIMEOUT,
    })
}

#[derive(Debug, Deserialize)]
struct ComposeConfig {
    services: BTreeMap<String, ComposeServiceConfig>,
    #[serde(default)]
    volumes: BTreeMap<String, ComposeTopLevelVolume>,
}

#[derive(Debug, Deserialize)]
struct ComposeTopLevelVolume {
    name: Option<String>,
    #[serde(default)]
    labels: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct ComposeServiceConfig {
    container_name: String,
    image: String,
    #[serde(default)]
    environment: BTreeMap<String, String>,
    #[serde(default)]
    labels: BTreeMap<String, String>,
    #[serde(default)]
    ports: Vec<ComposePortConfig>,
    #[serde(default)]
    volumes: Vec<ComposeVolumeConfig>,
}

#[derive(Debug, Deserialize)]
struct ComposePortConfig {
    host_ip: String,
    protocol: String,
    published: String,
    target: u16,
}

#[derive(Debug, Deserialize)]
struct ComposeVolumeConfig {
    source: String,
    target: String,
    #[serde(rename = "type")]
    kind: String,
}

fn verify_compose_config(
    output: &[u8],
    spec: &LocalServiceSpec,
    expected_environment: &BTreeMap<String, String>,
) -> Result<()> {
    let config: ComposeConfig =
        serde_json::from_slice(output).context("parse Docker Compose config JSON")?;
    let service = config
        .services
        .get(spec.service.label())
        .context("Docker Compose config does not declare the requested first-class service")?;
    ensure!(
        service.container_name == resource_name(spec),
        "Docker Compose resource identity does not match the typed local service"
    );
    ensure!(
        service.image == spec.image,
        "Docker Compose image does not match the typed local service"
    );
    for (name, expected) in &spec.ownership {
        ensure!(
            service.labels.get(name) == Some(expected),
            "Docker Compose ownership is missing or mismatched"
        );
    }
    ensure!(
        expected_environment
            .iter()
            .all(|(name, expected)| service.environment.get(name) == Some(expected)),
        "Docker Compose environment does not match the typed local service"
    );
    ensure!(
        service.ports.len() == 1
            && service.ports[0].host_ip == spec.bind_address
            && service.ports[0].published == spec.host_port.to_string()
            && service.ports[0].target == spec.container_port
            && service.ports[0].protocol == "tcp",
        "Docker Compose port configuration does not match the typed local service"
    );
    match &spec.volume {
        Some(volume) => {
            ensure!(
                service.volumes.len() == 1
                    && service.volumes[0].kind == "volume"
                    && service.volumes[0].target == volume.container_path,
                "Docker Compose volume configuration does not match the typed local service"
            );
            let top_level = config
                .volumes
                .get(&service.volumes[0].source)
                .context("Docker Compose named volume declaration is missing")?;
            ensure!(
                top_level.name.as_deref() == Some(volume.name.as_str()),
                "Docker Compose named volume identity does not match the typed local service"
            );
            for (name, expected) in &spec.ownership {
                ensure!(
                    top_level.labels.get(name) == Some(expected),
                    "Docker Compose volume ownership is missing or mismatched"
                );
            }
        }
        None => ensure!(
            service.volumes.is_empty(),
            "Docker Compose config declares an unsupported volume for the typed local service"
        ),
    }
    Ok(())
}

fn apple_run_arguments(spec: &LocalServiceSpec) -> Vec<OsString> {
    let mut result = vec![
        "run".into(),
        "--detach".into(),
        "--rm".into(),
        "--name".into(),
        resource_name(spec).into(),
        "--platform".into(),
        "linux/arm64".into(),
    ];
    for (name, value) in &spec.ownership {
        result.extend(["--label".into(), format!("{name}={value}").into()]);
    }
    for name in spec
        .environment
        .keys()
        .chain(spec.secret_environment.iter())
    {
        result.extend(["--env".into(), name.into()]);
    }
    result.extend([
        "--publish".into(),
        format!(
            "{}:{}:{}",
            spec.bind_address, spec.host_port, spec.container_port
        )
        .into(),
    ]);
    if let Some(volume) = &spec.volume {
        result.extend([
            "--volume".into(),
            format!("{}:{}", volume.name, volume.container_path).into(),
        ]);
    }
    result.push(spec.image.clone().into());
    result
}

fn service_environment(
    spec: &LocalServiceSpec,
    source_environment: &BTreeMap<String, String>,
) -> Result<BTreeMap<String, String>> {
    let mut resolved = spec.environment.clone();
    for name in &spec.secret_environment {
        let value = match name.as_str() {
            "POSTGRES_PASSWORD" => environment_or(
                source_environment,
                "MINCO_POSTGRES_PASSWORD",
                POSTGRES_PASSWORD,
            ),
            other => bail!("unsupported local service secret source `{other}`"),
        };
        resolved.insert(name.clone(), value);
    }
    Ok(resolved)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResourceState {
    Running,
    Stopped,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct InspectedResource {
    name: String,
    state: ResourceState,
}

#[derive(Debug, Deserialize)]
struct AppleInspectRecord {
    id: String,
    configuration: AppleConfiguration,
    status: AppleStatus,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AppleConfiguration {
    image: AppleImage,
    init_process: AppleInitProcess,
    #[serde(default)]
    labels: BTreeMap<String, String>,
    #[serde(default)]
    mounts: Vec<AppleMount>,
    platform: ApplePlatform,
    #[serde(default)]
    published_ports: Vec<ApplePublishedPort>,
}

#[derive(Debug, Deserialize)]
struct AppleImage {
    reference: String,
    descriptor: Option<AppleImageDescriptor>,
}

#[derive(Debug, Deserialize)]
struct AppleImageDescriptor {
    digest: String,
}

#[derive(Debug, Deserialize)]
struct AppleInitProcess {
    #[serde(default)]
    environment: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct AppleMount {
    destination: String,
    #[serde(rename = "type")]
    kind: AppleMountKind,
}

#[derive(Debug, Deserialize)]
struct AppleMountKind {
    volume: Option<AppleVolumeMount>,
}

#[derive(Debug, Deserialize)]
struct AppleVolumeMount {
    name: String,
}

#[derive(Debug, Deserialize)]
struct ApplePlatform {
    architecture: String,
    os: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApplePublishedPort {
    container_port: u16,
    count: u16,
    host_address: String,
    host_port: u16,
    proto: String,
}

#[derive(Debug, Deserialize)]
struct AppleStatus {
    state: String,
}

fn verify_apple_inspect(
    output: &[u8],
    spec: &LocalServiceSpec,
    expected_environment: &BTreeMap<String, String>,
) -> Result<InspectedResource> {
    let records: Vec<AppleInspectRecord> =
        serde_json::from_slice(output).context("parse Apple Container inspect JSON")?;
    ensure!(
        records.len() == 1,
        "Apple Container inspect returned an ambiguous resource set"
    );
    let record = &records[0];
    ensure!(
        record.id == resource_name(spec),
        "Apple Container resource identity does not match the requested service"
    );
    for (name, expected) in &spec.ownership {
        ensure!(
            record.configuration.labels.get(name) == Some(expected),
            "Apple Container resource ownership is missing or mismatched"
        );
    }
    ensure!(
        apple_image_matches(&record.configuration.image, &spec.image),
        "Apple Container resource configuration does not match the requested service"
    );
    ensure!(
        record.configuration.platform.os == "linux"
            && record.configuration.platform.architecture == "arm64",
        "Apple Container resource platform does not match native linux/arm64"
    );
    ensure!(
        record.configuration.published_ports.len() == 1
            && record.configuration.published_ports[0].container_port == spec.container_port
            && record.configuration.published_ports[0].host_port == spec.host_port
            && record.configuration.published_ports[0].host_address == spec.bind_address
            && record.configuration.published_ports[0].count == 1
            && record.configuration.published_ports[0].proto == "tcp",
        "Apple Container resource port configuration does not match the requested service"
    );
    match &spec.volume {
        Some(volume) => ensure!(
            record.configuration.mounts.iter().any(|mount| {
                mount.destination == volume.container_path
                    && mount
                        .kind
                        .volume
                        .as_ref()
                        .is_some_and(|actual| actual.name == volume.name)
            }),
            "Apple Container resource volume configuration does not match the requested service"
        ),
        None => ensure!(
            record.configuration.mounts.is_empty(),
            "Apple Container resource has an unexpected volume configuration"
        ),
    }
    let actual_environment = record
        .configuration
        .init_process
        .environment
        .iter()
        .filter_map(|entry| entry.split_once('='))
        .collect::<BTreeMap<_, _>>();
    ensure!(
        expected_environment.iter().all(|(name, expected)| {
            actual_environment.get(name.as_str()).copied() == Some(expected.as_str())
        }),
        "Apple Container resource environment configuration does not match the requested service"
    );
    let state = match record.status.state.as_str() {
        "running" => ResourceState::Running,
        "stopped" => ResourceState::Stopped,
        _ => bail!("Apple Container resource has an unsupported lifecycle state"),
    };
    Ok(InspectedResource {
        name: record.id.clone(),
        state,
    })
}

fn apple_image_matches(actual: &AppleImage, expected: &str) -> bool {
    if actual.reference == expected {
        return true;
    }
    let Some((expected_name, expected_digest)) = expected.rsplit_once('@') else {
        return false;
    };
    let Some((actual_name, actual_digest)) = actual.reference.rsplit_once('@') else {
        return false;
    };
    let expected_repository = expected_name
        .rsplit_once(':')
        .map_or(expected_name, |(repository, _)| repository);
    actual_name == expected_repository
        && actual_digest == expected_digest
        && actual
            .descriptor
            .as_ref()
            .is_some_and(|descriptor| descriptor.digest == expected_digest)
}

#[derive(Debug, Deserialize)]
struct AppleVolumeInspectRecord {
    id: String,
    configuration: AppleVolumeConfiguration,
}

#[derive(Debug, Deserialize)]
struct AppleVolumeConfiguration {
    name: String,
    #[serde(default)]
    labels: BTreeMap<String, String>,
    driver: String,
    format: String,
}

fn verify_apple_volume_inspect(output: &[u8], spec: &LocalServiceSpec) -> Result<()> {
    let expected = spec
        .volume
        .as_ref()
        .context("service does not declare a persistent volume")?;
    let records: Vec<AppleVolumeInspectRecord> =
        serde_json::from_slice(output).context("parse Apple Container volume inspect JSON")?;
    ensure!(
        records.len() == 1,
        "Apple Container volume inspect returned an ambiguous resource set"
    );
    let record = &records[0];
    ensure!(
        record.id == expected.name && record.configuration.name == expected.name,
        "Apple Container volume identity does not match the requested service"
    );
    for (name, value) in &spec.ownership {
        ensure!(
            record.configuration.labels.get(name) == Some(value),
            "Apple Container volume ownership is missing or mismatched"
        );
    }
    ensure!(
        record.configuration.driver == "local" && record.configuration.format == "ext4",
        "Apple Container volume configuration does not match the requested service"
    );
    Ok(())
}

#[derive(Debug, Deserialize)]
struct DockerVolumeInspectRecord {
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "Driver")]
    driver: String,
    #[serde(rename = "Labels", default)]
    labels: BTreeMap<String, String>,
}

fn verify_docker_volume_inspect(output: &[u8], spec: &LocalServiceSpec) -> Result<()> {
    let expected = spec
        .volume
        .as_ref()
        .context("service does not declare a persistent volume")?;
    let records: Vec<DockerVolumeInspectRecord> =
        serde_json::from_slice(output).context("parse Docker volume inspect JSON")?;
    ensure!(
        records.len() == 1,
        "Docker volume inspect returned an ambiguous resource set"
    );
    let record = &records[0];
    ensure!(
        record.name == expected.name && record.driver == "local",
        "Docker volume identity does not match the requested service"
    );
    for (name, value) in &spec.ownership {
        ensure!(
            record.labels.get(name) == Some(value),
            "Docker volume ownership is missing or mismatched"
        );
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
struct DockerInspectRecord {
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "State")]
    state: DockerState,
    #[serde(rename = "Config")]
    configuration: DockerConfiguration,
    #[serde(rename = "NetworkSettings")]
    network_settings: DockerNetworkSettings,
    #[serde(rename = "HostConfig", default)]
    host_config: DockerHostConfig,
    #[serde(rename = "Mounts", default)]
    mounts: Vec<DockerMount>,
}

#[derive(Debug, Deserialize)]
struct DockerState {
    #[serde(rename = "Status")]
    status: String,
}

#[derive(Debug, Deserialize)]
struct DockerConfiguration {
    #[serde(rename = "Image")]
    image: String,
    #[serde(rename = "Env", default)]
    environment: Vec<String>,
    #[serde(rename = "Labels", default)]
    labels: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct DockerNetworkSettings {
    #[serde(rename = "Ports", default)]
    ports: BTreeMap<String, Option<Vec<DockerPortBinding>>>,
}

#[derive(Debug, Default, Deserialize)]
struct DockerHostConfig {
    #[serde(rename = "PortBindings", default)]
    port_bindings: BTreeMap<String, Option<Vec<DockerPortBinding>>>,
}

#[derive(Debug, Deserialize)]
struct DockerPortBinding {
    #[serde(rename = "HostIp")]
    host_ip: String,
    #[serde(rename = "HostPort")]
    host_port: String,
}

#[derive(Debug, Deserialize)]
struct DockerMount {
    #[serde(rename = "Type")]
    kind: String,
    #[serde(rename = "Name")]
    name: Option<String>,
    #[serde(rename = "Destination")]
    destination: String,
}

fn verify_docker_inspect(
    output: &[u8],
    spec: &LocalServiceSpec,
    expected_environment: &BTreeMap<String, String>,
) -> Result<InspectedResource> {
    let records: Vec<DockerInspectRecord> =
        serde_json::from_slice(output).context("parse Docker inspect JSON")?;
    ensure!(
        records.len() == 1,
        "Docker inspect returned an ambiguous resource set"
    );
    let record = &records[0];
    let name = record.name.trim_start_matches('/');
    ensure!(
        name == resource_name(spec),
        "Docker resource identity does not match the requested service"
    );
    for (label, expected) in &spec.ownership {
        ensure!(
            record.configuration.labels.get(label) == Some(expected),
            "Docker resource ownership is missing or mismatched"
        );
    }
    ensure!(
        record.configuration.image == spec.image,
        "Docker resource configuration does not match the requested service"
    );
    let port_key = format!("{}/tcp", spec.container_port);
    let bindings = record
        .network_settings
        .ports
        .get(&port_key)
        .and_then(Option::as_ref)
        .or_else(|| {
            record
                .host_config
                .port_bindings
                .get(&port_key)
                .and_then(Option::as_ref)
        })
        .context("Docker resource loopback port mapping is missing")?;
    ensure!(
        bindings.len() == 1
            && bindings[0].host_ip == spec.bind_address
            && bindings[0].host_port == spec.host_port.to_string(),
        "Docker resource port configuration does not match the requested service"
    );
    match &spec.volume {
        Some(volume) => ensure!(
            record.mounts.len() == 1
                && record.mounts[0].kind == "volume"
                && record.mounts[0].name.as_deref() == Some(volume.name.as_str())
                && record.mounts[0].destination == volume.container_path,
            "Docker resource volume configuration does not match the requested service"
        ),
        None => ensure!(
            record.mounts.is_empty(),
            "Docker resource has an unexpected volume configuration"
        ),
    }
    let actual_environment = record
        .configuration
        .environment
        .iter()
        .filter_map(|entry| entry.split_once('='))
        .collect::<BTreeMap<_, _>>();
    ensure!(
        expected_environment.iter().all(|(environment, expected)| {
            actual_environment.get(environment.as_str()).copied() == Some(expected.as_str())
        }),
        "Docker resource environment configuration does not match the requested service"
    );
    let state = match record.state.status.as_str() {
        "running" => ResourceState::Running,
        "created" | "exited" | "dead" => ResourceState::Stopped,
        _ => bail!("Docker resource has an unsupported lifecycle state"),
    };
    Ok(InspectedResource {
        name: name.into(),
        state,
    })
}

fn normalized(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    let mut separator = false;
    for character in value
        .chars()
        .map(|character| character.to_ascii_lowercase())
    {
        if character.is_ascii_alphanumeric() {
            result.push(character);
            separator = false;
        } else if !result.is_empty() && !separator {
            result.push('-');
            separator = true;
        }
    }
    while result.ends_with('-') {
        result.pop();
    }
    if result.is_empty() {
        "app".to_owned()
    } else {
        result
    }
}

fn scoped_resource_name(application: &str, workspace: &str, suffix: &str) -> String {
    const PREFIX: &str = "minco-";
    let maximum_application = 63_usize.saturating_sub(
        PREFIX.len() + application_separator().len() + workspace.len() + suffix.len(),
    );
    let mut bounded_application = application.to_owned();
    bounded_application.truncate(maximum_application);
    while bounded_application.ends_with('-') {
        bounded_application.pop();
    }
    if bounded_application.is_empty() {
        bounded_application.push_str("app");
    }
    format!(
        "{PREFIX}{bounded_application}{}{workspace}{suffix}",
        application_separator()
    )
}

const fn application_separator() -> &'static str {
    "-"
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser as _;
    use std::collections::{BTreeMap, VecDeque};
    use std::sync::Mutex;

    type RecordedCommand = (String, Vec<String>);
    type ProbeResponses = BTreeMap<RecordedCommand, VecDeque<CommandOutput>>;

    #[derive(Default)]
    struct ProbeRunner {
        responses: Mutex<ProbeResponses>,
        calls: Mutex<Vec<RecordedCommand>>,
    }

    impl ProbeRunner {
        fn with(mut self, program: &str, arguments: &[&str], output: CommandOutput) -> Self {
            self.responses
                .get_mut()
                .expect("probe responses")
                .entry((
                    program.into(),
                    arguments.iter().map(ToString::to_string).collect(),
                ))
                .or_default()
                .push_back(output);
            self
        }

        fn with_arguments(
            mut self,
            program: &str,
            arguments: Vec<String>,
            output: CommandOutput,
        ) -> Self {
            self.responses
                .get_mut()
                .expect("probe responses")
                .entry((program.into(), arguments))
                .or_default()
                .push_back(output);
            self
        }

        fn recorded_calls(&self) -> Vec<RecordedCommand> {
            self.calls.lock().expect("recorded calls").clone()
        }
    }

    impl CommandRunner for ProbeRunner {
        fn run(&self, command: &CommandRequest) -> Result<CommandOutput> {
            let key = (
                command.program.clone(),
                command
                    .arguments
                    .iter()
                    .map(|argument| argument.to_string_lossy().into_owned())
                    .collect(),
            );
            self.calls.lock().expect("probe calls").push(key.clone());
            Ok(self
                .responses
                .lock()
                .expect("probe responses")
                .get_mut(&key)
                .and_then(VecDeque::pop_front)
                .unwrap_or_else(CommandOutput::failure))
        }
    }

    fn success(value: &str) -> CommandOutput {
        CommandOutput::success(value.as_bytes().to_vec())
    }

    #[test]
    fn docker_absent_volume_diagnostic_is_classified_as_missing() {
        assert!(output_says_missing(&CommandOutput {
            successful: false,
            stdout: Vec::new(),
            stderr: b"Error response from daemon: get volume: no such volume".to_vec(),
        }));
    }

    #[test]
    fn empty_failed_inspection_is_not_treated_as_resource_absence() {
        let temporary = tempfile::tempdir().expect("temporary project");
        let compose_file = temporary.path().join("compose.yaml");
        std::fs::write(&compose_file, "services: {}\n").expect("write Compose file");
        let mut arguments = arguments(Service::Postgres);
        arguments.compose_file = compose_file;
        let spec = LocalServiceSpec::from_arguments(&arguments, &BTreeMap::new())
            .expect("canonical PostgreSQL service");
        let expected_environment =
            service_environment(&spec, &BTreeMap::new()).expect("resolved PostgreSQL environment");
        let volume = spec.volume.as_ref().expect("PostgreSQL volume");
        let empty_failure = CommandOutput {
            successful: false,
            stdout: Vec::new(),
            stderr: Vec::new(),
        };
        let runner = ProbeRunner::default()
            .with(
                "docker",
                &["inspect", &resource_name(&spec)],
                empty_failure.clone(),
            )
            .with(
                "docker",
                &["volume", "inspect", &volume.name],
                empty_failure.clone(),
            )
            .with(
                "container",
                &["volume", "inspect", &volume.name],
                empty_failure,
            );

        assert!(inspect_resource(&runner, Runtime::Docker, &spec, &expected_environment).is_err());
        assert!(inspect_docker_volume(&runner, &spec).is_err());
        assert!(inspect_apple_volume(&runner, &spec).is_err());
    }

    fn apple_inspect_bytes(
        spec: &LocalServiceSpec,
        expected_environment: &BTreeMap<String, String>,
        state: &str,
    ) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!([{
            "id": resource_name(spec),
            "configuration": {
                "image": {"reference": spec.image},
                "initProcess": {"environment": expected_environment.iter().map(|(name, value)| format!("{name}={value}")).collect::<Vec<_>>()},
                "labels": spec.ownership,
                "mounts": [],
                "platform": {"architecture": "arm64", "os": "linux"},
                "publishedPorts": [{
                    "containerPort": spec.container_port,
                    "count": 1,
                    "hostAddress": spec.bind_address,
                    "hostPort": spec.host_port,
                    "proto": "tcp"
                }]
            },
            "status": {"state": state}
        }]))
        .expect("Apple inspect JSON")
    }

    fn docker_inspect_bytes(
        spec: &LocalServiceSpec,
        expected_environment: &BTreeMap<String, String>,
        running: bool,
    ) -> Vec<u8> {
        let mounts = spec
            .volume
            .iter()
            .map(|volume| {
                serde_json::json!({
                    "Type": "volume",
                    "Name": volume.name,
                    "Destination": volume.container_path
                })
            })
            .collect::<Vec<_>>();
        serde_json::to_vec(&serde_json::json!([{
            "Name": format!("/{}", resource_name(spec)),
            "State": {"Status": if running { "running" } else { "exited" }, "Running": running},
            "Config": {
                "Image": spec.image,
                "Env": expected_environment.iter().map(|(name, value)| format!("{name}={value}")).collect::<Vec<_>>(),
                "Labels": spec.ownership
            },
            "NetworkSettings": {"Ports": {format!("{}/tcp", spec.container_port): [{
                "HostIp": spec.bind_address,
                "HostPort": spec.host_port.to_string()
            }]}},
            "Mounts": mounts
        }]))
        .expect("Docker inspect JSON")
    }

    fn arguments(service: Service) -> ServiceArgs {
        ServiceArgs {
            service,
            application: "Orders API".to_owned(),
            compose_file: PathBuf::from("infra/local/compose.yaml"),
            port: match service {
                Service::Postgres => 55_432,
                Service::Rustack => 4_566,
            },
            aws_services: vec!["s3".to_owned(), "ssm".to_owned()],
            runtime: RuntimePreference::Auto,
        }
    }

    fn strings(arguments: Vec<OsString>) -> Vec<String> {
        arguments
            .into_iter()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn typed_postgres_spec_owns_identity_without_serializing_secret_values() {
        let temporary = tempfile::tempdir().expect("temporary project");
        let compose_file = temporary.path().join("infra/local/compose.yaml");
        std::fs::create_dir_all(compose_file.parent().expect("compose parent"))
            .expect("create compose parent");
        std::fs::write(&compose_file, "services: {}\n").expect("write Compose file");
        let mut arguments = arguments(Service::Postgres);
        arguments.application = "Orders Café 🚀".into();
        arguments.compose_file = compose_file;
        let environment = BTreeMap::from([(
            "MINCO_POSTGRES_PASSWORD".into(),
            "do-not-serialize-this-password".into(),
        )]);

        let spec = LocalServiceSpec::from_arguments(&arguments, &environment)
            .expect("canonical PostgreSQL service");
        let serialized = serde_json::to_string(&spec).expect("serialize canonical spec");

        assert_eq!(spec.bind_address, "127.0.0.1");
        assert_eq!(spec.container_port, 5_432);
        assert_eq!(spec.host_port, 55_432);
        assert_eq!(spec.secret_environment, ["POSTGRES_PASSWORD"]);
        assert_eq!(spec.ownership["dev.minco.managed"], "true");
        assert_eq!(spec.ownership["dev.minco.schema"], "1");
        assert_eq!(spec.ownership["dev.minco.service"], "postgres");
        assert_eq!(spec.ownership["dev.minco.application"], "orders-caf");
        assert_eq!(spec.ownership["dev.minco.workspace"].len(), 16);
        assert_eq!(spec.ownership["dev.minco.configuration"].len(), 64);
        assert!(!serialized.contains("do-not-serialize-this-password"));
        assert!(!spec.configuration_digest().contains("do-not-serialize"));
    }

    #[test]
    fn image_overrides_must_remain_immutable_sha256_references() {
        let temporary = tempfile::tempdir().expect("temporary project");
        let compose_file = temporary.path().join("compose.yaml");
        std::fs::write(&compose_file, "services: {}\n").expect("write Compose file");
        let mut arguments = arguments(Service::Rustack);
        arguments.compose_file = compose_file;
        let mutable = BTreeMap::from([(
            "MINCO_RUSTACK_IMAGE".into(),
            "ghcr.io/tyrchen/rustack:latest".into(),
        )]);

        let error = LocalServiceSpec::from_arguments(&arguments, &mutable)
            .expect_err("mutable image override must fail closed");

        assert!(error.to_string().contains("immutable sha256 digest"));
        assert!(!error.to_string().contains("latest"));
    }

    #[test]
    fn long_application_names_preserve_the_workspace_identity_in_every_resource_name() {
        let first = tempfile::tempdir().expect("first project");
        let second = tempfile::tempdir().expect("second project");
        let first_compose = first.path().join("compose.yaml");
        let second_compose = second.path().join("compose.yaml");
        std::fs::write(&first_compose, "services: {}\n").expect("first Compose file");
        std::fs::write(&second_compose, "services: {}\n").expect("second Compose file");
        let application =
            "A very long Café application name repeated repeatedly repeatedly repeatedly 🚀";
        let mut first_arguments = arguments(Service::Postgres);
        first_arguments.application = application.into();
        first_arguments.compose_file = first_compose.clone();
        let mut second_arguments = first_arguments.clone();
        second_arguments.compose_file = second_compose;
        let first = LocalServiceSpec::from_arguments(&first_arguments, &BTreeMap::new())
            .expect("first canonical service");
        let second = LocalServiceSpec::from_arguments(&second_arguments, &BTreeMap::new())
            .expect("second canonical service");

        for (name, spec) in [
            (resource_name(&first), &first),
            (compose_project_name(&first), &first),
            (
                first
                    .volume
                    .as_ref()
                    .expect("PostgreSQL volume")
                    .name
                    .clone(),
                &first,
            ),
        ] {
            assert!(name.len() <= 63);
            assert!(name.contains(&spec.ownership[OWNERSHIP_WORKSPACE]));
        }
        assert_ne!(resource_name(&first), resource_name(&second));
        assert_ne!(
            first.volume.as_ref().expect("first volume").name,
            second.volume.as_ref().expect("second volume").name
        );

        #[cfg(unix)]
        {
            let alias = first_compose.with_file_name("compose-alias.yaml");
            std::os::unix::fs::symlink(&first_compose, &alias).expect("Compose symlink alias");
            let mut alias_arguments = first_arguments;
            alias_arguments.compose_file = alias;
            let alias = LocalServiceSpec::from_arguments(&alias_arguments, &BTreeMap::new())
                .expect("symlink alias service");
            assert_eq!(
                alias.ownership[OWNERSHIP_WORKSPACE],
                first.ownership[OWNERSHIP_WORKSPACE]
            );
            assert_eq!(resource_name(&alias), resource_name(&first));
        }
    }

    #[test]
    fn apple_command_carries_exact_ownership_without_secret_values_or_force() {
        let temporary = tempfile::tempdir().expect("temporary project");
        let compose_file = temporary.path().join("compose.yaml");
        std::fs::write(&compose_file, "services: {}\n").expect("write Compose file");
        let mut arguments = arguments(Service::Postgres);
        arguments.compose_file = compose_file;
        let source_environment = BTreeMap::from([(
            "MINCO_POSTGRES_PASSWORD".into(),
            "do-not-place-in-argv".into(),
        )]);
        let spec = LocalServiceSpec::from_arguments(&arguments, &source_environment)
            .expect("canonical PostgreSQL service");

        let command = strings(apple_run_arguments(&spec));
        let child_environment =
            service_environment(&spec, &source_environment).expect("resolved service environment");

        for (name, value) in &spec.ownership {
            assert!(
                command
                    .windows(2)
                    .any(|pair| { pair == ["--label", &format!("{name}={value}")] })
            );
        }
        assert!(
            command
                .windows(2)
                .any(|pair| { pair == ["--publish", "127.0.0.1:55432:5432"] })
        );
        assert!(
            command
                .windows(2)
                .any(|pair| { pair == ["--platform", "linux/arm64"] })
        );
        assert!(!command.iter().any(|argument| argument == "--force"));
        assert!(
            !command
                .iter()
                .any(|argument| argument == "do-not-place-in-argv")
        );
        assert_eq!(
            child_environment["POSTGRES_PASSWORD"],
            "do-not-place-in-argv"
        );
    }

    #[test]
    fn apple_inspect_requires_exact_owned_configuration_and_redacts_mismatches() {
        let temporary = tempfile::tempdir().expect("temporary project");
        let compose_file = temporary.path().join("compose.yaml");
        std::fs::write(&compose_file, "services: {}\n").expect("write Compose file");
        let mut arguments = arguments(Service::Postgres);
        arguments.compose_file = compose_file;
        let source_environment = BTreeMap::from([(
            "MINCO_POSTGRES_PASSWORD".into(),
            "expected-secret-value".into(),
        )]);
        let spec = LocalServiceSpec::from_arguments(&arguments, &source_environment)
            .expect("canonical PostgreSQL service");
        let expected_environment =
            service_environment(&spec, &source_environment).expect("resolved service environment");
        let inspected_environment = expected_environment
            .iter()
            .map(|(name, value)| format!("{name}={value}"))
            .collect::<Vec<_>>();
        let exact = serde_json::json!([{
            "id": resource_name(&spec),
            "configuration": {
                "id": resource_name(&spec),
                "image": {"reference": spec.image, "descriptor": {"digest": "sha256:index", "mediaType": "application/vnd.oci.image.index.v1+json", "size": 1}},
                "initProcess": {"environment": inspected_environment},
                "labels": spec.ownership,
                "mounts": [{
                    "destination": "/var/lib/postgresql",
                    "type": {"volume": {"name": spec.volume.as_ref().expect("volume").name}}
                }],
                "platform": {"architecture": "arm64", "os": "linux"},
                "publishedPorts": [{"containerPort": 5432, "count": 1, "hostAddress": "127.0.0.1", "hostPort": 55432, "proto": "tcp"}]
            },
            "status": {"state": "running"}
        }]);

        let inspected = verify_apple_inspect(
            &serde_json::to_vec(&exact).expect("inspect JSON"),
            &spec,
            &expected_environment,
        )
        .expect("exact owned Apple resource");
        assert_eq!(inspected.state, ResourceState::Running);

        let mut canonical_digest = exact.clone();
        let (tagged_repository, digest) = spec.image.rsplit_once('@').expect("pinned image");
        let repository = tagged_repository
            .rsplit_once(':')
            .map_or(tagged_repository, |(repository, _)| repository);
        canonical_digest[0]["configuration"]["image"] = serde_json::json!({
            "reference": format!("{repository}@{digest}"),
            "descriptor": {"digest": digest}
        });
        verify_apple_inspect(
            &serde_json::to_vec(&canonical_digest).expect("canonical digest JSON"),
            &spec,
            &expected_environment,
        )
        .expect("Apple canonical digest reference");

        let mut foreign = exact.clone();
        foreign[0]["configuration"]["labels"] = serde_json::json!({});
        let error = verify_apple_inspect(
            &serde_json::to_vec(&foreign).expect("foreign JSON"),
            &spec,
            &expected_environment,
        )
        .expect_err("missing ownership must fail");
        assert!(error.to_string().contains("ownership"));

        let mut wrong_secret = exact;
        wrong_secret[0]["configuration"]["initProcess"]["environment"] =
            serde_json::json!(["POSTGRES_PASSWORD=actual-secret-value"]);
        let error = verify_apple_inspect(
            &serde_json::to_vec(&wrong_secret).expect("mismatch JSON"),
            &spec,
            &expected_environment,
        )
        .expect_err("environment mismatch must fail");
        assert!(error.to_string().contains("configuration"));
        assert!(!error.to_string().contains("expected-secret-value"));
        assert!(!error.to_string().contains("actual-secret-value"));

        assert!(verify_apple_inspect(b"not-json", &spec, &expected_environment).is_err());
    }

    #[test]
    fn runtime_selection_is_deterministic_and_versions_fail_closed() {
        let docker = ProbeRunner::default()
            .with("docker", &["--version"], success("Docker version 29.7.1"))
            .with(
                "docker",
                &["compose", "version", "--short"],
                success("5.3.1\n"),
            )
            .with("docker", &["info"], success("ready"))
            .with(
                "container",
                &["--version"],
                success("container CLI version 1.2.0 (build: release, commit: exact)"),
            )
            .with("container", &["system", "status"], success("running"));
        assert_eq!(
            resolve_runtime_with(&docker, RuntimePreference::Auto).expect("Docker auto"),
            Runtime::Docker
        );

        let explicit_docker = ProbeRunner::default()
            .with("docker", &["--version"], success("Docker version 29.7.1"))
            .with(
                "docker",
                &["compose", "version", "--short"],
                success("5.3.1\n"),
            )
            .with("docker", &["info"], success("ready"));
        assert_eq!(
            resolve_runtime_with(&explicit_docker, RuntimePreference::Docker)
                .expect("explicit Docker"),
            Runtime::Docker
        );

        let apple = ProbeRunner::default()
            .with(
                "container",
                &["--version"],
                success("container CLI version 1.2.0 (build: release, commit: unspeci)"),
            )
            .with("container", &["system", "status"], success("running"));
        assert_eq!(
            resolve_runtime_with(&apple, RuntimePreference::Auto).expect("Apple fallback"),
            Runtime::Apple
        );
        let explicit_apple = ProbeRunner::default()
            .with(
                "container",
                &["--version"],
                success("container CLI version 1.2.0 (build: release, commit: unspeci)"),
            )
            .with("container", &["system", "status"], success("running"));
        assert_eq!(
            resolve_runtime_with(&explicit_apple, RuntimePreference::Apple)
                .expect("explicit Apple"),
            Runtime::Apple
        );

        let stopped_docker_with_apple = ProbeRunner::default()
            .with("docker", &["--version"], success("Docker version 29.7.1"))
            .with(
                "docker",
                &["compose", "version", "--short"],
                success("5.3.1\n"),
            )
            .with(
                "container",
                &["--version"],
                success("container CLI version 1.2.0 (build: release, commit: exact)"),
            )
            .with("container", &["system", "status"], success("running"));
        assert_eq!(
            resolve_runtime_with(&stopped_docker_with_apple, RuntimePreference::Auto)
                .expect("Apple fallback when Docker daemon is stopped"),
            Runtime::Apple
        );

        let stopped_docker = ProbeRunner::default()
            .with("docker", &["--version"], success("Docker version 29.7.1"))
            .with(
                "docker",
                &["compose", "version", "--short"],
                success("5.3.1\n"),
            );
        let error = resolve_runtime_with(&stopped_docker, RuntimePreference::Auto)
            .expect_err("stopped Docker must fail");
        assert!(error.to_string().contains("installed but not ready"));

        let missing = ProbeRunner::default();
        assert!(
            resolve_runtime_with(&missing, RuntimePreference::Auto)
                .expect_err("no runtime")
                .to_string()
                .contains("no supported container runtime")
        );

        let future_apple = ProbeRunner::default()
            .with(
                "container",
                &["--version"],
                success("container CLI version 2.0.0 (build: release, commit: exact)"),
            )
            .with("container", &["system", "status"], success("running"));
        let error = resolve_runtime_with(&future_apple, RuntimePreference::Apple)
            .expect_err("unqualified Apple major must fail");
        assert!(error.to_string().contains("1.2.x"));
    }

    #[test]
    fn auto_start_uses_the_ready_runtime_when_the_other_system_is_stopped() {
        let temporary = tempfile::tempdir().expect("temporary project");
        let compose_file = temporary.path().join("compose.yaml");
        std::fs::write(&compose_file, "services: {}\n").expect("write Compose file");
        let mut arguments = arguments(Service::Rustack);
        arguments.compose_file = compose_file;
        arguments.runtime = RuntimePreference::Auto;
        let source_environment = BTreeMap::new();
        let spec = LocalServiceSpec::from_arguments(&arguments, &source_environment)
            .expect("canonical Rustack service");
        let expected_environment =
            service_environment(&spec, &source_environment).expect("resolved environment");
        let resource = resource_name(&spec);

        let docker_only = ProbeRunner::default()
            .with("docker", &["--version"], success("Docker version 29.7.1"))
            .with(
                "docker",
                &["compose", "version", "--short"],
                success("5.3.1\n"),
            )
            .with("docker", &["info"], success("ready"))
            .with(
                "container",
                &["--version"],
                success("container CLI version 1.2.0 (build: release, commit: exact)"),
            )
            .with("docker", &["inspect", &resource], CommandOutput::failure());
        assert_eq!(
            select_start_runtime(&docker_only, &arguments, None, &spec, &expected_environment,)
                .expect("Docker start while Apple system is stopped"),
            Runtime::Docker
        );

        let apple_only = ProbeRunner::default()
            .with("docker", &["--version"], success("Docker version 29.7.1"))
            .with(
                "docker",
                &["compose", "version", "--short"],
                success("5.3.1\n"),
            )
            .with(
                "container",
                &["--version"],
                success("container CLI version 1.2.0 (build: release, commit: exact)"),
            )
            .with("container", &["system", "status"], success("running"))
            .with(
                "container",
                &["inspect", &resource],
                CommandOutput::failure(),
            );
        assert_eq!(
            select_start_runtime(&apple_only, &arguments, None, &spec, &expected_environment,)
                .expect("Apple fallback while Docker daemon is stopped"),
            Runtime::Apple
        );
    }

    #[test]
    fn receipt_is_atomic_secret_free_and_guarded_by_a_process_lock() {
        let temporary = tempfile::tempdir().expect("temporary project");
        let compose_file = temporary.path().join("compose.yaml");
        std::fs::write(&compose_file, "services: {}\n").expect("write Compose file");
        let mut arguments = arguments(Service::Postgres);
        arguments.compose_file = compose_file;
        let source_environment = BTreeMap::from([(
            "MINCO_POSTGRES_PASSWORD".into(),
            "receipt-must-not-contain-this".into(),
        )]);
        let spec = LocalServiceSpec::from_arguments(&arguments, &source_environment)
            .expect("canonical PostgreSQL service");
        let directory = temporary.path().join("target/minco/dev");
        let first_lock = ServiceLock::acquire(&directory, &spec).expect("first service lock");
        let error = ServiceLock::acquire(&directory, &spec).expect_err("concurrent lock must fail");
        assert!(error.to_string().contains("already active"));

        let receipt = LifecycleReceipt::for_spec(Runtime::Apple, &spec);
        write_receipt_atomic(&directory, &receipt).expect("write lifecycle receipt");
        let rendered = std::fs::read(receipt_path(&directory, &spec)).expect("receipt bytes");
        assert!(!String::from_utf8_lossy(&rendered).contains("receipt-must-not-contain-this"));
        assert_eq!(
            read_receipt(&directory, &spec)
                .expect("read receipt")
                .expect("receipt exists"),
            receipt
        );

        let changed_environment =
            BTreeMap::from([("MINCO_POSTGRES_DB".into(), "different_database".into())]);
        let changed = LocalServiceSpec::from_arguments(&arguments, &changed_environment)
            .expect("changed service configuration");
        let error = read_receipt(&directory, &changed)
            .expect_err("stale receipt must not authorize changed configuration");
        assert!(error.to_string().contains("disagrees"));

        drop(first_lock);
        ServiceLock::acquire(&directory, &spec).expect("lock released after drop");
        std::fs::write(receipt_path(&directory, &spec), b"{broken")
            .expect("corrupt receipt fixture");
        let error = read_receipt(&directory, &spec).expect_err("corrupt receipt must fail");
        assert!(error.to_string().contains("corrupt"));
    }

    #[test]
    fn receipt_and_lock_identity_include_application_within_one_workspace() {
        let temporary = tempfile::tempdir().expect("temporary project");
        let compose_file = temporary.path().join("compose.yaml");
        std::fs::write(&compose_file, "services: {}\n").expect("write Compose file");
        let mut first_arguments = arguments(Service::Postgres);
        first_arguments.compose_file = compose_file;
        first_arguments.application = "first-app".into();
        let mut second_arguments = first_arguments.clone();
        second_arguments.application = "second-app".into();
        let first = LocalServiceSpec::from_arguments(&first_arguments, &BTreeMap::new())
            .expect("first app spec");
        let second = LocalServiceSpec::from_arguments(&second_arguments, &BTreeMap::new())
            .expect("second app spec");
        let base = state_base(temporary.path());

        assert_ne!(receipt_path(&base, &first), receipt_path(&base, &second));
        let _first_lock = ServiceLock::acquire(&base, &first).expect("first app lock");
        let _second_lock = ServiceLock::acquire(&base, &second).expect("second app lock");
    }

    #[test]
    fn docker_inspect_requires_exact_owned_loopback_configuration() {
        let temporary = tempfile::tempdir().expect("temporary project");
        let compose_file = temporary.path().join("compose.yaml");
        std::fs::write(&compose_file, "services: {}\n").expect("write Compose file");
        let mut arguments = arguments(Service::Rustack);
        arguments.compose_file = compose_file;
        let source_environment = BTreeMap::new();
        let spec = LocalServiceSpec::from_arguments(&arguments, &source_environment)
            .expect("canonical Rustack service");
        let expected_environment =
            service_environment(&spec, &source_environment).expect("resolved environment");
        let exact = serde_json::json!([{
            "Name": format!("/{}", resource_name(&spec)),
            "State": {"Status": "running", "Running": true},
            "Config": {
                "Image": spec.image,
                "Env": expected_environment.iter().map(|(name, value)| format!("{name}={value}")).collect::<Vec<_>>(),
                "Labels": spec.ownership
            },
            "NetworkSettings": {"Ports": {"4566/tcp": [{"HostIp": "127.0.0.1", "HostPort": "4566"}]}},
            "Mounts": []
        }]);

        let inspected = verify_docker_inspect(
            &serde_json::to_vec(&exact).expect("Docker inspect JSON"),
            &spec,
            &expected_environment,
        )
        .expect("exact owned Docker resource");
        assert_eq!(inspected.name, resource_name(&spec));
        assert_eq!(inspected.state, ResourceState::Running);

        let stopped = serde_json::json!([{
            "Name": format!("/{}", resource_name(&spec)),
            "State": {"Status": "exited", "Running": false},
            "Config": {
                "Image": spec.image,
                "Env": expected_environment.iter().map(|(name, value)| format!("{name}={value}")).collect::<Vec<_>>(),
                "Labels": spec.ownership
            },
            "HostConfig": {"PortBindings": {"4566/tcp": [{"HostIp": "127.0.0.1", "HostPort": "4566"}]}},
            "NetworkSettings": {"Ports": {}},
            "Mounts": []
        }]);
        let stopped = verify_docker_inspect(
            &serde_json::to_vec(&stopped).expect("stopped Docker inspect JSON"),
            &spec,
            &expected_environment,
        )
        .expect("stopped exact owned Docker resource");
        assert_eq!(stopped.state, ResourceState::Stopped);

        let mut unexpected_mount = exact.clone();
        unexpected_mount[0]["Mounts"] = serde_json::json!([{
            "Type": "bind",
            "Name": null,
            "Destination": "/foreign"
        }]);
        assert!(
            verify_docker_inspect(
                &serde_json::to_vec(&unexpected_mount).expect("unexpected mount JSON"),
                &spec,
                &expected_environment,
            )
            .is_err()
        );

        let mut foreign = exact;
        foreign[0]["Config"]["Labels"][OWNERSHIP_WORKSPACE] =
            serde_json::json!("different-workspace");
        let error = verify_docker_inspect(
            &serde_json::to_vec(&foreign).expect("foreign JSON"),
            &spec,
            &expected_environment,
        )
        .expect_err("foreign workspace must fail");
        assert!(error.to_string().contains("ownership"));
        assert!(verify_docker_inspect(b"[]", &spec, &expected_environment).is_err());
    }

    #[tokio::test]
    async fn foreign_same_named_apple_container_is_rejected_without_mutation() {
        let temporary = tempfile::tempdir().expect("temporary project");
        let compose_file = temporary.path().join("compose.yaml");
        std::fs::write(&compose_file, "services: {}\n").expect("write Compose file");
        let mut arguments = arguments(Service::Rustack);
        arguments.compose_file = compose_file;
        arguments.runtime = RuntimePreference::Apple;
        let source_environment = BTreeMap::new();
        let spec = LocalServiceSpec::from_arguments(&arguments, &source_environment)
            .expect("canonical Rustack service");
        let expected_environment =
            service_environment(&spec, &source_environment).expect("resolved environment");
        let foreign = serde_json::json!([{
            "id": resource_name(&spec),
            "configuration": {
                "image": {"reference": spec.image},
                "initProcess": {"environment": expected_environment.iter().map(|(name, value)| format!("{name}={value}")).collect::<Vec<_>>()},
                "labels": {},
                "mounts": [],
                "platform": {"architecture": "arm64", "os": "linux"},
                "publishedPorts": [{"containerPort": 4566, "count": 1, "hostAddress": "127.0.0.1", "hostPort": 4566, "proto": "tcp"}]
            },
            "status": {"state": "running"}
        }]);
        let runner = ProbeRunner::default()
            .with(
                "container",
                &["--version"],
                success("container CLI version 1.2.0 (build: release, commit: exact)"),
            )
            .with("container", &["system", "status"], success("running"))
            .with(
                "container",
                &["inspect", &resource_name(&spec)],
                CommandOutput::success(serde_json::to_vec(&foreign).expect("foreign inspect")),
            );

        let error = start_with_runner(&runner, &arguments, &source_environment, temporary.path())
            .await
            .expect_err("foreign same-named container must fail");

        assert!(error.to_string().contains("ownership"));
        let calls = runner.recorded_calls();
        assert!(!calls.iter().any(|(program, arguments)| {
            program == "container"
                && arguments
                    .first()
                    .is_some_and(|action| matches!(action.as_str(), "run" | "stop" | "delete"))
        }));
    }

    #[test]
    fn apple_persistent_volume_requires_exact_minco_ownership() {
        let temporary = tempfile::tempdir().expect("temporary project");
        let compose_file = temporary.path().join("compose.yaml");
        std::fs::write(&compose_file, "services: {}\n").expect("write Compose file");
        let mut arguments = arguments(Service::Postgres);
        arguments.compose_file = compose_file;
        let spec = LocalServiceSpec::from_arguments(&arguments, &BTreeMap::new())
            .expect("canonical PostgreSQL service");
        let volume = spec.volume.as_ref().expect("PostgreSQL volume");
        let exact = serde_json::json!([{
            "id": volume.name,
            "configuration": {
                "name": volume.name,
                "labels": spec.ownership,
                "driver": "local",
                "format": "ext4"
            }
        }]);
        verify_apple_volume_inspect(&serde_json::to_vec(&exact).expect("volume JSON"), &spec)
            .expect("exact owned volume");

        let mut foreign = exact;
        foreign[0]["configuration"]["labels"] = serde_json::json!({});
        assert!(
            verify_apple_volume_inspect(
                &serde_json::to_vec(&foreign).expect("foreign volume JSON"),
                &spec,
            )
            .expect_err("foreign volume must fail")
            .to_string()
            .contains("ownership")
        );
    }

    struct AlwaysReady;

    impl ReadinessChecker for AlwaysReady {
        async fn wait_until_ready(
            &self,
            _spec: &LocalServiceSpec,
            _environment: &BTreeMap<String, String>,
        ) -> Result<()> {
            Ok(())
        }
    }

    struct FailsReadiness(String);

    impl ReadinessChecker for FailsReadiness {
        async fn wait_until_ready(
            &self,
            _spec: &LocalServiceSpec,
            _environment: &BTreeMap<String, String>,
        ) -> Result<()> {
            bail!(self.0.clone())
        }
    }

    #[tokio::test]
    async fn apple_start_creates_and_verifies_only_owned_resources_then_writes_receipt() {
        let temporary = tempfile::tempdir().expect("temporary project");
        let compose_file = temporary.path().join("compose.yaml");
        std::fs::write(&compose_file, "services: {}\n").expect("write Compose file");
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("free port");
        let port = listener.local_addr().expect("free address").port();
        drop(listener);
        let mut arguments = arguments(Service::Postgres);
        arguments.compose_file = compose_file;
        arguments.port = port;
        arguments.runtime = RuntimePreference::Apple;
        let source_environment = BTreeMap::new();
        let spec = LocalServiceSpec::from_arguments(&arguments, &source_environment)
            .expect("canonical PostgreSQL service");
        let expected_environment =
            service_environment(&spec, &source_environment).expect("resolved environment");
        let volume = spec.volume.as_ref().expect("PostgreSQL volume");
        let volume_inspect = serde_json::to_vec(&serde_json::json!([{
            "id": volume.name,
            "configuration": {"name": volume.name, "labels": spec.ownership, "driver": "local", "format": "ext4"}
        }]))
        .expect("volume inspect");
        let container_inspect = serde_json::to_vec(&serde_json::json!([{
            "id": resource_name(&spec),
            "configuration": {
                "image": {"reference": spec.image},
                "initProcess": {"environment": expected_environment.iter().map(|(name, value)| format!("{name}={value}")).collect::<Vec<_>>()},
                "labels": spec.ownership,
                "mounts": [{"destination": volume.container_path, "type": {"volume": {"name": volume.name}}}],
                "platform": {"architecture": "arm64", "os": "linux"},
                "publishedPorts": [{"containerPort": 5432, "count": 1, "hostAddress": "127.0.0.1", "hostPort": port, "proto": "tcp"}]
            },
            "status": {"state": "running"}
        }]))
        .expect("container inspect");
        let volume_name = volume.name.clone();
        let resource = resource_name(&spec);
        let runner = ProbeRunner::default()
            .with(
                "container",
                &["--version"],
                success("container CLI version 1.2.0 (build: release, commit: exact)"),
            )
            .with("container", &["system", "status"], success("running"))
            .with(
                "container",
                &["inspect", &resource],
                CommandOutput::failure(),
            )
            .with(
                "container",
                &["volume", "inspect", &volume_name],
                CommandOutput::failure(),
            )
            .with_arguments(
                "container",
                apple_volume_create_arguments(&spec)
                    .into_iter()
                    .map(|value| value.to_string_lossy().into_owned())
                    .collect(),
                success(&volume_name),
            )
            .with(
                "container",
                &["volume", "inspect", &volume_name],
                CommandOutput::success(volume_inspect),
            )
            .with_arguments(
                "container",
                apple_run_arguments(&spec)
                    .into_iter()
                    .map(|value| value.to_string_lossy().into_owned())
                    .collect(),
                success(&resource),
            )
            .with(
                "container",
                &["inspect", &resource],
                CommandOutput::success(container_inspect),
            );

        start_with_dependencies(
            &runner,
            &AlwaysReady,
            &arguments,
            &source_environment,
            temporary.path(),
        )
        .await
        .expect("owned Apple start");

        let receipt = read_receipt(&state_base(temporary.path()), &spec)
            .expect("read receipt")
            .expect("receipt exists");
        assert_eq!(receipt.runtime, Runtime::Apple);
        assert_eq!(receipt.resource, resource);
        let calls = runner.recorded_calls();
        assert!(
            !calls
                .iter()
                .any(|(_, arguments)| { arguments.iter().any(|argument| argument == "--force") })
        );
    }

    #[test]
    fn auto_stop_uses_only_the_receipted_runtime_and_removes_stale_receipt() {
        let temporary = tempfile::tempdir().expect("temporary project");
        let compose_file = temporary.path().join("compose.yaml");
        std::fs::write(&compose_file, "services: {}\n").expect("write Compose file");
        let mut arguments = arguments(Service::Rustack);
        arguments.compose_file = compose_file;
        arguments.runtime = RuntimePreference::Auto;
        let source_environment = BTreeMap::new();
        let spec = LocalServiceSpec::from_arguments(&arguments, &source_environment)
            .expect("canonical Rustack service");
        let expected_environment =
            service_environment(&spec, &source_environment).expect("resolved environment");
        let resource = resource_name(&spec);
        let inspect = serde_json::to_vec(&serde_json::json!([{
            "id": resource,
            "configuration": {
                "image": {"reference": spec.image},
                "initProcess": {"environment": expected_environment.iter().map(|(name, value)| format!("{name}={value}")).collect::<Vec<_>>()},
                "labels": spec.ownership,
                "mounts": [],
                "platform": {"architecture": "arm64", "os": "linux"},
                "publishedPorts": [{"containerPort": 4566, "count": 1, "hostAddress": "127.0.0.1", "hostPort": 4566, "proto": "tcp"}]
            },
            "status": {"state": "running"}
        }]))
        .expect("container inspect");
        let base = state_base(temporary.path());
        write_receipt_atomic(&base, &LifecycleReceipt::for_spec(Runtime::Apple, &spec))
            .expect("write Apple receipt");
        let runner = ProbeRunner::default()
            .with(
                "container",
                &["--version"],
                success("container CLI version 1.2.0 (build: release, commit: exact)"),
            )
            .with("container", &["system", "status"], success("running"))
            .with(
                "container",
                &["inspect", &resource],
                CommandOutput::success(inspect),
            )
            .with(
                "container",
                &["stop", "--time", "10", &resource],
                success(&resource),
            )
            .with(
                "container",
                &["inspect", &resource],
                CommandOutput::failure(),
            );

        stop_with_runner(&runner, &arguments, &source_environment, temporary.path())
            .expect("receipt-bound stop");

        assert!(!receipt_path(&base, &spec).exists());
        let calls = runner.recorded_calls();
        assert!(!calls.iter().any(|(program, arguments)| {
            program == "docker"
                && arguments
                    .iter()
                    .any(|argument| matches!(argument.as_str(), "stop" | "down"))
        }));
    }

    #[test]
    fn stop_reports_the_exact_residual_when_the_receipted_runtime_is_unavailable() {
        let temporary = tempfile::tempdir().expect("temporary project");
        let compose_file = temporary.path().join("compose.yaml");
        std::fs::write(&compose_file, "services: {}\n").expect("write Compose file");
        let mut arguments = arguments(Service::Rustack);
        arguments.compose_file = compose_file;
        let spec = LocalServiceSpec::from_arguments(&arguments, &BTreeMap::new())
            .expect("canonical Rustack service");
        let base = state_base(temporary.path());
        write_receipt_atomic(&base, &LifecycleReceipt::for_spec(Runtime::Apple, &spec))
            .expect("write Apple receipt");
        let runner = ProbeRunner::default().with(
            "container",
            &["--version"],
            success("container CLI version 1.2.0 (build: release, commit: exact)"),
        );

        let error = stop_with_runner(&runner, &arguments, &BTreeMap::new(), temporary.path())
            .expect_err("unavailable receipted runtime must fail");

        assert!(error.to_string().contains(&resource_name(&spec)));
        assert!(error.to_string().contains("Apple Container"));
        assert!(receipt_path(&base, &spec).exists());
        let calls = runner.recorded_calls();
        assert!(!calls.iter().any(|(_, arguments)| {
            arguments
                .iter()
                .any(|argument| matches!(argument.as_str(), "stop" | "delete" | "down"))
        }));
    }

    #[test]
    fn receiptless_auto_stop_discovers_one_exact_owned_runtime() {
        let temporary = tempfile::tempdir().expect("temporary project");
        let compose_file = temporary.path().join("compose.yaml");
        std::fs::write(&compose_file, "services: {}\n").expect("write Compose file");
        let mut arguments = arguments(Service::Rustack);
        arguments.compose_file = compose_file;
        let source_environment = BTreeMap::new();
        let spec = LocalServiceSpec::from_arguments(&arguments, &source_environment)
            .expect("canonical Rustack service");
        let expected_environment =
            service_environment(&spec, &source_environment).expect("resolved environment");
        let resource = resource_name(&spec);
        let runner = ProbeRunner::default()
            .with("docker", &["--version"], success("Docker version 29.7.1"))
            .with(
                "docker",
                &["compose", "version", "--short"],
                success("5.3.1\n"),
            )
            .with("docker", &["info"], success("ready"))
            .with(
                "container",
                &["--version"],
                success("container CLI version 1.2.0 (build: release, commit: exact)"),
            )
            .with("container", &["system", "status"], success("running"))
            .with("docker", &["inspect", &resource], CommandOutput::failure())
            .with(
                "container",
                &["inspect", &resource],
                CommandOutput::success(apple_inspect_bytes(
                    &spec,
                    &expected_environment,
                    "running",
                )),
            )
            .with(
                "container",
                &["inspect", &resource],
                CommandOutput::success(apple_inspect_bytes(
                    &spec,
                    &expected_environment,
                    "running",
                )),
            )
            .with(
                "container",
                &["stop", "--time", "10", &resource],
                success(&resource),
            )
            .with(
                "container",
                &["inspect", &resource],
                CommandOutput::failure(),
            );

        stop_with_runner(&runner, &arguments, &source_environment, temporary.path())
            .expect("ownership-discovered stop");

        let calls = runner.recorded_calls();
        assert!(calls.iter().any(|(program, arguments)| {
            program == "container" && arguments.first().is_some_and(|argument| argument == "stop")
        }));
        assert!(!calls.iter().any(|(program, arguments)| {
            program == "docker"
                && arguments
                    .iter()
                    .any(|argument| matches!(argument.as_str(), "stop" | "down"))
        }));
    }

    #[test]
    fn receiptless_auto_stop_rejects_ambiguous_owned_resources_without_mutation() {
        let temporary = tempfile::tempdir().expect("temporary project");
        let compose_file = temporary.path().join("compose.yaml");
        std::fs::write(&compose_file, "services: {}\n").expect("write Compose file");
        let mut arguments = arguments(Service::Rustack);
        arguments.compose_file = compose_file;
        let source_environment = BTreeMap::new();
        let spec = LocalServiceSpec::from_arguments(&arguments, &source_environment)
            .expect("canonical Rustack service");
        let expected_environment =
            service_environment(&spec, &source_environment).expect("resolved environment");
        let resource = resource_name(&spec);
        let runner = ProbeRunner::default()
            .with("docker", &["--version"], success("Docker version 29.7.1"))
            .with(
                "docker",
                &["compose", "version", "--short"],
                success("5.3.1\n"),
            )
            .with("docker", &["info"], success("ready"))
            .with(
                "container",
                &["--version"],
                success("container CLI version 1.2.0 (build: release, commit: exact)"),
            )
            .with("container", &["system", "status"], success("running"))
            .with(
                "docker",
                &["inspect", &resource],
                CommandOutput::success(docker_inspect_bytes(&spec, &expected_environment, true)),
            )
            .with(
                "container",
                &["inspect", &resource],
                CommandOutput::success(apple_inspect_bytes(
                    &spec,
                    &expected_environment,
                    "running",
                )),
            );

        let error = stop_with_runner(&runner, &arguments, &source_environment, temporary.path())
            .expect_err("ambiguous exact ownership must fail");

        assert!(
            error
                .to_string()
                .contains("both Docker Compose and Apple Container")
        );
        let calls = runner.recorded_calls();
        assert!(!calls.iter().any(|(_, arguments)| {
            arguments
                .iter()
                .any(|argument| matches!(argument.as_str(), "stop" | "delete" | "down"))
        }));
    }

    #[tokio::test]
    async fn auto_start_reuses_the_receipted_runtime_even_when_docker_would_win_fresh_selection() {
        let temporary = tempfile::tempdir().expect("temporary project");
        let compose_file = temporary.path().join("compose.yaml");
        std::fs::write(&compose_file, "services: {}\n").expect("write Compose file");
        let mut arguments = arguments(Service::Rustack);
        arguments.compose_file = compose_file;
        let source_environment = BTreeMap::new();
        let spec = LocalServiceSpec::from_arguments(&arguments, &source_environment)
            .expect("canonical Rustack service");
        let expected_environment =
            service_environment(&spec, &source_environment).expect("resolved environment");
        let resource = resource_name(&spec);
        write_receipt_atomic(
            &state_base(temporary.path()),
            &LifecycleReceipt::for_spec(Runtime::Apple, &spec),
        )
        .expect("write Apple receipt");
        let runner = ProbeRunner::default()
            .with(
                "container",
                &["--version"],
                success("container CLI version 1.2.0 (build: release, commit: exact)"),
            )
            .with("container", &["system", "status"], success("running"))
            .with(
                "container",
                &["inspect", &resource],
                CommandOutput::success(apple_inspect_bytes(
                    &spec,
                    &expected_environment,
                    "running",
                )),
            );

        start_with_dependencies(
            &runner,
            &AlwaysReady,
            &arguments,
            &source_environment,
            temporary.path(),
        )
        .await
        .expect("receipt-bound idempotent start");

        let calls = runner.recorded_calls();
        assert!(!calls.iter().any(|(program, _)| program == "docker"));
        assert!(!calls.iter().any(|(_, arguments)| {
            arguments
                .iter()
                .any(|argument| matches!(argument.as_str(), "run" | "start" | "delete"))
        }));
    }

    #[tokio::test]
    async fn receiptless_auto_start_rejects_owned_resources_on_both_runtimes() {
        let temporary = tempfile::tempdir().expect("temporary project");
        let compose_file = temporary.path().join("compose.yaml");
        std::fs::write(&compose_file, "services: {}\n").expect("write Compose file");
        let mut arguments = arguments(Service::Rustack);
        arguments.compose_file = compose_file;
        let source_environment = BTreeMap::new();
        let spec = LocalServiceSpec::from_arguments(&arguments, &source_environment)
            .expect("canonical Rustack service");
        let expected_environment =
            service_environment(&spec, &source_environment).expect("resolved environment");
        let resource = resource_name(&spec);
        let runner = ProbeRunner::default()
            .with("docker", &["--version"], success("Docker version 29.7.1"))
            .with(
                "docker",
                &["compose", "version", "--short"],
                success("5.3.1\n"),
            )
            .with("docker", &["info"], success("ready"))
            .with(
                "container",
                &["--version"],
                success("container CLI version 1.2.0 (build: release, commit: exact)"),
            )
            .with("container", &["system", "status"], success("running"))
            .with(
                "docker",
                &["inspect", &resource],
                CommandOutput::success(docker_inspect_bytes(&spec, &expected_environment, true)),
            )
            .with(
                "container",
                &["inspect", &resource],
                CommandOutput::success(apple_inspect_bytes(
                    &spec,
                    &expected_environment,
                    "running",
                )),
            );

        let error = start_with_dependencies(
            &runner,
            &AlwaysReady,
            &arguments,
            &source_environment,
            temporary.path(),
        )
        .await
        .expect_err("ambiguous start must fail");

        assert!(
            error
                .to_string()
                .contains("both Docker Compose and Apple Container")
        );
        let calls = runner.recorded_calls();
        assert!(!calls.iter().any(|(_, arguments)| {
            arguments.iter().any(|argument| {
                matches!(
                    argument.as_str(),
                    "run" | "start" | "stop" | "delete" | "down"
                )
            })
        }));
    }

    #[tokio::test]
    async fn docker_start_validates_compose_then_starts_the_exact_owned_service() {
        let temporary = tempfile::tempdir().expect("temporary project");
        let compose_file = temporary.path().join("compose.yaml");
        std::fs::write(&compose_file, "services: {}\n").expect("write Compose file");
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("free port");
        let port = listener.local_addr().expect("free address").port();
        drop(listener);
        let mut arguments = arguments(Service::Rustack);
        arguments.compose_file = compose_file;
        arguments.port = port;
        arguments.runtime = RuntimePreference::Docker;
        let source_environment = BTreeMap::new();
        let spec = LocalServiceSpec::from_arguments(&arguments, &source_environment)
            .expect("canonical Rustack service");
        let expected_environment =
            service_environment(&spec, &source_environment).expect("resolved environment");
        let resource = resource_name(&spec);
        let compose_config = serde_json::to_vec(&serde_json::json!({
            "services": {
                "rustack": {
                    "container_name": resource,
                    "image": spec.image,
                    "environment": expected_environment,
                    "labels": spec.ownership,
                    "ports": [{
                        "host_ip": spec.bind_address,
                        "mode": "ingress",
                        "protocol": "tcp",
                        "published": spec.host_port.to_string(),
                        "target": spec.container_port
                    }],
                    "volumes": []
                }
            },
            "volumes": {}
        }))
        .expect("Compose config JSON");
        let config_arguments =
            compose_arguments(&arguments, &spec, &["config", "--format", "json"]);
        let up_arguments = compose_arguments(
            &arguments,
            &spec,
            &["up", "--detach", "--no-deps", spec.service.label()],
        );
        let runner = ProbeRunner::default()
            .with("docker", &["--version"], success("Docker version 29.7.1"))
            .with(
                "docker",
                &["compose", "version", "--short"],
                success("5.3.1\n"),
            )
            .with("docker", &["info"], success("ready"))
            .with("docker", &["inspect", &resource], CommandOutput::failure())
            .with_arguments(
                "docker",
                strings(config_arguments),
                CommandOutput::success(compose_config),
            )
            .with_arguments("docker", strings(up_arguments), success(&resource))
            .with(
                "docker",
                &["inspect", &resource],
                CommandOutput::success(docker_inspect_bytes(&spec, &expected_environment, true)),
            );

        start_with_dependencies(
            &runner,
            &AlwaysReady,
            &arguments,
            &source_environment,
            temporary.path(),
        )
        .await
        .expect("owned Docker start");

        let receipt = read_receipt(&state_base(temporary.path()), &spec)
            .expect("read receipt")
            .expect("receipt exists");
        assert_eq!(receipt.runtime, Runtime::Docker);
        let calls = runner.recorded_calls();
        let config_index = calls
            .iter()
            .position(|(_, arguments)| arguments.iter().any(|argument| argument == "config"))
            .expect("Compose config call");
        let up_index = calls
            .iter()
            .position(|(_, arguments)| arguments.iter().any(|argument| argument == "up"))
            .expect("Compose up call");
        assert!(config_index < up_index);
        assert!(!calls.iter().any(|(_, arguments)| {
            arguments
                .iter()
                .any(|argument| matches!(argument.as_str(), "down" | "--force"))
        }));
    }

    #[tokio::test]
    async fn docker_start_rejects_a_foreign_same_named_volume_before_compose_mutation() {
        let temporary = tempfile::tempdir().expect("temporary project");
        let compose_file = temporary.path().join("compose.yaml");
        std::fs::write(&compose_file, "services: {}\n").expect("write Compose file");
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("free port");
        let port = listener.local_addr().expect("free address").port();
        drop(listener);
        let mut arguments = arguments(Service::Postgres);
        arguments.compose_file = compose_file;
        arguments.port = port;
        arguments.runtime = RuntimePreference::Docker;
        let source_environment = BTreeMap::new();
        let spec = LocalServiceSpec::from_arguments(&arguments, &source_environment)
            .expect("canonical PostgreSQL service");
        let volume = spec.volume.as_ref().expect("PostgreSQL volume");
        let foreign_volume = serde_json::to_vec(&serde_json::json!([{
            "Name": volume.name,
            "Driver": "local",
            "Labels": {}
        }]))
        .expect("foreign Docker volume inspect");
        let resource = resource_name(&spec);
        let runner = ProbeRunner::default()
            .with("docker", &["--version"], success("Docker version 29.7.1"))
            .with(
                "docker",
                &["compose", "version", "--short"],
                success("5.3.1\n"),
            )
            .with("docker", &["info"], success("ready"))
            .with("docker", &["inspect", &resource], CommandOutput::failure())
            .with(
                "docker",
                &["volume", "inspect", &volume.name],
                CommandOutput::success(foreign_volume),
            );

        let error = start_with_dependencies(
            &runner,
            &AlwaysReady,
            &arguments,
            &source_environment,
            temporary.path(),
        )
        .await
        .expect_err("foreign Docker volume must fail closed");

        assert!(error.to_string().contains("volume ownership"));
        let calls = runner.recorded_calls();
        assert!(!calls.iter().any(|(_, arguments)| {
            arguments
                .iter()
                .any(|argument| matches!(argument.as_str(), "up" | "start" | "stop" | "down"))
        }));
    }

    #[tokio::test]
    async fn occupied_foreign_port_fails_before_runtime_mutation() {
        let temporary = tempfile::tempdir().expect("temporary project");
        let compose_file = temporary.path().join("compose.yaml");
        std::fs::write(&compose_file, "services: {}\n").expect("write Compose file");
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("occupied port");
        let port = listener.local_addr().expect("occupied address").port();
        let mut arguments = arguments(Service::Rustack);
        arguments.compose_file = compose_file;
        arguments.port = port;
        arguments.runtime = RuntimePreference::Apple;
        let spec = LocalServiceSpec::from_arguments(&arguments, &BTreeMap::new())
            .expect("canonical Rustack service");
        let resource = resource_name(&spec);
        let runner = ProbeRunner::default()
            .with(
                "container",
                &["--version"],
                success("container CLI version 1.2.0 (build: release, commit: exact)"),
            )
            .with("container", &["system", "status"], success("running"))
            .with(
                "container",
                &["inspect", &resource],
                CommandOutput::failure(),
            );

        let error = start_with_dependencies(
            &runner,
            &AlwaysReady,
            &arguments,
            &BTreeMap::new(),
            temporary.path(),
        )
        .await
        .expect_err("occupied foreign port must fail");

        drop(listener);
        assert!(error.to_string().contains("foreign or unverified process"));
        let calls = runner.recorded_calls();
        assert!(!calls.iter().any(|(_, arguments)| {
            arguments
                .iter()
                .any(|argument| matches!(argument.as_str(), "run" | "start" | "stop" | "delete"))
        }));
    }

    #[tokio::test]
    async fn occupied_port_is_reused_only_after_exact_running_resource_verification() {
        let temporary = tempfile::tempdir().expect("temporary project");
        let compose_file = temporary.path().join("compose.yaml");
        std::fs::write(&compose_file, "services: {}\n").expect("write Compose file");
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("occupied port");
        let port = listener.local_addr().expect("occupied address").port();
        let mut arguments = arguments(Service::Rustack);
        arguments.compose_file = compose_file;
        arguments.port = port;
        arguments.runtime = RuntimePreference::Apple;
        let source_environment = BTreeMap::new();
        let spec = LocalServiceSpec::from_arguments(&arguments, &source_environment)
            .expect("canonical Rustack service");
        let expected_environment =
            service_environment(&spec, &source_environment).expect("resolved environment");
        let resource = resource_name(&spec);
        let runner = ProbeRunner::default()
            .with(
                "container",
                &["--version"],
                success("container CLI version 1.2.0 (build: release, commit: exact)"),
            )
            .with("container", &["system", "status"], success("running"))
            .with(
                "container",
                &["inspect", &resource],
                CommandOutput::success(apple_inspect_bytes(
                    &spec,
                    &expected_environment,
                    "running",
                )),
            );

        start_with_dependencies(
            &runner,
            &AlwaysReady,
            &arguments,
            &source_environment,
            temporary.path(),
        )
        .await
        .expect("exact running owned service may reuse its occupied mapping");

        drop(listener);
        let calls = runner.recorded_calls();
        assert!(!calls.iter().any(|(_, arguments)| {
            arguments
                .iter()
                .any(|argument| matches!(argument.as_str(), "run" | "start" | "stop" | "delete"))
        }));
    }

    #[tokio::test]
    async fn failed_docker_start_redacts_diagnostics_and_removes_only_the_created_container() {
        let temporary = tempfile::tempdir().expect("temporary project");
        let compose_file = temporary.path().join("compose.yaml");
        std::fs::write(&compose_file, "services: {}\n").expect("write Compose file");
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("free port");
        let port = listener.local_addr().expect("free address").port();
        drop(listener);
        let mut arguments = arguments(Service::Postgres);
        arguments.compose_file = compose_file;
        arguments.port = port;
        arguments.runtime = RuntimePreference::Docker;
        let secret = "must-not-escape-diagnostics";
        let source_environment =
            BTreeMap::from([("MINCO_POSTGRES_PASSWORD".into(), secret.into())]);
        let spec = LocalServiceSpec::from_arguments(&arguments, &source_environment)
            .expect("canonical PostgreSQL service");
        let expected_environment =
            service_environment(&spec, &source_environment).expect("resolved environment");
        let resource = resource_name(&spec);
        let volume = spec.volume.as_ref().expect("PostgreSQL volume");
        let volume_inspect = serde_json::to_vec(&serde_json::json!([{
            "Name": volume.name,
            "Driver": "local",
            "Labels": spec.ownership
        }]))
        .expect("Docker volume inspect");
        let compose_config = serde_json::to_vec(&serde_json::json!({
            "services": {
                "postgres": {
                    "container_name": resource,
                    "image": spec.image,
                    "environment": expected_environment,
                    "labels": spec.ownership,
                    "ports": [{
                        "host_ip": spec.bind_address,
                        "protocol": "tcp",
                        "published": spec.host_port.to_string(),
                        "target": spec.container_port
                    }],
                    "volumes": [{
                        "type": "volume",
                        "source": "minco-postgres",
                        "target": volume.container_path
                    }]
                }
            },
            "volumes": {
                "minco-postgres": {
                    "name": volume.name,
                    "labels": spec.ownership
                }
            }
        }))
        .expect("Compose config JSON");
        let config_arguments =
            compose_arguments(&arguments, &spec, &["config", "--format", "json"]);
        let up_arguments = compose_arguments(
            &arguments,
            &spec,
            &["up", "--detach", "--no-deps", "postgres"],
        );
        let runner = ProbeRunner::default()
            .with("docker", &["--version"], success("Docker version 29.7.1"))
            .with(
                "docker",
                &["compose", "version", "--short"],
                success("5.3.1\n"),
            )
            .with("docker", &["info"], success("ready"))
            .with("docker", &["inspect", &resource], CommandOutput::failure())
            .with(
                "docker",
                &["volume", "inspect", &volume.name],
                CommandOutput::failure(),
            )
            .with_arguments(
                "docker",
                strings(config_arguments),
                CommandOutput::success(compose_config),
            )
            .with_arguments("docker", strings(up_arguments), success(&resource))
            .with(
                "docker",
                &["volume", "inspect", &volume.name],
                CommandOutput::success(volume_inspect),
            )
            .with(
                "docker",
                &["inspect", &resource],
                CommandOutput::success(docker_inspect_bytes(&spec, &expected_environment, true)),
            )
            .with(
                "docker",
                &["logs", "--tail", "50", &resource],
                success(&format!("database log contains {secret}")),
            )
            .with(
                "docker",
                &["inspect", &resource],
                CommandOutput::success(docker_inspect_bytes(&spec, &expected_environment, true)),
            )
            .with(
                "docker",
                &["stop", "--timeout", "10", &resource],
                success(&resource),
            )
            .with(
                "docker",
                &["inspect", &resource],
                CommandOutput::success(docker_inspect_bytes(&spec, &expected_environment, false)),
            )
            .with("docker", &["rm", &resource], success(&resource))
            .with("docker", &["inspect", &resource], CommandOutput::failure());

        let error = start_with_dependencies(
            &runner,
            &FailsReadiness(format!("readiness contains {secret}")),
            &arguments,
            &source_environment,
            temporary.path(),
        )
        .await
        .expect_err("failed readiness must clean the fresh container");

        assert!(error.to_string().contains("recent runtime logs"));
        assert!(!error.to_string().contains(secret));
        let calls = runner.recorded_calls();
        assert!(calls.iter().any(|(program, arguments)| {
            program == "docker" && arguments == &["rm", &resource]
        }));
        assert!(!calls.iter().any(|(_, arguments)| {
            arguments
                .iter()
                .any(|argument| argument == "down" || argument == "--force")
                || arguments
                    .first()
                    .is_some_and(|argument| argument == "volume")
                    && arguments.get(1).is_some_and(|argument| argument == "rm")
        }));
        assert!(!receipt_path(&state_base(temporary.path()), &spec).exists());
    }

    #[test]
    fn rustack_uses_selected_services_and_the_pinned_release() {
        let temporary = tempfile::tempdir().expect("temporary project");
        let compose_file = temporary.path().join("compose.yaml");
        std::fs::write(&compose_file, "services: {}\n").expect("write Compose file");
        let mut arguments = arguments(Service::Rustack);
        arguments.compose_file = compose_file;
        let spec = LocalServiceSpec::from_arguments(&arguments, &BTreeMap::new())
            .expect("canonical Rustack service");
        assert_eq!(spec.environment["SERVICES"], "s3,ssm");
        assert_eq!(spec.image, RUSTACK_IMAGE);
        assert!(RUSTACK_IMAGE.contains(":0.9.1@sha256:"));
    }

    #[test]
    fn generated_compose_uses_the_canonical_images_and_ownership_schema() {
        let compose = include_str!("../templates/app/infra/local/compose.yaml.tmpl");
        assert!(compose.contains(POSTGRES_IMAGE));
        assert!(compose.contains(RUSTACK_IMAGE));
        for label in [
            OWNERSHIP_MANAGED,
            OWNERSHIP_SCHEMA,
            OWNERSHIP_APPLICATION,
            OWNERSHIP_WORKSPACE,
            OWNERSHIP_SERVICE,
            OWNERSHIP_CONFIGURATION,
        ] {
            assert!(compose.contains(label), "generated Compose omitted {label}");
        }
        assert!(compose.contains("127.0.0.1:${MINCO_POSTGRES_PORT:-55432}:5432"));
        assert!(compose.contains("127.0.0.1:${MINCO_RUSTACK_PORT:-4566}:4566"));
    }

    #[test]
    fn rustack_health_requires_structured_running_state_for_every_requested_service() {
        let requested = vec!["ssm".to_owned(), "sts".to_owned()];
        verify_rustack_health(
            br#"{"services":{"ssm":"running","sts":"running"}}"#,
            &requested,
        )
        .expect("all requested services running");

        let error = verify_rustack_health(
            br#"{"services":{"ssm":"running","sts":"starting"}}"#,
            &requested,
        )
        .expect_err("non-running service must fail");
        assert!(error.to_string().contains("every requested service"));
        assert!(verify_rustack_health(br#"{"status":"running"}"#, &requested).is_err());
        assert!(verify_rustack_health(b"not-json", &requested).is_err());
    }

    #[test]
    fn invalid_ports_and_empty_rustack_service_sets_fail_closed() {
        assert!(
            crate::Cli::try_parse_from([
                "cargo-minco",
                "__local-service",
                "start",
                "postgres",
                "--application",
                "orders",
                "--compose-file",
                "infra/local/compose.yaml",
                "--port",
                "0",
            ])
            .is_err()
        );

        let temporary = tempfile::tempdir().expect("temporary directory");
        let compose_file = temporary.path().join("compose.yaml");
        std::fs::write(&compose_file, "services: {}\n").expect("write Compose file");
        let mut arguments = arguments(Service::Rustack);
        arguments.compose_file = compose_file;
        arguments.aws_services.clear();
        assert!(validate(&arguments, true).is_err());
        arguments.aws_services = vec!["appsync".into()];
        assert!(
            validate(&arguments, true)
                .expect_err("unsupported Rustack service")
                .to_string()
                .contains("Rustack 0.9.1")
        );
    }

    #[test]
    fn runtime_aliases_are_parseable() {
        let cli = crate::Cli::try_parse_from([
            "cargo-minco",
            "__local-service",
            "stop",
            "rustack",
            "--application",
            "orders",
            "--compose-file",
            "infra/local/compose.yaml",
            "--port",
            "4566",
            "--runtime",
            "apple-container",
        ])
        .expect("valid helper command");
        let crate::Command::LocalService(LocalServiceArgs {
            action: Action::Stop(arguments),
        }) = cli.command
        else {
            panic!("expected stop action");
        };
        assert_eq!(arguments.runtime, RuntimePreference::Apple);
    }
}
