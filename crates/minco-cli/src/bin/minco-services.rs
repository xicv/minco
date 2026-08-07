#![forbid(unsafe_code)]

use anyhow::{Context, Result, bail, ensure};
use clap::{Args, Parser, Subcommand, ValueEnum};
use sha2::{Digest as _, Sha256};
use std::{
    env,
    ffi::OsString,
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

const POSTGRES_CONTAINER_PORT: u16 = 5_432;
const POSTGRES_DATABASE: &str = "minco_orders";
const POSTGRES_IMAGE: &str = "docker.io/library/postgres:18-alpine";
const POSTGRES_PASSWORD: &str = "minco";
const POSTGRES_USER: &str = "minco";
const RUSTACK_CONTAINER_PORT: u16 = 4_566;
const RUSTACK_IMAGE: &str = concat!(
    "ghcr.io/tyrchen/rustack:0.9.1@",
    "sha256:18cd91395e17453e2c34b299e45f4679dc2427473dc1db6541bbe212fd70a104"
);
const COMMAND_TIMEOUT: Duration = Duration::from_secs(5);
const READY_TIMEOUT: Duration = Duration::from_secs(60);
const RETRY_INTERVAL: Duration = Duration::from_millis(250);

#[derive(Debug, Parser)]
#[command(
    name = "minco-services",
    about = "Internal local-service runtime used by cargo minco dev"
)]
struct Cli {
    #[command(subcommand)]
    action: Action,
}

#[derive(Debug, Subcommand)]
enum Action {
    Start(ServiceArgs),
    Stop(ServiceArgs),
}

#[derive(Debug, Clone, Args)]
struct ServiceArgs {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Service {
    Postgres,
    Rustack,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Runtime {
    Docker,
    Apple,
}

impl Runtime {
    const fn label(self) -> &'static str {
        match self {
            Self::Docker => "Docker Compose",
            Self::Apple => "Apple Container",
        }
    }

    fn ready(self) -> bool {
        match self {
            Self::Docker => {
                command_succeeds("docker", &["compose", "version"])
                    && command_succeeds("docker", &["info"])
            }
            Self::Apple => {
                cfg!(all(target_os = "macos", target_arch = "aarch64"))
                    && command_succeeds("container", &["system", "status"])
            }
        }
    }
}

fn main() -> Result<()> {
    match Cli::parse().action {
        Action::Start(arguments) => start(&arguments),
        Action::Stop(arguments) => stop(&arguments),
    }
}

fn start(arguments: &ServiceArgs) -> Result<()> {
    validate(arguments, true)?;
    let runtime = resolve_runtime(arguments.runtime)?;
    let result = start_with(runtime, arguments).and_then(|()| wait_until_ready(arguments));
    if let Err(error) = result {
        diagnostics(runtime, arguments);
        if let Err(cleanup_error) = stop_with(runtime, arguments) {
            eprintln!("minco: failed startup cleanup: {cleanup_error:#}");
        }
        return Err(error);
    }
    println!(
        "minco: {} is ready on 127.0.0.1:{} via {}",
        arguments.service.label(),
        arguments.port,
        runtime.label()
    );
    Ok(())
}

fn stop(arguments: &ServiceArgs) -> Result<()> {
    validate(arguments, false)?;
    if arguments.runtime != RuntimePreference::Auto {
        let runtime = resolve_runtime(arguments.runtime)?;
        stop_with(runtime, arguments)?;
        println!(
            "minco: stopped {} via {}",
            arguments.service.label(),
            runtime.label()
        );
        return Ok(());
    }

    let mut attempted = false;
    let mut failures = Vec::new();
    for runtime in [Runtime::Docker, Runtime::Apple] {
        if runtime.ready() {
            attempted = true;
            if let Err(error) = stop_with(runtime, arguments) {
                failures.push(format!("{}: {error:#}", runtime.label()));
            }
        }
    }
    if !attempted {
        println!(
            "minco: {} is already stopped because no container runtime is ready",
            arguments.service.label()
        );
        return Ok(());
    }
    ensure!(
        failures.is_empty(),
        "failed to stop {}: {}",
        arguments.service.label(),
        failures.join("; ")
    );
    println!("minco: stopped {}", arguments.service.label());
    Ok(())
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
    }
    Ok(())
}

fn resolve_runtime(preference: RuntimePreference) -> Result<Runtime> {
    let requested = match preference {
        RuntimePreference::Docker => Some(Runtime::Docker),
        RuntimePreference::Apple => Some(Runtime::Apple),
        RuntimePreference::Auto => None,
    };
    if let Some(runtime) = requested {
        ensure_runtime_ready(runtime)?;
        return Ok(runtime);
    }
    if Runtime::Docker.ready() {
        return Ok(Runtime::Docker);
    }
    if Runtime::Apple.ready() {
        return Ok(Runtime::Apple);
    }
    if command_succeeds("docker", &["--version"]) {
        bail!(
            "Docker is installed but not ready; start the daemon and ensure `docker compose version` succeeds, or set MINCO_CONTAINER_RUNTIME=apple"
        );
    }
    if command_succeeds("container", &["--version"]) {
        bail!(
            "Apple Container is installed but not ready; run `container system start`, or set MINCO_CONTAINER_RUNTIME=docker"
        );
    }
    bail!(
        "no supported container runtime is ready; install/start Docker, or on Apple silicon macOS 26 install Apple Container and run `container system start`"
    )
}

fn ensure_runtime_ready(runtime: Runtime) -> Result<()> {
    match runtime {
        Runtime::Docker => {
            ensure!(
                command_succeeds("docker", &["--version"]),
                "Docker CLI is not available"
            );
            ensure!(
                command_succeeds("docker", &["compose", "version"]),
                "Docker Compose v2 is not available"
            );
            ensure!(
                command_succeeds("docker", &["info"]),
                "Docker daemon is not ready"
            );
        }
        Runtime::Apple => {
            ensure!(
                cfg!(all(target_os = "macos", target_arch = "aarch64")),
                "Apple Container requires Apple silicon macOS 26 or newer"
            );
            ensure!(
                command_succeeds("container", &["--version"]),
                "Apple Container CLI is not available"
            );
            ensure!(
                command_succeeds("container", &["system", "status"]),
                "Apple Container services are not ready; run `container system start`"
            );
        }
    }
    Ok(())
}

fn command_succeeds(program: &str, arguments: &[&str]) -> bool {
    let Ok(mut child) = Command::new(program)
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    else {
        return false;
    };
    let deadline = Instant::now() + COMMAND_TIMEOUT;
    while Instant::now() < deadline {
        match child.try_wait() {
            Ok(Some(status)) => return status.success(),
            Ok(None) => thread::sleep(Duration::from_millis(50)),
            Err(_) => break,
        }
    }
    let _ = child.kill();
    let _ = child.wait();
    false
}

fn start_with(runtime: Runtime, arguments: &ServiceArgs) -> Result<()> {
    match runtime {
        Runtime::Docker => {
            let mut command = compose_command(arguments);
            command.args(["up", "-d", arguments.service.label()]);
            run(&mut command, "start local service with Docker Compose")
        }
        Runtime::Apple => start_apple(arguments),
    }
}

fn stop_with(runtime: Runtime, arguments: &ServiceArgs) -> Result<()> {
    match runtime {
        Runtime::Docker => {
            let mut command = compose_command(arguments);
            command.args(["stop", "--timeout", "10", arguments.service.label()]);
            run(&mut command, "stop local service with Docker Compose")
        }
        Runtime::Apple => stop_apple(arguments),
    }
}

fn compose_command(arguments: &ServiceArgs) -> Command {
    let mut command = Command::new("docker");
    command
        .arg("compose")
        .arg("--project-name")
        .arg(project_name(&arguments.application, &arguments.compose_file))
        .arg("-f")
        .arg(&arguments.compose_file);
    command
}

fn start_apple(arguments: &ServiceArgs) -> Result<()> {
    let name = container_name(arguments);
    if apple_exists(&name) {
        let mut cleanup = Command::new("container");
        cleanup.args(["delete", "--force", &name]);
        run(&mut cleanup, "remove stale Apple container")?;
    }
    let mut command = Command::new("container");
    command
        .envs(apple_environment(arguments))
        .args(apple_arguments(arguments));
    run(&mut command, "start local service with Apple Container")
}

fn stop_apple(arguments: &ServiceArgs) -> Result<()> {
    let name = container_name(arguments);
    if !apple_exists(&name) {
        return Ok(());
    }
    let mut command = Command::new("container");
    command.args(["stop", "--time", "10", &name]);
    run(&mut command, "stop Apple container")
}

fn apple_exists(name: &str) -> bool {
    command_succeeds("container", &["inspect", name])
}

fn apple_arguments(arguments: &ServiceArgs) -> Vec<OsString> {
    let name = container_name(arguments);
    let mut result = vec![
        "run".into(),
        "--detach".into(),
        "--rm".into(),
        "--name".into(),
        name.clone().into(),
    ];
    match arguments.service {
        Service::Postgres => result.extend([
            "--env".into(),
            "POSTGRES_DB".into(),
            "--env".into(),
            "POSTGRES_USER".into(),
            "--env".into(),
            "POSTGRES_PASSWORD".into(),
            "--publish".into(),
            format!(
                "127.0.0.1:{}:{POSTGRES_CONTAINER_PORT}",
                arguments.port
            )
            .into(),
            "--volume".into(),
            format!("{}:/var/lib/postgresql", volume_name(&name)).into(),
            env_or("MINCO_POSTGRES_IMAGE", POSTGRES_IMAGE).into(),
        ]),
        Service::Rustack => result.extend([
            "--env".into(),
            "SERVICES".into(),
            "--env".into(),
            "DEFAULT_REGION".into(),
            "--env".into(),
            "LOG_LEVEL".into(),
            "--publish".into(),
            format!(
                "127.0.0.1:{}:{RUSTACK_CONTAINER_PORT}",
                arguments.port
            )
            .into(),
            env_or("MINCO_RUSTACK_IMAGE", RUSTACK_IMAGE).into(),
        ]),
    }
    result
}

fn apple_environment(arguments: &ServiceArgs) -> Vec<(String, String)> {
    match arguments.service {
        Service::Postgres => vec![
            ("POSTGRES_DB".into(), env_or("MINCO_POSTGRES_DB", POSTGRES_DATABASE)),
            ("POSTGRES_USER".into(), env_or("MINCO_POSTGRES_USER", POSTGRES_USER)),
            (
                "POSTGRES_PASSWORD".into(),
                env_or("MINCO_POSTGRES_PASSWORD", POSTGRES_PASSWORD),
            ),
        ],
        Service::Rustack => vec![
            ("SERVICES".into(), arguments.aws_services.join(",")),
            ("DEFAULT_REGION".into(), default_region()),
            ("LOG_LEVEL".into(), env_or("MINCO_RUSTACK_LOG_LEVEL", "info")),
        ],
    }
}

fn run(command: &mut Command, description: &str) -> Result<()> {
    let status = command
        .status()
        .with_context(|| format!("failed to {description}"))?;
    ensure!(status.success(), "{description} exited with {status}");
    Ok(())
}

fn wait_until_ready(arguments: &ServiceArgs) -> Result<()> {
    let deadline = Instant::now() + READY_TIMEOUT;
    let user = env_or("MINCO_POSTGRES_USER", POSTGRES_USER);
    let database = env_or("MINCO_POSTGRES_DB", POSTGRES_DATABASE);
    while Instant::now() < deadline {
        let ready = match arguments.service {
            Service::Postgres => postgres_ready(arguments.port, &user, &database),
            Service::Rustack => rustack_ready(arguments.port),
        };
        if ready {
            return Ok(());
        }
        thread::sleep(RETRY_INTERVAL);
    }
    bail!(
        "{} did not become ready at 127.0.0.1:{} within {} seconds",
        arguments.service.label(),
        arguments.port,
        READY_TIMEOUT.as_secs()
    )
}

fn postgres_ready(port: u16, user: &str, database: &str) -> bool {
    let timeout = Duration::from_millis(500);
    let Ok(mut stream) = TcpStream::connect_timeout(
        &SocketAddr::from(([127, 0, 0, 1], port)),
        timeout,
    ) else {
        return false;
    };
    if stream.set_read_timeout(Some(timeout)).is_err()
        || stream.set_write_timeout(Some(timeout)).is_err()
        || stream.write_all(&postgres_message(user, database)).is_err()
    {
        return false;
    }
    let mut response = [0_u8; 9];
    stream.read_exact(&mut response).is_ok()
        && response[0] == b'R'
        && u32::from_be_bytes([response[1], response[2], response[3], response[4]]) >= 8
}

fn postgres_message(user: &str, database: &str) -> Vec<u8> {
    let mut message = vec![0_u8; 4];
    message.extend(196_608_u32.to_be_bytes());
    message.extend(b"user\0");
    message.extend(user.as_bytes());
    message.push(0);
    message.extend(b"database\0");
    message.extend(database.as_bytes());
    message.extend([0, 0]);
    let length = u32::try_from(message.len()).expect("Postgres startup message is bounded");
    message[..4].copy_from_slice(&length.to_be_bytes());
    message
}

fn rustack_ready(port: u16) -> bool {
    let timeout = Duration::from_millis(500);
    let Ok(mut stream) = TcpStream::connect_timeout(
        &SocketAddr::from(([127, 0, 0, 1], port)),
        timeout,
    ) else {
        return false;
    };
    if stream.set_read_timeout(Some(timeout)).is_err()
        || stream.set_write_timeout(Some(timeout)).is_err()
    {
        return false;
    }
    let request = format!(
        "GET /_localstack/health HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n"
    );
    if stream.write_all(request.as_bytes()).is_err() {
        return false;
    }
    let mut response = String::new();
    (&mut stream)
        .take(4_096)
        .read_to_string(&mut response)
        .is_ok()
        && (response.starts_with("HTTP/1.1 200") || response.starts_with("HTTP/1.0 200"))
        && response.contains("\"running\"")
}

fn diagnostics(runtime: Runtime, arguments: &ServiceArgs) {
    eprintln!(
        "minco: {} failed readiness; recent container logs follow",
        arguments.service.label()
    );
    let mut command = match runtime {
        Runtime::Docker => {
            let mut command = compose_command(arguments);
            command.args([
                "logs",
                "--no-color",
                "--tail",
                "50",
                arguments.service.label(),
            ]);
            command
        }
        Runtime::Apple => {
            let mut command = Command::new("container");
            command
                .args(["logs", "-n", "50"])
                .arg(container_name(arguments));
            command
        }
    };
    let _ = command.status();
}

fn default_region() -> String {
    env::var("AWS_REGION")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            env::var("AWS_DEFAULT_REGION")
                .ok()
                .filter(|value| !value.trim().is_empty())
        })
        .unwrap_or_else(|| "ap-southeast-2".to_owned())
}

fn env_or(name: &str, default: &str) -> String {
    env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| default.to_owned())
}

fn project_name(application: &str, compose_file: &Path) -> String {
    bounded_name(
        "minco-",
        &normalized(application),
        &format!("-{}", fingerprint(compose_file)),
    )
}

fn container_name(arguments: &ServiceArgs) -> String {
    bounded_name(
        "",
        &project_name(&arguments.application, &arguments.compose_file),
        &format!("-{}", arguments.service.label()),
    )
}

fn fingerprint(compose_file: &Path) -> String {
    let identity = if compose_file.is_absolute() {
        compose_file.to_path_buf()
    } else {
        env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(compose_file)
    };
    let digest = Sha256::digest(identity.to_string_lossy().as_bytes());
    format!(
        "{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        digest[0], digest[1], digest[2], digest[3], digest[4], digest[5]
    )
}

fn volume_name(container: &str) -> String {
    bounded_name("", container, "-data")
}

fn normalized(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    let mut separator = false;
    for character in value.chars().map(|character| character.to_ascii_lowercase()) {
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

fn bounded_name(prefix: &str, component: &str, suffix: &str) -> String {
    let maximum = 63_usize.saturating_sub(prefix.len() + suffix.len());
    let mut component = component.to_owned();
    component.truncate(maximum);
    while component.ends_with('-') {
        component.pop();
    }
    format!("{prefix}{component}{suffix}")
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn names_are_stable_bounded_and_workspace_scoped() {
        let mut arguments = arguments(Service::Rustack);
        arguments.compose_file = PathBuf::from("/workspace/orders/infra/local/compose.yaml");
        let first = container_name(&arguments);
        assert!(first.starts_with("minco-orders-api-"));
        assert!(first.ends_with("-rustack"));
        assert_eq!(first, container_name(&arguments));
        arguments.compose_file = PathBuf::from("/workspace/copy/infra/local/compose.yaml");
        assert_ne!(first, container_name(&arguments));
        arguments.application = "A".repeat(100);
        assert!(container_name(&arguments).len() <= 63);
    }

    #[test]
    fn apple_arguments_keep_secrets_out_of_argv() {
        let arguments = arguments(Service::Postgres);
        let command = strings(apple_arguments(&arguments));
        let environment = apple_environment(&arguments)
            .into_iter()
            .collect::<std::collections::BTreeMap<_, _>>();
        assert!(command.contains(&"127.0.0.1:55432:5432".to_owned()));
        assert!(command.contains(&"POSTGRES_PASSWORD".to_owned()));
        assert!(!command.contains(&"POSTGRES_PASSWORD=minco".to_owned()));
        assert_eq!(environment["POSTGRES_PASSWORD"], POSTGRES_PASSWORD);
        assert!(command.contains(&format!(
            "{}:/var/lib/postgresql",
            volume_name(&container_name(&arguments))
        )));
    }

    #[test]
    fn rustack_uses_selected_services_and_the_pinned_release() {
        let arguments = arguments(Service::Rustack);
        let command = strings(apple_arguments(&arguments));
        let environment = apple_environment(&arguments)
            .into_iter()
            .collect::<std::collections::BTreeMap<_, _>>();
        assert_eq!(environment["SERVICES"], "s3,ssm");
        assert_eq!(command.last().map(String::as_str), Some(RUSTACK_IMAGE));
        assert!(RUSTACK_IMAGE.contains(":0.9.1@sha256:"));
    }

    #[test]
    fn postgres_probe_declares_protocol_three_and_local_credentials() {
        let message = postgres_message(POSTGRES_USER, POSTGRES_DATABASE);
        let length = u32::from_be_bytes([message[0], message[1], message[2], message[3]]);
        assert_eq!(usize::try_from(length).expect("usize length"), message.len());
        assert_eq!(&message[4..8], &196_608_u32.to_be_bytes());
        assert!(message.windows(b"user\0minco\0".len()).any(|value| {
            value == b"user\0minco\0"
        }));
    }

    #[test]
    fn invalid_ports_and_empty_rustack_service_sets_fail_closed() {
        assert!(Cli::try_parse_from([
            "minco-services",
            "start",
            "postgres",
            "--application",
            "orders",
            "--compose-file",
            "infra/local/compose.yaml",
            "--port",
            "0",
        ])
        .is_err());

        let temporary = tempfile::tempdir().expect("temporary directory");
        let compose_file = temporary.path().join("compose.yaml");
        std::fs::write(&compose_file, "services: {}\n").expect("write Compose file");
        let mut arguments = arguments(Service::Rustack);
        arguments.compose_file = compose_file;
        arguments.aws_services.clear();
        assert!(validate(&arguments, true).is_err());
    }

    #[test]
    fn runtime_aliases_are_parseable() {
        let cli = Cli::try_parse_from([
            "minco-services",
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
        let Action::Stop(arguments) = cli.action else {
            panic!("expected stop action");
        };
        assert_eq!(arguments.runtime, RuntimePreference::Apple);
    }
}
