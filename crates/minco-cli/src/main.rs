// The CLI implementation remains in the binary target. Public visibility lets
// sibling command modules share internal types; it is not part of the library
// documentation target's API.
#![allow(unreachable_pub)]

mod architecture;
mod config;
mod config_cmd;
mod db_cmd;
mod feedback_cmd;
mod generator_cmd;
mod new_cmd;
mod plugin_cmd;
mod process;
mod roadmap;
mod update;
mod upgrade_cmd;
mod vcs;

use anyhow::{Context, Result, bail};
use architecture::validate_architecture;
use chrono::Utc;
use clap::{Args, Parser, Subcommand, ValueEnum};
use config::{MincoManifest, discover_root};
use config_cmd::ConfigCommand;
use feedback_cmd::FeedbackArgs;
use generator_cmd::{MakeCommand, StubsCommand};
use minco_config::EnvironmentClass;
use minco_contract::{
    Severity as ContractSeverity, diff_contracts, generate_rust, load_contract,
    load_contract_source,
};
use minco_core::{ApplicationGraph, PluginId, PluginManager, PluginSelection};
use minco_dev::{
    DevDatabase, DevEvent, DevGraph, DevOptions, DevPlan, DevStream, ServiceKind, Supervisor,
};
use minco_plan::{
    DatabaseCostEstimate, DatabaseDeployment, DeploymentConfig, DeploymentPlan, FunctionRole,
    Severity as PlanSeverity, TriggerPlan, estimate_database_cost, estimate_runtime_cost,
    render_sam_with_code_uris,
};
use minco_release::{FileDigest, ReleaseManifest};
use new_cmd::{DatabaseChoice, NewProjectOptions, VcsChoice, create_project};
use plugin_cmd::{load_catalog, scaffold_plugin, set_plugin_state, validate_catalog};
use process::{capture, command_available, run_shell};
use roadmap::{
    load_roadmap, load_tasks, ready_tasks, render_roadmap_mermaid, render_task_mermaid,
    validate_task_graph,
};
use serde::Serialize;
use serde_json::json;
use std::{
    ffi::{OsStr, OsString},
    fs,
    path::{Path, PathBuf},
};
use uuid::Uuid;

#[derive(Debug, Parser)]
#[command(
    name = "minco",
    version,
    about = "Contract-first Rust development and deployment control plane"
)]
struct Cli {
    #[arg(long, global = true)]
    root: Option<PathBuf>,
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    New(NewArgs),
    Doctor,
    /// Run the graph-declared local development topology.
    Dev(DevArgs),
    Check(CheckArgs),
    #[command(subcommand)]
    Config(ConfigCommand),
    #[command(subcommand)]
    Contract(ContractCommand),
    #[command(subcommand)]
    Make(MakeCommand),
    #[command(subcommand)]
    Stubs(StubsCommand),
    Inspect,
    Explain(ExplainArgs),
    #[command(subcommand)]
    Deploy(DeployCommand),
    Cost(PlanInput),
    Perf(PlanInput),
    Architecture,
    #[command(subcommand)]
    Roadmap(RoadmapCommand),
    #[command(subcommand)]
    Task(TaskCommand),
    #[command(subcommand)]
    Plugin(PluginCommand),
    #[command(subcommand)]
    Test(TestCommand),
    #[command(subcommand)]
    Db(DbCommand),
    #[command(subcommand)]
    Release(ReleaseCommand),
    #[command(subcommand)]
    Update(UpdateCommand),
    #[command(subcommand)]
    Upgrade(UpgradeCommand),
    #[command(subcommand)]
    Vcs(VcsCommand),
    /// Inspect and advance the first-class client feedback loop.
    Feedback(FeedbackArgs),
}

#[derive(Debug, Args)]
struct NewArgs {
    /// Lower-kebab-case application and package prefix.
    name: String,
    /// Destination directory; defaults to the application name.
    #[arg(long)]
    directory: Option<PathBuf>,
    /// Initial persistence runtime and deployment profile.
    #[arg(long, value_enum, default_value_t = DatabaseChoice::Postgres)]
    database: DatabaseChoice,
    /// Version-control initialization. JJ is the Minco default.
    #[arg(long, value_enum, default_value_t = VcsChoice::Jj)]
    vcs: VcsChoice,
}

#[derive(Debug, Clone, Copy, Args)]
struct CheckArgs {
    #[arg(long)]
    with_cargo: bool,
    #[arg(long)]
    with_optional: bool,
}

#[derive(Debug, Clone, Args)]
// These booleans are independent user-facing flags, including Clap's explicit
// positive/negative frontend pair.
#[allow(clippy::struct_excessive_bools)]
struct DevArgs {
    /// Print the deterministic development plan without starting anything.
    #[arg(long)]
    dry_run: bool,
    /// Typed runtime configuration environment.
    #[arg(long)]
    environment: Option<String>,
    /// Named development/deployment profile; defaults to the manifest selection.
    #[arg(long)]
    profile: Option<String>,
    /// Do not apply the declared local migration command.
    #[arg(long)]
    no_migrate: bool,
    /// Explicit local seed profile to apply.
    #[arg(long)]
    seed: Option<String>,
    /// Start a declared worker that is disabled by default.
    #[arg(long = "with-worker")]
    with_workers: Vec<String>,
    /// Omit a declared worker that is enabled by default.
    #[arg(long = "without-worker")]
    without_workers: Vec<String>,
    /// Start the application-defined frontend process.
    #[arg(long, conflicts_with = "no_frontend")]
    frontend: bool,
    /// Omit the application-defined frontend process.
    #[arg(long, conflicts_with = "frontend")]
    no_frontend: bool,
    /// Override the local API port.
    #[arg(long, value_parser = clap::value_parser!(u16).range(1..))]
    port: Option<u16>,
    /// Override the local Rustack port.
    #[arg(long, value_parser = clap::value_parser!(u16).range(1..))]
    rustack_port: Option<u16>,
}

#[derive(Debug, Clone, Subcommand)]
enum ContractCommand {
    Check,
    Sync {
        #[arg(long)]
        check: bool,
    },
    /// Compare the current contract with the contract stored at a VCS revision.
    Diff {
        #[arg(long)]
        against: String,
    },
}

#[derive(Debug, Clone, Copy, Subcommand)]
enum UpgradeCommand {
    /// Inventory application-facing compatibility boundaries for an upgrade review.
    Report,
}

#[derive(Debug, Args)]
struct ExplainArgs {
    operation_id: String,
}

#[derive(Debug, Clone, Args)]
struct PlanInput {
    #[arg(long)]
    config: Option<PathBuf>,
}

#[derive(Debug, Subcommand)]
enum DeployCommand {
    Plan {
        #[command(flatten)]
        input: PlanInput,
        #[arg(long, conflicts_with = "stdout")]
        output: Option<PathBuf>,
        #[arg(long)]
        stdout: bool,
    },
    RenderSam {
        #[command(flatten)]
        input: PlanInput,
        #[arg(long, default_value = "infra/aws/generated/template.yaml")]
        output: PathBuf,
    },
}

#[derive(Debug, Subcommand)]
enum RoadmapCommand {
    Status,
    Render {
        #[arg(long, value_enum, default_value_t = DiagramFormat::Mermaid)]
        format: DiagramFormat,
        #[arg(long)]
        output: Option<PathBuf>,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum DiagramFormat {
    Mermaid,
    Json,
}

#[derive(Debug, Subcommand)]
enum TaskCommand {
    List,
    Ready,
    Next,
    Show {
        id: String,
    },
    Graph {
        #[arg(long)]
        output: Option<PathBuf>,
    },
    Verify {
        id: String,
    },
}

#[derive(Debug, Subcommand)]
enum PluginCommand {
    List,
    Enable { id: String },
    Disable { id: String },
    New { id: String },
    Validate,
}

#[derive(Debug, Clone, Copy, Subcommand)]
enum TestCommand {
    Unit,
    Feature,
    E2e,
    All,
}

#[derive(Debug, Clone, Args)]
struct DbTargetArgs {
    /// Migration set to inspect. Omitting this and the URL environment lists source state only.
    #[arg(long)]
    set: Option<String>,
    /// Name of the environment variable containing the database URL.
    #[arg(long, requires = "set")]
    database_url_env: Option<String>,
}

#[derive(Debug, Clone, Args)]
struct DbMigrateArgs {
    /// Migration set to apply, including its declared dependency closure.
    #[arg(long)]
    set: String,
    /// Name of the environment variable containing the direct migration database URL.
    #[arg(long)]
    database_url_env: String,
    /// Digest emitted by `minco db plan --set <id>`.
    #[arg(long)]
    expected_plan_digest: String,
    /// Durable JSON receipt destination.
    #[arg(long)]
    receipt: PathBuf,
    /// Permit plans containing data-rewrite or destructive migrations.
    #[arg(long)]
    allow_destructive: bool,
}

#[derive(Debug, Clone, Args)]
struct DbSeedArgs {
    /// Seed class to plan or apply: reference, demo, test, or bootstrap.
    #[arg(long)]
    profile: Option<String>,
    /// Declared environment class used for the seed allowlist; defaults to local.
    #[arg(long)]
    environment: Option<String>,
    /// Seed set to inspect or apply.
    #[arg(long)]
    set: Option<String>,
    /// Name of the environment variable containing the direct seed database URL.
    #[arg(long)]
    database_url_env: Option<String>,
    /// Digest emitted by the matching seed dry-run.
    #[arg(long)]
    expected_plan_digest: Option<String>,
    /// Durable JSON receipt destination for an applied seed plan.
    #[arg(long)]
    receipt: Option<PathBuf>,
    /// Produce the complete seed plan without connecting or mutating.
    #[arg(long)]
    dry_run: bool,
    /// Verify seed source, or the selected target when a URL environment is provided.
    #[arg(long)]
    verify: bool,
    /// Permit plans containing destructive seed operations.
    #[arg(long)]
    allow_destructive: bool,
    /// Exact environment acknowledgement required for bootstrap execution.
    #[arg(long)]
    authorize_bootstrap: Option<String>,
}

#[derive(Debug, Clone, Subcommand)]
enum DbCommand {
    Plan {
        #[arg(long)]
        set: Option<String>,
    },
    Status(DbTargetArgs),
    Verify(DbTargetArgs),
    Migrate(DbMigrateArgs),
    Seed(DbSeedArgs),
}

#[derive(Debug, Subcommand)]
enum ReleaseCommand {
    Create {
        #[arg(long)]
        artifact: PathBuf,
        #[arg(long, default_value = "infra/aws/generated/plan.json")]
        plan: PathBuf,
        #[arg(long, default_value = "infra/aws/generated/template.yaml")]
        template: PathBuf,
        #[arg(long, default_value = "target/minco/release.json")]
        output: PathBuf,
    },
    Verify {
        manifest: PathBuf,
    },
}

#[derive(Debug, Clone, Copy, Subcommand)]
enum UpdateCommand {
    Check,
    Apply {
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        toolchain: bool,
        #[arg(long)]
        dependencies: bool,
        #[arg(long)]
        run_checks: bool,
    },
}

#[derive(Debug, Subcommand)]
enum VcsCommand {
    Init,
    Status,
    TaskStart {
        id: String,
        #[arg(long)]
        destination: Option<PathBuf>,
    },
    TaskFinish {
        id: String,
        #[arg(long)]
        message: String,
        #[arg(long)]
        push: bool,
    },
}

#[derive(Debug, Serialize)]
struct DoctorCheck {
    name: String,
    available: bool,
    required: bool,
    required_for: String,
}

fn normalize_cargo_subcommand_args(mut args: Vec<OsString>) -> Vec<OsString> {
    if args
        .get(1)
        .is_some_and(|value| value.as_os_str() == OsStr::new("minco"))
    {
        args.remove(1);
    }
    args
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse_from(normalize_cargo_subcommand_args(
        std::env::args_os().collect(),
    ));
    let Cli {
        root,
        json: as_json,
        command,
    } = cli;
    let command = match command {
        Command::New(args) => {
            let report = create_project(&NewProjectOptions {
                name: args.name,
                directory: args.directory,
                database: args.database,
                vcs: args.vcs,
            })?;
            return print_value(&report, as_json);
        }
        other => other,
    };

    let root = discover_root(root)?;
    let command = match command {
        Command::Upgrade(command) => return upgrade_cmd::execute(&root, command, as_json),
        other => other,
    };
    let manifest = MincoManifest::load(&root)?;
    match command {
        Command::New(_) => unreachable!("new is handled before project discovery"),
        Command::Doctor => doctor(&root, as_json),
        Command::Dev(args) => dev(&root, &manifest, args, as_json).await,
        Command::Check(args) => check(&root, &manifest, args, as_json),
        Command::Config(command) => config_cmd::execute(&root, &manifest, command, as_json),
        Command::Contract(command) => contract(&root, &manifest, command, as_json),
        Command::Make(command) => generator_cmd::execute(&root, &manifest, command, as_json),
        Command::Stubs(command) => generator_cmd::execute_stubs(&root, &command, as_json),
        Command::Inspect => inspect(&root, &manifest, as_json),
        Command::Explain(args) => explain(&root, &manifest, &args.operation_id, as_json),
        Command::Deploy(command) => deploy(&root, &manifest, command, as_json),
        Command::Cost(input) => cost(&root, &manifest, input, as_json),
        Command::Perf(input) => perf(&root, &manifest, input, as_json),
        Command::Architecture => architecture(&root, &manifest, as_json),
        Command::Roadmap(command) => roadmap_command(&root, &manifest, command, as_json),
        Command::Task(command) => task_command(&root, &manifest, command, as_json),
        Command::Plugin(command) => plugin_command(&root, &manifest, command, as_json),
        Command::Test(command) => test_command(&root, &manifest, command, as_json),
        Command::Db(command) => db_cmd::execute(&root, &manifest, command, as_json).await,
        Command::Release(command) => release_command(&root, &manifest, command, as_json),
        Command::Update(command) => update_command(&root, command, as_json),
        Command::Upgrade(_) => unreachable!("upgrade is handled before strict manifest loading"),
        Command::Vcs(command) => vcs_command(&root, command, as_json),
        Command::Feedback(args) => feedback_cmd::execute(&root, args, as_json).await,
    }
}

async fn dev(root: &Path, manifest: &MincoManifest, args: DevArgs, as_json: bool) -> Result<()> {
    let environment = args
        .environment
        .clone()
        .unwrap_or_else(|| manifest.development.default_environment.clone());
    let configuration = config_cmd::load_graph(root, manifest, &environment, &[])
        .map_err(|diagnostics| anyhow::anyhow!("invalid runtime configuration: {diagnostics:?}"))?;
    let environment_class = configuration.environment().class;
    if args.seed.is_some()
        && matches!(
            environment_class,
            EnvironmentClass::Staging | EnvironmentClass::Production
        )
    {
        bail!("cargo minco dev refuses seed profiles in staging or production environments");
    }

    let profile_id = args
        .profile
        .clone()
        .unwrap_or_else(|| manifest.development.default_profile.clone());
    let profile = manifest
        .development
        .profiles
        .get(&profile_id)
        .with_context(|| format!("development profile `{profile_id}` is not declared"))?;
    validate_project_file(
        root,
        &profile.deployment_config,
        "development deployment config",
    )?;
    validate_project_file(
        root,
        &manifest.development.compose_file,
        "development Compose file",
    )?;
    let deployment = load_plan(root, manifest, Some(profile.deployment_config.clone()))?;
    ensure_plan_valid(&deployment)?;

    let database = match &deployment.database {
        DatabaseDeployment::NeonPostgres { .. }
        | DatabaseDeployment::SelfHostedPostgres { .. }
        | DatabaseDeployment::RdsPostgres { .. }
        | DatabaseDeployment::AuroraServerlessV2 { .. } => DevDatabase::Postgres,
        DatabaseDeployment::SqlitePersistentHost { .. }
        | DatabaseDeployment::SqliteLambdaMutable { .. } => DevDatabase::Sqlite,
        DatabaseDeployment::DynamoDbOnDemand { .. } => DevDatabase::None,
    };
    let api = manifest
        .development
        .api
        .clone()
        .context("minco.toml must declare development.api")?;
    let http_functions = deployment
        .functions
        .iter()
        .filter(|function| function.role == FunctionRole::HttpApi)
        .map(|function| function.name.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    if !http_functions.contains(api.id.as_str()) {
        bail!(
            "development API `{}` is not declared as an HTTP function in profile `{profile_id}`",
            api.id
        );
    }
    let deployed_workers = deployment
        .functions
        .iter()
        .filter(|function| function.role == FunctionRole::Worker)
        .map(|function| function.name.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    for worker in &manifest.development.workers {
        if !deployed_workers.contains(worker.id.as_str()) {
            bail!(
                "development worker `{}` is absent from deployment profile `{profile_id}`",
                worker.id
            );
        }
    }
    let mut schedules = deployment.scheduled_wakeups.clone();
    schedules.extend(deployment.triggers.iter().filter_map(|trigger| {
        if let TriggerPlan::Schedule { id, .. } = trigger {
            Some(id.clone())
        } else {
            None
        }
    }));
    let region = deployment.region.clone();
    let graph = DevGraph {
        application: deployment.application.clone(),
        environment: environment.clone(),
        compose_file: manifest
            .development
            .compose_file
            .to_string_lossy()
            .into_owned(),
        database,
        local_aws_services: deployment.local_aws_services.clone(),
        api,
        workers: manifest.development.workers.clone(),
        frontend: manifest.development.frontend.clone(),
        migration: profile.migration.clone(),
        seeds: profile.seeds.clone(),
        schedules,
    };
    let frontend = match (args.frontend, args.no_frontend) {
        (true, false) => Some(true),
        (false, true) => Some(false),
        (false, false) => None,
        (true, true) => unreachable!("clap rejects conflicting frontend flags"),
    };
    let options = DevOptions {
        profile: profile_id,
        migrate: !args.no_migrate,
        seed: args.seed,
        with_workers: args.with_workers.into_iter().collect(),
        without_workers: args.without_workers.into_iter().collect(),
        frontend,
        port: args.port,
        rustack_port: args.rustack_port,
    };
    let plan = DevPlan::derive(&graph, &options)?;
    if args.dry_run {
        return print_value(&plan, as_json);
    }

    let runtime_environment =
        development_runtime_environment(&plan, database, &environment, environment_class, &region)?;
    if as_json {
        println!(
            "{}",
            serde_json::to_string(&json!({"kind": "plan", "plan": &plan}))?
        );
    } else {
        println!(
            "[minco] {} process(es), {} local service(s), profile {}",
            plan.processes.len(),
            plan.services.len(),
            plan.profile
        );
    }
    let (events, receiver) = tokio::sync::mpsc::unbounded_channel();
    let renderer = tokio::spawn(render_development_events(receiver, as_json));
    let result = Supervisor::new(root)
        .run_until(
            &plan,
            &runtime_environment,
            async {
                let _ = tokio::signal::ctrl_c().await;
            },
            events,
        )
        .await;
    renderer
        .await
        .context("development event renderer failed")??;
    result?;
    Ok(())
}

fn validate_project_file(root: &Path, relative: &Path, label: &str) -> Result<()> {
    if relative.as_os_str().is_empty()
        || !relative
            .components()
            .all(|component| matches!(component, std::path::Component::Normal(_)))
    {
        bail!("{label} must be a project-relative path");
    }
    let canonical = fs::canonicalize(root.join(relative))
        .with_context(|| format!("resolve {label} {}", relative.display()))?;
    if !canonical.starts_with(root) || !canonical.is_file() {
        bail!("{label} must resolve to a file inside the project");
    }
    Ok(())
}

fn development_runtime_environment(
    plan: &DevPlan,
    database: DevDatabase,
    environment: &str,
    environment_class: EnvironmentClass,
    region: &str,
) -> Result<std::collections::BTreeMap<String, String>> {
    let api_port = plan
        .processes
        .iter()
        .find(|process| process.role == minco_dev::ProcessRole::Api)
        .and_then(|process| process.command.environment.get("PORT"))
        .cloned()
        .unwrap_or_else(|| "3000".into());
    let allow_development_headers = matches!(
        environment_class,
        EnvironmentClass::Local | EnvironmentClass::Development | EnvironmentClass::Test
    );
    let environment_class = match environment_class {
        EnvironmentClass::Local => "local",
        EnvironmentClass::Test => "test",
        EnvironmentClass::Development => "development",
        EnvironmentClass::Staging => "staging",
        EnvironmentClass::Production => "production",
    };
    let mut values = std::collections::BTreeMap::from([
        (
            "ALLOW_DEVELOPMENT_HEADERS".into(),
            allow_development_headers.to_string(),
        ),
        ("API_HOST".into(), "127.0.0.1".into()),
        ("API_PORT".into(), api_port),
        ("APP_ENV".into(), environment.into()),
        ("AWS_EC2_METADATA_DISABLED".into(), "true".into()),
        (
            "MINCO_DEV_ENVIRONMENT_CLASS".into(),
            environment_class.into(),
        ),
    ]);
    match database {
        DevDatabase::Postgres => {
            let database_url = std::env::var("MINCO_LOCAL_DATABASE_URL")
                .unwrap_or_else(|_| "postgres://minco:minco@127.0.0.1:55432/minco_orders".into());
            if database_url.is_empty() {
                bail!("MINCO_LOCAL_DATABASE_URL must not be empty");
            }
            validate_local_postgres_url(&database_url)?;
            values.insert("DATABASE_KIND".into(), "postgres".into());
            values.insert("DATABASE_URL".into(), database_url.clone());
            values.insert("DATABASE_MIGRATION_URL".into(), database_url.clone());
            values.insert("MIGRATION_DATABASE_URL".into(), database_url);
        }
        DevDatabase::Sqlite => {
            values.insert("DATABASE_KIND".into(), "sqlite".into());
            values.insert("DATABASE_PATH".into(), "target/minco/orders.db".into());
            values.insert(
                "DATABASE_MIGRATION_URL".into(),
                "sqlite://target/minco/orders.db".into(),
            );
            values.insert("SQLITE_PATH".into(), "target/minco/orders.db".into());
        }
        DevDatabase::None => {
            values.insert("DATABASE_KIND".into(), "memory".into());
        }
    }
    if let Some(rustack) = plan
        .services
        .iter()
        .find(|service| service.kind == ServiceKind::Rustack)
    {
        let port = rustack
            .port
            .context("Rustack service must expose a local port")?;
        values.extend([
            ("AWS_ACCESS_KEY_ID".into(), "test".into()),
            ("AWS_DEFAULT_REGION".into(), region.into()),
            (
                "AWS_ENDPOINT_URL".into(),
                format!("http://127.0.0.1:{port}"),
            ),
            ("AWS_SECRET_ACCESS_KEY".into(), "test".into()),
        ]);
    }
    Ok(values)
}

fn validate_local_postgres_url(value: &str) -> Result<()> {
    let parsed = url::Url::parse(value).context("MINCO_LOCAL_DATABASE_URL must be a valid URL")?;
    let loopback = match parsed.host_str() {
        Some("localhost") => true,
        Some(host) => host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|address| address.is_loopback()),
        None => false,
    };
    if !matches!(parsed.scheme(), "postgres" | "postgresql") || !loopback {
        bail!("MINCO_LOCAL_DATABASE_URL must use PostgreSQL on a loopback host");
    }
    Ok(())
}

async fn render_development_events(
    mut events: tokio::sync::mpsc::UnboundedReceiver<DevEvent>,
    as_json: bool,
) -> Result<()> {
    while let Some(event) = events.recv().await {
        if as_json {
            println!("{}", serde_json::to_string(&event)?);
            continue;
        }
        match event {
            DevEvent::Starting { id } => println!("[{id}] starting"),
            DevEvent::Ready { id } => println!("[{id}] ready"),
            DevEvent::Log { id, stream, line } => {
                let stream = match stream {
                    DevStream::Stdout => "out",
                    DevStream::Stderr => "err",
                };
                println!("[{id}:{stream}] {line}");
            }
            DevEvent::Stopping { id } => println!("[{id}] stopping"),
            DevEvent::Stopped { id } => println!("[{id}] stopped"),
            DevEvent::Failed { id } => println!("[{id}] failed"),
        }
    }
    Ok(())
}

fn doctor(root: &Path, as_json: bool) -> Result<()> {
    let checks = [
        ("python3", true, "static validation and bootstrap"),
        ("uv", true, "locked Python validation dependencies"),
        ("rustc", true, "Rust compilation"),
        ("cargo", true, "build, test and CLI execution"),
        ("rustfmt", true, "format gate"),
        ("clippy-driver", true, "lint gate"),
        ("jj", true, "default version-control workflow"),
        ("git", true, "GitHub transport in colocated JJ repositories"),
        ("docker", false, "local PostgreSQL and Rustack"),
        ("cargo-lambda", false, "native Lambda ZIP build"),
        (
            "sam",
            false,
            "AWS template validation and local API emulation",
        ),
        ("aws", false, "real AWS deployment and verification"),
    ]
    .into_iter()
    .map(|(name, required, required_for)| DoctorCheck {
        name: name.into(),
        available: command_available(name),
        required,
        required_for: required_for.into(),
    })
    .collect::<Vec<_>>();
    print_value(&checks, as_json)?;
    let missing_core = checks
        .iter()
        .filter(|check| check.required && !check.available)
        .count();
    if missing_core > 0 {
        bail!("{missing_core} core development tools are unavailable; see the doctor report");
    }
    let _ = root;
    Ok(())
}

fn check(root: &Path, manifest: &MincoManifest, args: CheckArgs, as_json: bool) -> Result<()> {
    let quality_path = root.join(&manifest.quality);
    let quality: toml::Value = toml::from_str(&fs::read_to_string(&quality_path)?)?;
    let mut commands = quality_commands(&quality, "static")?;
    if args.with_cargo {
        commands.extend(quality_commands(&quality, "rust")?);
    }
    if args.with_optional {
        commands.extend(quality_commands(&quality, "security")?);
        commands.extend(quality_commands(&quality, "e2e")?);
    }
    let mut results = Vec::new();
    for command in commands {
        let result = run_shell(root, &command, !as_json)?;
        if !result.success {
            if as_json {
                results.push(result);
                print_value(&results, true)?;
            }
            bail!("quality gate failed: {command}");
        }
        results.push(result);
    }
    print_value(&results, as_json)
}

fn quality_commands(value: &toml::Value, gate: &str) -> Result<Vec<String>> {
    value
        .get("gates")
        .and_then(|value| value.get(gate))
        .and_then(|value| value.get("commands"))
        .and_then(toml::Value::as_array)
        .context(format!("quality gate {gate} has no command list"))?
        .iter()
        .map(|value| {
            value
                .as_str()
                .context("quality commands must be strings")
                .map(str::to_owned)
        })
        .collect::<Result<Vec<_>>>()
}

fn contract(
    root: &Path,
    manifest: &MincoManifest,
    command: ContractCommand,
    as_json: bool,
) -> Result<()> {
    let report = load_contract(root.join(&manifest.contract))?;
    match command {
        ContractCommand::Check => {
            print_value(&report, as_json)?;
            if report
                .findings
                .iter()
                .any(|finding| finding.severity == ContractSeverity::Error)
            {
                bail!("contract validation failed");
            }
        }
        ContractCommand::Sync { check } => {
            if !report.is_valid() {
                print_value(&report, as_json)?;
                bail!("contract validation failed; generation was not attempted");
            }
            let generated = generate_rust(&report.document);
            let path = root.join(&manifest.generated);
            if check {
                let existing = fs::read_to_string(&path).unwrap_or_default();
                if existing != generated {
                    bail!("{} is stale; run `minco contract sync`", path.display());
                }
            } else {
                ensure_parent(&path)?;
                fs::write(&path, generated)?;
            }
            print_value(
                &json!({"generated": path, "check": check, "contract_sha256": report.document.sha256}),
                as_json,
            )?;
        }
        ContractCommand::Diff { against } => {
            validate_project_file(root, &manifest.contract, "OpenAPI contract")?;
            validate_revision(&against)?;
            let contract_path = manifest
                .contract
                .to_str()
                .context("OpenAPI contract path must be valid UTF-8")?;
            let baseline_source = revision_file(root, &against, contract_path)?;
            let baseline_name = format!("{against}:{contract_path}");
            let baseline = load_contract_source(baseline_name, &baseline_source)?;
            if !baseline.is_valid() {
                bail!(
                    "baseline contract at {against} is invalid: {}",
                    serde_json::to_string(&baseline.findings)?
                );
            }
            if !report.is_valid() {
                bail!(
                    "candidate contract is invalid: {}",
                    serde_json::to_string(&report.findings)?
                );
            }
            let mut candidate = report.document;
            candidate.source = contract_path.into();
            print_value(&diff_contracts(&baseline.document, &candidate), as_json)?;
        }
    }
    Ok(())
}

fn validate_revision(revision: &str) -> Result<()> {
    if revision.is_empty()
        || revision.len() > 256
        || revision.starts_with('-')
        || !revision.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(byte, b'@' | b'.' | b'_' | b'/' | b'+' | b'^' | b'-')
        })
    {
        bail!("revision must be a bounded VCS name, commit ID, or simple ancestry expression");
    }
    Ok(())
}

fn revision_file(root: &Path, revision: &str, path: &str) -> Result<String> {
    if root.join(".jj").exists() && command_available("jj") {
        return capture(root, "jj", &["file", "show", "-r", revision, path]);
    }
    if root.join(".git").exists() && command_available("git") {
        let object = format!("{revision}:{path}");
        return capture(root, "git", &["show", &object]);
    }
    bail!("contract diff requires JJ or Git revision access")
}

fn inspect(root: &Path, manifest: &MincoManifest, as_json: bool) -> Result<()> {
    print_value(&inspect_value(root, manifest)?, as_json)
}

fn inspect_value(root: &Path, manifest: &MincoManifest) -> Result<serde_json::Value> {
    let contract = load_contract(root.join(&manifest.contract))?;
    let catalog = load_catalog(root, &manifest.plugin_catalog)?;
    let deployment = load_plan(root, manifest, None)?;
    let roadmap = load_roadmap(&root.join(&manifest.roadmap))?;
    let tasks = load_tasks(&root.join(&manifest.tasks))?;
    let manager = minco::default_plugin_manager()?;
    let selection = load_plugin_selection(manifest, &manager)?;
    let composed = manager.compose(&selection)?;
    Ok(json!({
        "application": manifest.name,
        "contract": {
            "title": contract.document.title,
            "version": contract.document.version,
            "sha256": contract.document.sha256,
            "operations": contract.document.operations,
        },
        "plugins": catalog.plugin,
        "registrations": composed.registration_provenance(),
        "deployment": deployment,
        "roadmap": roadmap,
        "tasks": tasks,
    }))
}

fn explain(root: &Path, manifest: &MincoManifest, operation_id: &str, as_json: bool) -> Result<()> {
    print_value(&explain_value(root, manifest, operation_id)?, as_json)
}

fn explain_value(
    root: &Path,
    manifest: &MincoManifest,
    operation_id: &str,
) -> Result<serde_json::Value> {
    let trace = manifest.operations.get(operation_id);
    let contract = trace
        .and_then(|value| value.contract.as_ref())
        .unwrap_or(&manifest.contract);
    let report = load_contract(root.join(contract))?;
    let operation = report
        .document
        .operations
        .iter()
        .find(|operation| operation.operation_id == operation_id)
        .with_context(|| format!("operation {operation_id} is not in the contract"))?;
    let generated = trace
        .and_then(|value| value.generated.as_ref())
        .or_else(|| (contract == &manifest.contract).then_some(&manifest.generated));
    let deployment = load_plan(root, manifest, None)?;
    let deployment_function = deployment.http_api_function_id();
    let deployment_trigger = deployment.http_api_trigger_id();
    Ok(json!({
        "operation": operation,
        "contract": contract,
        "generated": generated,
        "handler_module": trace.and_then(|value| value.handler.as_deref()),
        "application_module": trace.and_then(|value| value.application.as_deref()),
        "adapters": trace.map_or_else(Vec::new, |value| value.adapters.clone()),
        "tests": trace.map_or_else(Vec::new, |value| value.tests.clone()),
        "deployment_config": manifest.deployment_config,
        "deployment_function": deployment_function,
        "deployment_trigger": deployment_trigger,
    }))
}

fn deploy(
    root: &Path,
    manifest: &MincoManifest,
    command: DeployCommand,
    as_json: bool,
) -> Result<()> {
    match command {
        DeployCommand::Plan {
            input,
            output,
            stdout,
        } => {
            let plan = load_plan(root, manifest, input.config)?;
            ensure_plan_valid(&plan)?;
            if stdout {
                use std::io::Write as _;
                std::io::stdout().write_all(&canonical_json(&plan)?)?;
                return Ok(());
            }
            let output = output.unwrap_or_else(|| PathBuf::from("infra/aws/generated/plan.json"));
            let output = root.join(output);
            ensure_parent(&output)?;
            fs::write(&output, canonical_json(&plan)?)?;
            print_value(
                &json!({"plan": output, "diagnostics": plan.validate()}),
                as_json,
            )
        }
        DeployCommand::RenderSam { input, output } => {
            let plan = load_plan(root, manifest, input.config)?;
            ensure_plan_valid(&plan)?;
            let output = root.join(output);
            let code_uris = plan
                .functions
                .iter()
                .map(|function| {
                    let code_uri = template_relative_path(root, &output, &function.artifact_path)?;
                    Ok((
                        function.name.clone(),
                        code_uri.to_string_lossy().into_owned(),
                    ))
                })
                .collect::<Result<std::collections::BTreeMap<_, _>>>()?;
            let template = render_sam_with_code_uris(&plan, &code_uris)?;
            ensure_parent(&output)?;
            fs::write(&output, template)?;
            print_value(
                &json!({"template": output, "database_profile": plan.database.kind_name()}),
                as_json,
            )
        }
    }
}

fn template_relative_path(root: &Path, template: &Path, artifact: &str) -> Result<PathBuf> {
    let template_parent = template
        .parent()
        .context("deployment template output has no parent directory")?;
    let template_parent = template_parent
        .strip_prefix(root)
        .context("deployment template output must be inside the repository")?;
    let artifact = Path::new(artifact);
    let artifact = if artifact.is_absolute() {
        artifact
            .strip_prefix(root)
            .context("deployment artifact must be inside the repository")?
    } else {
        artifact
    };
    let valid_relative = |path: &Path| {
        path.components()
            .all(|component| matches!(component, std::path::Component::Normal(_)))
    };
    if !valid_relative(template_parent) || !valid_relative(artifact) {
        bail!("deployment paths must be normalized repository descendants");
    }
    let template_components = template_parent.components().collect::<Vec<_>>();
    let artifact_components = artifact.components().collect::<Vec<_>>();
    let common = template_components
        .iter()
        .zip(&artifact_components)
        .take_while(|(left, right)| left == right)
        .count();
    let mut relative = PathBuf::new();
    for _ in common..template_components.len() {
        relative.push("..");
    }
    for component in &artifact_components[common..] {
        relative.push(component.as_os_str());
    }
    Ok(relative)
}

fn cost(root: &Path, manifest: &MincoManifest, input: PlanInput, as_json: bool) -> Result<()> {
    let plan = load_plan(root, manifest, input.config)?;
    let estimate = estimate_database_cost(&plan.database);
    let runtime = estimate_runtime_cost(&plan);
    print_value(
        &json!({
            "database": estimate,
            "runtime": runtime,
            "database_profile": plan.database.kind_name(),
            "structural_diagnostics": plan.validate(),
            "overall_estimate_complete": estimate.complete && runtime.complete,
            "note": "The estimate exposes selected fixed/request-based resources, schedule wakes, worker connection pressure, and missing regional rates. It does not guess a complete cloud bill.",
        }),
        as_json,
    )
}

fn perf(root: &Path, manifest: &MincoManifest, input: PlanInput, as_json: bool) -> Result<()> {
    let plan = load_plan(root, manifest, input.config)?;
    let mut diagnostics = plan.validate();
    let mut artifacts = Vec::new();
    for function in &plan.functions {
        let artifact = root.join(&function.artifact_path);
        let digest = artifact
            .is_file()
            .then(|| FileDigest::from_rooted_path(root, &artifact))
            .transpose()?;
        if let Some(digest) = &digest
            && digest.bytes > plan.performance_policy.target_artifact_bytes
        {
            diagnostics.push(minco_plan::PlanDiagnostic {
                code: "MINCO-PERF-003".into(),
                severity: PlanSeverity::Warning,
                message: format!(
                    "function {} artifact is {} bytes; target is {}",
                    function.name, digest.bytes, plan.performance_policy.target_artifact_bytes
                ),
            });
        }
        artifacts.push(json!({
            "function_id": function.name,
            "role": function.role,
            "artifact": artifact,
            "digest": digest,
        }));
    }
    let primary_artifact = artifacts.first().context("plan has no function")?;
    print_value(
        &json!({
            "artifact": primary_artifact["artifact"],
            "artifact_bytes": primary_artifact["digest"]["bytes"],
            "artifacts": artifacts,
            "policy": plan.performance_policy,
            "diagnostics": diagnostics,
        }),
        as_json,
    )
}

fn architecture(root: &Path, manifest: &MincoManifest, as_json: bool) -> Result<()> {
    let report = validate_architecture(root, &manifest.architecture)?;
    print_value(&report, as_json)?;
    if report.findings.is_empty() {
        Ok(())
    } else {
        bail!("architecture dependency validation failed")
    }
}

fn roadmap_command(
    root: &Path,
    manifest: &MincoManifest,
    command: RoadmapCommand,
    as_json: bool,
) -> Result<()> {
    let roadmap = load_roadmap(&root.join(&manifest.roadmap))?;
    match command {
        RoadmapCommand::Status => print_value(&roadmap, as_json),
        RoadmapCommand::Render { format, output } => {
            let rendered = match format {
                DiagramFormat::Mermaid => render_roadmap_mermaid(&roadmap),
                DiagramFormat::Json => serde_json::to_string_pretty(&roadmap)?,
            };
            if let Some(output) = output {
                let output = root.join(output);
                ensure_parent(&output)?;
                fs::write(&output, &rendered)?;
                print_value(&json!({"output": output}), as_json)
            } else {
                println!("{rendered}");
                Ok(())
            }
        }
    }
}

fn task_command(
    root: &Path,
    manifest: &MincoManifest,
    command: TaskCommand,
    as_json: bool,
) -> Result<()> {
    let tasks = load_tasks(&root.join(&manifest.tasks))?;
    validate_task_graph(&tasks)?;
    match command {
        TaskCommand::List => print_value(&tasks, as_json),
        TaskCommand::Ready => print_value(&ready_tasks(&tasks), as_json),
        TaskCommand::Next => {
            let next = ready_tasks(&tasks)
                .into_iter()
                .next()
                .context("no task is currently ready")?;
            print_value(next, as_json)
        }
        TaskCommand::Show { id } => {
            let task = tasks
                .iter()
                .find(|task| task.id == id)
                .with_context(|| format!("unknown task {id}"))?;
            print_value(task, as_json)
        }
        TaskCommand::Graph { output } => {
            let graph = render_task_mermaid(&tasks);
            if let Some(output) = output {
                let output = root.join(output);
                ensure_parent(&output)?;
                fs::write(&output, &graph)?;
                print_value(&json!({"output": output}), as_json)
            } else {
                println!("{graph}");
                Ok(())
            }
        }
        TaskCommand::Verify { id } => {
            let task = tasks
                .iter()
                .find(|task| task.id == id)
                .with_context(|| format!("unknown task {id}"))?;
            let mut results = Vec::new();
            for command in &task.checks {
                let result = run_shell(root, command, !as_json)?;
                if !result.success {
                    if as_json {
                        results.push(result);
                        print_value(&results, true)?;
                    }
                    bail!("task check failed: {command}");
                }
                results.push(result);
            }
            print_value(&results, as_json)
        }
    }
}

fn plugin_command(
    root: &Path,
    manifest: &MincoManifest,
    command: PluginCommand,
    as_json: bool,
) -> Result<()> {
    match command {
        PluginCommand::List => {
            let catalog = load_catalog(root, &manifest.plugin_catalog)?;
            print_value(
                &json!({"catalog": catalog, "selection": manifest.plugins}),
                as_json,
            )
        }
        PluginCommand::Enable { id } => {
            set_plugin_state(root, &id, true)?;
            print_value(&json!({"plugin": id, "enabled": true}), as_json)
        }
        PluginCommand::Disable { id } => {
            set_plugin_state(root, &id, false)?;
            print_value(&json!({"plugin": id, "enabled": false}), as_json)
        }
        PluginCommand::New { id } => {
            scaffold_plugin(root, &id)?;
            print_value(&json!({"plugin": id, "created": true}), as_json)
        }
        PluginCommand::Validate => {
            let catalog = load_catalog(root, &manifest.plugin_catalog)?;
            let findings = validate_catalog(root, &catalog)?;
            print_value(&findings, as_json)?;
            if !findings.is_empty() {
                bail!("plugin catalog validation failed");
            }
            Ok(())
        }
    }
}

fn test_command(
    root: &Path,
    manifest: &MincoManifest,
    command: TestCommand,
    as_json: bool,
) -> Result<()> {
    let fallback_unit = vec!["cargo test --workspace --lib --all-features".to_owned()];
    let fallback_feature = vec!["cargo test --workspace --tests --all-features".to_owned()];
    let fallback_e2e = if root.join("scripts/test/e2e.sh").is_file() {
        vec!["scripts/test/e2e.sh".to_owned()]
    } else {
        Vec::new()
    };
    let fallback_all = {
        let mut commands = vec!["cargo test --workspace --all-targets --all-features".to_owned()];
        commands.extend(fallback_e2e.clone());
        commands
    };

    let commands = match command {
        TestCommand::Unit => configured_or(&manifest.commands.unit, fallback_unit),
        TestCommand::Feature => configured_or(&manifest.commands.feature, fallback_feature),
        TestCommand::E2e => configured_or(&manifest.commands.e2e, fallback_e2e),
        TestCommand::All => configured_or(&manifest.commands.all, fallback_all),
    };
    if commands.is_empty() {
        bail!("no test command is configured for this test level");
    }

    let mut results = Vec::new();
    for command in commands {
        let result = run_shell(root, &command, !as_json)?;
        if !result.success {
            if as_json {
                results.push(result);
                print_value(&results, true)?;
            }
            bail!("test command failed: {command}");
        }
        results.push(result);
    }
    print_value(&results, as_json)
}

fn configured_or(configured: &[String], fallback: Vec<String>) -> Vec<String> {
    if configured.is_empty() {
        fallback
    } else {
        configured.to_vec()
    }
}

fn release_command(
    root: &Path,
    manifest: &MincoManifest,
    command: ReleaseCommand,
    as_json: bool,
) -> Result<()> {
    match command {
        ReleaseCommand::Create {
            artifact,
            plan,
            template,
            output,
        } => {
            let artifact = root.join(artifact);
            let plan = root.join(plan);
            let template = root.join(template);
            if !artifact.is_file() {
                bail!("release artifact {} does not exist", artifact.display());
            }
            if !plan.is_file() {
                bail!("deployment plan {} does not exist", plan.display());
            }
            if !template.is_file() {
                bail!("deployment template {} does not exist", template.display());
            }
            let source_change = vcs::source_change(root)?;
            let short_change = source_change.chars().take(12).collect::<String>();
            let release_suffix = if short_change.is_empty() {
                Uuid::now_v7()
                    .simple()
                    .to_string()
                    .chars()
                    .take(12)
                    .collect::<String>()
            } else {
                short_change
            };
            let release_id = format!("{}.{}", Utc::now().format("%Y-%m-%d"), release_suffix);
            let rust_version = if command_available("rustc") {
                process::capture(root, "rustc", &["--version"])?
            } else {
                "unverified".into()
            };
            let mut migration_paths = Vec::new();
            for migration_root in &manifest.migrations.roots {
                migration_paths.extend(collect_files(&root.join(migration_root), "sql")?);
            }
            migration_paths.sort();
            migration_paths.dedup();
            let migrations = migration_paths
                .into_iter()
                .map(|path| FileDigest::from_rooted_path(root, path))
                .collect::<Result<Vec<_>, _>>()?;
            let release = ReleaseManifest {
                schema_version: 2,
                release_id,
                created_at: Utc::now(),
                source_change,
                rust_version,
                minco_version: env!("CARGO_PKG_VERSION").into(),
                artifact: FileDigest::from_rooted_path(root, &artifact)?,
                contract: FileDigest::from_rooted_path(root, root.join(&manifest.contract))?,
                migration_set: migrations,
                cargo_lock: root
                    .join("Cargo.lock")
                    .is_file()
                    .then(|| FileDigest::from_rooted_path(root, root.join("Cargo.lock")))
                    .transpose()?,
                deployment_plan: FileDigest::from_rooted_path(root, &plan)?,
                deployment_template: FileDigest::from_rooted_path(root, &template)?,
            };
            let output = root.join(output);
            ensure_parent(&output)?;
            release.write_json(&output)?;
            print_value(&release, as_json)
        }
        ReleaseCommand::Verify { manifest } => {
            let release = ReleaseManifest::read_json(root.join(manifest))?;
            release.verify_at(root)?;
            print_value(
                &json!({"verified": true, "release_id": release.release_id}),
                as_json,
            )
        }
    }
}

fn update_command(root: &Path, command: UpdateCommand, as_json: bool) -> Result<()> {
    let report = match command {
        UpdateCommand::Check => update::check(root)?,
        UpdateCommand::Apply {
            yes,
            toolchain,
            dependencies,
            run_checks,
        } => update::apply(root, yes, toolchain, dependencies, run_checks)?,
    };
    print_value(&report, as_json)
}

fn vcs_command(root: &Path, command: VcsCommand, as_json: bool) -> Result<()> {
    match command {
        VcsCommand::Init => {
            vcs::initialize(root)?;
            print_value(&json!({"initialized": true, "colocated": true}), as_json)
        }
        VcsCommand::Status => {
            let status = vcs::status(root)?;
            if as_json {
                print_value(&json!({"status": status}), true)
            } else {
                println!("{status}");
                Ok(())
            }
        }
        VcsCommand::TaskStart { id, destination } => {
            let result = vcs::start_task(root, &id, destination)?;
            print_value(&result, as_json)
        }
        VcsCommand::TaskFinish { id, message, push } => {
            vcs::finish_task(root, &id, &message, push)?;
            print_value(
                &json!({"task": id, "finished": true, "pushed": push}),
                as_json,
            )
        }
    }
}

fn load_plan(
    root: &Path,
    manifest: &MincoManifest,
    config: Option<PathBuf>,
) -> Result<DeploymentPlan> {
    let contract = load_contract(root.join(&manifest.contract))?;
    if !contract.is_valid() {
        bail!("the OpenAPI contract is invalid");
    }
    let config_path = root.join(config.unwrap_or_else(|| manifest.deployment_config.clone()));
    let config: DeploymentConfig = toml::from_str(
        &fs::read_to_string(&config_path)
            .with_context(|| format!("read {}", config_path.display()))?,
    )
    .with_context(|| format!("parse {}", config_path.display()))?;
    let application_graph = load_application_graph(manifest)?;
    Ok(config.into_plan_with_graph(&contract.document, application_graph))
}

fn load_application_graph(manifest: &MincoManifest) -> Result<ApplicationGraph> {
    let manager = minco::default_plugin_manager()?;
    let selection = load_plugin_selection(manifest, &manager)?;
    Ok(manager.build_graph(&selection)?)
}

pub(crate) fn load_plugin_selection(
    manifest: &MincoManifest,
    manager: &PluginManager,
) -> Result<PluginSelection> {
    let registered = manager
        .descriptors()
        .into_iter()
        .map(|descriptor| descriptor.id)
        .collect::<std::collections::BTreeSet<_>>();
    let mut selection = PluginSelection::default();
    for id in &manifest.plugins.enabled {
        let id = PluginId::new(id.clone())?;
        if !registered.contains(&id) {
            bail!("enabled plugin {id} is not statically linked into the deployment planner");
        }
        selection.enabled.insert(id);
    }
    for id in &manifest.plugins.disabled {
        let id = PluginId::new(id.clone())?;
        if registered.contains(&id) {
            selection.disabled.insert(id);
        }
    }
    for (id, configuration) in &manifest.plugins.configuration {
        let id = PluginId::new(id.clone())?;
        if !registered.contains(&id) {
            bail!("configured plugin {id} is not statically linked into the deployment planner");
        }
        selection
            .configuration
            .insert(id, serde_json::to_value(configuration)?);
    }
    Ok(selection)
}

fn ensure_plan_valid(plan: &DeploymentPlan) -> Result<()> {
    let errors = plan
        .validate()
        .into_iter()
        .filter(|diagnostic| diagnostic.severity == PlanSeverity::Error)
        .collect::<Vec<_>>();
    if errors.is_empty() {
        Ok(())
    } else {
        bail!(
            "deployment plan failed policy validation: {}",
            serde_json::to_string_pretty(&errors)?
        )
    }
}

pub(crate) fn print_value<T: Serialize + ?Sized>(value: &T, as_json: bool) -> Result<()> {
    if as_json {
        println!("{}", serde_json::to_string_pretty(value)?);
    } else {
        let value = serde_json::to_value(value)?;
        match value {
            serde_json::Value::String(value) => println!("{value}"),
            other => println!("{}", serde_json::to_string_pretty(&other)?),
        }
    }
    Ok(())
}

fn canonical_json<T: Serialize + ?Sized>(value: &T) -> Result<Vec<u8>> {
    let value = serde_json::to_value(value)?;
    let mut rendered = serde_json::to_vec_pretty(&value)?;
    rendered.push(b'\n');
    Ok(rendered)
}

fn ensure_parent(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    Ok(())
}

fn collect_files(root: &Path, extension: &str) -> Result<Vec<PathBuf>> {
    let mut output = Vec::new();
    if !root.exists() {
        return Ok(output);
    }
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            output.extend(collect_files(&path, extension)?);
        } else if path.extension().and_then(|value| value.to_str()) == Some(extension) {
            output.push(path);
        }
    }
    output.sort();
    Ok(output)
}

#[allow(dead_code)]
fn _assert_cost_estimate_is_serializable(value: &DatabaseCostEstimate) -> Result<String> {
    Ok(serde_json::to_string(value)?)
}

#[cfg(test)]
mod cli_argument_tests {
    use super::*;

    #[test]
    fn cargo_subcommand_token_is_removed() {
        let values = normalize_cargo_subcommand_args(
            ["cargo-minco", "minco", "doctor"]
                .into_iter()
                .map(OsString::from)
                .collect(),
        );
        assert_eq!(
            values,
            vec![OsString::from("cargo-minco"), OsString::from("doctor")]
        );
    }

    #[test]
    fn contract_aware_operation_generator_has_dry_run_json_cli_shape() {
        let cli = Cli::try_parse_from([
            "cargo-minco",
            "make",
            "operation",
            "placeOrder",
            "--dry-run",
            "--json",
        ])
        .expect("operation generator should expose a dry-run JSON plan");

        assert!(matches!(
            cli.command,
            Command::Make(MakeCommand::Operation(generator_cmd::OperationArgs {
                operation_id,
                dry_run: true,
            })) if operation_id == "placeOrder"
        ));
        assert!(cli.json);
    }

    #[test]
    fn contract_diff_has_an_explicit_revision_and_json_shape() {
        let cli = Cli::try_parse_from([
            "cargo-minco",
            "contract",
            "diff",
            "--against",
            "main",
            "--json",
        ])
        .expect("contract diff arguments");

        assert!(matches!(
            cli.command,
            Command::Contract(ContractCommand::Diff { against }) if against == "main"
        ));
        assert!(cli.json);
    }

    #[test]
    fn contract_diff_rejects_option_like_or_shell_shaped_revisions() {
        assert!(validate_revision("-c").is_err());
        assert!(validate_revision("--config=evil").is_err());
        assert!(validate_revision("main;touch-pwned").is_err());
        assert!(validate_revision("main branch").is_err());
        assert!(validate_revision("main").is_ok());
        assert!(validate_revision("@-").is_ok());
        assert!(validate_revision("release/0.3.1^").is_ok());
    }

    #[test]
    fn upgrade_report_has_a_deterministic_json_cli_shape() {
        let cli = Cli::try_parse_from(["cargo-minco", "upgrade", "report", "--json"])
            .expect("upgrade report arguments");

        assert!(matches!(
            cli.command,
            Command::Upgrade(UpgradeCommand::Report)
        ));
        assert!(cli.json);
    }

    #[test]
    fn direct_binary_arguments_are_unchanged() {
        let values = normalize_cargo_subcommand_args(
            ["cargo-minco", "doctor"]
                .into_iter()
                .map(OsString::from)
                .collect(),
        );
        assert_eq!(
            values,
            vec![OsString::from("cargo-minco"), OsString::from("doctor")]
        );
    }

    #[test]
    fn development_command_parses_every_explicit_topology_override() {
        let cli = Cli::try_parse_from([
            "cargo-minco",
            "dev",
            "--dry-run",
            "--environment",
            "local",
            "--profile",
            "sqlite",
            "--no-migrate",
            "--seed",
            "demo",
            "--with-worker",
            "email",
            "--without-worker",
            "events",
            "--frontend",
            "--port",
            "31000",
            "--rustack-port",
            "45666",
            "--json",
        ])
        .expect("development arguments");

        assert!(matches!(
            cli.command,
            Command::Dev(DevArgs {
                dry_run: true,
                environment: Some(environment),
                profile: Some(profile),
                no_migrate: true,
                seed: Some(seed),
                with_workers,
                without_workers,
                frontend: true,
                no_frontend: false,
                port: Some(31_000),
                rustack_port: Some(45_666),
            }) if environment == "local"
                && profile == "sqlite"
                && seed == "demo"
                && with_workers == ["email"]
                && without_workers == ["events"]
        ));
        assert!(cli.json);
    }

    #[test]
    fn development_database_override_rejects_remote_hosts() {
        validate_local_postgres_url("postgres://minco:minco@127.0.0.1:55432/minco_orders")
            .expect("loopback PostgreSQL URL");

        let error = validate_local_postgres_url(
            "postgres://operator:secret@database.example.invalid/orders",
        )
        .expect_err("remote development database must fail");
        assert!(error.to_string().contains("loopback"));
        assert!(!error.to_string().contains("operator"));
        assert!(!error.to_string().contains("secret"));
    }

    #[test]
    fn typed_configuration_commands_have_stable_cli_shapes() {
        let check = Cli::try_parse_from(["cargo-minco", "config", "check", "--environment", "dev"])
            .expect("config check arguments");
        assert!(matches!(
            check.command,
            Command::Config(ConfigCommand::Check(config_cmd::ConfigEnvironmentArgs {
                environment,
                ..
            })) if environment == "dev"
        ));

        let difference = Cli::try_parse_from([
            "cargo-minco",
            "config",
            "diff",
            "--from",
            "dev",
            "--to",
            "production",
        ])
        .expect("config diff arguments");
        assert!(matches!(
            difference.command,
            Command::Config(ConfigCommand::Diff { from, to })
                if from == "dev" && to == "production"
        ));
    }

    #[test]
    fn database_lifecycle_commands_keep_planning_read_only_and_migration_explicit() {
        let plan = Cli::try_parse_from(["cargo-minco", "db", "plan", "--set", "orders-sqlite"])
            .expect("db plan arguments");
        assert!(matches!(
            plan.command,
            Command::Db(DbCommand::Plan { set: Some(set) }) if set == "orders-sqlite"
        ));

        let status = Cli::try_parse_from([
            "cargo-minco",
            "db",
            "status",
            "--set",
            "orders-sqlite",
            "--database-url-env",
            "MINCO_TEST_DATABASE_URL",
        ])
        .expect("db status arguments");
        assert!(matches!(
            status.command,
            Command::Db(DbCommand::Status(DbTargetArgs {
                set: Some(set),
                database_url_env: Some(database_url_env),
            })) if set == "orders-sqlite" && database_url_env == "MINCO_TEST_DATABASE_URL"
        ));

        let migrate = Cli::try_parse_from([
            "cargo-minco",
            "db",
            "migrate",
            "--set",
            "orders-sqlite",
            "--database-url-env",
            "MINCO_TEST_DATABASE_URL",
            "--expected-plan-digest",
            "abc123",
            "--receipt",
            "target/migration-receipt.json",
        ])
        .expect("db migrate arguments");
        assert!(matches!(
            migrate.command,
            Command::Db(DbCommand::Migrate(DbMigrateArgs {
                set,
                database_url_env,
                expected_plan_digest,
                allow_destructive: false,
                ..
            })) if set == "orders-sqlite"
                && database_url_env == "MINCO_TEST_DATABASE_URL"
                && expected_plan_digest == "abc123"
        ));
    }

    #[test]
    fn seed_commands_make_dry_run_and_bootstrap_authority_explicit() {
        let demo = Cli::try_parse_from([
            "cargo-minco",
            "db",
            "seed",
            "--profile",
            "demo",
            "--dry-run",
        ])
        .expect("demo seed dry-run arguments");
        assert!(matches!(
            demo.command,
            Command::Db(DbCommand::Seed(DbSeedArgs {
                profile: Some(profile),
                environment,
                dry_run: true,
                ..
            })) if profile == "demo" && environment.is_none()
        ));

        let bootstrap = Cli::try_parse_from([
            "cargo-minco",
            "db",
            "seed",
            "--profile",
            "bootstrap",
            "--environment",
            "production",
            "--set",
            "orders-postgres-seeds",
            "--database-url-env",
            "MINCO_SEED_DATABASE_URL",
            "--expected-plan-digest",
            "abc123",
            "--receipt",
            "target/minco/bootstrap-receipt.json",
            "--authorize-bootstrap",
            "production",
        ])
        .expect("bootstrap seed arguments");
        assert!(matches!(
            bootstrap.command,
            Command::Db(DbCommand::Seed(DbSeedArgs {
                profile: Some(profile),
                environment,
                authorize_bootstrap: Some(authority),
                dry_run: false,
                ..
            })) if profile == "bootstrap"
                && environment.as_deref() == Some("production")
                && authority == "production"
        ));
    }

    #[test]
    fn target_inspection_requires_a_set_and_never_accepts_a_database_url_value() {
        assert!(
            Cli::try_parse_from([
                "cargo-minco",
                "db",
                "status",
                "--database-url-env",
                "MINCO_TEST_DATABASE_URL",
            ])
            .is_err()
        );
        assert!(
            Cli::try_parse_from([
                "cargo-minco",
                "db",
                "status",
                "--set",
                "orders-sqlite",
                "--database-url",
                "sqlite://secret.db",
            ])
            .is_err()
        );
    }

    #[test]
    fn sam_artifact_path_is_relative_to_the_template() {
        let root = Path::new("/repo");
        let template = root.join("infra/aws/generated/template.yaml");
        let relative =
            template_relative_path(root, &template, "target/lambda/orders-lambda/bootstrap.zip")
                .expect("relative path");
        assert_eq!(
            relative,
            PathBuf::from("../../../target/lambda/orders-lambda/bootstrap.zip")
        );
        assert!(
            template_relative_path(
                root,
                &PathBuf::from("/outside/template.yaml"),
                "target/lambda/orders-lambda/bootstrap.zip",
            )
            .is_err()
        );
    }

    #[test]
    fn generated_json_is_key_sorted_and_newline_terminated() {
        let value = json!({
            "z": {"b": 1, "a": 2},
            "a": 3,
        });
        let rendered = String::from_utf8(canonical_json(&value).unwrap()).unwrap();
        assert_eq!(
            rendered,
            "{\n  \"a\": 3,\n  \"z\": {\n    \"a\": 2,\n    \"b\": 1\n  }\n}\n"
        );
    }

    #[test]
    fn explain_traces_an_operation_owned_by_a_plugin_contract() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .expect("workspace root");
        let manifest = MincoManifest::load(&root).expect("workspace manifest");
        let value =
            explain_value(&root, &manifest, "createFeedback").expect("Feedback operation trace");

        assert_eq!(value["operation"]["operation_id"], "createFeedback");
        assert_eq!(
            value["contract"],
            "plugins/minco-plugin-feedback/openapi/feedback.openapi.yaml"
        );
        assert_eq!(
            value["handler_module"],
            "plugins/minco-plugin-feedback/src/http.rs#create_feedback"
        );
        assert!(value["generated"].is_null());
        assert_eq!(value["deployment_function"], "api");
    }

    #[test]
    fn deployment_plan_contains_the_manifest_selected_plugin_graph() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .expect("workspace root");
        let manifest = MincoManifest::load(&root).expect("workspace manifest");

        let plan = load_plan(&root, &manifest, None).expect("deployment plan");
        let ids = plan
            .application_graph
            .plugins
            .iter()
            .map(|plugin| plugin.id.as_str())
            .collect::<Vec<_>>();

        assert_eq!(ids, ["health", "idempotency", "observability"]);
        assert_eq!(plan.local_aws_services, ["ssm", "sts"]);
    }

    #[test]
    fn configured_plugin_resources_change_the_plan_service_projection() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .expect("workspace root");
        let mut manifest = MincoManifest::load(&root).expect("workspace manifest");
        manifest.plugins.enabled.insert("static-site".into());

        let plan = load_plan(&root, &manifest, None).expect("deployment plan");

        assert!(
            plan.application_graph
                .resources
                .contains_key("static-site-bucket")
        );
        assert_eq!(plan.local_aws_services, ["s3", "ssm", "sts"]);
    }

    #[test]
    fn inspection_contains_only_bounded_registration_metadata() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .expect("workspace root");
        let manifest = MincoManifest::load(&root).expect("workspace manifest");

        let value = inspect_value(&root, &manifest).expect("inspection value");
        let registrations = &value["registrations"];
        let services = registrations["services"]
            .as_array()
            .expect("service registrations");

        assert_eq!(
            services
                .iter()
                .map(|service| service["owner"]["plugin_id"].as_str().unwrap())
                .collect::<Vec<_>>(),
            ["health", "idempotency", "observability"]
        );
        assert!(services.iter().all(|service| {
            service
                .as_object()
                .is_some_and(|value| value.keys().eq(["owner", "rust_type"].iter().copied()))
        }));
        assert_eq!(registrations["contributions"], json!([]));

        let serialized = serde_json::to_string(registrations).unwrap();
        assert!(!serialized.contains("service_count"));
        assert!(!serialized.contains("configuration"));
        assert!(!serialized.contains("Debug"));
    }
}
