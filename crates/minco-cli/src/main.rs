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
use minco_deploy_aws::{
    ChangeSetReceipt, ChangeSetReceiptInput, ChangeSetType, CloudFormationChangeSet,
    DeploymentTarget, DeploymentTargetCatalog, DriftState, EnvironmentExpectation,
    EnvironmentObservation, MigrationState as DeploymentMigrationState, SourceState, StackDrift,
    caller_role_arn, verify_guards,
};
use minco_dev::{
    DevDatabase, DevEvent, DevGraph, DevOptions, DevPlan, DevStream, ServiceKind, Supervisor,
};
use minco_plan::{
    DatabaseCostEstimate, DatabaseDeployment, DeploymentConfig, DeploymentPlan, FunctionRole,
    Severity as PlanSeverity, TriggerPlan, estimate_database_cost, estimate_runtime_cost,
    render_sam_with_code_uris,
};
use minco_release::{
    DatabasePlanBinding, DatabasePlanKind, DatabaseSourceDigests, DeploymentReceipt,
    DeploymentReceiptInput, FileDigest, FunctionArtifact, ReleaseEnvironment, ReleaseManifest,
    ReleaseManifestInput, ToolchainIdentity,
};
use new_cmd::{DatabaseChoice, NewProjectOptions, VcsChoice, create_project};
use plugin_cmd::{load_catalog, scaffold_plugin, set_plugin_state, validate_catalog};
use process::{capture, command_available, run_shell};
use roadmap::{
    load_roadmap, load_tasks, ready_tasks, render_roadmap_mermaid, render_task_mermaid,
    validate_task_graph,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    ffi::{OsStr, OsString},
    fs,
    path::{Path, PathBuf},
    process::{Command as ProcessCommand, Output, Stdio},
    thread,
    time::Duration,
};

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
    /// Build and seal an exact, independently verifiable release package.
    Package(PackageArgs),
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

#[derive(Debug, Clone, Args)]
struct PackageArgs {
    #[arg(long)]
    config: Option<PathBuf>,
    #[arg(long)]
    environment: Option<String>,
    #[arg(long, default_value = "target/minco/plan.json")]
    plan: PathBuf,
    #[arg(long, default_value = "target/minco/template.yaml")]
    template: PathBuf,
    #[arg(long, default_value = "target/minco/release.json")]
    output: PathBuf,
    /// Repository-relative detached signature or provenance statement.
    #[arg(long = "attestation")]
    attestations: Vec<PathBuf>,
}

#[derive(Debug, Clone, Args)]
struct ChangeSetArgs {
    #[arg(long, default_value = "infra/aws/deployment-targets.toml")]
    target_config: PathBuf,
    #[arg(long)]
    environment: Option<String>,
    #[arg(long, default_value = "target/minco/release.json")]
    manifest: PathBuf,
    #[arg(long, default_value = "target/minco/change-set.json")]
    output: PathBuf,
    #[arg(long)]
    approve_release_digest: Option<String>,
    #[arg(long)]
    dry_run: bool,
}

#[derive(Debug, Clone, Args)]
struct ApplyArgs {
    #[arg(long, default_value = "target/minco/change-set.json")]
    changeset: PathBuf,
    #[arg(long, default_value = "target/minco/migration-plan.json")]
    migration_plan: PathBuf,
    #[arg(long, default_value = "target/minco/migration-receipt.json")]
    migration_receipt: PathBuf,
    #[arg(long, default_value = "target/minco/deployment-receipt.json")]
    receipt: PathBuf,
    #[arg(long)]
    approve_changeset_digest: Option<String>,
    #[arg(long)]
    dry_run: bool,
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
    Changeset(ChangeSetArgs),
    Apply(ApplyArgs),
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
        Command::Package(args) => package_command(&root, &manifest, args, as_json),
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
        DeployCommand::Changeset(args) => change_set_command(root, &args, as_json),
        DeployCommand::Apply(args) => apply_change_set_command(root, &args, as_json),
    }
}

fn change_set_command(root: &Path, args: &ChangeSetArgs, as_json: bool) -> Result<()> {
    validate_project_file(root, &args.target_config, "deployment target configuration")?;
    if root.join(&args.manifest).exists() {
        validate_project_file(root, &args.manifest, "release manifest")?;
    } else if args.manifest.is_absolute()
        || !args
            .manifest
            .components()
            .all(|component| matches!(component, std::path::Component::Normal(_)))
    {
        bail!("release manifest must be a project-relative path");
    }
    let catalog =
        DeploymentTargetCatalog::from_toml(&fs::read_to_string(root.join(&args.target_config))?)?;
    let selected = catalog.select(args.environment.as_deref())?;
    let review_output = package_output_path(root, &args.output)?;

    let mut blockers = Vec::new();
    if !selected.target.enabled {
        blockers.push("target_disabled");
    }
    let release = if root.join(&args.manifest).is_file() {
        if let Ok(release) = ReleaseManifest::read_json(root.join(&args.manifest))
            .and_then(|release| release.verify_at(root).map(|()| release))
        {
            if release.environment.environment != selected.environment
                || release.environment.region != selected.target.expected_region
            {
                blockers.push("release_environment_mismatch");
            }
            match vcs::source_snapshot(root) {
                Ok(source) if source.change == release.source_change => {}
                Ok(_) => blockers.push("source_mismatch"),
                Err(_) => blockers.push("source_unproved"),
            }
            match args.approve_release_digest.as_deref() {
                None => blockers.push("release_approval_missing"),
                Some(digest) if digest != release.release_digest => {
                    blockers.push("release_approval_mismatch");
                }
                Some(_) => {}
            }
            Some(release)
        } else {
            blockers.push("release_manifest_invalid");
            None
        }
    } else {
        blockers.push("release_manifest_missing");
        if args.approve_release_digest.is_none() {
            blockers.push("release_approval_missing");
        }
        None
    };
    if root.join(&args.output).exists() {
        blockers.push("review_output_exists");
    }

    let plan = json!({
        "schema_version": 1,
        "operation": "create_change_set",
        "dry_run": args.dry_run,
        "external_aws_contact": false,
        "infrastructure_apply": false,
        "target_config": args.target_config,
        "target": {
            "environment": selected.environment,
            "enabled": selected.target.enabled,
            "expected_account_id": selected.target.expected_account_id,
            "expected_region": selected.target.expected_region,
            "expected_role_arn": selected.target.expected_role_arn,
            "stack_name": selected.target.stack_name,
            "artifact_bucket": selected.target.artifact_bucket,
        },
        "release_manifest": args.manifest,
        "release": release.as_ref().map(|release| json!({
            "release_id": release.release_id,
            "release_digest": release.release_digest,
            "source_change": release.source_change,
        })),
        "review_output": args.output,
        "guard_requirements": [
            "exact_source",
            "verified_release",
            "expected_account",
            "expected_region",
            "expected_role",
            "drift_in_sync_or_new_stack",
            "release_digest_approval",
        ],
        "steps": [
            "verify_local_source_and_release",
            "verify_caller_identity",
            "inspect_stack_and_drift",
            "upload_exact_release_artifacts",
            "create_unexecuted_change_set",
            "classify_provider_changes",
            "write_immutable_review_receipt",
        ],
        "blockers": blockers,
    });
    if args.dry_run {
        return print_value(&plan, as_json);
    }
    if !blockers.is_empty() {
        bail!("change-set guards failed: {}", blockers.join(", "));
    }
    let release = release.context("verified release is required")?;
    create_change_set(
        root,
        args,
        &selected.target,
        &selected.environment,
        &release,
        &review_output,
        as_json,
    )
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct AwsCallerIdentity {
    account: String,
    arn: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct AwsDescribeStacks {
    stacks: Vec<AwsStack>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct AwsStack {
    stack_status: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct AwsDriftDetection {
    stack_drift_detection_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct AwsDriftStatus {
    stack_drift_detection_status: String,
    stack_drift_status: Option<String>,
    detection_status_reason: Option<String>,
}

#[allow(clippy::too_many_arguments)]
fn create_change_set(
    root: &Path,
    args: &ChangeSetArgs,
    target: &DeploymentTarget,
    environment: &str,
    release: &ReleaseManifest,
    review_output: &Path,
    as_json: bool,
) -> Result<()> {
    if !command_available("aws") || !command_available("sam") {
        bail!("live change-set creation requires both `aws` and `sam`");
    }

    let identity: AwsCallerIdentity = aws_json(
        root,
        &target.expected_region,
        "inspect AWS caller identity",
        &["sts", "get-caller-identity"],
    )?;
    let role_arn = caller_role_arn(&identity.arn).context(
        "AWS caller identity must be the exact configured IAM role or an assumed-role session",
    )?;
    let (change_set_type, drift) = inspect_stack_and_drift(root, target)?;
    let source = vcs::source_snapshot(root)?;

    verify_guards(
        &EnvironmentExpectation {
            account_id: target.expected_account_id.clone(),
            region: target.expected_region.clone(),
            environment: environment.to_owned(),
            role_arn: target.expected_role_arn.clone(),
            release_id: release.release_id.clone(),
            release_digest: release.release_digest.clone(),
            configuration_digest: release.configuration_digest.clone(),
            migration_plan_digest: None,
        },
        &EnvironmentObservation {
            account_id: identity.account,
            region: target.expected_region.clone(),
            environment: release.environment.environment.clone(),
            role_arn,
            release_id: release.release_id.clone(),
            release_digest: release.release_digest.clone(),
            release_verified: true,
            configuration_digest: release.configuration_digest.clone(),
            drift: DriftState::Clean,
            migration: DeploymentMigrationState::NotRequired,
            source: if source.change == release.source_change {
                SourceState::Clean
            } else {
                SourceState::Dirty
            },
            operator_approval_digest: args.approve_release_digest.clone(),
        },
    )?;

    run_cloud_output(
        root,
        "aws",
        "verify the pre-existing artifact bucket",
        &[
            "s3api".into(),
            "head-bucket".into(),
            "--bucket".into(),
            target.artifact_bucket.clone(),
            "--region".into(),
            target.expected_region.clone(),
        ],
    )?;

    let packaged_template_relative = PathBuf::from(format!(
        "target/minco/change-sets/{}/packaged-template.yaml",
        release.release_id
    ));
    let packaged_template = package_output_path(root, &packaged_template_relative)?;
    ensure_parent(&packaged_template)?;
    let template = root.join(&release.deployment_template.path);
    run_cloud_output(
        root,
        "sam",
        "package the exact release template",
        &[
            "package".into(),
            "--template-file".into(),
            template.display().to_string(),
            "--s3-bucket".into(),
            target.artifact_bucket.clone(),
            "--s3-prefix".into(),
            format!("minco/releases/{}", release.release_id),
            "--output-template-file".into(),
            packaged_template.display().to_string(),
            "--region".into(),
            target.expected_region.clone(),
            "--no-progressbar".into(),
        ],
    )?;
    if vcs::source_snapshot(root)?.change != release.source_change {
        bail!("source changed while packaging the reviewed release");
    }
    release.verify_at(root)?;
    let current_catalog =
        DeploymentTargetCatalog::from_toml(&fs::read_to_string(root.join(&args.target_config))?)?;
    let current_target = current_catalog.select(Some(environment))?;
    if current_target.target != *target {
        bail!("deployment target changed while preparing the change set");
    }
    let release_manifest = FileDigest::from_rooted_path(root, root.join(&args.manifest))?;
    let target_config = FileDigest::from_rooted_path(root, root.join(&args.target_config))?;
    let packaged_template_digest = FileDigest::from_rooted_path(root, &packaged_template)?;

    let change_set_name = format!("minco-{}", &release.release_digest[..24]);
    let mut create_args = vec![
        "cloudformation".into(),
        "create-change-set".into(),
        "--stack-name".into(),
        target.stack_name.clone(),
        "--change-set-name".into(),
        change_set_name.clone(),
        "--change-set-type".into(),
        aws_change_set_type(change_set_type).into(),
        "--template-body".into(),
        format!("file://{}", packaged_template.display()),
        "--capabilities".into(),
        "CAPABILITY_IAM".into(),
        "--client-token".into(),
        release.release_digest.clone(),
        "--description".into(),
        format!("Minco reviewed release {}", release.release_id),
        "--parameters".into(),
        format!(
            "ParameterKey=DatabaseUrlParameterName,ParameterValue={}",
            target.database_url_parameter_name
        ),
        format!(
            "ParameterKey=DatabaseUrlKmsKeyArn,ParameterValue={}",
            target.database_kms_key_arn.as_deref().unwrap_or_default()
        ),
        format!(
            "ParameterKey=LambdaSubnetIds,ParameterValue={}",
            target.lambda_subnet_ids.join(",")
        ),
        format!(
            "ParameterKey=LambdaSecurityGroupIds,ParameterValue={}",
            target.lambda_security_group_ids.join(",")
        ),
        "--tags".into(),
        format!("Key=MincoEnvironment,Value={environment}"),
        format!("Key=MincoReleaseId,Value={}", release.release_id),
        format!("Key=MincoReleaseDigest,Value={}", release.release_digest),
        "--region".into(),
        target.expected_region.clone(),
        "--output".into(),
        "json".into(),
    ];
    run_cloud_output(
        root,
        "aws",
        "create the unexecuted CloudFormation change set",
        &create_args,
    )?;

    create_args.clear();
    run_cloud_output(
        root,
        "aws",
        "wait for CloudFormation change-set creation",
        &[
            "cloudformation".into(),
            "wait".into(),
            "change-set-create-complete".into(),
            "--stack-name".into(),
            target.stack_name.clone(),
            "--change-set-name".into(),
            change_set_name.clone(),
            "--region".into(),
            target.expected_region.clone(),
        ],
    )?;
    let described = run_cloud_output(
        root,
        "aws",
        "describe the created CloudFormation change set",
        &[
            "cloudformation".into(),
            "describe-change-set".into(),
            "--stack-name".into(),
            target.stack_name.clone(),
            "--change-set-name".into(),
            change_set_name,
            "--include-property-values".into(),
            "--region".into(),
            target.expected_region.clone(),
            "--output".into(),
            "json".into(),
        ],
    )?;
    let change_set = CloudFormationChangeSet::from_aws_json(&described.stdout)?;
    if change_set.change_set_type != change_set_type {
        bail!("provider change-set type did not match the guarded stack state");
    }

    let receipt = ChangeSetReceipt::seal(ChangeSetReceiptInput {
        source_change: release.source_change.clone(),
        release_manifest,
        release_id: release.release_id.clone(),
        release_digest: release.release_digest.clone(),
        release_approval_digest: args
            .approve_release_digest
            .clone()
            .context("release digest approval is required")?,
        configuration_digest: release.configuration_digest.clone(),
        environment: release.environment.clone(),
        expected_account_id: target.expected_account_id.clone(),
        expected_role_arn: target.expected_role_arn.clone(),
        target_config,
        packaged_template: packaged_template_digest,
        drift,
        change_set,
    })?;
    ensure_parent(review_output)?;
    receipt.write_json(review_output)?;
    receipt.verify_at(root)?;
    print_value(&receipt, as_json)
}

fn inspect_stack_and_drift(
    root: &Path,
    target: &DeploymentTarget,
) -> Result<(ChangeSetType, StackDrift)> {
    let Some(stack) = describe_target_stack(root, target)? else {
        return Ok((ChangeSetType::Create, StackDrift::NotApplicableNewStack));
    };
    require_stable_update_stack(&stack.stack_status)?;
    Ok((
        ChangeSetType::Update,
        detect_clean_stack_drift(root, target)?,
    ))
}

fn describe_target_stack(root: &Path, target: &DeploymentTarget) -> Result<Option<AwsStack>> {
    let described = run_cloud_output_allow_failure(
        root,
        "aws",
        &[
            "cloudformation".into(),
            "describe-stacks".into(),
            "--stack-name".into(),
            target.stack_name.clone(),
            "--region".into(),
            target.expected_region.clone(),
            "--output".into(),
            "json".into(),
        ],
    )?;
    if !described.status.success() {
        let stderr = String::from_utf8_lossy(&described.stderr);
        if stderr.contains("ValidationError") && stderr.contains("does not exist") {
            return Ok(None);
        }
        bail!(
            "inspect the target CloudFormation stack failed: {}",
            bounded_provider_error(&described.stderr)
        );
    }
    let response: AwsDescribeStacks =
        serde_json::from_slice(&described.stdout).context("parse CloudFormation stack response")?;
    let [stack] = response.stacks.as_slice() else {
        bail!("CloudFormation stack lookup did not return exactly one stack");
    };
    Ok(Some(AwsStack {
        stack_status: stack.stack_status.clone(),
    }))
}

fn require_stable_update_stack(status: &str) -> Result<()> {
    if !matches!(
        status,
        "CREATE_COMPLETE"
            | "UPDATE_COMPLETE"
            | "UPDATE_ROLLBACK_COMPLETE"
            | "IMPORT_COMPLETE"
            | "IMPORT_ROLLBACK_COMPLETE"
    ) {
        bail!("CloudFormation stack is not in a stable updateable state: {status}");
    }
    Ok(())
}

fn detect_clean_stack_drift(root: &Path, target: &DeploymentTarget) -> Result<StackDrift> {
    let detection: AwsDriftDetection = aws_json(
        root,
        &target.expected_region,
        "start CloudFormation drift detection",
        &[
            "cloudformation",
            "detect-stack-drift",
            "--stack-name",
            &target.stack_name,
        ],
    )?;
    for _ in 0..120 {
        let status: AwsDriftStatus = aws_json(
            root,
            &target.expected_region,
            "poll CloudFormation drift detection",
            &[
                "cloudformation",
                "describe-stack-drift-detection-status",
                "--stack-drift-detection-id",
                &detection.stack_drift_detection_id,
            ],
        )?;
        match status.stack_drift_detection_status.as_str() {
            "DETECTION_IN_PROGRESS" => thread::sleep(Duration::from_secs(5)),
            "DETECTION_FAILED" => {
                bail!(
                    "CloudFormation drift detection failed: {}",
                    status
                        .detection_status_reason
                        .as_deref()
                        .unwrap_or("provider supplied no reason")
                );
            }
            "DETECTION_COMPLETE" => {
                if status.stack_drift_status.as_deref() != Some("IN_SYNC") {
                    bail!(
                        "CloudFormation stack drift is not clean: {}",
                        status.stack_drift_status.as_deref().unwrap_or("unknown")
                    );
                }
                return Ok(StackDrift::InSync {
                    detection_id: detection.stack_drift_detection_id,
                    checked_at: chrono::Utc::now().to_rfc3339(),
                });
            }
            other => bail!("CloudFormation returned unknown drift detection status {other}"),
        }
    }
    bail!("CloudFormation drift detection did not complete within 10 minutes")
}

fn inspect_stack_before_apply(
    root: &Path,
    target: &DeploymentTarget,
    change_set_type: ChangeSetType,
) -> Result<()> {
    let stack = describe_target_stack(root, target)?;
    let status = stack.as_ref().map(|stack| stack.stack_status.as_str());
    if apply_stack_requires_drift(change_set_type, status)? {
        detect_clean_stack_drift(root, target)?;
    }
    Ok(())
}

fn apply_stack_requires_drift(
    change_set_type: ChangeSetType,
    stack_status: Option<&str>,
) -> Result<bool> {
    match (change_set_type, stack_status) {
        (ChangeSetType::Create, Some("REVIEW_IN_PROGRESS")) => Ok(false),
        (ChangeSetType::Update, Some(status)) => {
            require_stable_update_stack(status)?;
            Ok(true)
        }
        (ChangeSetType::Import, _) => bail!("import change sets are not supported"),
        (ChangeSetType::Create, status) => {
            bail!("new-stack change set has unexpected stack state: {status:?}")
        }
        (ChangeSetType::Update, None) => {
            bail!("update change set target stack no longer exists")
        }
    }
}

const fn aws_change_set_type(change_set_type: ChangeSetType) -> &'static str {
    match change_set_type {
        ChangeSetType::Create => "CREATE",
        ChangeSetType::Update => "UPDATE",
        ChangeSetType::Import => "IMPORT",
    }
}

fn aws_json<T: for<'de> Deserialize<'de>>(
    root: &Path,
    region: &str,
    label: &str,
    args: &[&str],
) -> Result<T> {
    let mut owned = args.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>();
    owned.extend([
        "--region".into(),
        region.into(),
        "--output".into(),
        "json".into(),
    ]);
    let output = run_cloud_output(root, "aws", label, &owned)?;
    serde_json::from_slice(&output.stdout).with_context(|| format!("parse response for {label}"))
}

fn run_cloud_output(root: &Path, program: &str, label: &str, args: &[String]) -> Result<Output> {
    let output = run_cloud_output_allow_failure(root, program, args)?;
    if !output.status.success() {
        bail!("{label} failed: {}", bounded_provider_error(&output.stderr));
    }
    Ok(output)
}

fn run_cloud_output_allow_failure(root: &Path, program: &str, args: &[String]) -> Result<Output> {
    ProcessCommand::new(program)
        .args(args)
        .current_dir(root)
        .env("AWS_PAGER", "")
        .env("SAM_CLI_TELEMETRY", "0")
        .stdin(Stdio::null())
        .output()
        .with_context(|| format!("run {program} for guarded cloud operation"))
}

fn bounded_provider_error(stderr: &[u8]) -> String {
    String::from_utf8_lossy(stderr)
        .chars()
        .take(4_096)
        .collect::<String>()
        .trim()
        .to_owned()
}

#[derive(Debug, Deserialize)]
struct MigrationReceiptEvidence {
    schema_version: u32,
    source_change: String,
    catalog_digest: String,
    plan_digest: String,
    selected_set: String,
    outcome: String,
    failure_code: Option<String>,
    after: Vec<MigrationAfterEvidence>,
    verification: Vec<MigrationSetEvidence>,
}

#[derive(Debug, Deserialize)]
struct MigrationSetEvidence {
    set_id: String,
    verified: bool,
}

#[derive(Debug, Deserialize)]
struct MigrationAfterEvidence {
    set_id: String,
    status: minco_db::MigrationStatus,
}

struct VerifiedApplyInputs {
    change_set: ChangeSetReceipt,
    change_set_receipt: FileDigest,
    release: ReleaseManifest,
    target: DeploymentTarget,
    database_plan: DatabasePlanBinding,
    migration_receipt: FileDigest,
}

fn apply_change_set_command(root: &Path, args: &ApplyArgs, as_json: bool) -> Result<()> {
    for (path, label) in [
        (&args.changeset, "change-set receipt"),
        (&args.migration_plan, "migration plan"),
        (&args.migration_receipt, "migration receipt"),
    ] {
        if root.join(path).exists() {
            validate_project_file(root, path, label)?;
        } else if path.is_absolute()
            || !path
                .components()
                .all(|component| matches!(component, std::path::Component::Normal(_)))
        {
            bail!("{label} must be a project-relative path");
        }
    }
    let receipt_output = package_output_path(root, &args.receipt)?;
    let mut blockers = Vec::new();
    for (path, code) in [
        (&args.changeset, "change_set_receipt_missing"),
        (&args.migration_plan, "migration_plan_missing"),
        (&args.migration_receipt, "migration_receipt_missing"),
    ] {
        if !root.join(path).is_file() {
            blockers.push(code);
        }
    }
    if receipt_output.exists() {
        blockers.push("deployment_receipt_exists");
    }

    let verified = if blockers.is_empty() {
        if let Ok(verified) = verify_apply_inputs(root, args) {
            Some(verified)
        } else {
            blockers.push("apply_evidence_invalid");
            None
        }
    } else {
        None
    };
    if args.approve_changeset_digest.is_none() {
        blockers.push("changeset_approval_missing");
    } else if let (Some(verified), Some(approval)) =
        (&verified, args.approve_changeset_digest.as_deref())
        && approval != verified.change_set.receipt_digest
    {
        blockers.push("changeset_approval_mismatch");
    }

    let plan = json!({
        "schema_version": 1,
        "operation": "apply_change_set",
        "dry_run": args.dry_run,
        "external_aws_contact": false,
        "infrastructure_apply": false,
        "change_set_receipt": args.changeset,
        "migration_plan": args.migration_plan,
        "migration_receipt": args.migration_receipt,
        "deployment_receipt": args.receipt,
        "change_set": verified.as_ref().map(|verified| json!({
            "receipt_digest": verified.change_set.receipt_digest,
            "release_id": verified.change_set.release_id,
            "change_set_id": verified.change_set.change_set.change_set_id,
            "stack_name": verified.change_set.change_set.stack_name,
            "change_set_type": verified.change_set.change_set.change_set_type,
            "review": verified.change_set.change_set.review,
            "migration_plan_digest": verified.database_plan.plan_digest,
        })),
        "guard_requirements": [
            "exact_source",
            "verified_release",
            "immutable_change_set_receipt",
            "current_expected_account_region_role",
            "current_clean_drift_or_new_stack",
            "current_available_change_set",
            "verified_migration_receipt",
            "exact_change_set_digest_approval",
        ],
        "blockers": blockers,
    });
    if args.dry_run {
        return print_value(&plan, as_json);
    }
    if !blockers.is_empty() {
        bail!("deployment apply guards failed: {}", blockers.join(", "));
    }
    let verified = verified.context("verified apply evidence is required")?;
    apply_verified_change_set(root, args, verified, &receipt_output, as_json)
}

fn verify_apply_inputs(root: &Path, args: &ApplyArgs) -> Result<VerifiedApplyInputs> {
    let change_set = ChangeSetReceipt::from_json(&fs::read(root.join(&args.changeset))?)?;
    change_set.verify_at(root)?;
    if !change_set.change_set.review.imports.is_empty()
        || !change_set.change_set.review.indeterminate.is_empty()
        || !change_set.change_set.review.metadata_syncs.is_empty()
    {
        bail!("change set contains unsupported import, dynamic, or drift-sync actions");
    }
    let release = ReleaseManifest::read_json(root.join(&change_set.release_manifest.path))
        .and_then(|release| {
            release.verify_at(root)?;
            Ok(release)
        })?;
    let source = vcs::source_snapshot(root)?;
    if source.change != change_set.source_change {
        bail!("current source does not match the reviewed change set");
    }

    let catalog = DeploymentTargetCatalog::from_toml(&fs::read_to_string(
        root.join(&change_set.target_config.path),
    )?)?;
    let selected = catalog.select(Some(&change_set.environment.environment))?;
    let (database_plan, migration_receipt) = verify_migration_evidence(
        root,
        &release,
        &args.migration_plan,
        &args.migration_receipt,
    )?;
    Ok(VerifiedApplyInputs {
        change_set_receipt: FileDigest::from_rooted_path(root, root.join(&args.changeset))?,
        change_set,
        release,
        target: selected.target,
        database_plan,
        migration_receipt,
    })
}

fn verify_migration_evidence(
    root: &Path,
    release: &ReleaseManifest,
    plan_path: &Path,
    receipt_path: &Path,
) -> Result<(DatabasePlanBinding, FileDigest)> {
    let plan: minco_db::MigrationPlan =
        serde_json::from_slice(&fs::read(root.join(plan_path))?).context("parse migration plan")?;
    let selected_set = plan
        .selected_set
        .as_deref()
        .context("deployment migration plan must select exactly one set")?
        .to_owned();
    let receipt: MigrationReceiptEvidence =
        serde_json::from_slice(&fs::read(root.join(receipt_path))?)
            .context("parse migration receipt")?;
    validate_migration_binding(
        &plan,
        &receipt,
        &release.source_change,
        &release.database_sources.migration_catalog,
    )?;
    Ok((
        DatabasePlanBinding {
            kind: DatabasePlanKind::Migration,
            schema_version: plan.schema_version,
            catalog_digest: plan.catalog_digest,
            plan_digest: plan.digest,
            file: FileDigest::from_rooted_path(root, root.join(plan_path))?,
            selected_set: Some(selected_set),
            environment: Some(release.environment.environment.clone()),
        },
        FileDigest::from_rooted_path(root, root.join(receipt_path))?,
    ))
}

fn validate_migration_binding(
    plan: &minco_db::MigrationPlan,
    receipt: &MigrationReceiptEvidence,
    expected_source_change: &str,
    expected_catalog_digest: &str,
) -> Result<()> {
    let selected_set = plan
        .selected_set
        .as_deref()
        .context("deployment migration plan must select exactly one set")?;
    let planned_sets = plan
        .sets
        .iter()
        .map(|set| set.id.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    let verified_sets = receipt
        .verification
        .iter()
        .map(|set| set.set_id.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    let applied_sets = receipt
        .after
        .iter()
        .map(|set| set.set_id.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    if plan.schema_version == 0
        || planned_sets.is_empty()
        || planned_sets.len() != plan.sets.len()
        || verified_sets != planned_sets
        || applied_sets != planned_sets
        || plan.catalog_digest != expected_catalog_digest
        || receipt.schema_version != 1
        || receipt.source_change != expected_source_change
        || receipt.catalog_digest != plan.catalog_digest
        || receipt.plan_digest != plan.digest
        || receipt.selected_set != selected_set
        || receipt.outcome != "succeeded"
        || receipt.failure_code.is_some()
        || receipt.verification.is_empty()
        || receipt
            .verification
            .iter()
            .any(|verification| !verification.verified)
        || receipt.after.iter().any(|after| {
            after.status.set_id != after.set_id
                || after.status.dirty_version.is_some()
                || after
                    .status
                    .entries
                    .iter()
                    .any(|entry| entry.state != minco_db::MigrationState::Applied)
        })
    {
        bail!("migration plan and successful verification receipt do not bind the exact release");
    }
    Ok(())
}

fn apply_verified_change_set(
    root: &Path,
    args: &ApplyArgs,
    verified: VerifiedApplyInputs,
    receipt_output: &Path,
    as_json: bool,
) -> Result<()> {
    if !command_available("aws") {
        bail!("deployment apply requires `aws`");
    }
    let identity: AwsCallerIdentity = aws_json(
        root,
        &verified.target.expected_region,
        "inspect AWS caller identity before apply",
        &["sts", "get-caller-identity"],
    )?;
    let role_arn = caller_role_arn(&identity.arn).context(
        "AWS caller identity must be the exact configured IAM role or an assumed-role session",
    )?;
    let current_type = verified.change_set.change_set.change_set_type;
    inspect_stack_before_apply(root, &verified.target, current_type)?;
    let described = run_cloud_output(
        root,
        "aws",
        "re-inspect the approved CloudFormation change set",
        &[
            "cloudformation".into(),
            "describe-change-set".into(),
            "--change-set-name".into(),
            verified.change_set.change_set.change_set_id.clone(),
            "--include-property-values".into(),
            "--region".into(),
            verified.target.expected_region.clone(),
            "--output".into(),
            "json".into(),
        ],
    )?;
    let current_change_set = CloudFormationChangeSet::from_aws_json(&described.stdout)?;
    if current_change_set != verified.change_set.change_set
        || current_type != current_change_set.change_set_type
    {
        bail!("current provider change set does not match the approved immutable receipt");
    }
    let current_source = vcs::source_snapshot(root)?;
    verified.change_set.verify_at(root)?;
    for file in [
        &verified.change_set_receipt,
        &verified.database_plan.file,
        &verified.migration_receipt,
    ] {
        file.verify_at(root)?;
    }
    verify_guards(
        &EnvironmentExpectation {
            account_id: verified.change_set.expected_account_id.clone(),
            region: verified.change_set.environment.region.clone(),
            environment: verified.change_set.environment.environment.clone(),
            role_arn: verified.change_set.expected_role_arn.clone(),
            release_id: verified.change_set.release_id.clone(),
            release_digest: verified.change_set.release_digest.clone(),
            configuration_digest: verified.change_set.configuration_digest.clone(),
            migration_plan_digest: Some(verified.database_plan.plan_digest.clone()),
        },
        &EnvironmentObservation {
            account_id: identity.account,
            region: verified.target.expected_region.clone(),
            environment: verified.release.environment.environment.clone(),
            role_arn,
            release_id: verified.release.release_id.clone(),
            release_digest: verified.release.release_digest.clone(),
            release_verified: true,
            configuration_digest: verified.release.configuration_digest.clone(),
            drift: DriftState::Clean,
            migration: DeploymentMigrationState::Verified {
                plan_digest: verified.database_plan.plan_digest.clone(),
            },
            source: if current_source.change == verified.change_set.source_change {
                SourceState::Clean
            } else {
                SourceState::Dirty
            },
            operator_approval_digest: Some(verified.change_set.release_digest.clone()),
        },
    )?;

    let mut deployment = DeploymentReceipt::start(DeploymentReceiptInput {
        attempt_id: uuid::Uuid::now_v7().to_string(),
        release_manifest: verified.change_set.release_manifest.clone(),
        release_id: verified.release.release_id.clone(),
        release_digest: verified.release.release_digest.clone(),
        environment: verified.release.environment.clone(),
        configuration_digest: verified.release.configuration_digest.clone(),
        database_plans: vec![verified.database_plan],
        attestations: vec![verified.change_set_receipt, verified.migration_receipt],
    })?;
    ensure_parent(receipt_output)?;
    deployment.write_json(receipt_output)?;
    deployment.verify_at(root)?;

    if let Err(error) = run_cloud_output(
        root,
        "aws",
        "execute the exact approved CloudFormation change set",
        &[
            "cloudformation".into(),
            "execute-change-set".into(),
            "--change-set-name".into(),
            verified.change_set.change_set.change_set_id.clone(),
            "--client-request-token".into(),
            verified.change_set.receipt_digest.clone(),
            "--region".into(),
            verified.target.expected_region.clone(),
        ],
    ) {
        deployment.fail("cloudformation_execute_failed")?;
        deployment.write_json(receipt_output)?;
        return Err(error);
    }
    let waiter = match current_type {
        ChangeSetType::Create => "stack-create-complete",
        ChangeSetType::Update => "stack-update-complete",
        ChangeSetType::Import => {
            deployment.fail("unsupported_import_apply")?;
            deployment.write_json(receipt_output)?;
            bail!("import change sets are not supported");
        }
    };
    if let Err(error) = run_cloud_output(
        root,
        "aws",
        "wait for CloudFormation stack apply",
        &[
            "cloudformation".into(),
            "wait".into(),
            waiter.into(),
            "--stack-name".into(),
            verified.target.stack_name,
            "--region".into(),
            verified.target.expected_region,
        ],
    ) {
        deployment.fail("cloudformation_apply_failed")?;
        deployment.write_json(receipt_output)?;
        return Err(error);
    }
    print_value(
        &json!({
            "infrastructure_applied": true,
            "hosted_verification_pending": true,
            "deployment_receipt": deployment,
            "deployment_receipt_path": args.receipt,
        }),
        as_json,
    )
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

fn package_command(
    root: &Path,
    manifest: &MincoManifest,
    args: PackageArgs,
    as_json: bool,
) -> Result<()> {
    if manifest.commands.package.is_empty() {
        bail!("no package command is configured in [commands].package");
    }
    let source = vcs::source_snapshot(root)?;
    for command in &manifest.commands.package {
        if command.trim().is_empty() {
            bail!("package commands cannot be empty");
        }
        let result = run_shell(root, command, !as_json)?;
        if !result.success {
            bail!("package command failed: {command}");
        }
    }
    if vcs::source_snapshot(root)? != source {
        bail!(
            "package command changed the exact source revision; review and commit source changes"
        );
    }

    let plan = load_plan(root, manifest, args.config)?;
    ensure_plan_valid(&plan)?;
    let plan_path = package_output_path(root, &args.plan)?;
    let template_path = package_output_path(root, &args.template)?;
    let output = package_output_path(root, &args.output)?;
    if plan_path == template_path || plan_path == output || template_path == output {
        bail!("package plan, template, and release outputs must use distinct paths");
    }
    ensure_parent(&plan_path)?;
    fs::write(&plan_path, canonical_json(&plan)?)?;

    let code_uris = plan
        .functions
        .iter()
        .map(|function| {
            let code_uri = template_relative_path(root, &template_path, &function.artifact_path)?;
            Ok((
                function.name.clone(),
                code_uri.to_string_lossy().into_owned(),
            ))
        })
        .collect::<Result<std::collections::BTreeMap<_, _>>>()?;
    ensure_parent(&template_path)?;
    fs::write(
        &template_path,
        render_sam_with_code_uris(&plan, &code_uris)?,
    )?;
    if vcs::source_snapshot(root)? != source {
        bail!(
            "package outputs changed the exact source revision; outputs must remain under ignored target/"
        );
    }

    let release = seal_release(
        root,
        manifest,
        &plan,
        &plan_path,
        &template_path,
        &source.change,
        args.environment.as_deref(),
        None,
        &args.attestations,
    )?;
    ensure_parent(&output)?;
    release.write_json(&output)?;
    release.verify_at(root)?;
    print_value(&release, as_json)
}

fn package_output_path(root: &Path, relative: &Path) -> Result<PathBuf> {
    if relative.is_absolute() {
        bail!("package output paths must be repository-relative");
    }
    let components = relative.components().collect::<Vec<_>>();
    if components.len() < 2
        || components.first() != Some(&std::path::Component::Normal(OsStr::new("target")))
        || !components
            .iter()
            .all(|component| matches!(component, std::path::Component::Normal(_)))
    {
        bail!("package output paths must be normalized descendants of target/");
    }
    let root = root
        .canonicalize()
        .context("canonicalize the package repository root")?;
    let target = root.join("target");
    if target
        .symlink_metadata()
        .context("package commands must create the target/ directory")?
        .file_type()
        .is_symlink()
    {
        bail!("package output target/ must not be a symbolic link");
    }
    let target = target
        .canonicalize()
        .context("canonicalize the package target/ directory")?;
    let output = root.join(relative);
    if output
        .symlink_metadata()
        .is_ok_and(|metadata| metadata.file_type().is_symlink())
    {
        bail!("package output must not be a symbolic link");
    }
    let mut existing_ancestor = output.as_path();
    while !existing_ancestor.exists() {
        existing_ancestor = existing_ancestor
            .parent()
            .context("package output has no existing ancestor")?;
    }
    let existing_ancestor = existing_ancestor
        .canonicalize()
        .context("canonicalize the package output ancestor")?;
    if !existing_ancestor.starts_with(&target) {
        bail!("package output resolves outside target/");
    }
    Ok(output)
}

#[allow(clippy::too_many_arguments)]
fn seal_release(
    root: &Path,
    manifest: &MincoManifest,
    plan: &DeploymentPlan,
    plan_path: &Path,
    template_path: &Path,
    source_change: &str,
    environment: Option<&str>,
    artifact_override: Option<&Path>,
    attestation_paths: &[PathBuf],
) -> Result<ReleaseManifest> {
    let environment = environment.unwrap_or(&plan.environment);
    if environment != plan.environment {
        bail!(
            "configuration environment {environment} does not match deployment environment {}",
            plan.environment
        );
    }
    let configuration = config_cmd::load_graph(root, manifest, environment, &[])
        .map_err(|diagnostics| anyhow::anyhow!("invalid runtime configuration: {diagnostics:?}"))?;
    let configured_application = configuration
        .explain("application.name")
        .and_then(|explanation| explanation.value)
        .and_then(|value| value.as_str().map(str::to_owned))
        .context("typed configuration must contain non-secret application.name")?;
    if configured_application != plan.application {
        bail!(
            "configuration application {configured_application} does not match deployment application {}",
            plan.application
        );
    }
    let migration_catalog = minco_db::load_catalog(root, &manifest.migrations.roots)?;
    let seed_catalog = minco_db::load_seed_catalog(root, &manifest.seeds.roots)?;
    let rustc = capture(root, "rustc", &["--version"])
        .context("rustc is required to capture the release toolchain")?;
    let artifact_builder = command_available("cargo-lambda")
        .then(|| capture(root, "cargo", &["lambda", "--version"]))
        .transpose()?;

    let artifacts = if let Some(artifact) = artifact_override {
        let [function] = plan.functions.as_slice() else {
            bail!("release create --artifact requires a plan with exactly one function");
        };
        vec![FunctionArtifact {
            function_id: function.name.clone(),
            file: FileDigest::from_rooted_path(root, artifact)?,
        }]
    } else {
        plan.functions
            .iter()
            .map(|function| {
                let path = root.join(&function.artifact_path);
                if !path.is_file() {
                    bail!(
                        "package artifact for function {} does not exist at {}",
                        function.name,
                        path.display()
                    );
                }
                Ok(FunctionArtifact {
                    function_id: function.name.clone(),
                    file: FileDigest::from_rooted_path(root, path)?,
                })
            })
            .collect::<Result<Vec<_>>>()?
    };
    let attestations = attestation_paths
        .iter()
        .map(|path| {
            if path.is_absolute()
                || !path
                    .components()
                    .all(|component| matches!(component, std::path::Component::Normal(_)))
            {
                bail!("attestation paths must be normalized repository-relative paths");
            }
            FileDigest::from_rooted_path(root, root.join(path)).map_err(Into::into)
        })
        .collect::<Result<Vec<_>, _>>()?;

    ReleaseManifest::seal(ReleaseManifestInput {
        source_change: source_change.into(),
        environment: ReleaseEnvironment {
            application: plan.application.clone(),
            environment: plan.environment.clone(),
            region: plan.region.clone(),
        },
        toolchain: ToolchainIdentity {
            rustc,
            cargo_minco: env!("CARGO_PKG_VERSION").into(),
            artifact_builder,
        },
        artifacts,
        contract: FileDigest::from_rooted_path(root, root.join(&manifest.contract))?,
        configuration_digest: configuration.digest().into(),
        database_sources: DatabaseSourceDigests {
            migration_catalog: migration_catalog.digest,
            seed_catalog: seed_catalog.digest,
        },
        cargo_lock: root
            .join("Cargo.lock")
            .is_file()
            .then(|| FileDigest::from_rooted_path(root, root.join("Cargo.lock")))
            .transpose()?,
        deployment_plan: FileDigest::from_rooted_path(root, plan_path)?,
        deployment_template: FileDigest::from_rooted_path(root, template_path)?,
        attestations,
    })
    .map_err(Into::into)
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
            let source = vcs::source_snapshot(root)?;
            let deployment_plan: DeploymentPlan =
                serde_json::from_slice(&fs::read(&plan)?).context("parse deployment plan")?;
            ensure_plan_valid(&deployment_plan)?;
            let release = seal_release(
                root,
                manifest,
                &deployment_plan,
                &plan,
                &template,
                &source.change,
                None,
                Some(&artifact),
                &[],
            )?;
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
    fn package_is_a_first_class_top_level_command() {
        let cli = Cli::try_parse_from(["cargo-minco", "package"]).expect("package command");
        assert!(matches!(cli.command, Command::Package(_)));
    }

    #[test]
    fn deployment_change_set_has_a_non_contacting_dry_run_shape() {
        let cli =
            Cli::try_parse_from(["cargo-minco", "deploy", "changeset", "--dry-run", "--json"])
                .expect("change-set dry-run command");

        assert!(matches!(
            cli.command,
            Command::Deploy(DeployCommand::Changeset(ChangeSetArgs {
                dry_run: true,
                ..
            }))
        ));
        assert!(cli.json);
    }

    #[test]
    fn deployment_apply_has_separate_exact_receipt_approval_shape() {
        let cli = Cli::try_parse_from([
            "cargo-minco",
            "deploy",
            "apply",
            "--approve-changeset-digest",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "--dry-run",
            "--json",
        ])
        .expect("deployment apply dry-run command");

        assert!(matches!(
            cli.command,
            Command::Deploy(DeployCommand::Apply(ApplyArgs {
                dry_run: true,
                approve_changeset_digest: Some(_),
                ..
            }))
        ));
        assert!(cli.json);
    }

    #[test]
    fn deployment_apply_requires_successful_exact_migration_evidence() {
        let source_change = "a".repeat(64);
        let catalog_digest = "b".repeat(64);
        let plan_digest = "c".repeat(64);
        let plan = minco_db::MigrationPlan {
            schema_version: 1,
            catalog_digest: catalog_digest.clone(),
            selected_set: Some("orders".into()),
            digest: plan_digest.clone(),
            sets: vec![minco_db::MigrationSet {
                id: "orders".into(),
                owner: "orders".into(),
                backend: minco_db::DatabaseBackend::Postgres,
                root: PathBuf::from("migrations/orders"),
                history_table: "_sqlx_migrations_orders".into(),
                depends_on: Vec::new(),
                verify_tables: vec!["orders".into()],
                digest: "d".repeat(64),
                migrations: Vec::new(),
            }],
        };
        let mut receipt = MigrationReceiptEvidence {
            schema_version: 1,
            source_change: source_change.clone(),
            catalog_digest: catalog_digest.clone(),
            plan_digest,
            selected_set: "orders".into(),
            outcome: "succeeded".into(),
            failure_code: None,
            after: vec![MigrationAfterEvidence {
                set_id: "orders".into(),
                status: minco_db::MigrationStatus {
                    set_id: "orders".into(),
                    dirty_version: None,
                    entries: Vec::new(),
                },
            }],
            verification: vec![MigrationSetEvidence {
                set_id: "orders".into(),
                verified: true,
            }],
        };
        validate_migration_binding(&plan, &receipt, &source_change, &catalog_digest)
            .expect("exact successful migration evidence");

        receipt.outcome = "failed".into();
        assert!(
            validate_migration_binding(&plan, &receipt, &source_change, &catalog_digest).is_err()
        );
        receipt.outcome = "succeeded".into();
        receipt.source_change = "d".repeat(64);
        assert!(
            validate_migration_binding(&plan, &receipt, &source_change, &catalog_digest).is_err()
        );
    }

    #[test]
    fn create_change_set_apply_accepts_only_the_provider_review_state() {
        assert!(
            apply_stack_requires_drift(ChangeSetType::Create, Some("REVIEW_IN_PROGRESS"))
                .is_ok_and(|required| !required)
        );
        assert!(
            apply_stack_requires_drift(ChangeSetType::Create, Some("CREATE_COMPLETE")).is_err()
        );
        assert!(
            apply_stack_requires_drift(ChangeSetType::Update, Some("UPDATE_COMPLETE"))
                .is_ok_and(|required| required)
        );
    }

    #[test]
    fn package_outputs_are_confined_to_ignored_target_descendants() {
        let project = tempfile::tempdir().unwrap();
        let root = project.path();
        std::fs::create_dir(root.join("target")).unwrap();
        assert_eq!(
            package_output_path(root, Path::new("target/minco/release.json")).unwrap(),
            root.canonicalize()
                .unwrap()
                .join("target/minco/release.json")
        );
        for unsafe_path in [
            Path::new("target"),
            Path::new("infra/release.json"),
            Path::new("target/../release.json"),
            Path::new("/tmp/release.json"),
        ] {
            assert!(package_output_path(root, unsafe_path).is_err());
        }
    }

    #[cfg(unix)]
    #[test]
    fn package_outputs_reject_symlinked_target_escape() {
        let project = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::create_dir(project.path().join("target")).unwrap();
        std::os::unix::fs::symlink(outside.path(), project.path().join("target/escape")).unwrap();

        let error = package_output_path(project.path(), Path::new("target/escape/release.json"))
            .expect_err("symlinked package output must not escape target");
        assert!(error.to_string().contains("outside target"));
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
