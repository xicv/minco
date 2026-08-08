// The CLI implementation remains in the binary target. Public visibility lets
// sibling command modules share internal types; it is not part of the library
// documentation target's API.
#![allow(unreachable_pub)]

mod agent_cmd;
mod architecture;
mod config;
mod config_cmd;
mod db_cmd;
mod delivery_evidence;
mod feedback_cmd;
mod generator_cmd;
mod handover_cmd;
mod new_cmd;
mod plugin_cmd;
mod process;
mod roadmap;
mod service_runtime;
mod update;
mod upgrade_cmd;
mod vcs;

use agent_cmd::AgentCommand;
use anyhow::{Context, Result, bail};
use architecture::validate_architecture;
use base64::Engine as _;
use clap::{Args, Parser, Subcommand, ValueEnum};
use config::{MincoManifest, discover_root};
use config_cmd::ConfigCommand;
use feedback_cmd::FeedbackArgs;
use generator_cmd::{MakeCommand, NamedArgs, StubsCommand};
use handover_cmd::HandoverArgs;
use minco_config::EnvironmentClass;
use minco_contract::{
    CompatibilityClassification, Severity as ContractSeverity, diff_contracts, generate_rust,
    load_contract, load_contract_source,
};
use minco_core::{PluginId, PluginManager, PluginSelection};
use minco_deploy_aws::{
    CanaryExecutionReceipt, CanaryExecutionReceiptInput, CanaryShiftInput, ChangeSetReceipt,
    ChangeSetReceiptInput, ChangeSetType, CleanupReceipt, CleanupReceiptInput,
    CloudFormationChangeSet, DeploymentTarget, DeploymentTargetCatalog, DeploymentTargetLifecycle,
    DriftState, EnvironmentExpectation, EnvironmentObservation, HostedCheckResult,
    HostedVerificationInput, HostedVerificationReport, MigrationState as DeploymentMigrationState,
    PromotionOutcome, PromotionReceipt, PromotionReceiptInput, ReviewCostClass, ReviewManifest,
    ReviewManifestInput, ReviewResource, ReviewResourceRetention, ReviewScheduleCompletionAction,
    RollbackAssessmentInput, RollbackCompatibility, SourceState, StackDrift,
    StaticSiteCertificateObservation, StaticSiteDistributionStatus, StaticSiteDnsObservation,
    StaticSiteInvalidationStatus, StaticSiteObjectObservation, StaticSitePricingEvidence,
    StaticSiteProviderObservation, StaticSitePublicationReceipt, StaticSitePublicationReceiptInput,
    StaticSiteVerificationInput, StaticSiteVerificationReport, UntrustedFeedbackReference,
    assess_rollback_compatibility, caller_role_arn, plan_canary_shift, verify_guards,
    verify_promotion_boundary,
};
use minco_dev::{
    DevDatabase, DevEvent, DevGraph, DevOptions, DevPlan, DevStream, ServiceKind, Supervisor,
};
use minco_plan::{
    CostClass, DatabaseCostEstimate, DatabaseDeployment, DeploymentConfig, DeploymentPlan,
    FunctionRole, PreviewCleanupSchedule, PreviewLifecyclePlan, PreviewResource,
    PreviewResourceRetention, RealtimeDeployment, ScheduleCompletionAction,
    Severity as PlanSeverity, StaticSiteDeployment, TriggerPlan, estimate_database_cost,
    estimate_runtime_cost, render_sam_with_code_uris,
};
use minco_release::{
    DatabasePlanBinding, DatabasePlanKind, DatabaseSourceDigests, DeploymentOutcome,
    DeploymentReceipt, DeploymentReceiptInput, FileDigest, FunctionArtifact, ReleaseEnvironment,
    ReleaseManifest, ReleaseManifestInput, ToolchainIdentity, VerificationEvidence,
};
use new_cmd::{DatabaseChoice, NewProjectOptions, VcsChoice, create_project};
use plugin_cmd::{
    add_plugin, doctor_plugins, explain_plugin, init_plugin, load_catalog, remove_plugin,
    set_plugin_state_workflow, validate_catalog, validate_distribution_contracts,
};
use process::{capture, command_available, run_shell};
use roadmap::{
    load_roadmap, load_tasks, ready_tasks, render_roadmap_mermaid, render_task_mermaid,
    validate_task_graph,
};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest as _, Sha256};
use std::{
    ffi::{OsStr, OsString},
    fs,
    io::Write as _,
    path::{Path, PathBuf},
    process::{Command as ProcessCommand, Output, Stdio},
    sync::Arc,
    thread,
    time::Duration,
};

const LIVE_ALIAS_LOGICAL_ID: &str = "LiveFunctionAlias";
const LIVE_FUNCTION_VERSION_PARAMETER: &str = "LiveFunctionVersion";
const DEVELOPMENT_READINESS_TIMEOUT: Duration = Duration::from_mins(5);

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
    #[command(subcommand)]
    Agent(AgentCommand),
    Doctor,
    /// Run the graph-declared local development topology.
    Dev(DevArgs),
    #[command(name = "__local-service", hide = true)]
    LocalService(service_runtime::LocalServiceArgs),
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
    /// Plan or apply exact, preview-only environment cleanup.
    Destroy(DestroyArgs),
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
    /// Route live API traffic to an exact successfully verified release.
    Promote(PromoteArgs),
    /// Assess an exact older promoted release before routing with `promote`.
    Rollback(RollbackArgs),
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
    /// Produce deterministic client handover JSON and Markdown from exact release evidence.
    Handover(HandoverArgs),
    /// Expose a bounded, local-only, read-only `ProjectView` over child-process stdio.
    Mcp(McpArgs),
    /// Inspect the bounded `ProjectView` through an opt-in local workbench.
    Workbench(WorkbenchArgs),
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

#[derive(Debug, Clone, Copy, Args)]
struct McpArgs {
    /// Validate the bounded view and MCP surface without starting a protocol server.
    #[arg(long)]
    check: bool,
}

#[derive(Debug, Clone, Args)]
struct WorkbenchArgs {
    /// Validate the bounded view and workbench surface without serving or writing.
    #[arg(long)]
    check: bool,
    #[command(subcommand)]
    command: Option<WorkbenchCommand>,
}

#[derive(Debug, Clone, Subcommand)]
enum WorkbenchCommand {
    /// Export one deterministic snapshot into a new project-relative directory.
    Export(WorkbenchExportArgs),
    /// Serve the current bounded snapshot over an exact loopback origin.
    Serve(WorkbenchServeArgs),
}

#[derive(Debug, Clone, Args)]
struct WorkbenchExportArgs {
    #[arg(long, value_enum)]
    format: WorkbenchExportFormat,
    #[arg(long)]
    output: PathBuf,
}

#[derive(Debug, Clone, Copy, Args)]
struct WorkbenchServeArgs {
    /// Loopback TCP port; zero asks the operating system to choose an available port.
    #[arg(long, default_value_t = 0)]
    port: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum WorkbenchExportFormat {
    Json,
    Mermaid,
    Static,
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
    #[arg(long, default_value = "target/minco/static-site-release.json")]
    static_site_manifest: PathBuf,
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

#[derive(Debug, Clone, Args)]
struct DestroyArgs {
    #[arg(long, default_value = "infra/aws/deployment-targets.toml")]
    target_config: PathBuf,
    #[arg(long)]
    environment: Option<String>,
    #[arg(long, default_value = "target/minco/review.json")]
    review: PathBuf,
    #[arg(long, default_value = "target/minco/cleanup-receipt.json")]
    receipt: PathBuf,
    #[arg(long)]
    approve_review_digest: Option<String>,
    #[arg(long)]
    dry_run: bool,
}

#[derive(Debug, Clone, Args)]
struct DeployVerifyArgs {
    #[arg(long, default_value = "target/minco/release.json")]
    manifest: PathBuf,
    #[arg(long, default_value = "target/minco/deployment-receipt.json")]
    receipt: PathBuf,
    #[arg(long, default_value = "target/minco/hosted-verification.json")]
    output: PathBuf,
    #[arg(long)]
    static_site: bool,
    #[arg(long, default_value = "target/minco/static-site-publication.json")]
    static_site_publication: PathBuf,
    #[arg(long, default_value = "target/minco/static-site-verification.json")]
    static_site_output: PathBuf,
    #[arg(long)]
    dry_run: bool,
}

#[derive(Debug, Clone, Args)]
struct ReviewArgs {
    #[arg(long, default_value = "infra/aws/deployment-targets.toml")]
    target_config: PathBuf,
    #[arg(long)]
    environment: Option<String>,
    #[arg(long, default_value = "target/minco/release.json")]
    manifest: PathBuf,
    #[arg(long, default_value = "target/minco/deployment-receipt.json")]
    deployment_receipt: PathBuf,
    #[arg(long = "feedback")]
    feedback: Vec<String>,
    #[arg(long = "delivery-trace")]
    delivery_trace: Vec<PathBuf>,
    #[arg(long, default_value = "target/minco/review.json")]
    output: PathBuf,
    #[arg(long)]
    dry_run: bool,
}

#[derive(Debug, Clone, Args)]
struct StaticSitePublishInput {
    #[arg(long, default_value = "infra/aws/deployment-targets.toml")]
    target_config: PathBuf,
    #[arg(long)]
    environment: Option<String>,
    #[arg(long, default_value = "target/minco/release.json")]
    manifest: PathBuf,
    #[arg(long, default_value = "target/minco/deployment-receipt.json")]
    deployment_receipt: PathBuf,
    #[arg(long, default_value = "target/minco/static-site-publication.json")]
    output: PathBuf,
}

#[derive(Debug, Clone, Subcommand)]
enum StaticSiteCommand {
    Plan {
        #[command(flatten)]
        input: StaticSitePublishInput,
    },
    Apply {
        #[command(flatten)]
        input: StaticSitePublishInput,
        #[arg(long)]
        approve_release_digest: String,
    },
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct HostedVerificationObservation {
    endpoint: String,
    executed_artifact_digest: String,
    executed_version: String,
    checks: Vec<HostedCheckResult>,
}

#[derive(Debug, Clone, Args)]
struct PromoteArgs {
    #[arg(long, default_value = "target/minco/release.json")]
    manifest: PathBuf,
    #[arg(long, default_value = "target/minco/deployment-receipt.json")]
    receipt: PathBuf,
    #[arg(long, default_value = "target/minco/hosted-verification.json")]
    verification: PathBuf,
    #[arg(long, default_value = "target/minco/promotion-receipt.json")]
    output: PathBuf,
    #[arg(long)]
    approve_verification_digest: Option<String>,
    #[arg(long)]
    dry_run: bool,
    /// Plan an opt-in alarm-guarded API alias canary.
    #[arg(long)]
    canary: bool,
    #[arg(long, default_value = "infra/aws/deployment-targets.toml")]
    target_config: PathBuf,
    #[arg(long)]
    environment: Option<String>,
    #[arg(long, default_value = "target/minco/canary-receipt.json")]
    canary_output: PathBuf,
}

#[derive(Debug, Clone, Args)]
struct RollbackArgs {
    /// Clean exact-source checkout containing the current promotion evidence.
    #[arg(long)]
    current_root: Option<PathBuf>,
    /// Clean exact-source checkout containing the older target promotion evidence.
    #[arg(long)]
    target_root: Option<PathBuf>,
    #[arg(long, default_value = "target/minco/promotion-receipt.json")]
    current_promotion: PathBuf,
    #[arg(
        long,
        default_value = "target/minco/rollback-target-promotion-receipt.json"
    )]
    target_promotion: PathBuf,
    /// Exact operator-reviewed evidence that the older application can read current data.
    #[arg(long)]
    data_compatibility_evidence: Option<PathBuf>,
    #[arg(long)]
    dry_run: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RollbackDataCompatibilityEvidence {
    schema_version: u32,
    current_release_id: String,
    target_release_id: String,
    decision: RollbackCompatibility,
    reviewed_by: String,
    reason: String,
}

#[derive(Debug, Subcommand)]
enum DeployCommand {
    Plan {
        #[command(flatten)]
        input: PlanInput,
        #[arg(long)]
        environment: Option<String>,
        #[arg(long, default_value = "infra/aws/deployment-targets.toml")]
        target_config: PathBuf,
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
    Verify(DeployVerifyArgs),
    Review(ReviewArgs),
    StaticSite {
        #[command(subcommand)]
        command: StaticSiteCommand,
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
    /// Add a catalog plugin through Minco's static facade registration.
    Add {
        /// Stable plugin ID or catalog crate name.
        plugin: String,
        /// Print the complete deterministic plan without changing files.
        #[arg(long)]
        dry_run: bool,
    },
    /// Explain a plugin's complete archive-visible contract without loading its code.
    Explain {
        /// Stable plugin ID or catalog crate name.
        plugin: String,
    },
    /// Diagnose catalog, compatibility, selection, Cargo, and static registration drift.
    Doctor,
    /// Adopt an existing local plugin package into the reviewed catalog.
    Init {
        /// Project-relative local plugin package directory.
        path: PathBuf,
        /// Print the complete deterministic plan without changing files.
        #[arg(long)]
        dry_run: bool,
    },
    /// Plan removal and refuse while application behavior or data remains owned by the plugin.
    Remove {
        /// Stable plugin ID or catalog crate name.
        plugin: String,
        /// Print the complete deterministic plan and blockers without changing files.
        #[arg(long)]
        dry_run: bool,
    },
    Enable {
        id: String,
        #[arg(long)]
        dry_run: bool,
    },
    Disable {
        id: String,
        #[arg(long)]
        dry_run: bool,
    },
    New {
        id: String,
        #[arg(long)]
        dry_run: bool,
    },
    Validate,
    Test {
        /// Stable plugin ID or catalog crate name.
        #[arg(required_unless_present = "all", conflicts_with = "all")]
        plugin: Option<String>,
        /// Test every local catalog component with the public offline conformance kit.
        #[arg(long)]
        all: bool,
    },
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
    let explicit_root = root.is_some();
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
        Command::LocalService(arguments) => return service_runtime::execute(arguments).await,
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
        Command::LocalService(_) => {
            unreachable!("local service command is handled before project discovery")
        }
        Command::Agent(command) => agent_cmd::execute(&root, command, as_json),
        Command::Doctor => doctor(&root, as_json),
        Command::Dev(args) => dev(&root, &manifest, args, as_json).await,
        Command::Check(args) => check(&root, &manifest, args, as_json),
        Command::Config(command) => config_cmd::execute(&root, &manifest, command, as_json),
        Command::Contract(command) => contract(&root, &manifest, command, as_json),
        Command::Make(command) => generator_cmd::execute(&root, &manifest, command, as_json),
        Command::Stubs(command) => generator_cmd::execute_stubs(&root, &command, as_json),
        Command::Inspect => inspect(&root, &manifest, as_json),
        Command::Explain(args) => explain(&root, &manifest, &args.operation_id, as_json),
        Command::Deploy(command) => Box::pin(deploy(&root, &manifest, command, as_json)).await,
        Command::Destroy(args) => destroy_command(&root, &args, as_json),
        Command::Cost(input) => cost(&root, &manifest, input, as_json),
        Command::Perf(input) => perf(&root, &manifest, input, as_json),
        Command::Architecture => architecture(&root, &manifest, as_json),
        Command::Roadmap(command) => roadmap_command(&root, &manifest, command, as_json),
        Command::Task(command) => task_command(&root, &manifest, command, as_json),
        Command::Plugin(command) => plugin_command(&root, &manifest, command, as_json),
        Command::Test(command) => test_command(&root, &manifest, command, as_json),
        Command::Db(command) => db_cmd::execute(&root, &manifest, command, as_json).await,
        Command::Package(args) => package_command(&root, &manifest, args, as_json),
        Command::Promote(args) => promote_command(&root, &args, as_json),
        Command::Rollback(args) => rollback_command(&root, &args, as_json),
        Command::Release(command) => release_command(&root, &manifest, command, as_json),
        Command::Update(command) => update_command(&root, command, as_json),
        Command::Upgrade(_) => unreachable!("upgrade is handled before strict manifest loading"),
        Command::Vcs(command) => vcs_command(&root, command, as_json),
        Command::Feedback(args) => feedback_cmd::execute(&root, args, as_json).await,
        Command::Handover(args) => handover_cmd::execute(&root, &args, as_json),
        Command::Mcp(args) => mcp_command(&root, args, as_json, explicit_root).await,
        Command::Workbench(args) => {
            workbench_command(&root, &manifest, args, as_json, explicit_root).await
        }
    }
}

async fn mcp_command(root: &Path, args: McpArgs, as_json: bool, explicit_root: bool) -> Result<()> {
    if !args.check && !explicit_root {
        bail!(
            "starting the Minco MCP server requires an explicit canonical project root via --root"
        );
    }

    let canonical_root = root
        .canonicalize()
        .with_context(|| format!("resolve MCP project root {}", root.display()))?;
    let view = minco_project_view::load_project_view(&canonical_root)
        .context("build bounded Minco ProjectView")?;

    if args.check {
        let tools = minco_mcp::MincoMcp::tool_catalog();
        return print_value(
            &json!({
                "schema_version": 1,
                "status": "ok",
                "mode": "check",
                "read_only": true,
                "transport": "stdio",
                "listening_sockets": 0,
                "requires_explicit_root_to_serve": true,
                "max_message_bytes": minco_mcp::DEFAULT_MAX_MCP_MESSAGE_BYTES,
                "project": view.project,
                "summary": view.summary,
                "limits": view.limits,
                "input_usage": view.input_usage,
                "tool_names": tools.into_iter().map(|tool| tool.name).collect::<Vec<_>>(),
            }),
            as_json,
        );
    }

    minco_mcp::serve_stdio(view)
        .await
        .context("serve local Minco MCP over stdio")
}

async fn workbench_command(
    root: &Path,
    manifest: &MincoManifest,
    args: WorkbenchArgs,
    as_json: bool,
    explicit_root: bool,
) -> Result<()> {
    if args.check && args.command.is_some() {
        bail!("--check cannot be combined with a workbench subcommand");
    }
    if !args.check && args.command.is_none() {
        bail!("choose --check or a local workbench subcommand");
    }

    let canonical_root = root
        .canonicalize()
        .with_context(|| format!("resolve workbench project root {}", root.display()))?;
    let view = minco_project_view::load_project_view(&canonical_root)
        .context("build bounded Minco ProjectView")?;
    match args.command {
        None => print_value(&minco_workbench::check_report(&view), as_json),
        Some(WorkbenchCommand::Export(export)) => {
            let format = match export.format {
                WorkbenchExportFormat::Json => minco_workbench::ExportFormat::Json,
                WorkbenchExportFormat::Mermaid => minco_workbench::ExportFormat::Mermaid,
                WorkbenchExportFormat::Static => minco_workbench::ExportFormat::Static,
            };
            let canonical_inputs = workbench_canonical_inputs(manifest, &view);
            let report = minco_workbench::export_project_view(
                &view,
                minco_workbench::ExportRequest {
                    root: &canonical_root,
                    destination: &export.output,
                    canonical_inputs: &canonical_inputs,
                    format,
                },
            )
            .context("export bounded Minco ProjectView")?;
            print_value(&report, as_json)
        }
        Some(WorkbenchCommand::Serve(serve)) => {
            if !explicit_root {
                bail!(
                    "starting the Minco workbench requires an explicit canonical project root via --root"
                );
            }
            let listener = minco_workbench::bind_loopback(serve.port)
                .await
                .context("bind local Minco workbench")?;
            let address = listener
                .local_addr()
                .context("read local Minco workbench address")?;
            let origin = format!("http://{address}");
            if as_json {
                println!(
                    "{}",
                    serde_json::to_string(&json!({
                        "schema_version": 1,
                        "status": "serving",
                        "read_only": true,
                        "loopback": true,
                        "origin": origin,
                        "address": address,
                    }))?
                );
            } else {
                println!("Minco Workbench: {origin}");
            }
            std::io::stdout()
                .flush()
                .context("flush workbench origin")?;
            minco_workbench::serve_loopback(listener, view)
                .await
                .context("serve local Minco workbench")
        }
    }
}

fn workbench_canonical_inputs(
    manifest: &MincoManifest,
    view: &minco_project_view::ProjectView,
) -> Vec<PathBuf> {
    let mut inputs = vec![
        PathBuf::from("minco.toml"),
        manifest.contract.clone(),
        manifest.generated.clone(),
        manifest.deployment_config.clone(),
        manifest.roadmap.clone(),
        manifest.tasks.clone(),
        manifest.plugin_catalog.clone(),
        manifest.quality.clone(),
        manifest.configuration.root.clone(),
    ];
    inputs.extend(manifest.architecture.domain_roots.iter().cloned());
    inputs.extend(manifest.architecture.application_roots.iter().cloned());
    inputs.extend(manifest.architecture.api_roots.iter().cloned());
    inputs.extend(manifest.migrations.roots.iter().cloned());
    inputs.extend(manifest.seeds.roots.iter().cloned());
    for trace in manifest.operations.values() {
        inputs.extend(trace.contract.iter().cloned());
        inputs.extend(trace.generated.iter().cloned());
        inputs.extend(trace.tests.iter().map(PathBuf::from));
        inputs.extend(
            trace
                .handler
                .iter()
                .chain(trace.application.iter())
                .filter_map(|reference| reference.split('#').next())
                .filter(|path| !path.is_empty())
                .map(PathBuf::from),
        );
    }
    inputs.extend(view.provenance.iter().map(|source| source.path.clone()));
    inputs.sort();
    inputs.dedup();
    inputs
}

fn promote_command(root: &Path, args: &PromoteArgs, as_json: bool) -> Result<()> {
    for (path, label) in [
        (&args.manifest, "release manifest"),
        (&args.receipt, "deployment receipt"),
        (&args.verification, "hosted verification"),
        (&args.output, "promotion receipt"),
    ] {
        if path.as_os_str().is_empty()
            || path.is_absolute()
            || !path
                .components()
                .all(|component| matches!(component, std::path::Component::Normal(_)))
        {
            bail!("{label} must be a normalized project-relative path");
        }
    }
    if args.canary
        && (args.canary_output.as_os_str().is_empty()
            || args.canary_output.is_absolute()
            || !args
                .canary_output
                .components()
                .all(|component| matches!(component, std::path::Component::Normal(_))))
    {
        bail!("canary receipt must be a normalized project-relative path");
    }
    if args.canary
        && (args.target_config.as_os_str().is_empty()
            || args.target_config.is_absolute()
            || !args
                .target_config
                .components()
                .all(|component| matches!(component, std::path::Component::Normal(_))))
    {
        bail!("deployment target configuration must be a normalized project-relative path");
    }
    let output_path = package_output_path(root, &args.output)?;
    let canary_output_path = args
        .canary
        .then(|| package_output_path(root, &args.canary_output))
        .transpose()?;
    let mut blockers = Vec::new();
    if !root.join(&args.manifest).is_file() {
        blockers.push("release_manifest_missing");
    }
    if !root.join(&args.receipt).is_file() {
        blockers.push("deployment_receipt_missing");
    }
    if !root.join(&args.verification).is_file() {
        blockers.push("hosted_verification_missing");
    }
    if output_path.exists() {
        blockers.push("promotion_receipt_exists");
    }
    if args.approve_verification_digest.is_none() {
        blockers.push("verification_approval_missing");
    }
    if canary_output_path
        .as_ref()
        .is_some_and(|path| path.exists())
    {
        blockers.push("canary_receipt_exists");
    }
    let canary_policy = if args.canary {
        if root.join(&args.target_config).is_file() {
            let catalog = DeploymentTargetCatalog::from_toml(&fs::read_to_string(
                root.join(&args.target_config),
            )?)?;
            let selected = catalog.select(args.environment.as_deref())?;
            if selected.target.canary.is_none() {
                blockers.push("canary_policy_missing");
            }
            selected.target.canary
        } else {
            blockers.push("deployment_target_config_missing");
            None
        }
    } else {
        None
    };
    if args.dry_run {
        return print_value(
            &json!({
                "external_aws_contact": false,
                "rebuild": false,
                "replan": false,
                "routing_boundary": "lambda_alias",
                "release_manifest": args.manifest,
                "deployment_receipt": args.receipt,
                "hosted_verification": args.verification,
                "promotion_receipt": args.output,
                "canary_receipt": args.canary.then_some(&args.canary_output),
                "mode": if args.canary { "alarm_guarded_api_canary" } else { "immediate_alias_promotion" },
                "canary": canary_policy.as_ref().map(|policy| json!({
                    "initial_traffic_percent": policy.initial_traffic_percent,
                    "monitoring_minutes": policy.monitoring_minutes,
                    "alarm_arns": policy.alarm_arns,
                    "api_routing": policy.api_routing,
                    "worker_routing": policy.worker_routing,
                    "additional_resources": [],
                    "idle_compute_cost": "none",
                    "pricing_complete": false,
                    "cost_notes": ["existing externally managed CloudWatch alarm pricing is account-specific"],
                    "pre_traffic_verification_required": true,
                    "post_traffic_verification_required": true,
                    "alarm_or_missing_observation_action": "reverse_to_previous_alias_routing"
                })),
                "blockers": blockers,
            }),
            as_json,
        );
    }
    if !blockers.is_empty() {
        bail!("promotion is blocked: {}", blockers.join(", "));
    }
    if !command_available("aws") {
        bail!("promotion requires `aws`");
    }
    for (path, label) in [
        (&args.manifest, "release manifest"),
        (&args.receipt, "deployment receipt"),
        (&args.verification, "hosted verification"),
    ] {
        validate_project_file(root, path, label)?;
    }

    let evidence = verified_deployment_evidence(root, &args.manifest, &args.receipt)?;
    if evidence.deployment.outcome() != DeploymentOutcome::Succeeded {
        bail!("promotion requires a successfully hosted-verified deployment receipt");
    }
    let api_artifact = exact_api_artifact(&evidence.release)?;
    let hosted_verification = FileDigest::from_rooted_path(root, root.join(&args.verification))?;
    let hosted_bindings = evidence
        .deployment
        .verification()
        .iter()
        .filter(|verification| verification.kind == "hosted_verification")
        .collect::<Vec<_>>();
    let [verification] = hosted_bindings.as_slice() else {
        bail!("deployment receipt must contain exactly one hosted verification binding");
    };
    if verification.file != hosted_verification {
        bail!("deployment receipt does not bind the selected hosted verification report");
    }
    for verification in evidence.deployment.verification() {
        match verification.kind.as_str() {
            "hosted_verification" => {}
            "static_site_verification" => {
                let report: StaticSiteVerificationReport = read_strict_json(
                    &root.join(&verification.file.path),
                    "static-site verification report",
                )?;
                report.verify_structure()?;
                if report.release_digest != evidence.release.release_digest {
                    bail!("static-site verification does not bind the promoted release");
                }
            }
            kind => bail!("deployment receipt contains unsupported verification kind {kind}"),
        }
    }
    let report = HostedVerificationReport::read_json(
        root.join(&args.verification),
        &api_artifact.file.sha256,
    )?;
    let approval = args
        .approve_verification_digest
        .as_deref()
        .context("hosted verification digest approval is required")?;
    if approval != hosted_verification.sha256 {
        bail!("hosted verification approval does not match the exact report digest");
    }
    require_exact_source(root, &evidence.release)?;
    verify_current_caller(root, &evidence.target)?;
    let mut stack = describe_target_stack(root, &evidence.target)?
        .context("promotion target stack no longer exists")?;
    require_stable_update_stack(&stack.stack_status)?;
    let function_name = stack_output(&stack, "ApiFunctionName")?.to_owned();
    let candidate_endpoint = canonical_hosted_endpoint(stack_output(&stack, "CandidateApiUrl")?)?;
    if candidate_endpoint != report.endpoint {
        bail!("hosted verification endpoint does not match the current candidate stage");
    }
    verify_candidate_function(
        root,
        &evidence.target,
        &function_name,
        &report.executed_version,
        &report.executed_artifact_digest,
    )?;
    let previous_version = stack_parameter(&stack, LIVE_FUNCTION_VERSION_PARAMETER)?.to_owned();
    if previous_version == report.executed_version {
        bail!("live routing already targets the hosted-verified function version");
    }
    if previous_version != "candidate"
        && !previous_version
            .parse::<u64>()
            .ok()
            .is_some_and(|version| version > 0 && version.to_string() == previous_version)
    {
        bail!("current live function version is not a guarded routing value");
    }
    detect_clean_stack_drift(root, &evidence.target)?;
    if args.canary {
        let policy = evidence
            .target
            .canary
            .clone()
            .context("reviewed deployment target has no canary policy")?;
        let plan = plan_canary_shift(CanaryShiftInput {
            policy,
            expected_account_id: evidence.target.expected_account_id.clone(),
            expected_region: evidence.target.expected_region.clone(),
            stack_name: evidence.target.stack_name.clone(),
            function_name: function_name.clone(),
            alias_name: "live".into(),
            current_version: previous_version.clone(),
            candidate_version: report.executed_version.clone(),
            pre_traffic_verification_digest: hosted_verification.sha256.clone(),
        })?;
        execute_canary_qualification(
            root,
            &evidence.target,
            &evidence.change_set,
            &stack,
            plan,
            canary_output_path
                .as_deref()
                .context("canary receipt path is missing")?,
        )?;
        detect_clean_stack_drift(root, &evidence.target)?;
        stack = describe_target_stack(root, &evidence.target)?
            .context("promotion target stack disappeared after canary qualification")?;
        require_stable_update_stack(&stack.stack_status)?;
        if stack_parameter(&stack, LIVE_FUNCTION_VERSION_PARAMETER)? != previous_version {
            bail!("canary qualification changed the live function version parameter");
        }
    }
    let change_set = create_promotion_change_set(
        root,
        &evidence.target,
        &evidence.change_set,
        &stack,
        &hosted_verification.sha256,
        &report.executed_version,
    )?;
    verify_promotion_boundary(
        &change_set,
        &evidence.target.stack_name,
        LIVE_ALIAS_LOGICAL_ID,
    )?;

    ensure_parent(&output_path)?;
    let mut promotion = PromotionReceipt::start(PromotionReceiptInput {
        attempt_id: uuid::Uuid::now_v7().to_string(),
        release_id: evidence.release.release_id.clone(),
        release_digest: evidence.release.release_digest.clone(),
        environment: evidence.release.environment.clone(),
        deployment_receipt: evidence.deployment_receipt,
        hosted_verification,
        operator_approval_digest: approval.to_owned(),
        stack_name: evidence.target.stack_name.clone(),
        live_alias_logical_id: LIVE_ALIAS_LOGICAL_ID.into(),
        previous_version,
        promoted_version: report.executed_version.clone(),
        change_set,
    })?;
    promotion.write_json(&output_path)?;
    promotion.verify_at(root)?;
    let started_digest = promotion.receipt_digest.clone();

    if let Err(error) = verify_current_caller(root, &evidence.target) {
        promotion.fail("promotion_caller_changed")?;
        promotion.write_json(&output_path)?;
        return Err(error);
    }
    if let Err(error) = run_cloud_output(
        root,
        "aws",
        "execute the exact promotion routing change set",
        &[
            "cloudformation".into(),
            "execute-change-set".into(),
            "--change-set-name".into(),
            promotion.change_set.change_set_id.clone(),
            "--client-request-token".into(),
            started_digest,
            "--region".into(),
            evidence.target.expected_region.clone(),
        ],
    ) {
        promotion.fail("cloudformation_promotion_execute_failed")?;
        promotion.write_json(&output_path)?;
        return Err(error);
    }
    if let Err(error) = run_cloud_output(
        root,
        "aws",
        "wait for the promotion routing update",
        &[
            "cloudformation".into(),
            "wait".into(),
            "stack-update-complete".into(),
            "--stack-name".into(),
            evidence.target.stack_name.clone(),
            "--region".into(),
            evidence.target.expected_region.clone(),
        ],
    ) {
        promotion.fail("cloudformation_promotion_wait_failed")?;
        promotion.write_json(&output_path)?;
        return Err(error);
    }
    let postcheck = (|| -> Result<()> {
        verify_current_caller(root, &evidence.target)?;
        let current = describe_target_stack(root, &evidence.target)?
            .context("promotion target stack disappeared after update")?;
        require_stable_update_stack(&current.stack_status)?;
        if stack_parameter(&current, LIVE_FUNCTION_VERSION_PARAMETER)? != report.executed_version {
            bail!("live routing parameter does not match the promoted function version");
        }
        if stack_output(&current, "ApiFunctionName")? != function_name {
            bail!("promotion unexpectedly changed the API function identity");
        }
        verify_candidate_function(
            root,
            &evidence.target,
            &function_name,
            &report.executed_version,
            &report.executed_artifact_digest,
        )?;
        verify_function_alias(
            root,
            &evidence.target,
            &function_name,
            "live",
            &report.executed_version,
            &report.executed_artifact_digest,
        )
    })();
    if let Err(error) = postcheck {
        promotion.fail("promotion_postcheck_failed")?;
        promotion.write_json(&output_path)?;
        return Err(error);
    }
    promotion.succeed()?;
    promotion.write_json(&output_path)?;
    promotion.verify_at(root)?;
    print_value(
        &json!({
            "promoted": true,
            "rebuild": false,
            "replan": false,
            "routing_boundary": "lambda_alias",
            "promotion_receipt": promotion,
            "promotion_receipt_path": args.output,
            "hosted_verification_path": args.verification,
            "production_runtime_proof": false,
        }),
        as_json,
    )
}

fn rollback_command(root: &Path, args: &RollbackArgs, as_json: bool) -> Result<()> {
    for (path, label) in [
        (&args.current_promotion, "current promotion receipt"),
        (&args.target_promotion, "target promotion receipt"),
    ] {
        if path.as_os_str().is_empty()
            || path.is_absolute()
            || !path
                .components()
                .all(|component| matches!(component, std::path::Component::Normal(_)))
        {
            bail!("{label} must be a normalized project-relative path");
        }
    }
    if let Some(path) = &args.data_compatibility_evidence
        && (path.as_os_str().is_empty()
            || path.is_absolute()
            || !path
                .components()
                .all(|component| matches!(component, std::path::Component::Normal(_))))
    {
        bail!("data compatibility evidence must be a normalized project-relative path");
    }
    let current_root = rollback_evidence_root(root, args.current_root.as_deref(), "current")?;
    let target_root = rollback_evidence_root(root, args.target_root.as_deref(), "target")?;

    let mut blockers = Vec::new();
    if !current_root.join(&args.current_promotion).is_file() {
        blockers.push("current_promotion_receipt_missing");
    }
    if !target_root.join(&args.target_promotion).is_file() {
        blockers.push("target_promotion_receipt_missing");
    }
    if args
        .data_compatibility_evidence
        .as_ref()
        .is_some_and(|path| !root.join(path).is_file())
    {
        blockers.push("data_compatibility_evidence_missing");
    }
    if !blockers.is_empty() {
        if args.dry_run {
            return print_value(
                &json!({
                    "operation": "rollback_compatibility_assessment",
                    "external_aws_contact": false,
                    "rebuild": false,
                    "replan": false,
                    "reverse_sql": false,
                    "automatic_data_repair": false,
                    "reuse_historical_hosted_report": false,
                    "current_evidence_root": current_root,
                    "target_evidence_root": target_root,
                    "current_promotion_receipt": args.current_promotion,
                    "target_promotion_receipt": args.target_promotion,
                    "assessment": null,
                    "blockers": blockers,
                }),
                as_json,
            );
        }
        bail!("rollback assessment is blocked: {}", blockers.join(", "));
    }
    validate_project_file(
        &current_root,
        &args.current_promotion,
        "current promotion receipt",
    )?;
    validate_project_file(
        &target_root,
        &args.target_promotion,
        "target promotion receipt",
    )?;
    if let Some(path) = &args.data_compatibility_evidence {
        validate_project_file(root, path, "rollback data compatibility evidence")?;
    }

    let (current_promotion, current_deployment, current_release, current_target) =
        successful_promotion_release(&current_root, &args.current_promotion, "current")?;
    let (target_promotion, target_deployment, target_release, target_target) =
        successful_promotion_release(&target_root, &args.target_promotion, "target")?;
    require_exact_source(&current_root, &current_release)
        .context("current rollback evidence root is not at its sealed source revision")?;
    require_exact_source(&target_root, &target_release)
        .context("target rollback evidence root is not at its sealed source revision")?;
    let current_contract = load_contract(current_root.join(&current_release.contract.path))?;
    let target_contract = load_contract(target_root.join(&target_release.contract.path))?;
    if !current_contract.is_valid() || !target_contract.is_valid() {
        bail!("rollback assessment requires two valid sealed OpenAPI contracts");
    }
    let contract_report = diff_contracts(&current_contract.document, &target_contract.document);
    let contract = match contract_report.classification {
        CompatibilityClassification::NonBreaking => RollbackCompatibility::Compatible,
        CompatibilityClassification::Uncertain => RollbackCompatibility::OperatorDecisionRequired,
        CompatibilityClassification::Breaking => RollbackCompatibility::Incompatible,
    };
    let (data_compatibility, data_compatibility_evidence_digest) = if let Some(path) =
        &args.data_compatibility_evidence
    {
        let evidence: RollbackDataCompatibilityEvidence =
            read_strict_json(&root.join(path), "rollback data compatibility evidence")?;
        if evidence.schema_version != 1
            || evidence.current_release_id != current_release.release_id
            || evidence.target_release_id != target_release.release_id
            || evidence.decision == RollbackCompatibility::OperatorDecisionRequired
            || evidence.reviewed_by.trim().is_empty()
            || evidence.reviewed_by.len() > 256
            || evidence.reason.trim().is_empty()
            || evidence.reason.len() > 2_048
            || evidence.reviewed_by.chars().any(char::is_control)
            || evidence.reason.chars().any(char::is_control)
        {
            bail!(
                "rollback data compatibility evidence does not exactly bind the two releases and a compatible or incompatible decision"
            );
        }
        let digest = FileDigest::from_rooted_path(root, root.join(path))?.sha256;
        (evidence.decision, Some(digest))
    } else {
        (RollbackCompatibility::OperatorDecisionRequired, None)
    };
    let worker_artifacts = |release: &ReleaseManifest| {
        release
            .artifacts
            .iter()
            .filter(|artifact| artifact.function_id != "api")
            .map(|artifact| (artifact.function_id.clone(), artifact.file.sha256.clone()))
            .collect::<std::collections::BTreeMap<_, _>>()
    };
    let assessment = assess_rollback_compatibility(RollbackAssessmentInput {
        current_release_id: current_release.release_id.clone(),
        target_release_id: target_release.release_id.clone(),
        current_environment: format!(
            "{}/{}/{}@{}:{}#{}:{}",
            current_release.environment.application,
            current_release.environment.environment,
            current_release.environment.region,
            current_target.expected_account_id,
            current_target.expected_role_arn,
            current_promotion.stack_name,
            current_promotion.live_alias_logical_id,
        ),
        target_environment: format!(
            "{}/{}/{}@{}:{}#{}:{}",
            target_release.environment.application,
            target_release.environment.environment,
            target_release.environment.region,
            target_target.expected_account_id,
            target_target.expected_role_arn,
            target_promotion.stack_name,
            target_promotion.live_alias_logical_id,
        ),
        contract,
        current_configuration_digest: current_release.configuration_digest.clone(),
        target_configuration_digest: target_release.configuration_digest.clone(),
        current_deployment_plan_digest: current_release.deployment_plan.sha256.clone(),
        target_deployment_plan_digest: target_release.deployment_plan.sha256.clone(),
        current_migration_catalog_digest: current_release
            .database_sources
            .migration_catalog
            .clone(),
        target_migration_catalog_digest: target_release.database_sources.migration_catalog.clone(),
        current_migration_plan_bindings_digest: database_plan_bindings_digest(
            &current_deployment,
            DatabasePlanKind::Migration,
        )?,
        target_migration_plan_bindings_digest: database_plan_bindings_digest(
            &target_deployment,
            DatabasePlanKind::Migration,
        )?,
        current_seed_catalog_digest: current_release.database_sources.seed_catalog.clone(),
        target_seed_catalog_digest: target_release.database_sources.seed_catalog.clone(),
        current_seed_plan_bindings_digest: database_plan_bindings_digest(
            &current_deployment,
            DatabasePlanKind::Seed,
        )?,
        target_seed_plan_bindings_digest: database_plan_bindings_digest(
            &target_deployment,
            DatabasePlanKind::Seed,
        )?,
        data_compatibility,
        data_compatibility_evidence_digest,
        current_api_version: current_promotion.promoted_version,
        target_api_version: target_promotion.promoted_version.clone(),
        current_worker_artifacts: worker_artifacts(&current_release),
        target_worker_artifacts: worker_artifacts(&target_release),
    })?;
    print_value(
        &json!({
            "operation": "rollback_compatibility_assessment",
            "external_aws_contact": false,
            "rebuild": false,
            "replan": false,
            "reverse_sql": false,
            "automatic_data_repair": false,
            "reuse_historical_hosted_report": false,
            "current_evidence_root": current_root,
            "target_evidence_root": target_root,
            "assessment": assessment,
            "contract_report": contract_report,
            "routing_authorized": assessment.classification == minco_deploy_aws::RollbackClassification::Compatible,
            "next_required_boundary": {
                "action": "redeploy_exact_target_release_as_candidate_then_verify_and_promote",
                "target_release_manifest": target_release_path(&target_root, &target_promotion)?,
                "target_release_root": target_root,
                "target_source_revision": target_release.source_change,
                "rebuild": false,
                "replan": false,
                "reuse_historical_hosted_report_for_new_candidate": false,
                "reason": "the stable candidate alias currently names the latest deployment, so the older exact artifact must become a newly hosted-verified candidate before live routing"
            },
            "worker_routing": "preserve_current_worker_event_sources",
            "blockers": [],
        }),
        as_json,
    )
}

fn rollback_evidence_root(
    command_root: &Path,
    explicit_root: Option<&Path>,
    label: &str,
) -> Result<PathBuf> {
    let candidate = if let Some(root) = explicit_root {
        if !root.is_absolute() {
            bail!("{label} rollback evidence root must be an absolute path");
        }
        root
    } else {
        command_root
    };
    let metadata = candidate
        .symlink_metadata()
        .with_context(|| format!("inspect {label} rollback evidence root"))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        bail!("{label} rollback evidence root must be a non-symlink directory");
    }
    candidate
        .canonicalize()
        .with_context(|| format!("canonicalize {label} rollback evidence root"))
}

fn successful_promotion_release(
    root: &Path,
    receipt_path: &Path,
    label: &str,
) -> Result<(
    PromotionReceipt,
    DeploymentReceipt,
    ReleaseManifest,
    DeploymentTarget,
)> {
    let receipt = PromotionReceipt::read_json(root.join(receipt_path))?;
    receipt.verify_at(root)?;
    if receipt.outcome() != PromotionOutcome::Succeeded {
        bail!("{label} promotion receipt is not successful");
    }
    let deployment: DeploymentReceipt = read_strict_json(
        &root.join(&receipt.deployment_receipt.path),
        &format!("{label} deployment receipt"),
    )?;
    deployment.verify_at(root)?;
    let evidence = verified_deployment_evidence(
        root,
        Path::new(&deployment.release_manifest.path),
        Path::new(&receipt.deployment_receipt.path),
    )?;
    if evidence.deployment.outcome() != DeploymentOutcome::Succeeded
        || receipt.deployment_receipt != evidence.deployment_receipt
        || receipt.release_id != evidence.release.release_id
        || receipt.release_digest != evidence.release.release_digest
        || receipt.environment != evidence.release.environment
        || receipt.stack_name != evidence.target.stack_name
    {
        bail!("{label} promotion does not bind one exact successful deployment target");
    }
    Ok((
        receipt,
        evidence.deployment,
        evidence.release,
        evidence.target,
    ))
}

fn database_plan_bindings_digest(
    deployment: &DeploymentReceipt,
    kind: DatabasePlanKind,
) -> Result<String> {
    let mut bindings = deployment
        .database_plans
        .iter()
        .filter(|binding| binding.kind == kind)
        .map(|binding| {
            (
                binding.kind,
                binding.schema_version,
                &binding.catalog_digest,
                &binding.plan_digest,
                &binding.file.sha256,
                binding.file.bytes,
                &binding.selected_set,
                &binding.environment,
            )
        })
        .collect::<Vec<_>>();
    bindings.sort_unstable();
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&bindings)?)
    ))
}

fn target_release_path(root: &Path, promotion: &PromotionReceipt) -> Result<PathBuf> {
    let deployment: DeploymentReceipt = read_strict_json(
        &root.join(&promotion.deployment_receipt.path),
        "target deployment receipt",
    )?;
    Ok(PathBuf::from(deployment.release_manifest.path))
}

fn destroy_command(root: &Path, args: &DestroyArgs, as_json: bool) -> Result<()> {
    validate_project_file(root, &args.target_config, "deployment target configuration")?;
    if root.join(&args.review).exists() {
        validate_project_file(root, &args.review, "review manifest")?;
    } else if args.review.is_absolute()
        || !args
            .review
            .components()
            .all(|component| matches!(component, std::path::Component::Normal(_)))
    {
        bail!("review manifest must be a project-relative path");
    }
    let receipt_output = package_output_path(root, &args.receipt)?;
    let catalog =
        DeploymentTargetCatalog::from_toml(&fs::read_to_string(root.join(&args.target_config))?)?;
    let selected = catalog.select(args.environment.as_deref())?;
    let mut blockers = Vec::new();
    if !selected.target.enabled {
        blockers.push("target_disabled");
    }
    if selected.target.lifecycle != DeploymentTargetLifecycle::Preview {
        blockers.push("target_not_preview");
    }
    let preview = selected.target.preview.as_ref();
    let review = if root.join(&args.review).is_file() {
        if let Ok(review) = ReviewManifest::from_json(&fs::read(root.join(&args.review))?)
            .and_then(|review| review.verify_at(root).map(|()| review))
        {
            let configured_resources_match = preview.is_some_and(|preview| {
                preview
                    .resources
                    .iter()
                    .all(|resource| review.resources.contains(resource))
            });
            if review.environment.environment != selected.environment
                || review.environment.region != selected.target.expected_region
                || review.expected_account_id != selected.target.expected_account_id
                || review.expected_role_arn != selected.target.expected_role_arn
                || review.stack_name != selected.target.stack_name
                || !configured_resources_match
            {
                blockers.push("review_target_mismatch");
            }
            Some(review)
        } else {
            blockers.push("review_manifest_invalid");
            None
        }
    } else {
        blockers.push("review_manifest_missing");
        None
    };
    match (review.as_ref(), args.approve_review_digest.as_deref()) {
        (_, None) => blockers.push("review_approval_missing"),
        (Some(review), Some(approval)) if approval != review.manifest_digest => {
            blockers.push("review_approval_mismatch");
        }
        _ => {}
    }
    if receipt_output.exists() {
        blockers.push("cleanup_receipt_exists");
    }
    let reviewed_resources = review
        .as_ref()
        .map(|review| review.resources.as_slice())
        .or_else(|| preview.map(|preview| preview.resources.as_slice()))
        .unwrap_or_default();
    let deleted_resources = reviewed_resources
        .iter()
        .filter(|resource| resource.retention == ReviewResourceRetention::Delete)
        .cloned()
        .collect::<Vec<_>>();
    let retained_resources = reviewed_resources
        .iter()
        .filter(|resource| resource.retention == ReviewResourceRetention::Retain)
        .cloned()
        .collect::<Vec<_>>();

    let plan = json!({
        "schema_version": 1,
        "operation": "destroy_preview",
        "dry_run": args.dry_run,
        "external_aws_contact": false,
        "infrastructure_change": false,
        "cleanup_receipt_written": false,
        "target_config": args.target_config,
        "target": {
            "environment": selected.environment,
            "lifecycle": selected.target.lifecycle,
            "enabled": selected.target.enabled,
            "expected_account_id": selected.target.expected_account_id,
            "expected_region": selected.target.expected_region,
            "expected_role_arn": selected.target.expected_role_arn,
            "stack_name": selected.target.stack_name,
        },
        "review_manifest": args.review,
        "review": review.as_ref().map(|review| json!({
            "review_id": review.review_id,
            "manifest_digest": review.manifest_digest,
            "owner": review.owner,
            "expires_at": review.expires_at,
            "pricing_complete": review.pricing_complete,
        })),
        "deleted_resources": deleted_resources,
        "retained_resources": retained_resources,
        "cleanup_receipt": args.receipt,
        "guard_requirements": [
            "exact_preview_lifecycle",
            "exact_review_manifest",
            "exact_target_account_region_role_stack",
            "exact_resource_inventory",
            "termination_protection_disabled",
            "exact_review_digest_approval",
        ],
        "blockers": blockers,
    });
    if args.dry_run {
        return print_value(&plan, as_json);
    }
    if !blockers.is_empty() {
        bail!("preview cleanup is blocked: {}", blockers.join(", "));
    }
    let review = review.context("exact review manifest is required")?;
    apply_preview_cleanup(
        root,
        args,
        &selected.target,
        &review,
        &receipt_output,
        as_json,
    )
}

fn apply_preview_cleanup(
    root: &Path,
    args: &DestroyArgs,
    target: &DeploymentTarget,
    review: &ReviewManifest,
    receipt_output: &Path,
    as_json: bool,
) -> Result<()> {
    if !command_available("aws") {
        bail!("preview cleanup requires `aws`");
    }
    if vcs::source_snapshot(root)?.change != review.source_change {
        bail!("current source does not match the exact reviewed preview release");
    }
    review.verify_at(root)?;
    verify_current_caller(root, target)?;
    let stack = describe_target_stack(root, target)?
        .context("reviewed preview CloudFormation stack no longer exists")?;
    require_stable_update_stack(&stack.stack_status)?;
    if stack.enable_termination_protection != Some(false) {
        bail!("preview cleanup refuses enabled or unproved CloudFormation termination protection");
    }
    let provider_resources: AwsStackResources = aws_json(
        root,
        &target.expected_region,
        "inspect exact preview stack resources",
        &[
            "cloudformation",
            "list-stack-resources",
            "--stack-name",
            &target.stack_name,
        ],
    )?;
    verify_preview_resource_inventory(&review.resources, &provider_resources)?;
    let processed_template: AwsProcessedTemplate = aws_json(
        root,
        &target.expected_region,
        "inspect preview stack retention policy",
        &[
            "cloudformation",
            "get-template",
            "--stack-name",
            &target.stack_name,
            "--template-stage",
            "Processed",
        ],
    )?;
    verify_preview_retention_policy(&review.resources, &processed_template.template_body)?;

    let current_review = FileDigest::from_rooted_path(root, root.join(&args.review))?;
    let current_target = FileDigest::from_rooted_path(root, root.join(&args.target_config))?;
    if current_target != review.target_config {
        bail!("deployment target changed after the exact review was created");
    }
    let mut receipt = CleanupReceipt::start(CleanupReceiptInput {
        attempt_id: uuid::Uuid::now_v7().to_string(),
        review_manifest: current_review,
        review_id: review.review_id.clone(),
        review_digest: review.manifest_digest.clone(),
        environment: review.environment.clone(),
        expected_account_id: review.expected_account_id.clone(),
        expected_role_arn: review.expected_role_arn.clone(),
        stack_name: review.stack_name.clone(),
        target_config: current_target,
        deleted_resources: review
            .resources
            .iter()
            .filter(|resource| resource.retention == ReviewResourceRetention::Delete)
            .cloned()
            .collect(),
        retained_resources: review
            .resources
            .iter()
            .filter(|resource| resource.retention == ReviewResourceRetention::Retain)
            .cloned()
            .collect(),
    })?;
    receipt.verify_at(root)?;
    ensure_parent(receipt_output)?;
    receipt.write_json(receipt_output)?;

    if let Err(error) = run_cloud_output(
        root,
        "aws",
        "delete the exact reviewed preview stack",
        &[
            "cloudformation".into(),
            "delete-stack".into(),
            "--stack-name".into(),
            target.stack_name.clone(),
            "--client-request-token".into(),
            receipt.receipt_digest.clone(),
            "--deletion-mode".into(),
            "STANDARD".into(),
            "--region".into(),
            target.expected_region.clone(),
        ],
    ) {
        receipt.fail("cloudformation_delete_start_failed")?;
        receipt.write_json(receipt_output)?;
        return Err(error);
    }
    if let Err(error) = run_cloud_output(
        root,
        "aws",
        "wait for exact preview stack deletion",
        &[
            "cloudformation".into(),
            "wait".into(),
            "stack-delete-complete".into(),
            "--stack-name".into(),
            target.stack_name.clone(),
            "--region".into(),
            target.expected_region.clone(),
        ],
    ) {
        receipt.fail("cloudformation_delete_failed")?;
        receipt.write_json(receipt_output)?;
        return Err(error);
    }
    if describe_target_stack(root, target)?.is_some() {
        receipt.fail("cloudformation_absence_unproved")?;
        receipt.write_json(receipt_output)?;
        bail!("CloudFormation waiter completed but stack absence was not verified");
    }
    receipt.succeed(chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string())?;
    receipt.verify_at(root)?;
    receipt.write_json(receipt_output)?;
    CleanupReceipt::read_json(receipt_output)?.verify_at(root)?;
    print_value(
        &json!({
            "preview_destroyed": true,
            "stack_absence_verified": true,
            "deleted_resources": receipt.deleted_resources,
            "retained_resources": receipt.retained_resources,
            "cleanup_receipt": receipt,
            "cleanup_receipt_path": args.receipt,
        }),
        as_json,
    )
}

struct VerifiedDeploymentEvidence {
    release: ReleaseManifest,
    deployment: DeploymentReceipt,
    deployment_receipt: FileDigest,
    change_set_receipt: FileDigest,
    change_set: ChangeSetReceipt,
    target: DeploymentTarget,
}

fn verified_deployment_evidence(
    root: &Path,
    manifest_path: &Path,
    receipt_path: &Path,
) -> Result<VerifiedDeploymentEvidence> {
    let release_path = root.join(manifest_path);
    let release: ReleaseManifest = read_strict_json(&release_path, "release manifest")?;
    release.verify_at(root)?;
    let release_manifest = FileDigest::from_rooted_path(root, &release_path)?;
    let deployment_path = root.join(receipt_path);
    let deployment: DeploymentReceipt = read_strict_json(&deployment_path, "deployment receipt")?;
    deployment.verify_at(root)?;
    if deployment.release_manifest != release_manifest
        || deployment.release_id != release.release_id
        || deployment.release_digest != release.release_digest
        || deployment.environment != release.environment
        || deployment.configuration_digest != release.configuration_digest
    {
        bail!("deployment receipt does not bind the exact verified release");
    }
    let matching = deployment
        .attestations
        .iter()
        .filter_map(|file| {
            let bytes = fs::read(root.join(&file.path)).ok()?;
            let receipt = ChangeSetReceipt::from_json(&bytes).ok()?;
            (receipt.release_manifest == release_manifest
                && receipt.release_id == release.release_id
                && receipt.release_digest == release.release_digest
                && receipt.environment == release.environment)
                .then(|| (file.clone(), receipt))
        })
        .collect::<Vec<_>>();
    if matching.len() != 1 {
        bail!("deployment receipt must bind exactly one matching change-set receipt");
    }
    let (change_set_file, change_set) = matching
        .into_iter()
        .next()
        .context("matching change-set receipt disappeared")?;
    change_set_file.verify_at(root)?;
    change_set.verify_at(root)?;
    let catalog = DeploymentTargetCatalog::from_toml(&fs::read_to_string(
        root.join(&change_set.target_config.path),
    )?)?;
    let selected = catalog.select(Some(&release.environment.environment))?;
    if !selected.target.enabled
        || selected.target.expected_account_id != change_set.expected_account_id
        || selected.target.expected_region != release.environment.region
        || selected.target.expected_role_arn != change_set.expected_role_arn
        || selected.target.stack_name != change_set.change_set.stack_name
    {
        bail!("deployment target no longer matches the reviewed release environment");
    }
    Ok(VerifiedDeploymentEvidence {
        release,
        deployment,
        deployment_receipt: FileDigest::from_rooted_path(root, deployment_path)?,
        change_set_receipt: change_set_file,
        change_set,
        target: selected.target,
    })
}

fn read_strict_json<T>(path: &Path, label: &str) -> Result<T>
where
    T: DeserializeOwned + Serialize,
{
    let bytes = fs::read(path).with_context(|| format!("read {label}"))?;
    let value: serde_json::Value =
        serde_json::from_slice(&bytes).with_context(|| format!("parse {label} JSON"))?;
    let parsed: T =
        serde_json::from_value(value.clone()).with_context(|| format!("decode {label}"))?;
    if serde_json::to_value(&parsed)? != value {
        bail!("{label} contains unknown or non-canonical fields");
    }
    Ok(parsed)
}

fn exact_api_artifact(release: &ReleaseManifest) -> Result<&FunctionArtifact> {
    let artifacts = release
        .artifacts
        .iter()
        .filter(|artifact| artifact.function_id == "api")
        .collect::<Vec<_>>();
    let [artifact] = artifacts.as_slice() else {
        bail!("verified release must contain exactly one api artifact");
    };
    Ok(artifact)
}

fn require_exact_source(root: &Path, release: &ReleaseManifest) -> Result<()> {
    if vcs::source_snapshot(root)?.change != release.source_change {
        bail!("current source does not match the exact verified release");
    }
    Ok(())
}

fn verify_current_caller(root: &Path, target: &DeploymentTarget) -> Result<()> {
    let identity: AwsCallerIdentity = aws_json(
        root,
        &target.expected_region,
        "verify current AWS caller identity",
        &["sts", "get-caller-identity"],
    )?;
    let role_arn = caller_role_arn(&identity.arn).context(
        "AWS caller identity must be the exact configured IAM role or an assumed-role session",
    )?;
    if identity.account != target.expected_account_id || role_arn != target.expected_role_arn {
        bail!("current AWS caller does not match the reviewed deployment target");
    }
    Ok(())
}

fn stack_output<'a>(stack: &'a AwsStack, key: &str) -> Result<&'a str> {
    let values = stack
        .outputs
        .iter()
        .filter(|output| output.output_key == key)
        .filter_map(|output| output.output_value.as_deref())
        .collect::<Vec<_>>();
    let [value] = values.as_slice() else {
        bail!("CloudFormation stack must expose exactly one {key} output");
    };
    Ok(value)
}

fn stack_parameter<'a>(stack: &'a AwsStack, key: &str) -> Result<&'a str> {
    let values = stack
        .parameters
        .iter()
        .filter(|parameter| parameter.parameter_key == key)
        .filter_map(|parameter| parameter.parameter_value.as_deref())
        .collect::<Vec<_>>();
    let [value] = values.as_slice() else {
        bail!("CloudFormation stack must expose exactly one {key} parameter");
    };
    Ok(value)
}

fn canonical_hosted_endpoint(value: &str) -> Result<String> {
    let endpoint = url::Url::parse(value).context("parse hosted endpoint")?;
    if endpoint.scheme() != "https"
        || endpoint.host_str().is_none()
        || !endpoint.username().is_empty()
        || endpoint.password().is_some()
        || endpoint.query().is_some()
        || endpoint.fragment().is_some()
    {
        bail!("hosted endpoint is not a redacted HTTPS URL");
    }
    Ok(endpoint.to_string().trim_end_matches('/').to_owned())
}

fn expected_lambda_code_sha256(artifact_digest: &str) -> Result<String> {
    if artifact_digest.len() != 64
        || !artifact_digest
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        bail!("release artifact digest is not a lowercase SHA-256 value");
    }
    let bytes = artifact_digest
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            std::str::from_utf8(pair)
                .context("artifact digest contains invalid UTF-8")
                .and_then(|pair| {
                    u8::from_str_radix(pair, 16).context("artifact digest contains invalid hex")
                })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(base64::engine::general_purpose::STANDARD.encode(bytes))
}

fn verify_candidate_function(
    root: &Path,
    target: &DeploymentTarget,
    function_name: &str,
    expected_version: &str,
    expected_artifact_digest: &str,
) -> Result<()> {
    verify_function_alias(
        root,
        target,
        function_name,
        "candidate",
        expected_version,
        expected_artifact_digest,
    )
}

fn verify_function_alias(
    root: &Path,
    target: &DeploymentTarget,
    function_name: &str,
    alias: &str,
    expected_version: &str,
    expected_artifact_digest: &str,
) -> Result<()> {
    let label = format!("verify hosted {alias} Lambda identity");
    let configuration: AwsFunctionConfiguration = aws_json(
        root,
        &target.expected_region,
        &label,
        &[
            "lambda",
            "get-function-configuration",
            "--function-name",
            function_name,
            "--qualifier",
            alias,
        ],
    )?;
    if configuration.function_name != function_name
        || configuration.version != expected_version
        || configuration.last_update_status != "Successful"
        || configuration.code_sha256 != expected_lambda_code_sha256(expected_artifact_digest)?
    {
        bail!("{alias} Lambda does not match the hosted-verified artifact and version");
    }
    Ok(())
}

fn execute_canary_qualification(
    root: &Path,
    target: &DeploymentTarget,
    original: &ChangeSetReceipt,
    expected_stack: &AwsStack,
    plan: minco_deploy_aws::CanaryShiftPlan,
    receipt_path: &Path,
) -> Result<()> {
    if plan.expected_account_id != target.expected_account_id
        || plan.expected_region != target.expected_region
        || plan.stack_name != target.stack_name
    {
        bail!("canary plan does not bind the exact reviewed deployment target");
    }
    verify_canary_version_compatibility(root, target, &plan)?;
    verify_canary_alarm_preconditions(root, target, &plan.alarm_arns)?;
    ensure_parent(receipt_path)?;
    let change_set = create_canary_change_set(root, target, original, expected_stack, &plan, true)?;
    verify_promotion_boundary(&change_set, &target.stack_name, LIVE_ALIAS_LOGICAL_ID)?;
    let mut receipt = CanaryExecutionReceipt::start(CanaryExecutionReceiptInput {
        attempt_id: uuid::Uuid::now_v7().to_string(),
        plan,
        change_set,
    })?;
    receipt.write_json(receipt_path)?;
    let started_digest = receipt.receipt_digest.clone();
    verify_current_caller(root, target)?;
    let execution = run_cloud_output(
        root,
        "aws",
        "execute the alarm-guarded canary routing change set",
        &[
            "cloudformation".into(),
            "execute-change-set".into(),
            "--change-set-name".into(),
            receipt.change_set.change_set_id.clone(),
            "--client-request-token".into(),
            started_digest,
            "--region".into(),
            target.expected_region.clone(),
        ],
    )
    .and_then(|_| {
        run_cloud_output(
            root,
            "aws",
            "wait for canary traffic and alarm monitoring",
            &[
                "cloudformation".into(),
                "wait".into(),
                "stack-update-complete".into(),
                "--stack-name".into(),
                target.stack_name.clone(),
                "--region".into(),
                target.expected_region.clone(),
            ],
        )
        .map(|_| ())
    });
    if let Err(error) = execution {
        if canary_alias_is_unweighted(
            root,
            target,
            &receipt.plan.function_name,
            &receipt.plan.current_version,
        )? {
            receipt.reverse("cloudformation_canary_reversed")?;
            receipt.write_json(receipt_path)?;
        }
        return Err(error);
    }
    let postcheck = verify_weighted_canary_alias(root, target, &receipt.plan)
        .and_then(|()| verify_canary_alarm_preconditions(root, target, &receipt.plan.alarm_arns));
    let canary_stack = describe_target_stack(root, target)?
        .context("canary target stack disappeared after alarm monitoring")?;
    require_stable_update_stack(&canary_stack.stack_status)?;
    let postcheck = postcheck.and_then(|()| {
        if stack_parameter(&canary_stack, LIVE_FUNCTION_VERSION_PARAMETER)?
            != receipt.plan.current_version
        {
            bail!("canary unexpectedly changed the live function version parameter");
        }
        Ok(())
    });

    let cleanup =
        create_canary_change_set(root, target, original, &canary_stack, &receipt.plan, false)?;
    verify_promotion_boundary(&cleanup, &target.stack_name, LIVE_ALIAS_LOGICAL_ID)?;
    run_cloud_output(
        root,
        "aws",
        "execute the canary routing cleanup change set",
        &[
            "cloudformation".into(),
            "execute-change-set".into(),
            "--change-set-name".into(),
            cleanup.change_set_id.clone(),
            "--client-request-token".into(),
            format!("{}-cleanup", &receipt.receipt_digest[..48]),
            "--region".into(),
            target.expected_region.clone(),
        ],
    )?;
    run_cloud_output(
        root,
        "aws",
        "wait for canary routing cleanup",
        &[
            "cloudformation".into(),
            "wait".into(),
            "stack-update-complete".into(),
            "--stack-name".into(),
            target.stack_name.clone(),
            "--region".into(),
            target.expected_region.clone(),
        ],
    )?;
    if !canary_alias_is_unweighted(
        root,
        target,
        &receipt.plan.function_name,
        &receipt.plan.current_version,
    )? {
        bail!("canary cleanup did not restore the previous unweighted live alias");
    }
    if let Err(error) = postcheck {
        receipt.reverse_with_cleanup("canary_postcheck_failed", cleanup)?;
        receipt.write_json(receipt_path)?;
        return Err(error);
    }
    receipt.succeed(cleanup)?;
    receipt.write_json(receipt_path)?;
    Ok(())
}

fn create_canary_change_set(
    root: &Path,
    target: &DeploymentTarget,
    original: &ChangeSetReceipt,
    expected_stack: &AwsStack,
    plan: &minco_deploy_aws::CanaryShiftPlan,
    enable: bool,
) -> Result<CloudFormationChangeSet> {
    original.verify_at(root)?;
    verify_current_caller(root, target)?;
    if vcs::source_snapshot(root)?.change != original.source_change {
        bail!("source changed before creating the canary change set");
    }
    let current_stack =
        describe_target_stack(root, target)?.context("canary target stack no longer exists")?;
    if current_stack != *expected_stack {
        bail!("CloudFormation stack identity or routing inputs changed during canary review");
    }
    let mut parameter_keys = current_stack
        .parameters
        .iter()
        .map(|parameter| parameter.parameter_key.as_str())
        .collect::<Vec<_>>();
    parameter_keys.sort_unstable();
    if parameter_keys.windows(2).any(|keys| keys[0] == keys[1])
        || !parameter_keys.contains(&LIVE_FUNCTION_VERSION_PARAMETER)
    {
        bail!("CloudFormation stack parameters are missing or duplicated");
    }
    let parameters = parameter_keys
        .into_iter()
        .map(AwsChangeSetParameter::previous)
        .collect::<Vec<_>>();
    let parameters = aws_change_set_parameters(&parameters)?;
    let plan_digest = format!("{:x}", Sha256::digest(serde_json::to_vec(plan)?));
    let (phase, template_path) = if enable {
        (
            "start",
            render_canary_template(root, original, plan, &plan_digest)?,
        )
    } else {
        ("cleanup", root.join(&original.packaged_template.path))
    };
    let name = format!("minco-canary-{phase}-{}", &plan_digest[..24]);
    let mut create_args = vec![
        "cloudformation".into(),
        "create-change-set".into(),
        "--stack-name".into(),
        target.stack_name.clone(),
        "--change-set-name".into(),
        name.clone(),
        "--change-set-type".into(),
        "UPDATE".into(),
        "--template-body".into(),
        format!("file://{}", template_path.display()),
        "--capabilities".into(),
        "CAPABILITY_IAM".into(),
        "--client-token".into(),
        format!("{phase}-{}", &plan_digest[..48]),
        "--description".into(),
        format!(
            "Minco canary {phase}: version {} at {} basis points",
            plan.candidate_version, plan.candidate_weight_basis_points
        ),
        "--parameters".into(),
        parameters,
    ];
    if enable {
        let rollback = json!({
            "RollbackTriggers": plan.alarm_arns.iter().map(|arn| json!({
                "Arn": arn,
                "Type": "AWS::CloudWatch::Alarm"
            })).collect::<Vec<_>>(),
            "MonitoringTimeInMinutes": plan.monitoring_minutes,
        });
        create_args.extend([
            "--rollback-configuration".into(),
            serde_json::to_string(&rollback)?,
        ]);
    }
    create_args.extend([
        "--region".into(),
        target.expected_region.clone(),
        "--output".into(),
        "json".into(),
    ]);
    run_cloud_output(
        root,
        "aws",
        "create the unexecuted canary routing change set",
        &create_args,
    )?;
    run_cloud_output(
        root,
        "aws",
        "wait for canary change-set creation",
        &[
            "cloudformation".into(),
            "wait".into(),
            "change-set-create-complete".into(),
            "--stack-name".into(),
            target.stack_name.clone(),
            "--change-set-name".into(),
            name.clone(),
            "--region".into(),
            target.expected_region.clone(),
        ],
    )?;
    let described = run_cloud_output(
        root,
        "aws",
        "describe the canary routing change set",
        &[
            "cloudformation".into(),
            "describe-change-set".into(),
            "--stack-name".into(),
            target.stack_name.clone(),
            "--change-set-name".into(),
            name,
            "--include-property-values".into(),
            "--region".into(),
            target.expected_region.clone(),
            "--output".into(),
            "json".into(),
        ],
    )?;
    if enable {
        verify_canary_rollback_configuration(&described.stdout, plan)?;
    }
    let change_set =
        CloudFormationChangeSet::from_aws_json(&described.stdout, ChangeSetType::Update)?;
    verify_promotion_boundary(&change_set, &target.stack_name, LIVE_ALIAS_LOGICAL_ID)?;
    Ok(change_set)
}

fn render_canary_template(
    root: &Path,
    original: &ChangeSetReceipt,
    plan: &minco_deploy_aws::CanaryShiftPlan,
    plan_digest: &str,
) -> Result<PathBuf> {
    let rendered = render_canary_template_source(
        &fs::read(root.join(&original.packaged_template.path))?,
        plan,
    )?;
    let output = root.join(format!(
        "target/minco/change-sets/{}/canary-{}.yaml",
        original.release_id,
        &plan_digest[..24]
    ));
    ensure_parent(&output)?;
    if output.exists() {
        if fs::read_to_string(&output)? != rendered {
            bail!("canary template conflicts with an existing exact plan");
        }
    } else {
        fs::write(&output, rendered)?;
    }
    Ok(output)
}

fn render_canary_template_source(
    source: &[u8],
    plan: &minco_deploy_aws::CanaryShiftPlan,
) -> Result<String> {
    let mut template: serde_yaml_ng::Value = serde_yaml_ng::from_slice(source)?;
    let resources = yaml_mapping_value_mut(&mut template, "Resources")?;
    let alias = yaml_mapping_value_mut(resources, LIVE_ALIAS_LOGICAL_ID)?;
    let properties = yaml_mapping_value_mut(alias, "Properties")?;
    let properties = properties
        .as_mapping_mut()
        .context("LiveFunctionAlias Properties must be a mapping")?;
    let key = serde_yaml_ng::Value::String("RoutingConfig".into());
    if properties.contains_key(&key) {
        bail!("packaged template already declares live alias routing configuration");
    }
    let weights = serde_yaml_ng::Mapping::from_iter([(
        serde_yaml_ng::Value::String(plan.candidate_version.clone()),
        serde_yaml_ng::to_value(f64::from(plan.candidate_weight_basis_points) / 10_000.0)?,
    )]);
    let routing = serde_yaml_ng::Mapping::from_iter([(
        serde_yaml_ng::Value::String("AdditionalVersionWeights".into()),
        serde_yaml_ng::Value::Mapping(weights),
    )]);
    properties.insert(key, serde_yaml_ng::Value::Mapping(routing));
    serde_yaml_ng::to_string(&template).map_err(Into::into)
}

fn yaml_mapping_value_mut<'a>(
    value: &'a mut serde_yaml_ng::Value,
    key: &str,
) -> Result<&'a mut serde_yaml_ng::Value> {
    value
        .as_mapping_mut()
        .context("CloudFormation template node must be a mapping")?
        .get_mut(serde_yaml_ng::Value::String(key.into()))
        .with_context(|| format!("CloudFormation template is missing {key}"))
}

fn verify_canary_rollback_configuration(
    source: &[u8],
    plan: &minco_deploy_aws::CanaryShiftPlan,
) -> Result<()> {
    let value: serde_json::Value = serde_json::from_slice(source)?;
    let configuration = value
        .get("RollbackConfiguration")
        .context("canary change set omitted rollback configuration")?;
    let minutes = configuration
        .get("MonitoringTimeInMinutes")
        .and_then(serde_json::Value::as_u64);
    let mut alarms = configuration
        .get("RollbackTriggers")
        .and_then(serde_json::Value::as_array)
        .context("canary change set omitted rollback alarms")?
        .iter()
        .map(|trigger| {
            if trigger.get("Type").and_then(serde_json::Value::as_str)
                != Some("AWS::CloudWatch::Alarm")
            {
                bail!("canary rollback trigger has an unexpected type");
            }
            trigger
                .get("Arn")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
                .context("canary rollback trigger omitted its ARN")
        })
        .collect::<Result<Vec<_>>>()?;
    alarms.sort_unstable();
    if minutes != Some(u64::from(plan.monitoring_minutes)) || alarms != plan.alarm_arns {
        bail!("provider canary rollback configuration differs from the reviewed policy");
    }
    Ok(())
}

fn verify_canary_alarm_preconditions(
    root: &Path,
    target: &DeploymentTarget,
    alarm_arns: &[String],
) -> Result<()> {
    let arguments = canary_alarm_describe_arguments(alarm_arns)?;
    let arguments = arguments.iter().map(String::as_str).collect::<Vec<_>>();
    let observed: AwsDescribeAlarms = aws_json(
        root,
        &target.expected_region,
        "verify canary alarm preconditions",
        &arguments,
    )?;
    if !observed.composite_alarms.is_empty() {
        bail!("canary v1 accepts only metric alarm rollback triggers");
    }
    let mut alarms = observed.metric_alarms;
    alarms.sort_by(|left, right| left.alarm_arn.cmp(&right.alarm_arn));
    if alarms.len() != alarm_arns.len()
        || alarms.iter().zip(alarm_arns).any(|(observed, expected)| {
            observed.alarm_arn != *expected || observed.state_value != "OK"
        })
    {
        bail!(
            "every reviewed canary metric alarm must exist exactly and be in OK state before traffic"
        );
    }
    Ok(())
}

fn canary_alarm_describe_arguments(alarm_arns: &[String]) -> Result<Vec<String>> {
    let alarm_names = alarm_arns
        .iter()
        .map(|arn| {
            arn.split_once(":alarm:")
                .map(|(_, name)| name)
                .filter(|name| !name.is_empty())
                .context("reviewed canary alarm ARN has no alarm name")
        })
        .collect::<Result<Vec<_>>>()?;
    let mut arguments = vec![
        "cloudwatch".into(),
        "describe-alarms".into(),
        "--alarm-names".into(),
    ];
    arguments.extend(alarm_names.into_iter().map(str::to_owned));
    arguments.extend(["--alarm-types".into(), "MetricAlarms".into()]);
    Ok(arguments)
}

fn verify_canary_version_compatibility(
    root: &Path,
    target: &DeploymentTarget,
    plan: &minco_deploy_aws::CanaryShiftPlan,
) -> Result<()> {
    let configuration = |version: &str| {
        aws_json::<AwsFunctionConfiguration>(
            root,
            &target.expected_region,
            "verify weighted-alias version compatibility",
            &[
                "lambda",
                "get-function-configuration",
                "--function-name",
                &plan.function_name,
                "--qualifier",
                version,
            ],
        )
    };
    let current = configuration(&plan.current_version)?;
    let candidate = configuration(&plan.candidate_version)?;
    let current_function_matches = current.function_name == plan.function_name;
    let candidate_function_matches = candidate.function_name == plan.function_name;
    let current_version_matches = current.version == plan.current_version;
    let candidate_version_matches = candidate.version == plan.candidate_version;
    if !current_function_matches
        || !candidate_function_matches
        || !current_version_matches
        || !candidate_version_matches
        || current.role != candidate.role
        || current.dead_letter_config != candidate.dead_letter_config
    {
        bail!(
            "canary versions must be exact versions of one function with the same execution role and dead-letter configuration"
        );
    }
    Ok(())
}

fn verify_weighted_canary_alias(
    root: &Path,
    target: &DeploymentTarget,
    plan: &minco_deploy_aws::CanaryShiftPlan,
) -> Result<()> {
    let alias: AwsAliasConfiguration = aws_json(
        root,
        &target.expected_region,
        "verify weighted canary alias routing",
        &[
            "lambda",
            "get-alias",
            "--function-name",
            &plan.function_name,
            "--name",
            &plan.alias_name,
        ],
    )?;
    let expected_weight = f64::from(plan.candidate_weight_basis_points) / 10_000.0;
    let weights = alias
        .routing_config
        .map(|routing| routing.additional_version_weights)
        .unwrap_or_default();
    let function_matches = alias.function_name == plan.function_name;
    let base_version_matches = alias.function_version == plan.current_version;
    if !function_matches
        || !base_version_matches
        || weights.len() != 1
        || weights
            .get(&plan.candidate_version)
            .is_none_or(|weight| (weight - expected_weight).abs() > f64::EPSILON)
    {
        bail!("live alias does not match the exact alarm-guarded canary plan");
    }
    Ok(())
}

fn canary_alias_is_unweighted(
    root: &Path,
    target: &DeploymentTarget,
    function_name: &str,
    current_version: &str,
) -> Result<bool> {
    let alias: AwsAliasConfiguration = aws_json(
        root,
        &target.expected_region,
        "verify canary alias reversal",
        &[
            "lambda",
            "get-alias",
            "--function-name",
            function_name,
            "--name",
            "live",
        ],
    )?;
    Ok(alias.function_name == function_name
        && alias.function_version == current_version
        && alias
            .routing_config
            .is_none_or(|routing| routing.additional_version_weights.is_empty()))
}

fn create_promotion_change_set(
    root: &Path,
    target: &DeploymentTarget,
    original: &ChangeSetReceipt,
    expected_stack: &AwsStack,
    verification_digest: &str,
    promoted_version: &str,
) -> Result<CloudFormationChangeSet> {
    original.verify_at(root)?;
    verify_current_caller(root, target)?;
    if vcs::source_snapshot(root)?.change != original.source_change {
        bail!("source changed before creating the promotion change set");
    }
    let current_stack =
        describe_target_stack(root, target)?.context("promotion target stack no longer exists")?;
    if current_stack != *expected_stack {
        bail!("CloudFormation stack identity or routing inputs changed during promotion review");
    }
    let mut parameter_keys = current_stack
        .parameters
        .iter()
        .map(|parameter| parameter.parameter_key.as_str())
        .collect::<Vec<_>>();
    parameter_keys.sort_unstable();
    if parameter_keys.windows(2).any(|keys| keys[0] == keys[1])
        || !parameter_keys.contains(&LIVE_FUNCTION_VERSION_PARAMETER)
    {
        bail!("CloudFormation stack parameters are missing or duplicated");
    }
    let name = format!("minco-promote-{}", &verification_digest[..24]);
    let parameters: Vec<_> = parameter_keys
        .into_iter()
        .map(|key| {
            if key == LIVE_FUNCTION_VERSION_PARAMETER {
                AwsChangeSetParameter::value(key, promoted_version)
            } else {
                AwsChangeSetParameter::previous(key)
            }
        })
        .collect();
    let parameters = aws_change_set_parameters(&parameters)?;
    let create_args = vec![
        "cloudformation".into(),
        "create-change-set".into(),
        "--stack-name".into(),
        target.stack_name.clone(),
        "--change-set-name".into(),
        name.clone(),
        "--change-set-type".into(),
        "UPDATE".into(),
        "--template-body".into(),
        format!(
            "file://{}",
            root.join(&original.packaged_template.path).display()
        ),
        "--capabilities".into(),
        "CAPABILITY_IAM".into(),
        "--client-token".into(),
        verification_digest.into(),
        "--description".into(),
        format!(
            "Minco promote {} to Lambda version {promoted_version}",
            original.release_id
        ),
        "--parameters".into(),
        parameters,
        "--region".into(),
        target.expected_region.clone(),
        "--output".into(),
        "json".into(),
    ];
    run_cloud_output(
        root,
        "aws",
        "create the unexecuted promotion routing change set",
        &create_args,
    )?;
    run_cloud_output(
        root,
        "aws",
        "wait for promotion change-set creation",
        &[
            "cloudformation".into(),
            "wait".into(),
            "change-set-create-complete".into(),
            "--stack-name".into(),
            target.stack_name.clone(),
            "--change-set-name".into(),
            name.clone(),
            "--region".into(),
            target.expected_region.clone(),
        ],
    )?;
    let described = run_cloud_output(
        root,
        "aws",
        "describe the promotion routing change set",
        &[
            "cloudformation".into(),
            "describe-change-set".into(),
            "--stack-name".into(),
            target.stack_name.clone(),
            "--change-set-name".into(),
            name,
            "--include-property-values".into(),
            "--region".into(),
            target.expected_region.clone(),
            "--output".into(),
            "json".into(),
        ],
    )?;
    CloudFormationChangeSet::from_aws_json(&described.stdout, ChangeSetType::Update)
        .map_err(Into::into)
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
    for service in &graph.local_aws_services {
        if !service_runtime::supports_local_aws_service(service) {
            bail!("local AWS service `{service}` is not supported by Rustack 0.9.1");
        }
    }
    let requested_aws_services = graph.local_aws_services.clone();
    let rustack_port = options.rustack_port.unwrap_or(4_566);
    let mut dependency_compatible_graph = graph.clone();
    dependency_compatible_graph
        .local_aws_services
        .retain(|service| matches!(service.as_str(), "dynamodb" | "s3" | "sqs" | "ssm" | "sts"));
    let mut plan = DevPlan::derive(&dependency_compatible_graph, &options)?;
    service_runtime::normalize_dev_plan_services(
        &mut plan,
        &graph.application,
        &graph.compose_file,
        &requested_aws_services,
        rustack_port,
    );
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
    let service_program =
        std::env::current_exe().context("resolve the running cargo-minco path")?;
    let execution_plan = bind_local_service_program(&plan, &service_program)?;
    let result = Supervisor::new(root)
        .with_readiness_timeout(DEVELOPMENT_READINESS_TIMEOUT)
        .run_until(
            &execution_plan,
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

fn bind_local_service_program(plan: &DevPlan, program: &Path) -> Result<DevPlan> {
    let program = program
        .to_str()
        .context("the running cargo-minco path must be valid UTF-8")?;
    let mut execution_plan = plan.clone();
    for service in &mut execution_plan.services {
        if let Some(start) = &mut service.start {
            start.program = program.into();
        }
        if let Some(stop) = &mut service.stop {
            stop.program = program.into();
        }
    }
    Ok(execution_plan)
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
            ("AWS_REGION".into(), region.into()),
            (
                "AWS_ENDPOINT_URL".into(),
                format!("http://127.0.0.1:{port}"),
            ),
            ("AWS_S3_FORCE_PATH_STYLE".into(), "true".into()),
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

async fn deploy(
    root: &Path,
    manifest: &MincoManifest,
    command: DeployCommand,
    as_json: bool,
) -> Result<()> {
    match command {
        DeployCommand::Plan {
            input,
            environment,
            target_config,
            output,
            stdout,
        } => {
            let mut plan = load_plan(root, manifest, input.config)?;
            if let Some(environment) = environment {
                validate_project_file(root, &target_config, "deployment target configuration")?;
                let catalog = DeploymentTargetCatalog::from_toml(&fs::read_to_string(
                    root.join(&target_config),
                )?)?;
                let selected = catalog.select(Some(&environment))?;
                apply_plan_target(&mut plan, &selected.environment, &selected.target)?;
            }
            ensure_plan_valid(&plan)?;
            if stdout {
                use std::io::Write as _;
                std::io::stdout().write_all(&canonical_json(&plan)?)?;
                return Ok(());
            }
            let output = output.unwrap_or_else(|| {
                if plan.preview.is_some() {
                    PathBuf::from("target/minco/preview-plan.json")
                } else {
                    PathBuf::from("infra/aws/generated/plan.json")
                }
            });
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
        DeployCommand::Verify(args) => {
            verify_deployment_command(root, manifest, &args, as_json).await
        }
        DeployCommand::Review(args) => create_review_command(root, &args, as_json),
        DeployCommand::StaticSite { command } => {
            Box::pin(static_site_command(root, command, as_json)).await
        }
    }
}

async fn static_site_command(root: &Path, command: StaticSiteCommand, as_json: bool) -> Result<()> {
    match command {
        StaticSiteCommand::Plan { input } => {
            Box::pin(static_site_publication(root, &input, None, true, as_json)).await
        }
        StaticSiteCommand::Apply {
            input,
            approve_release_digest,
        } => {
            Box::pin(static_site_publication(
                root,
                &input,
                Some(&approve_release_digest),
                false,
                as_json,
            ))
            .await
        }
    }
}

fn create_review_command(root: &Path, args: &ReviewArgs, as_json: bool) -> Result<()> {
    validate_project_file(root, &args.target_config, "deployment target configuration")?;
    for (path, label) in [
        (&args.manifest, "release manifest"),
        (&args.deployment_receipt, "deployment receipt"),
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
    for trace in &args.delivery_trace {
        validate_project_file(root, trace, "delivery trace")?;
    }
    let output = package_output_path(root, &args.output)?;
    let catalog =
        DeploymentTargetCatalog::from_toml(&fs::read_to_string(root.join(&args.target_config))?)?;
    let selected = catalog.select(args.environment.as_deref())?;
    let mut blockers = Vec::new();
    if !selected.target.enabled {
        blockers.push("target_disabled");
    }
    if selected.target.lifecycle != DeploymentTargetLifecycle::Preview {
        blockers.push("target_not_preview");
    }
    if !root.join(&args.manifest).is_file() {
        blockers.push("release_manifest_missing");
    }
    if !root.join(&args.deployment_receipt).is_file() {
        blockers.push("deployment_receipt_missing");
    }
    if output.exists() {
        blockers.push("review_manifest_exists");
    }
    if args.feedback.iter().any(|reference| {
        reference
            .split_once('=')
            .is_none_or(|(id, digest)| id.is_empty() || !lower_sha256_is_valid(digest))
    }) {
        blockers.push("feedback_reference_invalid");
    }
    let plan = json!({
        "schema_version": 1,
        "operation": "create_preview_review",
        "dry_run": args.dry_run,
        "external_aws_contact": false,
        "review_manifest_written": false,
        "target": {
            "environment": selected.environment,
            "lifecycle": selected.target.lifecycle,
            "enabled": selected.target.enabled,
            "expected_account_id": selected.target.expected_account_id,
            "expected_region": selected.target.expected_region,
            "expected_role_arn": selected.target.expected_role_arn,
            "stack_name": selected.target.stack_name,
        },
        "release_manifest": args.manifest,
        "deployment_receipt": args.deployment_receipt,
        "review_manifest": args.output,
        "feedback_reference_count": args.feedback.len(),
        "explicit_delivery_trace_count": args.delivery_trace.len(),
        "guard_requirements": [
            "exact_source_and_release",
            "successful_hosted_verification",
            "exact_preview_target",
            "current_expected_account_region_role_stack",
            "termination_protection_disabled",
            "exact_provider_resource_and_retention_inventory",
        ],
        "blockers": blockers,
    });
    if args.dry_run {
        return print_value(&plan, as_json);
    }
    if !blockers.is_empty() {
        bail!("preview review is blocked: {}", blockers.join(", "));
    }
    materialize_preview_review(
        root,
        args,
        &selected.environment,
        &selected.target,
        &output,
        as_json,
    )
}

fn materialize_preview_review(
    root: &Path,
    args: &ReviewArgs,
    selected_environment: &str,
    selected_target: &DeploymentTarget,
    output: &Path,
    as_json: bool,
) -> Result<()> {
    if !command_available("aws") {
        bail!("preview review creation requires `aws`");
    }
    let evidence = verified_deployment_evidence(root, &args.manifest, &args.deployment_receipt)?;
    if evidence.deployment.outcome() != DeploymentOutcome::Succeeded {
        bail!("preview review requires a successfully hosted-verified deployment receipt");
    }
    if evidence.target != *selected_target
        || evidence.release.environment.environment != selected_environment
    {
        bail!("preview deployment evidence does not match the selected reviewed target");
    }
    require_exact_source(root, &evidence.release)?;
    verify_current_caller(root, selected_target)?;
    let stack = describe_target_stack(root, selected_target)?
        .context("preview deployment stack does not exist")?;
    require_stable_update_stack(&stack.stack_status)?;
    if stack.enable_termination_protection != Some(false) {
        bail!("preview review refuses enabled or unproved CloudFormation termination protection");
    }
    let provider_resources: AwsStackResources = aws_json(
        root,
        &selected_target.expected_region,
        "inspect preview stack resources for review",
        &[
            "cloudformation",
            "list-stack-resources",
            "--stack-name",
            &selected_target.stack_name,
        ],
    )?;
    let processed_template: AwsProcessedTemplate = aws_json(
        root,
        &selected_target.expected_region,
        "inspect preview stack retention for review",
        &[
            "cloudformation",
            "get-template",
            "--stack-name",
            &selected_target.stack_name,
            "--template-stage",
            "Processed",
        ],
    )?;
    let preview = selected_target
        .preview
        .as_ref()
        .context("selected target has no preview lifecycle policy")?;
    let resources = review_resources_from_provider(
        &preview.resources,
        &provider_resources,
        &processed_template.template_body,
    )?;
    let created_at = chrono::Utc::now();
    let expires_at = created_at + chrono::Duration::seconds(i64::from(preview.ttl_seconds));
    let verification = std::iter::once(evidence.deployment_receipt.clone())
        .chain(
            evidence
                .deployment
                .verification()
                .iter()
                .map(|verification| verification.file.clone()),
        )
        .collect();
    let mut delivery_trace = vec![evidence.change_set_receipt];
    delivery_trace.extend(
        args.delivery_trace
            .iter()
            .map(|path| FileDigest::from_rooted_path(root, root.join(path)))
            .collect::<Result<Vec<_>, _>>()?,
    );
    let feedback = args
        .feedback
        .iter()
        .map(|reference| {
            let (feedback_id, sha256) = reference
                .split_once('=')
                .context("Feedback reference must use ID=SHA256")?;
            Ok(UntrustedFeedbackReference {
                feedback_id: feedback_id.into(),
                sha256: sha256.into(),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let review = ReviewManifest::seal(ReviewManifestInput {
        source_change: evidence.release.source_change.clone(),
        release_manifest: evidence.deployment.release_manifest.clone(),
        release_id: evidence.release.release_id.clone(),
        release_digest: evidence.release.release_digest.clone(),
        artifacts: evidence
            .release
            .artifacts
            .iter()
            .map(|artifact| artifact.file.clone())
            .collect(),
        environment: evidence.release.environment,
        expected_account_id: selected_target.expected_account_id.clone(),
        expected_role_arn: selected_target.expected_role_arn.clone(),
        stack_name: selected_target.stack_name.clone(),
        target_config: FileDigest::from_rooted_path(root, root.join(&args.target_config))?,
        owner: preview.owner.clone(),
        created_at: created_at.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        expires_at: expires_at.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        resources,
        pricing_complete: preview.pricing_complete,
        cleanup_schedule: preview.cleanup_schedule.clone(),
        verification,
        feedback,
        delivery_trace,
    })?;
    review.verify_at(root)?;
    ensure_parent(output)?;
    review.write_json(output)?;
    print_value(
        &json!({
            "review_created": true,
            "external_aws_contact": true,
            "review_manifest": review,
            "review_manifest_path": args.output,
        }),
        as_json,
    )
}

fn lower_sha256_is_valid(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

async fn static_site_publication(
    root: &Path,
    input: &StaticSitePublishInput,
    approval: Option<&str>,
    dry_run: bool,
    as_json: bool,
) -> Result<()> {
    for (path, label) in [
        (&input.target_config, "deployment target configuration"),
        (&input.manifest, "release manifest"),
        (&input.deployment_receipt, "deployment receipt"),
        (&input.output, "static-site publication receipt"),
    ] {
        if path.as_os_str().is_empty()
            || path.is_absolute()
            || !path
                .components()
                .all(|component| matches!(component, std::path::Component::Normal(_)))
        {
            bail!("{label} must be a normalized project-relative path");
        }
    }
    let output = package_output_path(root, &input.output)?;
    let mut blockers = Vec::new();
    for (path, code) in [
        (&input.target_config, "target_config_missing"),
        (&input.manifest, "release_manifest_missing"),
        (&input.deployment_receipt, "deployment_receipt_missing"),
    ] {
        if !root.join(path).is_file() {
            blockers.push(code);
        }
    }
    if output.exists() {
        blockers.push("publication_receipt_exists");
    }

    let evidence = if blockers.iter().any(|code| {
        matches!(
            *code,
            "target_config_missing" | "release_manifest_missing" | "deployment_receipt_missing"
        )
    }) {
        None
    } else if let Ok(evidence) =
        verified_deployment_evidence(root, &input.manifest, &input.deployment_receipt)
    {
        Some(evidence)
    } else {
        blockers.push("deployment_evidence_invalid");
        None
    };
    let mut static_manifest_path = None;
    if let Some(evidence) = &evidence {
        if evidence.deployment.outcome() != DeploymentOutcome::Started {
            blockers.push("deployment_not_started");
        }
        if FileDigest::from_rooted_path(root, root.join(&input.target_config)).ok()
            != Some(evidence.change_set.target_config.clone())
        {
            blockers.push("target_config_mismatch");
        }
        match exact_static_site_manifest(root, &evidence.release) {
            Ok((file, _)) => static_manifest_path = Some(file.path),
            Err(_) => blockers.push("static_site_release_manifest_invalid"),
        }
        if !dry_run {
            match approval {
                None => blockers.push("release_approval_missing"),
                Some(digest) if digest != evidence.release.release_digest => {
                    blockers.push("release_approval_mismatch");
                }
                Some(_) => {}
            }
        }
    } else if !dry_run && approval.is_none() {
        blockers.push("release_approval_missing");
    }

    let plan = json!({
        "schema_version": 1,
        "operation": if dry_run { "static_site_publication_plan" } else { "static_site_publication_apply" },
        "external_aws_contact": false,
        "infrastructure_change": false,
        "exact_release_required": true,
        "stale_object_deletion_after_checksum_verification": true,
        "cloudfront_invalidation_wait": true,
        "release_manifest": input.manifest,
        "deployment_receipt": input.deployment_receipt,
        "static_site_manifest": static_manifest_path,
        "publication_receipt": input.output,
        "blockers": blockers,
    });
    if dry_run {
        return print_value(&plan, as_json);
    }
    if !blockers.is_empty() {
        bail!(
            "static-site publication is blocked: {}",
            blockers.join(", ")
        );
    }
    let evidence = evidence.context("verified deployment evidence is required")?;
    let (manifest_file, manifest) = exact_static_site_manifest(root, &evidence.release)?;
    require_exact_source(root, &evidence.release)?;
    verify_current_caller(root, &evidence.target)?;
    let stack = describe_target_stack(root, &evidence.target)?
        .context("static-site deployment stack no longer exists")?;
    require_stable_update_stack(&stack.stack_status)?;
    let bucket = stack_output(&stack, "StaticSiteBucketName")?.to_owned();
    let distribution_id = stack_output(&stack, "StaticSiteDistributionId")?.to_owned();
    let distribution_domain = stack_output(&stack, "StaticSiteDistributionDomainName")?.to_owned();
    let public_url = stack_output(&stack, "StaticSiteUrl")?.to_owned();

    let shared = aws_config::defaults(aws_config::BehaviorVersion::latest())
        .region(aws_config::Region::new(
            evidence.target.expected_region.clone(),
        ))
        .load()
        .await;
    let publisher = minco_aws_adapters::static_site::AwsStaticSitePublisher::new(
        aws_sdk_s3::Client::new(&shared),
        aws_sdk_cloudfront::Client::new(&shared),
        &bucket,
        "",
        Some(distribution_id.clone()),
        &public_url,
        true,
    )?;
    let publisher = minco::plugin_static_site::StaticSitePublisherService::new(Arc::new(publisher));
    let publication = publisher.publish_manifest(&manifest, root).await?;
    require_exact_source(root, &evidence.release)?;
    let receipt = StaticSitePublicationReceipt::seal(StaticSitePublicationReceiptInput {
        release_digest: evidence.release.release_digest,
        manifest_file,
        bucket,
        distribution_id,
        distribution_domain,
        publication,
    })?;
    ensure_parent(&output)?;
    receipt.write_json(&output)?;
    receipt.verify_at(root)?;
    print_value(
        &json!({
            "published": true,
            "receipt": receipt,
            "receipt_path": input.output,
        }),
        as_json,
    )
}

fn exact_static_site_manifest(
    root: &Path,
    release: &ReleaseManifest,
) -> Result<(
    FileDigest,
    minco::plugin_static_site::StaticSiteReleaseManifest,
)> {
    let deployment_plan = release_deployment_plan(root, release)?;
    let expected = deployment_plan
        .static_site
        .as_ref()
        .context("release deployment plan does not contain a static site")?;
    let expected = provider_static_site_plan(expected);
    let matching = release
        .attestations
        .iter()
        .filter_map(|file| {
            let manifest =
                read_strict_json::<minco::plugin_static_site::StaticSiteReleaseManifest>(
                    &root.join(&file.path),
                    "static-site release manifest",
                )
                .ok()?;
            (manifest.plan == expected).then(|| (file.clone(), manifest))
        })
        .collect::<Vec<_>>();
    let [(file, manifest)] = matching.as_slice() else {
        bail!("release must bind exactly one static-site release manifest");
    };
    file.verify_at(root)?;
    manifest.verify_at(root)?;
    Ok((file.clone(), manifest.clone()))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct AwsGetDistributionOutput {
    distribution: AwsDistributionObservation,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct AwsDistributionObservation {
    id: String,
    status: String,
    domain_name: String,
    distribution_config: AwsDistributionConfigObservation,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct AwsDistributionConfigObservation {
    aliases: AwsCloudFrontAliases,
    origins: AwsCloudFrontOrigins,
    viewer_certificate: AwsCloudFrontViewerCertificate,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct AwsCloudFrontAliases {
    #[serde(default)]
    items: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct AwsCloudFrontOrigins {
    items: Vec<AwsCloudFrontOrigin>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct AwsCloudFrontOrigin {
    domain_name: String,
    origin_access_control_id: String,
}

#[derive(Debug, Deserialize)]
struct AwsCloudFrontViewerCertificate {
    #[serde(rename = "ACMCertificateArn")]
    acm_certificate_arn: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct AwsGetInvalidationOutput {
    invalidation: AwsInvalidationObservation,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct AwsInvalidationObservation {
    id: String,
    status: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct AwsS3HeadObjectObservation {
    content_length: i64,
    #[serde(rename = "ChecksumSHA256")]
    checksum_sha256: Option<String>,
    content_type: Option<String>,
    cache_control: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct AwsDescribeCertificateOutput {
    certificate: AwsCertificateObservation,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct AwsCertificateObservation {
    certificate_arn: String,
    domain_name: String,
    #[serde(default)]
    subject_alternative_names: Vec<String>,
    status: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct AwsGetHostedZoneOutput {
    hosted_zone: AwsHostedZoneObservation,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct AwsHostedZoneObservation {
    id: String,
    name: String,
    config: AwsHostedZoneConfig,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct AwsHostedZoneConfig {
    private_zone: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct AwsResourceRecordSetsOutput {
    resource_record_sets: Vec<AwsResourceRecordSet>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct AwsResourceRecordSet {
    name: String,
    #[serde(rename = "Type")]
    record_type: String,
    alias_target: Option<AwsAliasTarget>,
}

#[derive(Debug, Deserialize)]
struct AwsAliasTarget {
    #[serde(rename = "DNSName")]
    dns_name: String,
}

async fn collect_static_site_verification(
    root: &Path,
    release: &ReleaseManifest,
    target: &DeploymentTarget,
    stack: &AwsStack,
    publication_path: &Path,
) -> Result<StaticSiteVerificationReport> {
    validate_project_file(root, publication_path, "static-site publication receipt")?;
    let publication = StaticSitePublicationReceipt::read_json(root.join(publication_path))?;
    publication.verify_at(root)?;
    let (manifest_file, manifest) = exact_static_site_manifest(root, release)?;
    if publication.release_digest != release.release_digest
        || publication.manifest_file != manifest_file
        || publication.bucket != stack_output(stack, "StaticSiteBucketName")?
        || publication.distribution_id != stack_output(stack, "StaticSiteDistributionId")?
        || publication.distribution_domain
            != stack_output(stack, "StaticSiteDistributionDomainName")?
    {
        bail!("static-site publication receipt does not match the current release and stack");
    }
    let invalidation_id = publication
        .publication
        .invalidation_id
        .as_deref()
        .context("static-site publication receipt has no completed invalidation")?;
    let distribution: AwsGetDistributionOutput = aws_json(
        root,
        &target.expected_region,
        "inspect the static-site CloudFront distribution",
        &[
            "cloudfront",
            "get-distribution",
            "--id",
            &publication.distribution_id,
        ],
    )?;
    let invalidation: AwsGetInvalidationOutput = aws_json(
        root,
        &target.expected_region,
        "inspect the static-site CloudFront invalidation",
        &[
            "cloudfront",
            "get-invalidation",
            "--distribution-id",
            &publication.distribution_id,
            "--id",
            invalidation_id,
        ],
    )?;
    let [origin] = distribution
        .distribution
        .distribution_config
        .origins
        .items
        .as_slice()
    else {
        bail!("static-site distribution must contain exactly one private origin");
    };

    let client = reqwest::Client::builder()
        .https_only(true)
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(30))
        .build()?;
    let base_url = url::Url::parse(&publication.publication.url)?;
    let mut objects = Vec::with_capacity(manifest.assets.len());
    for asset in &manifest.assets {
        let head: AwsS3HeadObjectObservation = aws_json(
            root,
            &target.expected_region,
            "verify a static-site S3 object",
            &[
                "s3api",
                "head-object",
                "--bucket",
                &publication.bucket,
                "--key",
                &asset.path,
                "--checksum-mode",
                "ENABLED",
            ],
        )?;
        let s3_sha256 = base64_sha256_to_hex(
            head.checksum_sha256
                .as_deref()
                .context("S3 HeadObject omitted the release SHA-256 checksum")?,
        )?;
        let asset_url = base_url.join(&asset.path)?;
        let response = client
            .get(asset_url)
            .header(reqwest::header::ACCEPT_ENCODING, "identity")
            .send()
            .await?;
        if response.status() != reqwest::StatusCode::OK {
            bail!(
                "CloudFront static asset {} returned status {}",
                asset.path,
                response.status()
            );
        }
        if response.content_length() != Some(asset.bytes) {
            bail!(
                "CloudFront static asset {} omitted or changed its bounded content length",
                asset.path
            );
        }
        let cloudfront_content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .context("CloudFront static asset omitted content type")?
            .to_str()
            .context("CloudFront static asset returned non-ASCII content type")?
            .to_owned();
        let cloudfront_cache_control = response
            .headers()
            .get(reqwest::header::CACHE_CONTROL)
            .context("CloudFront static asset omitted cache metadata")?
            .to_str()
            .context("CloudFront static asset returned non-ASCII cache metadata")?
            .to_owned();
        let bytes = response.bytes().await?;
        let cloudfront_sha256 = format!("{:x}", Sha256::digest(&bytes));
        objects.push(StaticSiteObjectObservation {
            path: asset.path.clone(),
            s3_bytes: u64::try_from(head.content_length)
                .context("S3 returned a negative static-site content length")?,
            s3_sha256,
            s3_content_type: head
                .content_type
                .context("S3 HeadObject omitted static-site content type")?,
            s3_cache_control: head
                .cache_control
                .context("S3 HeadObject omitted static-site cache metadata")?,
            cloudfront_bytes: u64::try_from(bytes.len()).context("asset length overflow")?,
            cloudfront_sha256,
            cloudfront_content_type,
            cloudfront_cache_control,
        });
    }

    let certificate = collect_static_site_certificate(root, target)?;
    let dns = collect_static_site_dns(root, target, &manifest)?;
    let pricing = StaticSitePricingEvidence {
        checked_on: target
            .static_site_pricing_checked_on
            .clone()
            .context("reviewed static-site pricing date is missing")?,
        source: target
            .static_site_pricing_source
            .clone()
            .context("reviewed static-site pricing source is missing")?,
        billing_model: target
            .static_site_billing_model
            .context("reviewed CloudFront billing model is missing")?,
        price_class: manifest.plan.price_class.clone(),
        flat_rate_eligibility: target
            .static_site_flat_rate_eligibility
            .context("reviewed CloudFront flat-rate eligibility is missing")?,
    };
    let distribution_status = match distribution.distribution.status.as_str() {
        "Deployed" => StaticSiteDistributionStatus::Deployed,
        "InProgress" => StaticSiteDistributionStatus::InProgress,
        status => bail!("unsupported CloudFront distribution status {status}"),
    };
    let invalidation_status = match invalidation.invalidation.status.as_str() {
        "Completed" => StaticSiteInvalidationStatus::Completed,
        "InProgress" => StaticSiteInvalidationStatus::InProgress,
        status => bail!("unsupported CloudFront invalidation status {status}"),
    };
    if distribution.distribution.id != publication.distribution_id
        || invalidation.invalidation.id != invalidation_id
    {
        bail!("CloudFront returned a different distribution or invalidation identity");
    }
    StaticSiteVerificationReport::complete(StaticSiteVerificationInput {
        release_digest: release.release_digest.clone(),
        expected_account_id: target.expected_account_id.clone(),
        deployment_region: target.expected_region.clone(),
        manifest,
        observation: StaticSiteProviderObservation {
            bucket: publication.bucket,
            distribution_id: distribution.distribution.id,
            distribution_domain: distribution.distribution.domain_name,
            distribution_status,
            distribution_aliases: distribution.distribution.distribution_config.aliases.items,
            distribution_certificate_arn: distribution
                .distribution
                .distribution_config
                .viewer_certificate
                .acm_certificate_arn,
            origin_domain: origin.domain_name.clone(),
            origin_access_control_id: origin.origin_access_control_id.clone(),
            invalidation_id: invalidation.invalidation.id,
            invalidation_status,
            certificate,
            dns,
            objects,
            pricing,
        },
    })
    .map_err(Into::into)
}

fn collect_static_site_certificate(
    root: &Path,
    target: &DeploymentTarget,
) -> Result<Option<StaticSiteCertificateObservation>> {
    let Some(arn) = target.static_site_certificate_arn.as_deref() else {
        return Ok(None);
    };
    let described: AwsDescribeCertificateOutput = aws_json(
        root,
        "us-east-1",
        "inspect the existing static-site ACM certificate",
        &["acm", "describe-certificate", "--certificate-arn", arn],
    )?;
    if described.certificate.certificate_arn != arn {
        bail!("ACM returned a different static-site certificate");
    }
    let mut names = described.certificate.subject_alternative_names;
    names.push(described.certificate.domain_name);
    names.sort_unstable();
    names.dedup();
    Ok(Some(StaticSiteCertificateObservation {
        arn: described.certificate.certificate_arn,
        status: described.certificate.status,
        names,
    }))
}

fn collect_static_site_dns(
    root: &Path,
    target: &DeploymentTarget,
    manifest: &minco::plugin_static_site::StaticSiteReleaseManifest,
) -> Result<Option<StaticSiteDnsObservation>> {
    if !manifest.plan.manage_dns_alias {
        return Ok(None);
    }
    let zone_id = target
        .static_site_hosted_zone_id
        .as_deref()
        .context("reviewed static-site hosted-zone ID is missing")?;
    let domain = manifest
        .plan
        .custom_domain
        .as_deref()
        .context("managed static-site DNS requires a custom domain")?;
    let zone: AwsGetHostedZoneOutput = aws_json(
        root,
        &target.expected_region,
        "inspect the static-site Route 53 hosted zone",
        &["route53", "get-hosted-zone", "--id", zone_id],
    )?;
    if zone.hosted_zone.id.trim_start_matches("/hostedzone/") != zone_id {
        bail!("Route 53 returned a different hosted zone");
    }
    let records: AwsResourceRecordSetsOutput = aws_json(
        root,
        &target.expected_region,
        "inspect the static-site Route 53 aliases",
        &[
            "route53",
            "list-resource-record-sets",
            "--hosted-zone-id",
            zone_id,
        ],
    )?;
    Ok(Some(StaticSiteDnsObservation {
        hosted_zone_id: zone_id.to_owned(),
        hosted_zone_name: zone.hosted_zone.name,
        private_zone: zone.hosted_zone.config.private_zone,
        a_target: exact_route53_alias(&records.resource_record_sets, domain, "A")?
            .context("static-site Route 53 A alias is missing")?,
        aaaa_target: exact_route53_alias(&records.resource_record_sets, domain, "AAAA")?,
    }))
}

fn exact_route53_alias(
    records: &[AwsResourceRecordSet],
    domain: &str,
    record_type: &str,
) -> Result<Option<String>> {
    let matching = records
        .iter()
        .filter(|record| {
            record.name.trim_end_matches('.') == domain && record.record_type == record_type
        })
        .collect::<Vec<_>>();
    match matching.as_slice() {
        [] => Ok(None),
        [record] => Ok(Some(
            record
                .alias_target
                .as_ref()
                .context("static-site DNS record is not an alias")?
                .dns_name
                .clone(),
        )),
        _ => bail!("static-site DNS contains duplicate {record_type} aliases"),
    }
}

fn base64_sha256_to_hex(value: &str) -> Result<String> {
    use std::fmt::Write as _;

    let bytes = base64::engine::general_purpose::STANDARD
        .decode(value)
        .context("decode S3 SHA-256 checksum")?;
    if bytes.len() != 32 {
        bail!("S3 SHA-256 checksum has an invalid length");
    }
    let mut digest = String::with_capacity(64);
    for byte in bytes {
        write!(&mut digest, "{byte:02x}").expect("writing to String cannot fail");
    }
    Ok(digest)
}

async fn verify_deployment_command(
    root: &Path,
    manifest: &MincoManifest,
    args: &DeployVerifyArgs,
    as_json: bool,
) -> Result<()> {
    let mut paths = vec![
        (&args.manifest, "release manifest"),
        (&args.receipt, "deployment receipt"),
        (&args.output, "hosted verification output"),
    ];
    if args.static_site {
        paths.extend([
            (
                &args.static_site_publication,
                "static-site publication receipt",
            ),
            (&args.static_site_output, "static-site verification output"),
        ]);
    }
    for (path, label) in paths {
        if path.as_os_str().is_empty()
            || path.is_absolute()
            || !path
                .components()
                .all(|component| matches!(component, std::path::Component::Normal(_)))
        {
            bail!("{label} must be a normalized project-relative path");
        }
    }
    let output_path = package_output_path(root, &args.output)?;
    let static_site_output_path = args
        .static_site
        .then(|| package_output_path(root, &args.static_site_output))
        .transpose()?;
    let mut blockers = Vec::new();
    if !root.join(&args.manifest).is_file() {
        blockers.push("release_manifest_missing");
    }
    if !root.join(&args.receipt).is_file() {
        blockers.push("deployment_receipt_missing");
    }
    if output_path.exists() {
        blockers.push("hosted_verification_output_exists");
    }
    if args.static_site && !root.join(&args.static_site_publication).is_file() {
        blockers.push("static_site_publication_missing");
    }
    if static_site_output_path
        .as_ref()
        .is_some_and(|path| path.exists())
    {
        blockers.push("static_site_verification_output_exists");
    }
    if args.dry_run {
        return print_value(
            &json!({
                "external_aws_contact": false,
                "external_http_contact": false,
                "deployment_receipt_transition": false,
                "release_manifest": args.manifest,
                "deployment_receipt": args.receipt,
                "hosted_verification_output": args.output,
                "static_site": args.static_site,
                "static_site_publication": args.static_site.then_some(&args.static_site_publication),
                "static_site_verification_output": args.static_site.then_some(&args.static_site_output),
                "blockers": blockers,
            }),
            as_json,
        );
    }
    if !blockers.is_empty() {
        bail!("hosted verification is blocked: {}", blockers.join(", "));
    }
    validate_project_file(root, &args.manifest, "release manifest")?;
    validate_project_file(root, &args.receipt, "deployment receipt")?;
    let evidence = verified_deployment_evidence(root, &args.manifest, &args.receipt)?;
    let mut deployment = evidence.deployment;
    if deployment.outcome() != DeploymentOutcome::Started {
        bail!("started deployment receipt does not bind the exact verified release");
    }
    let api_artifact = exact_api_artifact(&evidence.release)?;
    if manifest.commands.hosted_verify.len() != 1
        || manifest.commands.hosted_verify[0].trim().is_empty()
    {
        bail!("minco.toml must declare exactly one non-empty commands.hosted_verify command");
    }
    require_exact_source(root, &evidence.release)?;
    verify_current_caller(root, &evidence.target)?;
    let stack = describe_target_stack(root, &evidence.target)?
        .context("hosted verification target stack no longer exists")?;
    require_stable_update_stack(&stack.stack_status)?;
    let candidate_endpoint = canonical_hosted_endpoint(stack_output(&stack, "CandidateApiUrl")?)?;
    let function_name = stack_output(&stack, "ApiFunctionName")?.to_owned();
    ensure_parent(&output_path)?;
    let observation = args
        .output
        .with_extension(format!("{}.observation.json", deployment.attempt_id));
    let observation_path = root.join(&observation);
    if observation_path.exists() {
        bail!("hosted verification observation output already exists");
    }
    let command = &manifest.commands.hosted_verify[0];
    let collection = (|| -> Result<HostedVerificationReport> {
        let mut process = if cfg!(windows) {
            let mut process = ProcessCommand::new("cmd");
            process.args(["/C", command]);
            process
        } else {
            let mut process = ProcessCommand::new("sh");
            process.args(["-c", command]);
            process
        };
        let output = process
            .current_dir(root)
            .env("AWS_REGION", &evidence.target.expected_region)
            .env("AWS_DEFAULT_REGION", &evidence.target.expected_region)
            .env("MINCO_CANDIDATE_API_URL", &candidate_endpoint)
            .env("MINCO_FUNCTION_NAME", &function_name)
            .env("MINCO_HOSTED_OBSERVATION", &observation_path)
            .env("MINCO_RELEASE_MANIFEST", &args.manifest)
            .output()
            .with_context(|| format!("run configured hosted verification command {command}"))?;
        if !output.status.success() {
            bail!(
                "configured hosted verification command failed with exit code {:?}",
                output.status.code()
            );
        }
        let observation: HostedVerificationObservation =
            serde_json::from_slice(&fs::read(&observation_path)?)
                .context("parse strict hosted verification observation")?;
        let report = HostedVerificationReport::complete(HostedVerificationInput {
            endpoint: observation.endpoint,
            expected_artifact_digest: api_artifact.file.sha256.clone(),
            executed_artifact_digest: observation.executed_artifact_digest,
            executed_version: observation.executed_version,
            checks: observation.checks,
        })?;
        if report.endpoint != candidate_endpoint {
            bail!("hosted verification did not target the current candidate stage");
        }
        verify_candidate_function(
            root,
            &evidence.target,
            &function_name,
            &report.executed_version,
            &report.executed_artifact_digest,
        )?;
        report.write_json(&output_path)?;
        Ok(report)
    })();
    let report = match collection {
        Ok(report) => report,
        Err(error) => {
            let _ = fs::remove_file(&observation_path);
            deployment.fail("hosted_verification_failed")?;
            deployment.write_json(root.join(&args.receipt))?;
            return Err(error);
        }
    };
    let _ = fs::remove_file(&observation_path);
    let verification_file = FileDigest::from_rooted_path(root, &output_path)?;
    let mut verification = vec![VerificationEvidence {
        kind: "hosted_verification".into(),
        file: verification_file,
    }];
    let static_site_report = if args.static_site {
        let static_output = static_site_output_path
            .as_ref()
            .context("static-site verification output is required")?;
        let collection = collect_static_site_verification(
            root,
            &evidence.release,
            &evidence.target,
            &stack,
            &args.static_site_publication,
        )
        .await;
        let report = match collection {
            Ok(report) => report,
            Err(error) => {
                deployment.fail("static_site_verification_failed")?;
                deployment.write_json(root.join(&args.receipt))?;
                return Err(error);
            }
        };
        let materialized = (|| -> Result<VerificationEvidence> {
            use std::io::Write as _;

            ensure_parent(static_output)?;
            let mut output = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(static_output)?;
            output.write_all(&canonical_json(&report)?)?;
            output.sync_all()?;
            let strict: StaticSiteVerificationReport =
                read_strict_json(static_output, "static-site verification report")?;
            strict.verify_structure()?;
            Ok(VerificationEvidence {
                kind: "static_site_verification".into(),
                file: FileDigest::from_rooted_path(root, static_output)?,
            })
        })();
        match materialized {
            Ok(evidence) => verification.push(evidence),
            Err(error) => {
                let _ = fs::remove_file(static_output);
                deployment.fail("static_site_verification_failed")?;
                deployment.write_json(root.join(&args.receipt))?;
                return Err(error);
            }
        }
        Some(report)
    } else {
        None
    };
    deployment.succeed(verification)?;
    deployment.write_json(root.join(&args.receipt))?;
    deployment.verify_at(root)?;
    print_value(
        &json!({
            "hosted_verified": true,
            "report": report,
            "report_path": args.output,
            "static_site_report": static_site_report,
            "static_site_report_path": args.static_site.then_some(&args.static_site_output),
            "deployment_receipt": deployment,
            "deployment_receipt_path": args.receipt,
        }),
        as_json,
    )
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
            match release_deployment_plan(root, &release) {
                Ok(plan) => blockers.extend(static_site_target_blockers(&plan, &selected.target)),
                Err(_) => blockers.push("deployment_plan_invalid"),
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
            "static_site_certificate_configured": selected.target.static_site_certificate_arn.is_some(),
            "static_site_hosted_zone_configured": selected.target.static_site_hosted_zone_id.is_some(),
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

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct AwsStack {
    stack_status: String,
    #[serde(default)]
    enable_termination_protection: Option<bool>,
    #[serde(default)]
    outputs: Vec<AwsStackOutput>,
    #[serde(default)]
    parameters: Vec<AwsStackParameter>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct AwsStackResources {
    stack_resource_summaries: Vec<AwsStackResource>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct AwsStackResource {
    logical_resource_id: String,
    resource_type: String,
    resource_status: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct AwsProcessedTemplate {
    template_body: serde_json::Value,
}

fn verify_preview_resource_inventory(
    expected: &[minco_deploy_aws::ReviewResource],
    provider: &AwsStackResources,
) -> Result<()> {
    let mut expected_identity = expected
        .iter()
        .map(|resource| {
            (
                resource.logical_id.as_str(),
                resource.resource_type.as_str(),
            )
        })
        .collect::<Vec<_>>();
    expected_identity.sort_unstable();
    let mut provider_identity = provider
        .stack_resource_summaries
        .iter()
        .map(|resource| {
            (
                resource.logical_resource_id.as_str(),
                resource.resource_type.as_str(),
            )
        })
        .collect::<Vec<_>>();
    provider_identity.sort_unstable();
    if expected_identity != provider_identity {
        bail!("CloudFormation stack resources do not match the exact reviewed inventory");
    }
    if provider.stack_resource_summaries.iter().any(|resource| {
        !matches!(
            resource.resource_status.as_str(),
            "CREATE_COMPLETE"
                | "UPDATE_COMPLETE"
                | "UPDATE_ROLLBACK_COMPLETE"
                | "IMPORT_COMPLETE"
                | "IMPORT_ROLLBACK_COMPLETE"
        )
    }) {
        bail!("CloudFormation stack contains a resource in a non-terminal cleanup state");
    }
    Ok(())
}

fn verify_preview_retention_policy(
    expected: &[minco_deploy_aws::ReviewResource],
    template_body: &serde_json::Value,
) -> Result<()> {
    let template = normalized_processed_template(template_body)?;
    let resources = template
        .get("Resources")
        .and_then(serde_json::Value::as_object)
        .context("processed CloudFormation template has no resource map")?;
    for expected in expected {
        let resource = resources
            .get(&expected.logical_id)
            .and_then(serde_json::Value::as_object)
            .with_context(|| {
                format!(
                    "processed CloudFormation template lacks reviewed resource {}",
                    expected.logical_id
                )
            })?;
        if resource.get("Type").and_then(serde_json::Value::as_str)
            != Some(expected.resource_type.as_str())
        {
            bail!(
                "processed CloudFormation resource {} changed type",
                expected.logical_id
            );
        }
        let deletion_policy = resource
            .get("DeletionPolicy")
            .and_then(serde_json::Value::as_str);
        match expected.retention {
            ReviewResourceRetention::Retain if deletion_policy != Some("Retain") => {
                bail!(
                    "reviewed retained resource {} lacks DeletionPolicy Retain",
                    expected.logical_id
                );
            }
            ReviewResourceRetention::Delete
                if matches!(deletion_policy, Some("Retain" | "Snapshot")) =>
            {
                bail!(
                    "reviewed deleted resource {} has a retaining deletion policy",
                    expected.logical_id
                );
            }
            _ => {}
        }
    }
    Ok(())
}

fn normalized_processed_template(template_body: &serde_json::Value) -> Result<serde_json::Value> {
    if let Some(source) = template_body.as_str() {
        return if let Ok(value) = serde_json::from_str(source) {
            Ok(value)
        } else {
            let yaml: serde_yaml_ng::Value = serde_yaml_ng::from_str(source)?;
            Ok(serde_json::to_value(yaml)?)
        };
    }
    Ok(template_body.clone())
}

fn review_resources_from_provider(
    configured: &[ReviewResource],
    provider: &AwsStackResources,
    template_body: &serde_json::Value,
) -> Result<Vec<ReviewResource>> {
    let template = normalized_processed_template(template_body)?;
    let resources = template
        .get("Resources")
        .and_then(serde_json::Value::as_object)
        .context("processed CloudFormation template has no resource map")?;
    let mut reviewed = Vec::with_capacity(provider.stack_resource_summaries.len());
    for provider_resource in &provider.stack_resource_summaries {
        let template_resource = resources
            .get(&provider_resource.logical_resource_id)
            .and_then(serde_json::Value::as_object)
            .with_context(|| {
                format!(
                    "processed template lacks provider resource {}",
                    provider_resource.logical_resource_id
                )
            })?;
        let template_type = template_resource
            .get("Type")
            .and_then(serde_json::Value::as_str)
            .context("processed template resource has no type")?;
        if template_type != provider_resource.resource_type {
            bail!(
                "processed template and provider disagree about resource {}",
                provider_resource.logical_resource_id
            );
        }
        let deletion_policy = template_resource
            .get("DeletionPolicy")
            .and_then(serde_json::Value::as_str);
        let retention = match deletion_policy {
            Some("Retain") => ReviewResourceRetention::Retain,
            None | Some("Delete") => ReviewResourceRetention::Delete,
            Some(other) => bail!(
                "preview resource {} uses unsupported deletion policy {other}",
                provider_resource.logical_resource_id
            ),
        };
        if let Some(configured) = configured
            .iter()
            .find(|resource| resource.logical_id == provider_resource.logical_resource_id)
        {
            if configured.resource_type != provider_resource.resource_type
                || configured.retention != retention
            {
                bail!(
                    "provider resource {} contradicts preview target policy",
                    provider_resource.logical_resource_id
                );
            }
            reviewed.push(configured.clone());
        } else {
            reviewed.push(ReviewResource {
                logical_id: provider_resource.logical_resource_id.clone(),
                resource_type: provider_resource.resource_type.clone(),
                retention,
                idle_cost_class: review_resource_cost_class(&provider_resource.resource_type),
            });
        }
    }
    reviewed.sort_by(|left, right| left.logical_id.cmp(&right.logical_id));
    if configured
        .iter()
        .any(|configured| !reviewed.contains(configured))
    {
        bail!("preview target policy names a resource absent from the provider stack");
    }
    verify_preview_resource_inventory(&reviewed, provider)?;
    verify_preview_retention_policy(&reviewed, template_body)?;
    Ok(reviewed)
}

fn review_resource_cost_class(resource_type: &str) -> ReviewCostClass {
    match resource_type {
        "AWS::S3::Bucket" | "AWS::Logs::LogGroup" | "AWS::DynamoDB::Table" => {
            ReviewCostClass::StorageOnly
        }
        "AWS::Scheduler::Schedule" => ReviewCostClass::ScheduledWakeup,
        "AWS::Lambda::Function"
        | "AWS::ApiGatewayV2::Api"
        | "AWS::ApiGatewayV2::Stage"
        | "AWS::SQS::Queue"
        | "AWS::SNS::Topic" => ReviewCostClass::RequestOnly,
        "AWS::IAM::Role"
        | "AWS::Lambda::Alias"
        | "AWS::Lambda::Permission"
        | "AWS::Lambda::Version" => ReviewCostClass::ZeroCompute,
        _ => ReviewCostClass::FixedMonthly,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct AwsStackOutput {
    output_key: String,
    output_value: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct AwsStackParameter {
    parameter_key: String,
    parameter_value: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "PascalCase")]
struct AwsChangeSetParameter {
    parameter_key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    parameter_value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    use_previous_value: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "PascalCase")]
struct AwsChangeSetTag {
    key: String,
    value: String,
}

impl AwsChangeSetParameter {
    fn value(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            parameter_key: key.into(),
            parameter_value: Some(value.into()),
            use_previous_value: None,
        }
    }

    fn previous(key: impl Into<String>) -> Self {
        Self {
            parameter_key: key.into(),
            parameter_value: None,
            use_previous_value: Some(true),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct AwsFunctionConfiguration {
    function_name: String,
    code_sha256: String,
    last_update_status: String,
    version: String,
    role: String,
    #[serde(default)]
    dead_letter_config: AwsDeadLetterConfiguration,
}

#[derive(Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
struct AwsDeadLetterConfiguration {
    target_arn: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct AwsAliasConfiguration {
    function_name: String,
    function_version: String,
    routing_config: Option<AwsAliasRoutingConfiguration>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct AwsAliasRoutingConfiguration {
    #[serde(default)]
    additional_version_weights: std::collections::BTreeMap<String, f64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct AwsDescribeAlarms {
    #[serde(default)]
    metric_alarms: Vec<AwsAlarm>,
    #[serde(default)]
    composite_alarms: Vec<AwsAlarm>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct AwsAlarm {
    alarm_arn: String,
    state_value: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct AwsDriftDetection {
    stack_drift_detection_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct AwsDriftStatus {
    detection_status: String,
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
    let live_version = match change_set_type {
        ChangeSetType::Create | ChangeSetType::Import => None,
        ChangeSetType::Update => {
            let stack = describe_target_stack(root, target)?
                .context("existing deployment stack disappeared during review")?;
            Some(stack_parameter(&stack, LIVE_FUNCTION_VERSION_PARAMETER)?.to_owned())
        }
    };
    let live_version_parameter =
        live_version_change_set_parameter(change_set_type, live_version.as_deref())?;
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

    let head_bucket_args = [
        "s3api".into(),
        "head-bucket".into(),
        "--bucket".into(),
        target.artifact_bucket.clone(),
        "--region".into(),
        target.expected_region.clone(),
    ];
    wait_for_s3_bucket_visibility_with(
        "verify the pre-existing artifact bucket",
        15,
        Duration::from_secs(2),
        || {
            let output = run_cloud_output_allow_failure(root, "aws", &head_bucket_args)?;
            Ok(if output.status.success() {
                None
            } else {
                Some(output.stderr)
            })
        },
        thread::sleep,
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
    let mut parameters = vec![
        AwsChangeSetParameter::value(
            "DatabaseUrlParameterName",
            &target.database_url_parameter_name,
        ),
        AwsChangeSetParameter::value(
            "DatabaseUrlKmsKeyArn",
            target.database_kms_key_arn.as_deref().unwrap_or_default(),
        ),
        AwsChangeSetParameter::value("LambdaSubnetIds", target.lambda_subnet_ids.join(",")),
        AwsChangeSetParameter::value(
            "LambdaSecurityGroupIds",
            target.lambda_security_group_ids.join(","),
        ),
        live_version_parameter,
    ];
    parameters.extend(static_site_change_set_parameters(
        &release_deployment_plan(root, release)?,
        target,
    )?);
    let parameters = aws_change_set_parameters(&parameters)?;
    let tags = aws_change_set_tags(
        environment,
        &release.release_id,
        &release.release_digest,
        &target.stack_tags,
    )?;
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
        parameters,
        "--tags".into(),
        tags,
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
    let change_set = CloudFormationChangeSet::from_aws_json(&described.stdout, change_set_type)?;
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
    Ok(Some(stack.clone()))
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
        match status.detection_status.as_str() {
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

fn live_version_change_set_parameter(
    change_set_type: ChangeSetType,
    current: Option<&str>,
) -> Result<AwsChangeSetParameter> {
    match change_set_type {
        ChangeSetType::Create => Ok(AwsChangeSetParameter::value(
            LIVE_FUNCTION_VERSION_PARAMETER,
            "candidate",
        )),
        ChangeSetType::Update => {
            let current =
                current.context("existing stack lacks the explicit live routing parameter")?;
            let valid = current == "candidate"
                || current
                    .parse::<u64>()
                    .ok()
                    .is_some_and(|version| version > 0 && version.to_string() == current);
            if !valid {
                bail!("existing stack has an invalid live routing parameter");
            }
            Ok(AwsChangeSetParameter::previous(
                LIVE_FUNCTION_VERSION_PARAMETER,
            ))
        }
        ChangeSetType::Import => bail!("import change sets are not supported"),
    }
}

fn release_deployment_plan(root: &Path, release: &ReleaseManifest) -> Result<DeploymentPlan> {
    let plan: DeploymentPlan =
        serde_json::from_slice(&fs::read(root.join(&release.deployment_plan.path))?)
            .context("parse exact release deployment plan")?;
    ensure_plan_valid(&plan)?;
    Ok(plan)
}

fn static_site_target_blockers(
    plan: &DeploymentPlan,
    target: &DeploymentTarget,
) -> Vec<&'static str> {
    let mut blockers = Vec::new();
    if let Some(static_site) = &plan.static_site {
        if static_site.custom_domain.is_some() && target.static_site_certificate_arn.is_none() {
            blockers.push("static_site_certificate_missing");
        }
        if static_site.manage_dns_alias && target.static_site_hosted_zone_id.is_none() {
            blockers.push("static_site_hosted_zone_missing");
        }
        if target.static_site_pricing_checked_on.is_none()
            || target.static_site_pricing_source.is_none()
            || target.static_site_billing_model.is_none()
            || target.static_site_flat_rate_eligibility.is_none()
        {
            blockers.push("static_site_pricing_evidence_missing");
        }
    }
    blockers
}

fn static_site_change_set_parameters(
    plan: &DeploymentPlan,
    target: &DeploymentTarget,
) -> Result<Vec<AwsChangeSetParameter>> {
    let Some(static_site) = &plan.static_site else {
        return Ok(Vec::new());
    };
    let mut parameters = Vec::new();
    if static_site.custom_domain.is_some() {
        parameters.push(AwsChangeSetParameter::value(
            "StaticSiteCertificateArn",
            target
                .static_site_certificate_arn
                .as_deref()
                .context("static-site custom domain requires a reviewed certificate ARN")?,
        ));
    }
    if static_site.manage_dns_alias {
        parameters.push(AwsChangeSetParameter::value(
            "StaticSiteHostedZoneId",
            target
                .static_site_hosted_zone_id
                .as_deref()
                .context("static-site DNS management requires a reviewed hosted-zone ID")?,
        ));
    }
    Ok(parameters)
}

fn aws_change_set_parameters(parameters: &[AwsChangeSetParameter]) -> Result<String> {
    if parameters.is_empty() {
        bail!("CloudFormation change-set parameters must not be empty");
    }
    Ok(serde_json::to_string(&parameters)?)
}

fn aws_change_set_tags(
    environment: &str,
    release_id: &str,
    release_digest: &str,
    target_tags: &std::collections::BTreeMap<String, String>,
) -> Result<String> {
    let mut tags = vec![
        AwsChangeSetTag {
            key: "MincoEnvironment".into(),
            value: environment.into(),
        },
        AwsChangeSetTag {
            key: "MincoReleaseId".into(),
            value: release_id.into(),
        },
        AwsChangeSetTag {
            key: "MincoReleaseDigest".into(),
            value: release_digest.into(),
        },
    ];
    tags.extend(target_tags.iter().map(|(key, value)| AwsChangeSetTag {
        key: key.clone(),
        value: value.clone(),
    }));
    Ok(serde_json::to_string(&tags)?)
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

fn wait_for_s3_bucket_visibility_with<F, W>(
    label: &str,
    max_attempts: u32,
    delay: Duration,
    mut check: F,
    mut wait: W,
) -> Result<()>
where
    F: FnMut() -> Result<Option<Vec<u8>>>,
    W: FnMut(Duration),
{
    if max_attempts == 0 {
        bail!("S3 bucket visibility attempts must be positive");
    }

    let mut last_error = Vec::new();
    for attempt in 1..=max_attempts {
        match check()? {
            None => return Ok(()),
            Some(stderr) if !is_s3_bucket_not_found(&stderr) => {
                bail!("{label} failed: {}", bounded_provider_error(&stderr));
            }
            Some(stderr) => last_error = stderr,
        }
        if attempt < max_attempts {
            wait(delay);
        }
    }

    bail!(
        "{label} failed after {max_attempts} attempts: {}",
        bounded_provider_error(&last_error)
    )
}

fn is_s3_bucket_not_found(stderr: &[u8]) -> bool {
    let error = String::from_utf8_lossy(stderr);
    error.contains("(404)") || error.contains("NoSuchBucket") || error.contains("Not Found")
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
    let current_change_set =
        CloudFormationChangeSet::from_aws_json(&described.stdout, current_type)?;
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
        PluginCommand::Add { plugin, dry_run } => {
            let plan = add_plugin(root, manifest, &plugin, dry_run)?;
            print_value(&plan, as_json)
        }
        PluginCommand::Explain { plugin } => {
            print_value(&explain_plugin(root, manifest, &plugin)?, as_json)
        }
        PluginCommand::Doctor => {
            let descriptors = minco::default_plugin_manager()?.descriptors();
            let report = doctor_plugins(root, manifest, &descriptors)?;
            let passed = report.is_passed();
            print_value(&report, as_json)?;
            if !passed {
                bail!("plugin doctor found blocking drift");
            }
            Ok(())
        }
        PluginCommand::Init { path, dry_run } => {
            let plan = init_plugin(root, manifest, &path, dry_run)?;
            print_value(&plan, as_json)
        }
        PluginCommand::Remove { plugin, dry_run } => {
            let plan = remove_plugin(root, manifest, &plugin, dry_run)?;
            let safe = plan.is_safe();
            print_value(&plan, as_json)?;
            if !dry_run && !safe {
                bail!("plugin {plugin} cannot be removed safely");
            }
            Ok(())
        }
        PluginCommand::Enable { id, dry_run } => {
            let plan = set_plugin_state_workflow(root, manifest, &id, true, "enable", dry_run)?;
            print_value(&plan, as_json)
        }
        PluginCommand::Disable { id, dry_run } => {
            let plan = set_plugin_state_workflow(root, manifest, &id, false, "disable", dry_run)?;
            print_value(&plan, as_json)
        }
        PluginCommand::New { id, dry_run } => generator_cmd::execute(
            root,
            manifest,
            MakeCommand::Plugin(NamedArgs { name: id, dry_run }),
            as_json,
        ),
        PluginCommand::Validate => {
            let catalog = load_catalog(root, &manifest.plugin_catalog)?;
            let mut findings = validate_catalog(root, &catalog)?;
            let manager = minco::default_plugin_manager()?;
            findings.extend(validate_distribution_contracts(
                &catalog,
                &manager.descriptors(),
            ));
            print_value(&findings, as_json)?;
            if !findings.is_empty() {
                bail!("plugin catalog validation failed");
            }
            Ok(())
        }
        PluginCommand::Test { plugin, all } => {
            let catalog = load_catalog(root, &manifest.plugin_catalog)?;
            let descriptors = minco::default_plugin_manager()?
                .descriptors()
                .into_iter()
                .map(|descriptor| (descriptor.id.as_str().to_owned(), descriptor))
                .collect::<std::collections::BTreeMap<_, _>>();
            let selected = if all {
                catalog.plugin.iter().collect::<Vec<_>>()
            } else {
                let requested = plugin.context("plugin test requires an ID or --all")?;
                let matches = catalog
                    .plugin
                    .iter()
                    .filter(|candidate| {
                        candidate.id == requested || candidate.crate_name == requested
                    })
                    .collect::<Vec<_>>();
                match matches.as_slice() {
                    [candidate] => vec![*candidate],
                    [] => bail!("unknown plugin {requested}"),
                    _ => bail!("plugin reference {requested} is ambiguous in the catalog"),
                }
            };
            let mut reports = Vec::with_capacity(selected.len());
            for plugin in selected {
                let relative = plugin.path.as_ref().with_context(|| {
                    format!(
                        "plugin {} is registry-backed; run the public minco-test kit from its package workspace",
                        plugin.id
                    )
                })?;
                let mut conformance =
                    minco_test::PluginConformance::for_package(root.join(relative));
                if let Some(descriptor) = descriptors.get(plugin.id.as_str()) {
                    conformance = conformance.with_descriptor(descriptor.clone());
                }
                reports.push(conformance.run());
            }
            let failed = reports.iter().any(|report| !report.is_passed());
            print_value(&reports, as_json)?;
            if failed {
                bail!("plugin conformance failed");
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

    let mut attestations = args.attestations;
    if let Some(static_site) = &plan.static_site {
        let static_site_output = package_output_path(root, &args.static_site_manifest)?;
        if [
            plan_path.as_path(),
            template_path.as_path(),
            output.as_path(),
        ]
        .contains(&static_site_output.as_path())
        {
            bail!("static-site manifest output must be distinct from other package outputs");
        }
        if attestations.contains(&args.static_site_manifest) {
            bail!(
                "static-site release manifest is attached automatically and must not be repeated"
            );
        }
        let release_manifest = minco::plugin_static_site::StaticSiteReleaseManifest::build(
            &provider_static_site_plan(static_site),
            root,
        )?;
        ensure_parent(&static_site_output)?;
        fs::write(&static_site_output, canonical_json(&release_manifest)?)?;
        release_manifest.verify_at(root)?;
        attestations.push(args.static_site_manifest);
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
        &attestations,
    )?;
    ensure_parent(&output)?;
    release.write_json(&output)?;
    release.verify_at(root)?;
    print_value(&release, as_json)
}

fn provider_static_site_plan(
    plan: &StaticSiteDeployment,
) -> minco::plugin_static_site::StaticSitePlan {
    minco::plugin_static_site::StaticSitePlan {
        source_directory: plan.source_directory.clone(),
        index_document: plan.index_document.clone(),
        spa_fallback: plan.spa_fallback,
        immutable_cache_seconds: plan.immutable_cache_seconds,
        html_cache_seconds: plan.html_cache_seconds,
        price_class: plan.price_class.clone(),
        ipv6_enabled: plan.ipv6_enabled,
        custom_domain: plan.custom_domain.clone(),
        manage_dns_alias: plan.manage_dns_alias,
    }
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
    let target_metadata = match target.symlink_metadata() {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(root.join(relative));
        }
        Err(error) => return Err(error).context("inspect the package target/ directory"),
    };
    if target_metadata.file_type().is_symlink() {
        bail!("package output target/ must not be a symbolic link");
    }
    if !target_metadata.is_dir() {
        bail!("package output target/ must be a directory");
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
    let manager = minco::default_plugin_manager()?;
    let selection = load_plugin_selection(manifest, &manager)?;
    let composed = manager.compose(&selection)?;
    let static_site = composed
        .services
        .get_optional::<minco::plugin_static_site::StaticSitePlan>()?
        .map(|site| StaticSiteDeployment {
            source_directory: site.source_directory.clone(),
            index_document: site.index_document.clone(),
            spa_fallback: site.spa_fallback,
            immutable_cache_seconds: site.immutable_cache_seconds,
            html_cache_seconds: site.html_cache_seconds,
            price_class: site.price_class.clone(),
            ipv6_enabled: site.ipv6_enabled,
            custom_domain: site.custom_domain.clone(),
            manage_dns_alias: site.manage_dns_alias,
        });
    let realtime = composed
        .services
        .get_optional::<minco::plugin_realtime::RealtimePlan>()?
        .map(|realtime| RealtimeDeployment {
            namespace: realtime.namespace.clone(),
            max_event_bytes: realtime.max_event_bytes,
            subscriber_claim: realtime.subscriber_claim.clone(),
        });
    let mut plan = config.into_plan_with_graph(&contract.document, composed.graph);
    plan.static_site = static_site;
    plan.realtime = realtime;
    Ok(plan)
}

fn apply_plan_target(
    plan: &mut DeploymentPlan,
    environment: &str,
    target: &DeploymentTarget,
) -> Result<()> {
    if plan.region != target.expected_region {
        bail!(
            "deployment config Region {} does not match reviewed target Region {}",
            plan.region,
            target.expected_region
        );
    }
    environment.clone_into(&mut plan.environment);
    plan.preview = match target.lifecycle {
        DeploymentTargetLifecycle::Persistent => None,
        DeploymentTargetLifecycle::Preview => {
            let preview = target
                .preview
                .as_ref()
                .context("preview target has no lifecycle policy")?;
            let resources = preview
                .resources
                .iter()
                .map(|resource| PreviewResource {
                    logical_id: resource.logical_id.clone(),
                    resource_type: resource.resource_type.clone(),
                    retention: match resource.retention {
                        ReviewResourceRetention::Delete => PreviewResourceRetention::Delete,
                        ReviewResourceRetention::Retain => PreviewResourceRetention::Retain,
                    },
                    idle_cost_class: match resource.idle_cost_class {
                        ReviewCostClass::ZeroCompute => CostClass::ZeroCompute,
                        ReviewCostClass::RequestOnly => CostClass::RequestOnly,
                        ReviewCostClass::StorageOnly => CostClass::StorageOnly,
                        ReviewCostClass::ScheduledWakeup => CostClass::ScheduledWakeup,
                        ReviewCostClass::FixedMonthly => CostClass::FixedMonthly,
                    },
                })
                .collect();
            let cleanup_schedule =
                preview
                    .cleanup_schedule
                    .as_ref()
                    .map(|cleanup| PreviewCleanupSchedule {
                        expression: cleanup.expression.clone(),
                        action_after_completion: match cleanup.action_after_completion {
                            ReviewScheduleCompletionAction::Delete => {
                                ScheduleCompletionAction::Delete
                            }
                        },
                        residual_resources: cleanup.residual_resources.clone(),
                        manual_fallback: cleanup.manual_fallback.clone(),
                    });
            Some(PreviewLifecyclePlan {
                owner: preview.owner.clone(),
                ttl_seconds: preview.ttl_seconds,
                expected_account_id: target.expected_account_id.clone(),
                expected_region: target.expected_region.clone(),
                resources,
                pricing_complete: preview.pricing_complete,
                cleanup_schedule,
            })
        }
    };
    Ok(())
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
    fn resource_generator_has_dry_run_json_cli_shape() {
        let cli = Cli::try_parse_from([
            "cargo-minco",
            "make",
            "resource",
            "order",
            "--dry-run",
            "--json",
        ])
        .expect("resource generator should expose a dry-run JSON plan");

        assert!(matches!(
            cli.command,
            Command::Make(MakeCommand::Resource(generator_cmd::NamedArgs {
                name,
                dry_run: true,
            })) if name == "order"
        ));
        assert!(cli.json);
    }

    #[test]
    fn package_is_a_first_class_top_level_command() {
        let cli = Cli::try_parse_from(["cargo-minco", "package"]).expect("package command");
        assert!(matches!(cli.command, Command::Package(_)));
    }

    #[test]
    fn local_mcp_check_is_a_first_class_non_serving_command() {
        let cli = Cli::try_parse_from(["cargo-minco", "mcp", "--check", "--json"])
            .expect("local MCP check command");

        assert!(matches!(cli.command, Command::Mcp(McpArgs { check: true })));
        assert!(cli.json);
        assert!(cli.root.is_none());
    }

    #[test]
    fn local_workbench_check_is_a_first_class_non_serving_command() {
        let cli = Cli::try_parse_from(["cargo-minco", "workbench", "--check", "--json"])
            .expect("local workbench check command");

        assert!(matches!(
            cli.command,
            Command::Workbench(WorkbenchArgs {
                check: true,
                command: None,
            })
        ));
        assert!(cli.json);
        assert!(cli.root.is_none());
    }

    #[test]
    fn local_workbench_export_requires_an_explicit_format_and_output() {
        let cli = Cli::try_parse_from([
            "cargo-minco",
            "workbench",
            "export",
            "--format",
            "static",
            "--output",
            "target/workbench",
        ])
        .expect("local workbench export command");

        assert!(matches!(
            cli.command,
            Command::Workbench(WorkbenchArgs {
                check: false,
                command: Some(WorkbenchCommand::Export(WorkbenchExportArgs {
                    format: WorkbenchExportFormat::Static,
                    output,
                })),
            }) if output == Path::new("target/workbench")
        ));
    }

    #[test]
    fn local_workbench_serve_is_an_explicit_loopback_subcommand() {
        let cli = Cli::try_parse_from([
            "cargo-minco",
            "--root",
            "/tmp/project",
            "workbench",
            "serve",
            "--port",
            "0",
        ])
        .expect("local workbench serve command");

        assert!(matches!(
            cli.command,
            Command::Workbench(WorkbenchArgs {
                check: false,
                command: Some(WorkbenchCommand::Serve(WorkbenchServeArgs { port: 0 })),
            })
        ));
        assert_eq!(cli.root, Some(PathBuf::from("/tmp/project")));
    }

    #[test]
    fn rollback_and_canary_have_non_contacting_dry_run_shapes() {
        let rollback = Cli::try_parse_from(["cargo-minco", "rollback", "--dry-run", "--json"])
            .expect("rollback assessment command");
        assert!(matches!(
            rollback.command,
            Command::Rollback(RollbackArgs { dry_run: true, .. })
        ));
        assert!(rollback.json);

        let canary =
            Cli::try_parse_from(["cargo-minco", "promote", "--dry-run", "--canary", "--json"])
                .expect("canary qualification command");
        assert!(matches!(
            canary.command,
            Command::Promote(PromoteArgs {
                dry_run: true,
                canary: true,
                ..
            })
        ));
        assert!(canary.json);
    }

    #[test]
    fn rollback_evidence_roots_are_absolute_canonical_directories() {
        let command_root = tempfile::tempdir().expect("command root");
        let current_root = tempfile::tempdir().expect("current root");
        let resolved =
            rollback_evidence_root(command_root.path(), Some(current_root.path()), "current")
                .expect("absolute evidence root");

        assert_eq!(
            resolved,
            current_root.path().canonicalize().expect("canonical root")
        );
        let relative = rollback_evidence_root(
            command_root.path(),
            Some(Path::new("../historical")),
            "target",
        )
        .expect_err("relative evidence root must fail");
        assert!(relative.to_string().contains("must be an absolute path"));
    }

    #[cfg(unix)]
    #[test]
    fn rollback_evidence_roots_reject_symlink_directories() {
        let command_root = tempfile::tempdir().expect("command root");
        let evidence_root = tempfile::tempdir().expect("evidence root");
        let link = command_root.path().join("historical-release");
        std::os::unix::fs::symlink(evidence_root.path(), &link).expect("create directory symlink");

        let error = rollback_evidence_root(command_root.path(), Some(&link), "target")
            .expect_err("symlink evidence root must fail");
        assert!(
            error
                .to_string()
                .contains("must be a non-symlink directory")
        );
    }

    #[test]
    fn canary_template_and_provider_alarm_proof_are_exact() {
        let alarm = "arn:aws:cloudwatch:ap-southeast-2:111122223333:alarm:minco-api-errors";
        let plan = plan_canary_shift(CanaryShiftInput {
            policy: minco_deploy_aws::CanaryTargetPolicy {
                initial_traffic_percent: 10,
                monitoring_minutes: 15,
                alarm_arns: vec![alarm.into()],
                api_routing: "weighted_live_alias".into(),
                worker_routing: "preserve_current_event_sources".into(),
                provisioned_concurrency: false,
            },
            expected_account_id: "111122223333".into(),
            expected_region: "ap-southeast-2".into(),
            stack_name: "minco-orders-production".into(),
            function_name: "minco-orders-api".into(),
            alias_name: "live".into(),
            current_version: "12".into(),
            candidate_version: "13".into(),
            pre_traffic_verification_digest: "a".repeat(64),
        })
        .expect("canary plan");
        let rendered = render_canary_template_source(
            br"
Resources:
  LiveFunctionAlias:
    Type: AWS::Lambda::Alias
    Properties:
      FunctionName: api
      FunctionVersion: '12'
      Name: live
",
            &plan,
        )
        .expect("render concrete weighted alias");
        let template: serde_yaml_ng::Value =
            serde_yaml_ng::from_str(&rendered).expect("rendered YAML");
        assert_eq!(
            template["Resources"]["LiveFunctionAlias"]["Properties"]["RoutingConfig"]
                ["AdditionalVersionWeights"]["13"]
                .as_f64(),
            Some(0.1)
        );
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .expect("workspace root");
        let source = fs::read(root.join("infra/aws/generated/template.yaml"))
            .expect("reference SAM template");
        let before: serde_yaml_ng::Value =
            serde_yaml_ng::from_slice(&source).expect("reference SAM YAML");
        let rendered =
            render_canary_template_source(&source, &plan).expect("render reference canary SAM");
        assert!(rendered.contains("!Ref LiveFunctionVersion"));
        assert!(rendered.contains("!GetAtt ApiFunction.Version.Version"));
        let after: serde_yaml_ng::Value =
            serde_yaml_ng::from_str(&rendered).expect("rendered reference SAM YAML");
        assert_eq!(
            before["Resources"]
                .as_mapping()
                .map(serde_yaml_ng::Mapping::len),
            after["Resources"]
                .as_mapping()
                .map(serde_yaml_ng::Mapping::len),
            "canary rendering must preserve the resource inventory"
        );
        assert_eq!(
            after["Resources"]["LiveFunctionAlias"]["Properties"]["RoutingConfig"]
                ["AdditionalVersionWeights"]["13"]
                .as_f64(),
            Some(0.1)
        );

        let described = json!({
            "RollbackConfiguration": {
                "RollbackTriggers": [{
                    "Arn": alarm,
                    "Type": "AWS::CloudWatch::Alarm"
                }],
                "MonitoringTimeInMinutes": 15
            }
        });
        verify_canary_rollback_configuration(
            &serde_json::to_vec(&described).expect("provider JSON"),
            &plan,
        )
        .expect("exact rollback alarms");
        let mut changed = described;
        changed["RollbackConfiguration"]["MonitoringTimeInMinutes"] = json!(14);
        assert!(
            verify_canary_rollback_configuration(
                &serde_json::to_vec(&changed).expect("changed provider JSON"),
                &plan,
            )
            .is_err()
        );
    }

    #[test]
    fn canary_alarm_preflight_explicitly_requests_metric_alarms() {
        let alarms = [
            "arn:aws:cloudwatch:ap-southeast-2:123456789012:alarm:api-errors".into(),
            "arn:aws:cloudwatch:ap-southeast-2:123456789012:alarm:api-latency".into(),
        ];

        assert_eq!(
            canary_alarm_describe_arguments(&alarms).expect("metric alarm request"),
            [
                "cloudwatch",
                "describe-alarms",
                "--alarm-names",
                "api-errors",
                "api-latency",
                "--alarm-types",
                "MetricAlarms",
            ]
        );
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
    fn rollback_database_binding_digest_ignores_evidence_path_only_changes() {
        let receipt = |path: &str, plan_digest: &str, file_digest: &str| {
            DeploymentReceipt::start(DeploymentReceiptInput {
                attempt_id: "attempt-001".into(),
                release_manifest: FileDigest {
                    path: "target/minco/release.json".into(),
                    sha256: "a".repeat(64),
                    bytes: 512,
                },
                release_id: format!("minco.{}", "b".repeat(24)),
                release_digest: "b".repeat(64),
                environment: ReleaseEnvironment {
                    application: "orders".into(),
                    environment: "dev".into(),
                    region: "ap-southeast-2".into(),
                },
                configuration_digest: "c".repeat(64),
                database_plans: vec![DatabasePlanBinding {
                    kind: DatabasePlanKind::Migration,
                    schema_version: 1,
                    catalog_digest: "d".repeat(64),
                    plan_digest: plan_digest.into(),
                    file: FileDigest {
                        path: path.into(),
                        sha256: file_digest.into(),
                        bytes: 256,
                    },
                    selected_set: Some("orders-postgres".into()),
                    environment: Some("dev".into()),
                }],
                attestations: Vec::new(),
            })
            .expect("deployment receipt")
        };

        let initial = receipt(
            "target/minco/aws/01-prior-initial/database-migration-plan.json",
            &"e".repeat(64),
            &"f".repeat(64),
        );
        let current = receipt(
            "target/minco/aws/02-current/database-migration-plan.json",
            &"e".repeat(64),
            &"f".repeat(64),
        );
        let changed = receipt(
            "target/minco/aws/02-current/database-migration-plan.json",
            &"0".repeat(64),
            &"9".repeat(64),
        );

        let initial_digest = database_plan_bindings_digest(&initial, DatabasePlanKind::Migration)
            .expect("initial binding digest");
        assert_eq!(
            initial_digest,
            database_plan_bindings_digest(&current, DatabasePlanKind::Migration)
                .expect("current binding digest"),
            "the evidence namespace is not part of migration compatibility"
        );
        assert_ne!(
            initial_digest,
            database_plan_bindings_digest(&changed, DatabasePlanKind::Migration)
                .expect("changed binding digest"),
            "semantic migration plan changes must remain visible"
        );
    }

    #[test]
    fn deployment_change_set_includes_deterministic_target_stack_tags() {
        let tags = std::collections::BTreeMap::from([
            ("minco:run-id".to_owned(), "run-123".to_owned()),
            ("minco:managed".to_owned(), "true".to_owned()),
        ]);
        let rendered = aws_change_set_tags(
            "dev",
            "minco.aaaaaaaaaaaaaaaaaaaaaaaa",
            &"a".repeat(64),
            &tags,
        )
        .expect("serialize change-set tags");

        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&rendered).expect("tag JSON"),
            json!([
                {"Key": "MincoEnvironment", "Value": "dev"},
                {"Key": "MincoReleaseId", "Value": "minco.aaaaaaaaaaaaaaaaaaaaaaaa"},
                {"Key": "MincoReleaseDigest", "Value": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"},
                {"Key": "minco:managed", "Value": "true"},
                {"Key": "minco:run-id", "Value": "run-123"}
            ])
        );
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
    fn cloudformation_drift_status_uses_the_provider_detection_status_key() {
        let status: AwsDriftStatus = serde_json::from_str(
            r#"{
                "DetectionStatus": "DETECTION_COMPLETE",
                "StackDriftStatus": "IN_SYNC"
            }"#,
        )
        .expect("provider drift status response");

        assert_eq!(status.detection_status, "DETECTION_COMPLETE");
        assert_eq!(status.stack_drift_status.as_deref(), Some("IN_SYNC"));
    }

    #[test]
    fn artifact_bucket_visibility_retries_only_transient_not_found_responses() {
        let mut calls = 0;
        let mut waits = Vec::new();
        wait_for_s3_bucket_visibility_with(
            "verify the pre-existing artifact bucket",
            3,
            Duration::from_secs(2),
            || {
                calls += 1;
                Ok(if calls < 3 {
                    Some(b"An error occurred (404) when calling HeadBucket: Not Found".to_vec())
                } else {
                    None
                })
            },
            |delay| waits.push(delay),
        )
        .expect("transient bucket visibility");

        assert_eq!(calls, 3);
        assert_eq!(waits, vec![Duration::from_secs(2); 2]);

        let mut denied_calls = 0;
        let denied = wait_for_s3_bucket_visibility_with(
            "verify the pre-existing artifact bucket",
            3,
            Duration::ZERO,
            || {
                denied_calls += 1;
                Ok(Some(b"AccessDenied".to_vec()))
            },
            |_| panic!("non-404 provider errors must fail without waiting"),
        )
        .expect_err("access denied must not be retried");

        assert_eq!(denied_calls, 1);
        assert!(denied.to_string().contains("AccessDenied"));
    }

    #[test]
    fn artifact_bucket_visibility_fails_closed_after_the_retry_bound() {
        let mut calls = 0;
        let error = wait_for_s3_bucket_visibility_with(
            "verify the pre-existing artifact bucket",
            3,
            Duration::ZERO,
            || {
                calls += 1;
                Ok(Some(b"NoSuchBucket".to_vec()))
            },
            |_| {},
        )
        .expect_err("exhausted not-found responses must fail");

        assert_eq!(calls, 3);
        assert!(error.to_string().contains("failed after 3 attempts"));
        assert!(error.to_string().contains("NoSuchBucket"));
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
    fn hidden_local_service_command_is_version_coupled_to_cargo_minco() {
        let cli = Cli::try_parse_from([
            "cargo-minco",
            "__local-service",
            "start",
            "postgres",
            "--application",
            "orders",
            "--compose-file",
            "infra/local/compose.yaml",
            "--port",
            "55432",
        ])
        .expect("hidden local service command");

        assert!(matches!(
            cli.command,
            Command::LocalService(service_runtime::LocalServiceArgs {
                action: service_runtime::Action::Start(_),
            })
        ));
    }

    #[test]
    fn service_execution_binds_the_exact_cli_without_changing_serialized_plan_commands() {
        let symbolic = minco_dev::CommandSpec {
            program: "cargo-minco".into(),
            arguments: vec!["__local-service".into(), "start".into()],
            environment: std::collections::BTreeMap::new(),
        };
        let mut plan = DevPlan {
            schema_version: 1,
            application: "orders".into(),
            environment: "local".into(),
            profile: "default".into(),
            external_aws_contact: false,
            services: vec![minco_dev::ServicePlan {
                id: "postgres".into(),
                kind: ServiceKind::Postgres,
                port: Some(55_432),
                local_only: true,
                aws_services: Vec::new(),
                start: Some(symbolic.clone()),
                stop: Some(symbolic),
            }],
            lifecycle: Vec::new(),
            processes: Vec::new(),
            omitted_schedule_ids: Vec::new(),
        };
        plan.services[0]
            .stop
            .as_mut()
            .expect("stop command")
            .arguments[1] = "stop".into();
        let exact = Path::new("/opt/minco/bin/cargo-minco");

        let execution = bind_local_service_program(&plan, exact).expect("exact binding");

        assert_eq!(
            plan.services[0].start.as_ref().expect("start").program,
            "cargo-minco"
        );
        assert_eq!(
            execution.services[0].start.as_ref().expect("start").program,
            exact.to_str().expect("UTF-8 exact path")
        );
        assert_eq!(
            execution.services[0].stop.as_ref().expect("stop").program,
            exact.to_str().expect("UTF-8 exact path")
        );
    }

    #[test]
    fn development_readiness_timeout_allows_a_clean_native_build() {
        assert!(DEVELOPMENT_READINESS_TIMEOUT >= Duration::from_mins(2));
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
    fn local_aws_runtime_environment_has_no_public_endpoint_or_metadata_fallback() {
        let plan = DevPlan {
            schema_version: 1,
            application: "orders".into(),
            environment: "local".into(),
            profile: "default".into(),
            external_aws_contact: false,
            services: vec![minco_dev::ServicePlan {
                id: "rustack".into(),
                kind: ServiceKind::Rustack,
                port: Some(45_666),
                local_only: true,
                aws_services: vec!["s3".into(), "sts".into()],
                start: None,
                stop: None,
            }],
            lifecycle: Vec::new(),
            processes: Vec::new(),
            omitted_schedule_ids: Vec::new(),
        };

        let environment = development_runtime_environment(
            &plan,
            DevDatabase::None,
            "local",
            EnvironmentClass::Local,
            "ap-southeast-2",
        )
        .expect("local AWS runtime environment");

        assert_eq!(environment["AWS_ENDPOINT_URL"], "http://127.0.0.1:45666");
        assert_eq!(environment["AWS_REGION"], "ap-southeast-2");
        assert_eq!(environment["AWS_DEFAULT_REGION"], "ap-southeast-2");
        assert_eq!(environment["AWS_EC2_METADATA_DISABLED"], "true");
        assert_eq!(environment["AWS_S3_FORCE_PATH_STYLE"], "true");
        assert_eq!(environment["AWS_ACCESS_KEY_ID"], "test");
        assert_eq!(environment["AWS_SECRET_ACCESS_KEY"], "test");
        assert!(environment["AWS_ENDPOINT_URL"].starts_with("http://127.0.0.1:"));
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
    fn lambda_code_digest_conversion_matches_the_provider_encoding() {
        assert_eq!(
            expected_lambda_code_sha256(&"a".repeat(64)).expect("valid digest"),
            "qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqo="
        );
        assert!(expected_lambda_code_sha256("not-a-digest").is_err());
    }

    #[test]
    fn stack_routing_evidence_requires_exactly_one_value() {
        let stack = AwsStack {
            stack_status: "UPDATE_COMPLETE".into(),
            enable_termination_protection: Some(false),
            outputs: vec![
                AwsStackOutput {
                    output_key: "ApiFunctionName".into(),
                    output_value: Some("orders-api".into()),
                },
                AwsStackOutput {
                    output_key: "ApiFunctionName".into(),
                    output_value: Some("ambiguous-api".into()),
                },
            ],
            parameters: vec![AwsStackParameter {
                parameter_key: LIVE_FUNCTION_VERSION_PARAMETER.into(),
                parameter_value: Some("42".into()),
            }],
        };

        assert!(stack_output(&stack, "ApiFunctionName").is_err());
        assert_eq!(
            stack_parameter(&stack, LIVE_FUNCTION_VERSION_PARAMETER).expect("exact live parameter"),
            "42"
        );
    }

    #[test]
    fn preview_cleanup_requires_exact_stable_provider_resource_inventory() {
        let expected = vec![
            minco_deploy_aws::ReviewResource {
                logical_id: "ApiFunction".into(),
                resource_type: "AWS::Lambda::Function".into(),
                retention: ReviewResourceRetention::Delete,
                idle_cost_class: ReviewCostClass::RequestOnly,
            },
            minco_deploy_aws::ReviewResource {
                logical_id: "ApiLogGroup".into(),
                resource_type: "AWS::Logs::LogGroup".into(),
                retention: ReviewResourceRetention::Delete,
                idle_cost_class: ReviewCostClass::StorageOnly,
            },
        ];
        let provider = AwsStackResources {
            stack_resource_summaries: vec![
                AwsStackResource {
                    logical_resource_id: "ApiLogGroup".into(),
                    resource_type: "AWS::Logs::LogGroup".into(),
                    resource_status: "UPDATE_COMPLETE".into(),
                },
                AwsStackResource {
                    logical_resource_id: "ApiFunction".into(),
                    resource_type: "AWS::Lambda::Function".into(),
                    resource_status: "CREATE_COMPLETE".into(),
                },
            ],
        };

        verify_preview_resource_inventory(&expected, &provider).expect("exact inventory");
        let mut unexpected = provider.clone();
        unexpected.stack_resource_summaries.push(AwsStackResource {
            logical_resource_id: "UnreviewedBucket".into(),
            resource_type: "AWS::S3::Bucket".into(),
            resource_status: "CREATE_COMPLETE".into(),
        });
        assert!(verify_preview_resource_inventory(&expected, &unexpected).is_err());

        let mut unstable = provider;
        unstable.stack_resource_summaries[0].resource_status = "UPDATE_IN_PROGRESS".into();
        assert!(verify_preview_resource_inventory(&expected, &unstable).is_err());
    }

    #[test]
    fn preview_review_captures_provider_generated_resources_and_retention() {
        let configured = vec![minco_deploy_aws::ReviewResource {
            logical_id: "ApiFunction".into(),
            resource_type: "AWS::Lambda::Function".into(),
            retention: ReviewResourceRetention::Delete,
            idle_cost_class: ReviewCostClass::RequestOnly,
        }];
        let provider = AwsStackResources {
            stack_resource_summaries: vec![
                AwsStackResource {
                    logical_resource_id: "GeneratedExecutionRole".into(),
                    resource_type: "AWS::IAM::Role".into(),
                    resource_status: "CREATE_COMPLETE".into(),
                },
                AwsStackResource {
                    logical_resource_id: "ApiFunction".into(),
                    resource_type: "AWS::Lambda::Function".into(),
                    resource_status: "UPDATE_COMPLETE".into(),
                },
            ],
        };
        let template = serde_json::json!({
            "Resources": {
                "ApiFunction": { "Type": "AWS::Lambda::Function" },
                "GeneratedExecutionRole": {
                    "Type": "AWS::IAM::Role",
                    "DeletionPolicy": "Retain"
                }
            }
        });

        let reviewed = review_resources_from_provider(&configured, &provider, &template)
            .expect("capture processed provider inventory");

        assert_eq!(reviewed.len(), 2);
        let generated = reviewed
            .iter()
            .find(|resource| resource.logical_id == "GeneratedExecutionRole")
            .expect("generated role");
        assert_eq!(generated.retention, ReviewResourceRetention::Retain);
        assert_eq!(generated.idle_cost_class, ReviewCostClass::ZeroCompute);
    }

    #[test]
    fn ordinary_updates_preserve_live_routing_until_explicit_promotion() {
        assert_eq!(
            live_version_change_set_parameter(ChangeSetType::Create, None)
                .expect("new stack candidate routing"),
            AwsChangeSetParameter::value(LIVE_FUNCTION_VERSION_PARAMETER, "candidate")
        );
        assert_eq!(
            live_version_change_set_parameter(ChangeSetType::Update, Some("41"))
                .expect("preserve current published version"),
            AwsChangeSetParameter::previous(LIVE_FUNCTION_VERSION_PARAMETER)
        );
        assert!(
            live_version_change_set_parameter(ChangeSetType::Update, None).is_err(),
            "an existing stack without the explicit routing boundary must fail closed"
        );
    }

    #[test]
    fn cloudformation_change_set_parameters_preserve_comma_delimited_values_as_strings() {
        let rendered = aws_change_set_parameters(&[
            AwsChangeSetParameter::value("LambdaSubnetIds", "subnet-a,subnet-b"),
            AwsChangeSetParameter::previous("LiveFunctionVersion"),
        ])
        .expect("serialize CloudFormation parameters");

        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&rendered).expect("parameter JSON"),
            json!([
                {
                    "ParameterKey": "LambdaSubnetIds",
                    "ParameterValue": "subnet-a,subnet-b"
                },
                {
                    "ParameterKey": "LiveFunctionVersion",
                    "UsePreviousValue": true
                }
            ])
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
        let static_site = plan
            .static_site
            .expect("typed static-site deployment intent");
        assert_eq!(static_site.source_directory, "dist");
        assert_eq!(static_site.price_class, "PriceClass_100");
    }

    #[test]
    fn realtime_plugin_projects_typed_appsync_plan_intent() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .expect("workspace root");
        let mut manifest = MincoManifest::load(&root).expect("workspace manifest");
        manifest.plugins.enabled.insert("realtime".into());

        let plan = load_plan(&root, &manifest, None).expect("deployment plan");

        assert!(
            plan.application_graph
                .resources
                .contains_key("realtime-api")
        );
        let realtime = plan.realtime.expect("typed realtime deployment intent");
        assert_eq!(realtime.namespace, "minco");
        assert_eq!(realtime.max_event_bytes, 5 * 1024);
        assert_eq!(realtime.subscriber_claim, "sub");
        assert_eq!(plan.local_aws_services, ["appsync", "ssm", "sts"]);
    }

    #[test]
    fn s3_checksum_evidence_is_normalized_to_release_hex() {
        assert_eq!(
            base64_sha256_to_hex("LPJNul+wow4m6DsqxbninhsWHlwfp0JecwQzYpOLmCQ=")
                .expect("valid SHA-256"),
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
        assert!(base64_sha256_to_hex("too-short").is_err());
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
