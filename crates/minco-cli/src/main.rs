// The CLI implementation remains in the binary target. Public visibility lets
// sibling command modules share internal types; it is not part of the library
// documentation target's API.
#![allow(unreachable_pub)]

mod architecture;
mod config;
mod config_cmd;
mod feedback_cmd;
mod new_cmd;
mod plugin_cmd;
mod process;
mod roadmap;
mod update;
mod vcs;

use anyhow::{Context, Result, bail};
use architecture::validate_architecture;
use chrono::Utc;
use clap::{Args, Parser, Subcommand, ValueEnum};
use config::{MincoManifest, discover_root};
use config_cmd::ConfigCommand;
use feedback_cmd::FeedbackArgs;
use minco_contract::{Severity as ContractSeverity, generate_rust, load_contract};
use minco_core::{ApplicationGraph, PluginId, PluginManager, PluginSelection};
use minco_plan::{
    DatabaseCostEstimate, DeploymentConfig, DeploymentPlan, Severity as PlanSeverity,
    estimate_database_cost, estimate_runtime_cost, render_sam_with_code_uris,
};
use minco_release::{FileDigest, ReleaseManifest};
use new_cmd::{DatabaseChoice, NewProjectOptions, VcsChoice, create_project};
use plugin_cmd::{load_catalog, scaffold_plugin, set_plugin_state, validate_catalog};
use process::{command_available, run_shell};
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
    Check(CheckArgs),
    #[command(subcommand)]
    Config(ConfigCommand),
    #[command(subcommand)]
    Contract(ContractCommand),
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

#[derive(Debug, Clone, Copy, Subcommand)]
enum ContractCommand {
    Check,
    Sync {
        #[arg(long)]
        check: bool,
    },
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

#[derive(Debug, Clone, Copy, Subcommand)]
enum DbCommand {
    Migrate,
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
    let manifest = MincoManifest::load(&root)?;
    match command {
        Command::New(_) => unreachable!("new is handled before project discovery"),
        Command::Doctor => doctor(&root, as_json),
        Command::Check(args) => check(&root, &manifest, args, as_json),
        Command::Config(command) => config_cmd::execute(&root, &manifest, command, as_json),
        Command::Contract(command) => contract(&root, &manifest, command, as_json),
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
        Command::Db(command) => db_command(&root, &manifest, command, as_json),
        Command::Release(command) => release_command(&root, &manifest, command, as_json),
        Command::Update(command) => update_command(&root, command, as_json),
        Command::Vcs(command) => vcs_command(&root, command, as_json),
        Command::Feedback(args) => feedback_cmd::execute(&root, args, as_json).await,
    }
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
    }
    Ok(())
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
        TestCommand::Unit => configured_or(&manifest.commands.test_unit, fallback_unit),
        TestCommand::Feature => configured_or(&manifest.commands.test_feature, fallback_feature),
        TestCommand::E2e => configured_or(&manifest.commands.test_e2e, fallback_e2e),
        TestCommand::All => configured_or(&manifest.commands.test_all, fallback_all),
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

fn db_command(
    root: &Path,
    manifest: &MincoManifest,
    command: DbCommand,
    as_json: bool,
) -> Result<()> {
    match command {
        DbCommand::Migrate => {
            let command = manifest
                .commands
                .database_migrate
                .as_deref()
                .context("minco.toml must define commands.database_migrate")?;
            let result = run_shell(root, command, !as_json)?;
            if !result.success {
                bail!("database migration failed");
            }
            print_value(&result, as_json)
        }
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
