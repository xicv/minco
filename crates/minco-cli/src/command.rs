// The parent module is private; crate visibility documents the intended
// schema boundary without exposing these parser types as a public library API.
#![allow(clippy::redundant_pub_crate)]

use crate::{
    agent_cmd::AgentCommand,
    config_cmd::ConfigCommand,
    feedback_cmd::FeedbackArgs,
    generator_cmd::{MakeCommand, StubsCommand},
    handover_cmd::HandoverArgs,
    new_cmd::{DatabaseChoice, VcsChoice},
    service_runtime,
};
use clap::{Args, Parser, Subcommand, ValueEnum};
use minco_deploy_aws::{HostedCheckResult, RollbackCompatibility};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(
    name = "minco",
    version,
    about = "Contract-first Rust development and deployment control plane"
)]
pub(crate) struct Cli {
    #[arg(long, global = true)]
    pub(crate) root: Option<PathBuf>,
    #[arg(long, global = true)]
    pub(crate) json: bool,
    #[command(subcommand)]
    pub(crate) command: Command,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Command {
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
pub(crate) struct NewArgs {
    /// Lower-kebab-case application and package prefix.
    pub(crate) name: String,
    /// Destination directory; defaults to the application name.
    #[arg(long)]
    pub(crate) directory: Option<PathBuf>,
    /// Initial persistence runtime and deployment profile.
    #[arg(long, value_enum, default_value_t = DatabaseChoice::Postgres)]
    pub(crate) database: DatabaseChoice,
    /// Version-control initialization. JJ is the Minco default.
    #[arg(long, value_enum, default_value_t = VcsChoice::Jj)]
    pub(crate) vcs: VcsChoice,
}

#[derive(Debug, Clone, Copy, Args)]
pub(crate) struct CheckArgs {
    #[arg(long)]
    pub(crate) with_cargo: bool,
    #[arg(long)]
    pub(crate) with_optional: bool,
}

#[derive(Debug, Clone, Copy, Args)]
pub(crate) struct McpArgs {
    /// Validate the bounded view and MCP surface without starting a protocol server.
    #[arg(long)]
    pub(crate) check: bool,
}

#[derive(Debug, Clone, Args)]
pub(crate) struct WorkbenchArgs {
    /// Validate the bounded view and workbench surface without serving or writing.
    #[arg(long)]
    pub(crate) check: bool,
    #[command(subcommand)]
    pub(crate) command: Option<WorkbenchCommand>,
}

#[derive(Debug, Clone, Subcommand)]
pub(crate) enum WorkbenchCommand {
    /// Export one deterministic snapshot into a new project-relative directory.
    Export(WorkbenchExportArgs),
    /// Serve the current bounded snapshot over an exact loopback origin.
    Serve(WorkbenchServeArgs),
}

#[derive(Debug, Clone, Args)]
pub(crate) struct WorkbenchExportArgs {
    #[arg(long, value_enum)]
    pub(crate) format: WorkbenchExportFormat,
    #[arg(long)]
    pub(crate) output: PathBuf,
}

#[derive(Debug, Clone, Copy, Args)]
pub(crate) struct WorkbenchServeArgs {
    /// Loopback TCP port; zero asks the operating system to choose an available port.
    #[arg(long, default_value_t = 0)]
    pub(crate) port: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum WorkbenchExportFormat {
    Json,
    Mermaid,
    Static,
}

#[derive(Debug, Clone, Args)]
// These booleans are independent user-facing flags, including Clap's explicit
// positive/negative frontend pair.
#[allow(clippy::struct_excessive_bools)]
pub(crate) struct DevArgs {
    /// Print the deterministic development plan without starting anything.
    #[arg(long)]
    pub(crate) dry_run: bool,
    /// Typed runtime configuration environment.
    #[arg(long)]
    pub(crate) environment: Option<String>,
    /// Named development/deployment profile; defaults to the manifest selection.
    #[arg(long)]
    pub(crate) profile: Option<String>,
    /// Do not apply the declared local migration command.
    #[arg(long)]
    pub(crate) no_migrate: bool,
    /// Explicit local seed profile to apply.
    #[arg(long)]
    pub(crate) seed: Option<String>,
    /// Start a declared worker that is disabled by default.
    #[arg(long = "with-worker")]
    pub(crate) with_workers: Vec<String>,
    /// Omit a declared worker that is enabled by default.
    #[arg(long = "without-worker")]
    pub(crate) without_workers: Vec<String>,
    /// Start the application-defined frontend process.
    #[arg(long, conflicts_with = "no_frontend")]
    pub(crate) frontend: bool,
    /// Omit the application-defined frontend process.
    #[arg(long, conflicts_with = "frontend")]
    pub(crate) no_frontend: bool,
    /// Override the local API port.
    #[arg(long, value_parser = clap::value_parser!(u16).range(1..))]
    pub(crate) port: Option<u16>,
    /// Override the local Rustack port.
    #[arg(long, value_parser = clap::value_parser!(u16).range(1..))]
    pub(crate) rustack_port: Option<u16>,
}

#[derive(Debug, Clone, Subcommand)]
pub(crate) enum ContractCommand {
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
pub(crate) enum UpgradeCommand {
    /// Inventory application-facing compatibility boundaries for an upgrade review.
    Report,
}

#[derive(Debug, Args)]
pub(crate) struct ExplainArgs {
    pub(crate) operation_id: String,
}

#[derive(Debug, Clone, Args)]
pub(crate) struct PlanInput {
    #[arg(long)]
    pub(crate) config: Option<PathBuf>,
}

#[derive(Debug, Clone, Args)]
pub(crate) struct PackageArgs {
    #[arg(long)]
    pub(crate) config: Option<PathBuf>,
    #[arg(long)]
    pub(crate) environment: Option<String>,
    #[arg(long, default_value = "target/minco/plan.json")]
    pub(crate) plan: PathBuf,
    #[arg(long, default_value = "target/minco/template.yaml")]
    pub(crate) template: PathBuf,
    #[arg(long, default_value = "target/minco/release.json")]
    pub(crate) output: PathBuf,
    #[arg(long, default_value = "target/minco/static-site-release.json")]
    pub(crate) static_site_manifest: PathBuf,
    /// Repository-relative detached signature or provenance statement.
    #[arg(long = "attestation")]
    pub(crate) attestations: Vec<PathBuf>,
}

#[derive(Debug, Clone, Args)]
pub(crate) struct ChangeSetArgs {
    #[arg(long, default_value = "infra/aws/deployment-targets.toml")]
    pub(crate) target_config: PathBuf,
    #[arg(long)]
    pub(crate) environment: Option<String>,
    #[arg(long, default_value = "target/minco/release.json")]
    pub(crate) manifest: PathBuf,
    #[arg(long, default_value = "target/minco/change-set.json")]
    pub(crate) output: PathBuf,
    #[arg(long)]
    pub(crate) approve_release_digest: Option<String>,
    #[arg(long)]
    pub(crate) dry_run: bool,
}

#[derive(Debug, Clone, Args)]
pub(crate) struct ApplyArgs {
    #[arg(long, default_value = "target/minco/change-set.json")]
    pub(crate) changeset: PathBuf,
    #[arg(long, default_value = "target/minco/migration-plan.json")]
    pub(crate) migration_plan: PathBuf,
    #[arg(long, default_value = "target/minco/migration-receipt.json")]
    pub(crate) migration_receipt: PathBuf,
    #[arg(long, default_value = "target/minco/deployment-receipt.json")]
    pub(crate) receipt: PathBuf,
    #[arg(long)]
    pub(crate) approve_changeset_digest: Option<String>,
    #[arg(long)]
    pub(crate) dry_run: bool,
}

#[derive(Debug, Clone, Args)]
pub(crate) struct DestroyArgs {
    #[arg(long, default_value = "infra/aws/deployment-targets.toml")]
    pub(crate) target_config: PathBuf,
    #[arg(long)]
    pub(crate) environment: Option<String>,
    #[arg(long, default_value = "target/minco/review.json")]
    pub(crate) review: PathBuf,
    #[arg(long, default_value = "target/minco/cleanup-receipt.json")]
    pub(crate) receipt: PathBuf,
    #[arg(long)]
    pub(crate) approve_review_digest: Option<String>,
    #[arg(long)]
    pub(crate) dry_run: bool,
}

#[derive(Debug, Clone, Args)]
pub(crate) struct DeployVerifyArgs {
    #[arg(long, default_value = "target/minco/release.json")]
    pub(crate) manifest: PathBuf,
    #[arg(long, default_value = "target/minco/deployment-receipt.json")]
    pub(crate) receipt: PathBuf,
    #[arg(long, default_value = "target/minco/hosted-verification.json")]
    pub(crate) output: PathBuf,
    #[arg(long)]
    pub(crate) static_site: bool,
    #[arg(long, default_value = "target/minco/static-site-publication.json")]
    pub(crate) static_site_publication: PathBuf,
    #[arg(long, default_value = "target/minco/static-site-verification.json")]
    pub(crate) static_site_output: PathBuf,
    #[arg(long)]
    pub(crate) dry_run: bool,
}

#[derive(Debug, Clone, Args)]
pub(crate) struct ReviewArgs {
    #[arg(long, default_value = "infra/aws/deployment-targets.toml")]
    pub(crate) target_config: PathBuf,
    #[arg(long)]
    pub(crate) environment: Option<String>,
    #[arg(long, default_value = "target/minco/release.json")]
    pub(crate) manifest: PathBuf,
    #[arg(long, default_value = "target/minco/deployment-receipt.json")]
    pub(crate) deployment_receipt: PathBuf,
    #[arg(long = "feedback")]
    pub(crate) feedback: Vec<String>,
    #[arg(long = "delivery-trace")]
    pub(crate) delivery_trace: Vec<PathBuf>,
    #[arg(long, default_value = "target/minco/review.json")]
    pub(crate) output: PathBuf,
    #[arg(long)]
    pub(crate) dry_run: bool,
}

#[derive(Debug, Clone, Args)]
pub(crate) struct StaticSitePublishInput {
    #[arg(long, default_value = "infra/aws/deployment-targets.toml")]
    pub(crate) target_config: PathBuf,
    #[arg(long)]
    pub(crate) environment: Option<String>,
    #[arg(long, default_value = "target/minco/release.json")]
    pub(crate) manifest: PathBuf,
    #[arg(long, default_value = "target/minco/deployment-receipt.json")]
    pub(crate) deployment_receipt: PathBuf,
    #[arg(long, default_value = "target/minco/static-site-publication.json")]
    pub(crate) output: PathBuf,
}

#[derive(Debug, Clone, Subcommand)]
pub(crate) enum StaticSiteCommand {
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
pub(crate) struct HostedVerificationObservation {
    pub(crate) endpoint: String,
    pub(crate) executed_artifact_digest: String,
    pub(crate) executed_version: String,
    pub(crate) checks: Vec<HostedCheckResult>,
}

#[derive(Debug, Clone, Args)]
pub(crate) struct PromoteArgs {
    #[arg(long, default_value = "target/minco/release.json")]
    pub(crate) manifest: PathBuf,
    #[arg(long, default_value = "target/minco/deployment-receipt.json")]
    pub(crate) receipt: PathBuf,
    #[arg(long, default_value = "target/minco/hosted-verification.json")]
    pub(crate) verification: PathBuf,
    #[arg(long, default_value = "target/minco/promotion-receipt.json")]
    pub(crate) output: PathBuf,
    #[arg(long)]
    pub(crate) approve_verification_digest: Option<String>,
    #[arg(long)]
    pub(crate) dry_run: bool,
    /// Plan an opt-in alarm-guarded API alias canary.
    #[arg(long)]
    pub(crate) canary: bool,
    #[arg(long, default_value = "infra/aws/deployment-targets.toml")]
    pub(crate) target_config: PathBuf,
    #[arg(long)]
    pub(crate) environment: Option<String>,
    #[arg(long, default_value = "target/minco/canary-receipt.json")]
    pub(crate) canary_output: PathBuf,
}

#[derive(Debug, Clone, Args)]
pub(crate) struct RollbackArgs {
    /// Clean exact-source checkout containing the current promotion evidence.
    #[arg(long)]
    pub(crate) current_root: Option<PathBuf>,
    /// Clean exact-source checkout containing the older target promotion evidence.
    #[arg(long)]
    pub(crate) target_root: Option<PathBuf>,
    #[arg(long, default_value = "target/minco/promotion-receipt.json")]
    pub(crate) current_promotion: PathBuf,
    #[arg(
        long,
        default_value = "target/minco/rollback-target-promotion-receipt.json"
    )]
    pub(crate) target_promotion: PathBuf,
    /// Exact operator-reviewed evidence that the older application can read current data.
    #[arg(long)]
    pub(crate) data_compatibility_evidence: Option<PathBuf>,
    #[arg(long)]
    pub(crate) dry_run: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RollbackDataCompatibilityEvidence {
    pub(crate) schema_version: u32,
    pub(crate) current_release_id: String,
    pub(crate) target_release_id: String,
    pub(crate) decision: RollbackCompatibility,
    pub(crate) reviewed_by: String,
    pub(crate) reason: String,
}

#[derive(Debug, Subcommand)]
pub(crate) enum DeployCommand {
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
pub(crate) enum RoadmapCommand {
    Status,
    Render {
        #[arg(long, value_enum, default_value_t = DiagramFormat::Mermaid)]
        format: DiagramFormat,
        #[arg(long)]
        output: Option<PathBuf>,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub(crate) enum DiagramFormat {
    Mermaid,
    Json,
}

#[derive(Debug, Subcommand)]
pub(crate) enum TaskCommand {
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
pub(crate) enum PluginCommand {
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
pub(crate) enum TestCommand {
    Unit,
    Feature,
    E2e,
    All,
}

#[derive(Debug, Clone, Args)]
pub(crate) struct DbTargetArgs {
    /// Migration set to inspect. Omitting this and the URL environment lists source state only.
    #[arg(long)]
    pub(crate) set: Option<String>,
    /// Name of the environment variable containing the database URL.
    #[arg(long, requires = "set")]
    pub(crate) database_url_env: Option<String>,
}

#[derive(Debug, Clone, Args)]
pub(crate) struct DbMigrateArgs {
    /// Migration set to apply, including its declared dependency closure.
    #[arg(long)]
    pub(crate) set: String,
    /// Name of the environment variable containing the direct migration database URL.
    #[arg(long)]
    pub(crate) database_url_env: String,
    /// Digest emitted by `minco db plan --set <id>`.
    #[arg(long)]
    pub(crate) expected_plan_digest: String,
    /// Durable JSON receipt destination.
    #[arg(long)]
    pub(crate) receipt: PathBuf,
    /// Permit plans containing data-rewrite or destructive migrations.
    #[arg(long)]
    pub(crate) allow_destructive: bool,
}

#[derive(Debug, Clone, Args)]
pub(crate) struct DbSeedArgs {
    /// Seed class to plan or apply: reference, demo, test, or bootstrap.
    #[arg(long)]
    pub(crate) profile: Option<String>,
    /// Declared environment class used for the seed allowlist; defaults to local.
    #[arg(long)]
    pub(crate) environment: Option<String>,
    /// Seed set to inspect or apply.
    #[arg(long)]
    pub(crate) set: Option<String>,
    /// Name of the environment variable containing the direct seed database URL.
    #[arg(long)]
    pub(crate) database_url_env: Option<String>,
    /// Digest emitted by the matching seed dry-run.
    #[arg(long)]
    pub(crate) expected_plan_digest: Option<String>,
    /// Durable JSON receipt destination for an applied seed plan.
    #[arg(long)]
    pub(crate) receipt: Option<PathBuf>,
    /// Produce the complete seed plan without connecting or mutating.
    #[arg(long)]
    pub(crate) dry_run: bool,
    /// Verify seed source, or the selected target when a URL environment is provided.
    #[arg(long)]
    pub(crate) verify: bool,
    /// Permit plans containing destructive seed operations.
    #[arg(long)]
    pub(crate) allow_destructive: bool,
    /// Exact environment acknowledgement required for bootstrap execution.
    #[arg(long)]
    pub(crate) authorize_bootstrap: Option<String>,
}

#[derive(Debug, Clone, Subcommand)]
pub(crate) enum DbCommand {
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
pub(crate) enum ReleaseCommand {
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
pub(crate) enum UpdateCommand {
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
pub(crate) enum VcsCommand {
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
pub(crate) struct DoctorCheck {
    pub(crate) name: String,
    pub(crate) available: bool,
    pub(crate) required: bool,
    pub(crate) required_for: String,
}
