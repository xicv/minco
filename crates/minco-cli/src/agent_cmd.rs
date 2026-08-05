use anyhow::{Context, Result, bail};
use clap::{Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Component, Path, PathBuf},
};

const MANIFEST_PATH: &str = ".minco/agent-manifest.json";
const BUNDLE_JSON: &[u8] = include_bytes!("../assets/agent/bundle.json");
const SCENARIOS_JSON: &[u8] = include_bytes!("../assets/agent/evals/scenarios.json");
const CLAUDE_BRIDGE: &[u8] = include_bytes!("../templates/app/CLAUDE.md.tmpl");
const MAX_CONTEXT_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Subcommand)]
pub enum AgentCommand {
    /// Produce a deterministic, read-only projection plan.
    Plan {
        #[arg(long, value_enum)]
        target: AgentTarget,
    },
    /// Apply an exact, conflict-free projection plan.
    Sync {
        #[arg(long, value_enum)]
        target: AgentTarget,
        #[arg(long)]
        expect_plan_digest: String,
    },
    /// Diagnose projection ownership and drift without writing.
    Doctor {
        #[arg(long, value_enum, default_value = "all")]
        target: AgentTarget,
    },
    /// Return bounded project, operation, or task context without running checks.
    Context {
        #[arg(long, conflicts_with = "task")]
        operation: Option<String>,
        #[arg(long, conflicts_with = "operation")]
        task: Option<String>,
    },
    /// Validate installed projections and deterministic workflow contracts.
    Eval {
        #[arg(long, value_enum)]
        target: AgentTarget,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum AgentTarget {
    Codex,
    Claude,
    All,
}

impl AgentTarget {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::Claude => "claude",
            Self::All => "all",
        }
    }

    fn clients(self) -> &'static [Client] {
        match self {
            Self::Codex => &[Client::Codex],
            Self::Claude => &[Client::Claude],
            Self::All => &[Client::Claude, Client::Codex],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Client {
    Claude,
    Codex,
}

impl Client {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
        }
    }

    const fn projection_root(self) -> &'static str {
        match self {
            Self::Claude => ".claude/skills",
            Self::Codex => ".agents/skills",
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Asset {
    path: &'static str,
    contents: &'static [u8],
}

macro_rules! skill_assets {
    ($($name:literal),+ $(,)?) => {
        &[
            $(
                Asset {
                    path: concat!($name, "/SKILL.md"),
                    contents: include_bytes!(concat!("../assets/agent/skills/", $name, "/SKILL.md")),
                },
                Asset {
                    path: concat!($name, "/agents/openai.yaml"),
                    contents: include_bytes!(concat!("../assets/agent/skills/", $name, "/agents/openai.yaml")),
                },
                Asset {
                    path: concat!($name, "/references/workflow.md"),
                    contents: include_bytes!(concat!("../assets/agent/skills/", $name, "/references/workflow.md")),
                },
            )+
        ]
    };
}

const ASSETS: &[Asset] = skill_assets!(
    "minco-diagnose",
    "minco-framework-task",
    "minco-lifecycle",
    "minco-operation",
    "minco-plugin",
    "minco-release",
    "minco-review",
    "minco-web-application",
);

#[derive(Debug, Clone, Serialize)]
struct AgentPlan {
    schema_version: u32,
    operation: &'static str,
    minco_version: &'static str,
    bundle_digest: String,
    target: &'static str,
    safe: bool,
    actions: Vec<PlanAction>,
    conflicts: Vec<PlanConflict>,
    manual_actions: Vec<ManualAction>,
    plan_digest: String,
}

#[derive(Debug, Clone, Serialize)]
struct PlanAction {
    path: String,
    client: &'static str,
    action: ActionKind,
    desired_digest: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    current_digest: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    current_identity: Option<String>,
    #[serde(skip)]
    contents: Vec<u8>,
    #[serde(skip)]
    expected: secure::Expected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum ActionKind {
    Create,
    Update,
    Unchanged,
    Conflict,
}

#[derive(Debug, Clone, Serialize)]
struct PlanConflict {
    path: String,
    code: &'static str,
    detail: String,
}

#[derive(Debug, Clone, Serialize)]
struct ManualAction {
    client: &'static str,
    code: &'static str,
    status: &'static str,
    detail: &'static str,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct OwnershipManifest {
    schema_version: u32,
    minco_version: String,
    bundle_digest: String,
    clients: Vec<Client>,
    files: Vec<ManagedFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ManagedFile {
    path: String,
    client: Client,
    sha256: String,
}

#[derive(Debug, Serialize)]
struct SyncReport {
    schema_version: u32,
    operation: &'static str,
    target: &'static str,
    plan_digest: String,
    applied: bool,
    writes: usize,
}

#[derive(Debug, Serialize)]
struct DoctorReport {
    schema_version: u32,
    operation: &'static str,
    status: &'static str,
    minco_version: &'static str,
    bundle_digest: String,
    writes: usize,
    discovery: DiscoveryDiagnosis,
    projection: ProjectionDiagnosis,
    mcp: McpDiagnosis,
}

#[derive(Debug, Serialize)]
struct DiscoveryDiagnosis {
    manifest: &'static str,
    codex: &'static str,
    claude: &'static str,
}

#[derive(Debug, Serialize)]
struct ProjectionDiagnosis {
    target: &'static str,
    plan_digest: String,
    creates: usize,
    updates: usize,
    unchanged: usize,
    conflicts: usize,
}

#[derive(Debug, Serialize)]
struct McpDiagnosis {
    configured: Option<bool>,
    status: &'static str,
    detail: &'static str,
}

#[derive(Debug, Serialize)]
struct ContextReport {
    schema_version: u32,
    operation: &'static str,
    minco_version: &'static str,
    selection: ContextSelection,
    found: bool,
    project: ContextProject,
    project_view_limits: minco_project_view::ViewLimits,
    input_usage: minco_project_view::InputUsage,
    documentation: Vec<String>,
    context: Option<Value>,
    diagnostics: Vec<ContextDiagnostic>,
    bounds: ContextBounds,
}

#[derive(Debug, Serialize)]
struct ContextSelection {
    kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<String>,
}

#[derive(Debug, Serialize)]
struct ContextProject {
    name: String,
    source_digest: String,
    mode: &'static str,
    project_view_schema_version: u32,
}

#[derive(Debug, Serialize)]
struct ContextDiagnostic {
    code: &'static str,
    severity: &'static str,
    message: String,
}

#[derive(Debug, Serialize)]
struct ContextBounds {
    max_response_bytes: usize,
    writes: usize,
    commands_executed: usize,
    network_requests: usize,
    arbitrary_file_reads: usize,
}

#[derive(Debug, Serialize)]
struct EvaluationReport {
    schema_version: u32,
    operation: &'static str,
    status: &'static str,
    minco_version: &'static str,
    target: &'static str,
    bundle_digest: String,
    scenario_suite_digest: String,
    skills: SkillEvaluation,
    projection: ProjectionEvaluation,
    scenarios: ScenarioEvaluation,
    bounds: EvaluationBounds,
    forward_model: ForwardModelEvaluation,
}

#[derive(Debug, Serialize)]
struct SkillEvaluation {
    status: &'static str,
    checked: usize,
    files: usize,
    issues: Vec<EvaluationIssue>,
}

#[derive(Debug, Serialize)]
struct ProjectionEvaluation {
    status: &'static str,
    clients: Vec<ClientEvaluation>,
    parity: ParityEvaluation,
}

#[derive(Debug, Serialize)]
struct ClientEvaluation {
    client: &'static str,
    status: &'static str,
    checked_files: usize,
    matched_files: usize,
    issues: Vec<EvaluationIssue>,
}

#[derive(Debug, Serialize)]
struct ParityEvaluation {
    status: &'static str,
    compared_files: usize,
}

#[derive(Debug, Serialize)]
struct EvaluationIssue {
    path: String,
    code: &'static str,
    detail: String,
}

#[derive(Debug, Serialize)]
struct ScenarioEvaluation {
    status: &'static str,
    total: usize,
    trigger: usize,
    boundary: usize,
    skills_covered: usize,
    results: Vec<ScenarioResult>,
    issues: Vec<EvaluationIssue>,
}

#[derive(Debug, Serialize)]
struct ScenarioResult {
    id: String,
    skill: String,
    kind: String,
    status: &'static str,
}

#[derive(Debug, Serialize)]
struct EvaluationBounds {
    writes: usize,
    commands_executed: usize,
    network_requests: usize,
    model_invocations: usize,
}

#[derive(Debug, Serialize)]
struct ForwardModelEvaluation {
    status: &'static str,
    detail: &'static str,
}

#[derive(Debug, Deserialize)]
struct AgentBundle {
    schema_version: u32,
    minco_version: String,
    scenarios: String,
    skills: Vec<AgentBundleSkill>,
}

#[derive(Debug, Deserialize)]
struct AgentBundleSkill {
    name: String,
    path: String,
    mode: String,
    documentation: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct EvaluationSuite {
    schema_version: u32,
    scenarios: Vec<EvaluationScenario>,
}

#[derive(Debug, Deserialize)]
struct EvaluationScenario {
    id: String,
    skill: String,
    kind: String,
    prompt: String,
    required_concepts: Vec<String>,
    forbidden_actions: Vec<String>,
}

pub fn execute(root: &Path, command: AgentCommand, as_json: bool) -> Result<()> {
    match command {
        AgentCommand::Plan { target } => print(&build_plan(root, target)?, as_json),
        AgentCommand::Sync {
            target,
            expect_plan_digest,
        } => sync(root, target, &expect_plan_digest, as_json),
        AgentCommand::Doctor { target } => doctor(root, target, as_json),
        AgentCommand::Context { operation, task } => {
            context(root, operation.as_deref(), task.as_deref(), as_json)
        }
        AgentCommand::Eval { target } => evaluate(root, target, as_json),
    }
}

fn context(
    root: &Path,
    operation_id: Option<&str>,
    task_id: Option<&str>,
    as_json: bool,
) -> Result<()> {
    if let Some(identifier) = operation_id.or(task_id) {
        validate_context_identifier(identifier)?;
    }
    let view = minco_project_view::load_project_view(root)
        .context("build bounded Minco ProjectView for agent context")?;
    let project = ContextProject {
        name: view.project.name.clone(),
        source_digest: view.project.source_digest.clone(),
        mode: if view.project.name == "minco-framework" {
            "framework"
        } else {
            "application"
        },
        project_view_schema_version: view.schema_version,
    };
    let bounds = ContextBounds {
        max_response_bytes: MAX_CONTEXT_BYTES,
        writes: 0,
        commands_executed: 0,
        network_requests: 0,
        arbitrary_file_reads: 0,
    };
    let (selection, found, skill, projected, diagnostics) = match (operation_id, task_id) {
        (Some(operation_id), None) => operation_context(&view, operation_id),
        (None, Some(task_id)) => task_context(&view, task_id),
        (None, None) => (
            ContextSelection {
                kind: "project",
                id: None,
            },
            true,
            if project.mode == "framework" {
                "minco-framework-task"
            } else {
                "minco-web-application"
            },
            Some(json!({
                "summary": view.summary,
                "diagnostics": view.diagnostics,
            })),
            Vec::new(),
        ),
        (Some(_), Some(_)) => unreachable!("Clap rejects conflicting context selectors"),
    };
    let documentation = bundle_documentation(skill)?;
    let report = ContextReport {
        schema_version: 1,
        operation: "context",
        minco_version: env!("CARGO_PKG_VERSION"),
        selection,
        found,
        project,
        project_view_limits: view.limits,
        input_usage: view.input_usage,
        documentation,
        context: projected,
        diagnostics,
        bounds,
    };
    let response_bytes = serde_json::to_vec(&report)?.len();
    if response_bytes > MAX_CONTEXT_BYTES {
        bail!(
            "agent context response exceeds max_response_bytes={MAX_CONTEXT_BYTES}: {response_bytes}"
        );
    }
    print(&report, as_json)
}

fn operation_context(
    view: &minco_project_view::ProjectView,
    operation_id: &str,
) -> (
    ContextSelection,
    bool,
    &'static str,
    Option<Value>,
    Vec<ContextDiagnostic>,
) {
    let selection = ContextSelection {
        kind: "operation",
        id: Some(operation_id.into()),
    };
    let Some(node) = view.operation(operation_id) else {
        return (
            selection,
            false,
            "minco-operation",
            None,
            vec![ContextDiagnostic {
                code: "MINCO-AGENT-CONTEXT-OPERATION-ABSENT",
                severity: "information",
                message: format!(
                    "operation {operation_id:?} is absent from the bounded ProjectView"
                ),
            }],
        );
    };
    let edges = related_edges(view, &node.id);
    (
        selection,
        true,
        "minco-operation",
        Some(json!({"node": node, "edges": edges})),
        Vec::new(),
    )
}

fn task_context(
    view: &minco_project_view::ProjectView,
    task_id: &str,
) -> (
    ContextSelection,
    bool,
    &'static str,
    Option<Value>,
    Vec<ContextDiagnostic>,
) {
    let selection = ContextSelection {
        kind: "task",
        id: Some(task_id.into()),
    };
    let node_id = format!("task:{task_id}");
    let node = view.nodes.iter().find(|node| node.id == node_id);
    let readiness = view.task(task_id);
    let (Some(node), Some(readiness)) = (node, readiness) else {
        return (
            selection,
            false,
            "minco-framework-task",
            None,
            vec![ContextDiagnostic {
                code: "MINCO-AGENT-CONTEXT-TASK-ABSENT",
                severity: "information",
                message: format!("task {task_id:?} is absent from the bounded ProjectView"),
            }],
        );
    };
    let edges = related_edges(view, &node.id);
    (
        selection,
        true,
        "minco-framework-task",
        Some(json!({
            "node": node,
            "readiness": readiness,
            "edges": edges,
        })),
        Vec::new(),
    )
}

fn related_edges<'a>(
    view: &'a minco_project_view::ProjectView,
    node_id: &str,
) -> Vec<&'a minco_project_view::ProjectEdge> {
    view.edges
        .iter()
        .filter(|edge| edge.from == node_id || edge.to == node_id)
        .collect()
}

fn validate_context_identifier(identifier: &str) -> Result<()> {
    if identifier.is_empty()
        || identifier.len() > 128
        || !identifier
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-'))
    {
        bail!(
            "agent context selection must be a bounded exact identifier using ASCII letters, digits, dot, underscore, colon, or hyphen"
        );
    }
    Ok(())
}

fn bundle_documentation(skill_name: &str) -> Result<Vec<String>> {
    let bundle: AgentBundle =
        serde_json::from_slice(BUNDLE_JSON).context("parse packaged Minco agent bundle")?;
    if bundle.schema_version != 1 || bundle.minco_version != env!("CARGO_PKG_VERSION") {
        bail!(
            "packaged Minco agent bundle does not match cargo-minco {}",
            env!("CARGO_PKG_VERSION")
        );
    }
    bundle
        .skills
        .into_iter()
        .find(|skill| skill.name == skill_name)
        .map(|skill| skill.documentation)
        .with_context(|| format!("packaged Minco agent bundle has no skill {skill_name}"))
}

fn evaluate(root: &Path, target: AgentTarget, as_json: bool) -> Result<()> {
    let skills = evaluate_skills()?;
    let projection = evaluate_projection(root, target)?;
    let scenarios = evaluate_scenarios()?;
    let status = if skills.status == "passed"
        && projection.status == "passed"
        && scenarios.status == "passed"
    {
        "passed"
    } else {
        "failed"
    };
    print(
        &EvaluationReport {
            schema_version: 1,
            operation: "eval",
            status,
            minco_version: env!("CARGO_PKG_VERSION"),
            target: target.as_str(),
            bundle_digest: bundle_digest(),
            scenario_suite_digest: digest(SCENARIOS_JSON),
            skills,
            projection,
            scenarios,
            bounds: EvaluationBounds {
                writes: 0,
                commands_executed: 0,
                network_requests: 0,
                model_invocations: 0,
            },
            forward_model: ForwardModelEvaluation {
                status: "not_run",
                detail: "deterministic evaluation does not invoke Codex, Claude, or another hosted model",
            },
        },
        as_json,
    )
}

fn evaluate_skills() -> Result<SkillEvaluation> {
    let bundle: AgentBundle =
        serde_json::from_slice(BUNDLE_JSON).context("parse packaged Minco agent bundle")?;
    let mut issues = Vec::new();
    if bundle.schema_version != 1 || bundle.minco_version != env!("CARGO_PKG_VERSION") {
        issues.push(EvaluationIssue {
            path: "bundle.json".into(),
            code: "bundle_version_mismatch",
            detail: format!(
                "expected schema 1 and Minco {}, found schema {} and Minco {}",
                env!("CARGO_PKG_VERSION"),
                bundle.schema_version,
                bundle.minco_version
            ),
        });
    }
    if bundle.scenarios != "evals/scenarios.json" {
        issues.push(EvaluationIssue {
            path: "bundle.json".into(),
            code: "scenario_path_mismatch",
            detail: "scenario contract must remain at evals/scenarios.json".into(),
        });
    }
    let mut names = BTreeSet::new();
    for skill in &bundle.skills {
        if !names.insert(skill.name.as_str()) {
            issues.push(EvaluationIssue {
                path: skill.path.clone(),
                code: "duplicate_skill",
                detail: format!("skill {} appears more than once", skill.name),
            });
            continue;
        }
        if let Err(error) = validate_skill(skill) {
            issues.push(EvaluationIssue {
                path: skill.path.clone(),
                code: "invalid_skill",
                detail: error.to_string(),
            });
        }
    }
    let asset_names = ASSETS
        .iter()
        .filter_map(|asset| asset.path.split_once('/').map(|(name, _)| name))
        .collect::<BTreeSet<_>>();
    if asset_names != names {
        issues.push(EvaluationIssue {
            path: "bundle.json".into(),
            code: "bundle_asset_mismatch",
            detail: "bundle skill names and packaged asset directories differ".into(),
        });
    }
    for name in &asset_names {
        let count = ASSETS
            .iter()
            .filter(|asset| asset.path.starts_with(&format!("{name}/")))
            .count();
        if count != 3 {
            issues.push(EvaluationIssue {
                path: format!("skills/{name}"),
                code: "skill_asset_count_mismatch",
                detail: format!("expected three packaged files, found {count}"),
            });
        }
    }
    let status = if issues.is_empty() {
        "passed"
    } else {
        "failed"
    };
    Ok(SkillEvaluation {
        status,
        checked: bundle.skills.len(),
        files: ASSETS.len(),
        issues,
    })
}

fn validate_skill(skill: &AgentBundleSkill) -> Result<()> {
    if skill.path != format!("skills/{}", skill.name) {
        bail!("bundle path does not match the skill name");
    }
    if !matches!(skill.mode.as_str(), "application" | "framework" | "shared") {
        bail!("bundle mode is not application, framework, or shared");
    }
    if skill.documentation.is_empty()
        || !skill.documentation.iter().all(|identifier| {
            identifier
                .strip_prefix("minco-1.0.0:")
                .is_some_and(|relative| {
                    !relative.is_empty()
                        && Path::new(relative)
                            .components()
                            .all(|component| matches!(component, Component::Normal(_)))
                })
        })
    {
        bail!("skill has invalid versioned documentation identifiers");
    }
    let instruction_path = format!("{}/SKILL.md", skill.name);
    let instructions = asset_contents(&instruction_path)?;
    let source = std::str::from_utf8(instructions).context("SKILL.md is not UTF-8")?;
    let rest = source
        .strip_prefix("---\n")
        .context("SKILL.md has no YAML front matter start")?;
    let (front, body) = rest
        .split_once("\n---\n")
        .context("SKILL.md has no YAML front matter end")?;
    let metadata: serde_yaml_ng::Value =
        serde_yaml_ng::from_str(front).context("parse portable skill front matter")?;
    let mapping = metadata
        .as_mapping()
        .context("portable skill front matter is not a mapping")?;
    if mapping.len() != 2
        || metadata["name"].as_str() != Some(skill.name.as_str())
        || !metadata["description"].as_str().is_some_and(|description| {
            description.contains("Use when") && description.len() <= 1024
        })
    {
        bail!(
            "portable front matter must contain only matching name and bounded trigger description"
        );
    }
    if !body.contains("references/workflow.md") {
        bail!("SKILL.md does not route to references/workflow.md");
    }
    let reference_path = format!("{}/references/workflow.md", skill.name);
    if asset_contents(&reference_path)?.is_empty() {
        bail!("workflow reference is empty");
    }
    let metadata_path = format!("{}/agents/openai.yaml", skill.name);
    let openai: serde_yaml_ng::Value = serde_yaml_ng::from_slice(asset_contents(&metadata_path)?)
        .context("parse Codex skill metadata")?;
    let expected_prompt = format!("${}", skill.name);
    if !openai["interface"]["default_prompt"]
        .as_str()
        .is_some_and(|prompt| prompt.contains(&expected_prompt))
    {
        bail!("Codex default prompt does not name the portable skill");
    }
    if skill.name == "minco-release"
        && openai["policy"]["allow_implicit_invocation"].as_bool() != Some(false)
    {
        bail!("release skill permits implicit invocation");
    }
    Ok(())
}

fn asset_contents(path: &str) -> Result<&'static [u8]> {
    ASSETS
        .iter()
        .find(|asset| asset.path == path)
        .map(|asset| asset.contents)
        .with_context(|| format!("packaged asset is missing {path}"))
}

fn evaluate_projection(root: &Path, target: AgentTarget) -> Result<ProjectionEvaluation> {
    let plan = build_plan(root, target)?;
    let mut clients = Vec::new();
    for client in target.clients() {
        let mut matched_files = 0;
        let mut issues = Vec::new();
        for asset in ASSETS {
            let path = format!("{}/{}", client.projection_root(), asset.path);
            let action = plan.actions.iter().find(|action| action.path == path);
            match action.map(|action| action.action) {
                Some(ActionKind::Unchanged) => matched_files += 1,
                Some(ActionKind::Create) | None => issues.push(EvaluationIssue {
                    path,
                    code: "projection_missing",
                    detail: "projected skill file is not installed".into(),
                }),
                Some(ActionKind::Update) => issues.push(EvaluationIssue {
                    path,
                    code: "projection_outdated",
                    detail: "projected skill file does not match the packaged asset".into(),
                }),
                Some(ActionKind::Conflict) => issues.push(EvaluationIssue {
                    path,
                    code: "projection_conflict",
                    detail: "projected skill file has unsafe or ambiguous ownership".into(),
                }),
            }
        }
        for conflict in &plan.conflicts {
            let belongs_to_client = conflict.path.starts_with(client.projection_root())
                || (*client == Client::Claude
                    && matches!(conflict.path.as_str(), "AGENTS.md" | "CLAUDE.md"))
                || conflict.path == MANIFEST_PATH;
            if belongs_to_client && !issues.iter().any(|issue| issue.path == conflict.path) {
                issues.push(EvaluationIssue {
                    path: conflict.path.clone(),
                    code: "projection_plan_conflict",
                    detail: conflict.detail.clone(),
                });
            }
        }
        clients.push(ClientEvaluation {
            client: client.as_str(),
            status: if issues.is_empty() {
                "passed"
            } else {
                "failed"
            },
            checked_files: ASSETS.len(),
            matched_files,
            issues,
        });
    }
    let all_clients_passed = clients.iter().all(|client| client.status == "passed");
    let parity = if target.clients().len() == 2 {
        ParityEvaluation {
            status: if all_clients_passed {
                "passed"
            } else {
                "failed"
            },
            compared_files: ASSETS.len(),
        }
    } else {
        ParityEvaluation {
            status: "not_applicable",
            compared_files: 0,
        }
    };
    Ok(ProjectionEvaluation {
        status: if all_clients_passed {
            "passed"
        } else {
            "failed"
        },
        clients,
        parity,
    })
}

fn evaluate_scenarios() -> Result<ScenarioEvaluation> {
    let bundle: AgentBundle =
        serde_json::from_slice(BUNDLE_JSON).context("parse packaged Minco agent bundle")?;
    let suite: EvaluationSuite =
        serde_json::from_slice(SCENARIOS_JSON).context("parse packaged agent scenarios")?;
    let known_skills = bundle
        .skills
        .iter()
        .map(|skill| skill.name.as_str())
        .collect::<BTreeSet<_>>();
    let mut ids = BTreeSet::new();
    let mut covered_skills = BTreeSet::new();
    let mut coverage = BTreeSet::new();
    let mut trigger = 0;
    let mut boundary = 0;
    let mut results = Vec::new();
    let mut issues = Vec::new();
    if suite.schema_version != 1 {
        issues.push(EvaluationIssue {
            path: "evals/scenarios.json".into(),
            code: "scenario_schema_mismatch",
            detail: format!("expected schema 1, found {}", suite.schema_version),
        });
    }
    for scenario in &suite.scenarios {
        let valid_kind = match scenario.kind.as_str() {
            "trigger" => {
                trigger += 1;
                true
            }
            "boundary" => {
                boundary += 1;
                true
            }
            _ => false,
        };
        let valid = ids.insert(scenario.id.as_str())
            && known_skills.contains(scenario.skill.as_str())
            && valid_kind
            && !scenario.prompt.trim().is_empty()
            && !scenario.required_concepts.is_empty()
            && !scenario.forbidden_actions.is_empty();
        if valid {
            covered_skills.insert(scenario.skill.as_str());
            coverage.insert((scenario.skill.as_str(), scenario.kind.as_str()));
        } else {
            issues.push(EvaluationIssue {
                path: "evals/scenarios.json".into(),
                code: "invalid_scenario_contract",
                detail: format!(
                    "scenario {} has invalid or duplicate routing data",
                    scenario.id
                ),
            });
        }
        results.push(ScenarioResult {
            id: scenario.id.clone(),
            skill: scenario.skill.clone(),
            kind: scenario.kind.clone(),
            status: if valid { "passed" } else { "failed" },
        });
    }
    for skill in &known_skills {
        for kind in ["trigger", "boundary"] {
            if !coverage.contains(&(*skill, kind)) {
                issues.push(EvaluationIssue {
                    path: "evals/scenarios.json".into(),
                    code: "missing_scenario_coverage",
                    detail: format!("skill {skill} has no {kind} scenario"),
                });
            }
        }
    }
    let status = if issues.is_empty() {
        "passed"
    } else {
        "failed"
    };
    Ok(ScenarioEvaluation {
        status,
        total: suite.scenarios.len(),
        trigger,
        boundary,
        skills_covered: covered_skills.len(),
        results,
        issues,
    })
}

fn build_plan(root: &Path, target: AgentTarget) -> Result<AgentPlan> {
    let bundle_digest = bundle_digest();
    let desired = desired_files(target);
    let all_paths = all_projection_paths();
    let manifest_state = secure::inspect(root, Path::new(MANIFEST_PATH))?;
    let mut conflicts = Vec::new();
    let previous_manifest = load_manifest(&manifest_state, &all_paths, &mut conflicts);
    let previous_files = previous_manifest
        .as_ref()
        .map(|manifest| {
            manifest
                .files
                .iter()
                .map(|file| (file.path.as_str(), file))
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();
    let selected_clients = target.clients().iter().copied().collect::<BTreeSet<_>>();
    let mut actions = Vec::new();
    let mut integration_manual_actions = Vec::new();
    let mut next_files = previous_manifest
        .as_ref()
        .map(|manifest| {
            manifest
                .files
                .iter()
                .filter(|file| !selected_clients.contains(&file.client))
                .cloned()
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let claude_bridge_available = if selected_clients.contains(&Client::Claude) {
        match secure::inspect(root, Path::new("AGENTS.md"))? {
            secure::Inspection::Regular { .. } => true,
            secure::Inspection::Missing => {
                if previous_files.contains_key("CLAUDE.md") {
                    conflicts.push(PlanConflict {
                        path: "AGENTS.md".into(),
                        code: "managed_dependency_missing",
                        detail: "the managed Claude bridge requires AGENTS.md, which is absent"
                            .into(),
                    });
                } else {
                    integration_manual_actions.push(ManualAction {
                        client: "claude",
                        code: "agents_project_instructions_missing",
                        status: "manual",
                        detail: "add project-owned AGENTS.md instructions before creating the Claude bridge",
                    });
                }
                false
            }
            secure::Inspection::Unsafe { detail } => {
                conflicts.push(PlanConflict {
                    path: "AGENTS.md".into(),
                    code: "unsafe_path_entry",
                    detail,
                });
                false
            }
        }
    } else {
        true
    };

    for file in desired {
        if file.path == "CLAUDE.md" && !claude_bridge_available {
            continue;
        }
        let state = secure::inspect(root, Path::new(&file.path))?;
        let desired_digest = digest(&file.contents);
        let previous = previous_files.get(file.path.as_str()).copied();
        if file.allow_user_owned
            && previous.is_none()
            && matches!(state, secure::Inspection::Regular { .. })
        {
            integration_manual_actions.push(ManualAction {
                client: file.client.as_str(),
                code: "claude_project_instructions",
                status: "manual",
                detail: "CLAUDE.md is user-owned; preserve it and add @AGENTS.md manually if desired",
            });
            continue;
        }
        let (action, current_digest, expected) = match state {
            secure::Inspection::Missing if previous.is_some() => {
                conflicts.push(PlanConflict {
                    path: file.path.clone(),
                    code: "managed_file_missing",
                    detail: "the ownership manifest records a managed file that is now absent"
                        .into(),
                });
                (ActionKind::Conflict, None, secure::Expected::Missing)
            }
            secure::Inspection::Missing => (ActionKind::Create, None, secure::Expected::Missing),
            secure::Inspection::Regular { contents, identity } => {
                let current_digest = digest(&contents);
                match previous {
                    Some(managed) if managed.sha256 == current_digest => {
                        let action = if current_digest == desired_digest {
                            ActionKind::Unchanged
                        } else {
                            ActionKind::Update
                        };
                        (
                            action,
                            Some(current_digest.clone()),
                            secure::Expected::Regular {
                                identity,
                                digest: current_digest,
                            },
                        )
                    }
                    Some(_) => {
                        conflicts.push(PlanConflict {
                            path: file.path.clone(),
                            code: "managed_file_modified",
                            detail: "the current file digest differs from the ownership manifest"
                                .into(),
                        });
                        (
                            ActionKind::Conflict,
                            Some(current_digest.clone()),
                            secure::Expected::Regular {
                                identity,
                                digest: current_digest,
                            },
                        )
                    }
                    None => {
                        conflicts.push(PlanConflict {
                            path: file.path.clone(),
                            code: "user_owned_destination",
                            detail:
                                "the fixed projection destination exists without Minco ownership"
                                    .into(),
                        });
                        (
                            ActionKind::Conflict,
                            Some(current_digest.clone()),
                            secure::Expected::Regular {
                                identity,
                                digest: current_digest,
                            },
                        )
                    }
                }
            }
            secure::Inspection::Unsafe { detail } => {
                conflicts.push(PlanConflict {
                    path: file.path.clone(),
                    code: "unsafe_path_entry",
                    detail,
                });
                (ActionKind::Conflict, None, secure::Expected::Unsafe)
            }
        };
        let current_identity = expected.identity_token();
        next_files.push(ManagedFile {
            path: file.path.clone(),
            client: file.client,
            sha256: desired_digest.clone(),
        });
        actions.push(PlanAction {
            path: file.path,
            client: file.client.as_str(),
            action,
            desired_digest,
            current_digest,
            current_identity,
            contents: file.contents,
            expected,
        });
    }

    next_files.sort_by(|left, right| left.path.cmp(&right.path));
    next_files.dedup_by(|left, right| left.path == right.path);
    let clients = next_files
        .iter()
        .map(|file| file.client)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let next_manifest = OwnershipManifest {
        schema_version: 1,
        minco_version: env!("CARGO_PKG_VERSION").into(),
        bundle_digest: bundle_digest.clone(),
        clients,
        files: next_files,
    };
    let mut manifest_contents = serde_json::to_vec_pretty(&next_manifest)?;
    manifest_contents.push(b'\n');
    let manifest_digest = digest(&manifest_contents);
    let (manifest_action, manifest_current_digest, manifest_expected) = match manifest_state {
        secure::Inspection::Missing => (ActionKind::Create, None, secure::Expected::Missing),
        secure::Inspection::Regular { contents, identity } => {
            let current_digest = digest(&contents);
            let action = if previous_manifest.is_none() {
                ActionKind::Conflict
            } else if current_digest == manifest_digest {
                ActionKind::Unchanged
            } else {
                ActionKind::Update
            };
            (
                action,
                Some(current_digest.clone()),
                secure::Expected::Regular {
                    identity,
                    digest: current_digest,
                },
            )
        }
        secure::Inspection::Unsafe { detail } => {
            if !conflicts
                .iter()
                .any(|conflict| conflict.path == MANIFEST_PATH)
            {
                conflicts.push(PlanConflict {
                    path: MANIFEST_PATH.into(),
                    code: "unsafe_path_entry",
                    detail,
                });
            }
            (ActionKind::Conflict, None, secure::Expected::Unsafe)
        }
    };
    let manifest_current_identity = manifest_expected.identity_token();
    actions.push(PlanAction {
        path: MANIFEST_PATH.into(),
        client: "shared",
        action: manifest_action,
        desired_digest: manifest_digest,
        current_digest: manifest_current_digest,
        current_identity: manifest_current_identity,
        contents: manifest_contents,
        expected: manifest_expected,
    });
    actions.sort_by(|left, right| left.path.cmp(&right.path));
    conflicts.sort_by(|left, right| left.path.cmp(&right.path).then(left.code.cmp(right.code)));
    let mut manual_actions = target
        .clients()
        .iter()
        .map(|client| ManualAction {
            client: client.as_str(),
            code: "mcp_configuration",
            status: "manual",
            detail: "inspect client project configuration before adding the local Minco MCP server",
        })
        .collect::<Vec<_>>();
    manual_actions.append(&mut integration_manual_actions);
    manual_actions.sort_by(|left, right| {
        left.client
            .cmp(right.client)
            .then(left.code.cmp(right.code))
    });
    let mut plan = AgentPlan {
        schema_version: 1,
        operation: "plan",
        minco_version: env!("CARGO_PKG_VERSION"),
        bundle_digest,
        target: target.as_str(),
        safe: conflicts.is_empty(),
        actions,
        conflicts,
        manual_actions,
        plan_digest: String::new(),
    };
    plan.plan_digest = plan_digest(&plan)?;
    Ok(plan)
}

#[derive(Debug)]
struct DesiredFile {
    path: String,
    client: Client,
    contents: Vec<u8>,
    allow_user_owned: bool,
}

fn desired_files(target: AgentTarget) -> Vec<DesiredFile> {
    let mut files = target
        .clients()
        .iter()
        .flat_map(|client| {
            ASSETS.iter().map(move |asset| DesiredFile {
                path: format!("{}/{}", client.projection_root(), asset.path),
                client: *client,
                contents: asset.contents.to_vec(),
                allow_user_owned: false,
            })
        })
        .collect::<Vec<_>>();
    if target.clients().contains(&Client::Claude) {
        files.push(DesiredFile {
            path: "CLAUDE.md".into(),
            client: Client::Claude,
            contents: CLAUDE_BRIDGE.to_vec(),
            allow_user_owned: true,
        });
    }
    files.sort_by(|left, right| left.path.cmp(&right.path));
    files
}

fn all_projection_paths() -> BTreeMap<String, Client> {
    [AgentTarget::Claude, AgentTarget::Codex]
        .into_iter()
        .flat_map(desired_files)
        .map(|file| (file.path, file.client))
        .collect()
}

fn load_manifest(
    state: &secure::Inspection,
    allowed_paths: &BTreeMap<String, Client>,
    conflicts: &mut Vec<PlanConflict>,
) -> Option<OwnershipManifest> {
    let secure::Inspection::Regular { contents, .. } = state else {
        if let secure::Inspection::Unsafe { detail } = state {
            conflicts.push(PlanConflict {
                path: MANIFEST_PATH.into(),
                code: "unsafe_path_entry",
                detail: detail.clone(),
            });
        }
        return None;
    };
    let parsed = serde_json::from_slice::<OwnershipManifest>(contents);
    let Ok(manifest) = parsed else {
        conflicts.push(PlanConflict {
            path: MANIFEST_PATH.into(),
            code: "invalid_manifest",
            detail: "the ownership manifest is not valid schema-1 JSON".into(),
        });
        return None;
    };
    let valid = manifest.schema_version == 1
        && manifest.files.iter().all(|file| {
            allowed_paths
                .get(&file.path)
                .is_some_and(|client| *client == file.client)
                && file.sha256.len() == 64
        })
        && manifest
            .files
            .iter()
            .map(|file| file.path.as_str())
            .collect::<BTreeSet<_>>()
            .len()
            == manifest.files.len();
    if !valid {
        conflicts.push(PlanConflict {
            path: MANIFEST_PATH.into(),
            code: "invalid_manifest",
            detail: "the ownership manifest claims an invalid or duplicate fixed path".into(),
        });
        return None;
    }
    Some(manifest)
}

fn plan_digest(plan: &AgentPlan) -> Result<String> {
    let mut value = serde_json::to_value(plan)?;
    value["plan_digest"] = serde_json::Value::String(String::new());
    Ok(digest(&serde_json::to_vec(&value)?))
}

fn digest(contents: &[u8]) -> String {
    format!("{:x}", Sha256::digest(contents))
}

fn bundle_digest() -> String {
    let mut hasher = Sha256::new();
    hasher.update((BUNDLE_JSON.len() as u64).to_be_bytes());
    hasher.update(BUNDLE_JSON);
    for asset in ASSETS {
        hasher.update((asset.path.len() as u64).to_be_bytes());
        hasher.update(asset.path.as_bytes());
        hasher.update((asset.contents.len() as u64).to_be_bytes());
        hasher.update(asset.contents);
    }
    hasher.update(("CLAUDE.md".len() as u64).to_be_bytes());
    hasher.update(b"CLAUDE.md");
    hasher.update((CLAUDE_BRIDGE.len() as u64).to_be_bytes());
    hasher.update(CLAUDE_BRIDGE);
    format!("{:x}", hasher.finalize())
}

fn sync(root: &Path, target: AgentTarget, expected_digest: &str, as_json: bool) -> Result<()> {
    let plan = build_plan(root, target)?;
    if expected_digest != plan.plan_digest {
        bail!(
            "stale agent plan digest: expected {}, current {}",
            expected_digest,
            plan.plan_digest
        );
    }
    if !plan.safe {
        bail!(
            "agent sync refused because the exact plan contains {} conflict(s)",
            plan.conflicts.len()
        );
    }
    let writes = plan
        .actions
        .iter()
        .filter(|action| matches!(action.action, ActionKind::Create | ActionKind::Update))
        .count();
    if writes > 0 {
        secure::publish(
            root,
            plan.actions
                .iter()
                .filter(|action| matches!(action.action, ActionKind::Create | ActionKind::Update))
                .map(|action| secure::WriteRequest {
                    path: PathBuf::from(&action.path),
                    contents: action.contents.clone(),
                    expected: action.expected.clone(),
                })
                .collect(),
        )?;
    }
    print(
        &SyncReport {
            schema_version: 1,
            operation: "sync",
            target: target.as_str(),
            plan_digest: plan.plan_digest,
            applied: true,
            writes,
        },
        as_json,
    )
}

fn doctor(root: &Path, target: AgentTarget, as_json: bool) -> Result<()> {
    let plan = build_plan(root, target)?;
    let creates = action_count(&plan, ActionKind::Create);
    let updates = action_count(&plan, ActionKind::Update);
    let unchanged = action_count(&plan, ActionKind::Unchanged);
    let status = if !plan.safe {
        "blocked"
    } else if creates > 0 {
        "not_installed"
    } else if updates > 0 {
        "drifted"
    } else {
        "healthy"
    };
    let discovery = DiscoveryDiagnosis {
        manifest: discovery_status(&plan, MANIFEST_PATH),
        codex: client_discovery_status(&plan, Client::Codex, target),
        claude: client_discovery_status(&plan, Client::Claude, target),
    };
    print(
        &DoctorReport {
            schema_version: 1,
            operation: "doctor",
            status,
            minco_version: env!("CARGO_PKG_VERSION"),
            bundle_digest: plan.bundle_digest.clone(),
            writes: 0,
            discovery,
            projection: ProjectionDiagnosis {
                target: target.as_str(),
                plan_digest: plan.plan_digest,
                creates,
                updates,
                unchanged,
                conflicts: plan.conflicts.len(),
            },
            mcp: McpDiagnosis {
                configured: None,
                status: "unknown",
                detail: "client configuration is user-owned and is not parsed or rewritten",
            },
        },
        as_json,
    )
}

fn client_discovery_status(plan: &AgentPlan, client: Client, target: AgentTarget) -> &'static str {
    if !target.clients().contains(&client) {
        return "not_checked";
    }
    let prefix = client.projection_root();
    let relevant = plan
        .actions
        .iter()
        .filter(|action| action.path.starts_with(prefix))
        .collect::<Vec<_>>();
    if relevant
        .iter()
        .any(|action| action.action == ActionKind::Conflict)
    {
        "blocked"
    } else if relevant
        .iter()
        .all(|action| action.action == ActionKind::Create)
    {
        "absent"
    } else {
        "present"
    }
}

fn discovery_status(plan: &AgentPlan, path: &str) -> &'static str {
    match plan.actions.iter().find(|action| action.path == path) {
        Some(action) if action.action == ActionKind::Conflict => "blocked",
        Some(action) if action.action == ActionKind::Create => "absent",
        Some(_) => "present",
        None => "not_checked",
    }
}

fn action_count(plan: &AgentPlan, kind: ActionKind) -> usize {
    plan.actions
        .iter()
        .filter(|action| action.action == kind)
        .count()
}

fn print(value: &impl Serialize, as_json: bool) -> Result<()> {
    if as_json {
        println!("{}", serde_json::to_string(value)?);
    } else {
        println!("{}", serde_json::to_string_pretty(value)?);
    }
    Ok(())
}

fn validate_relative(path: &Path) -> Result<()> {
    if path.as_os_str().is_empty()
        || path.is_absolute()
        || !path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
    {
        bail!("agent projection path must be normalized and project-relative");
    }
    Ok(())
}

#[cfg(any(target_os = "linux", target_vendor = "apple"))]
mod secure {
    use super::{Context, Result, bail, digest, validate_relative};
    use rustix::{
        fd::OwnedFd,
        fs::{
            AtFlags, Mode, OFlags, RenameFlags, fstat, fsync, mkdirat, open, openat, renameat_with,
            statat, unlinkat,
        },
        io::Errno,
    };
    use std::{
        ffi::{OsStr, OsString},
        fs::File,
        io::{Read, Write},
        path::{Component, Path, PathBuf},
    };
    use uuid::Uuid;

    const DIRECTORY_FLAGS: OFlags = OFlags::RDONLY
        .union(OFlags::DIRECTORY)
        .union(OFlags::NOFOLLOW)
        .union(OFlags::CLOEXEC);
    const FILE_FLAGS: OFlags = OFlags::RDONLY
        .union(OFlags::NOFOLLOW)
        .union(OFlags::CLOEXEC);
    const MAX_MANAGED_BYTES: u64 = 1024 * 1024;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct Identity {
        device: rustix::fs::Dev,
        inode: u64,
    }

    #[derive(Debug, Clone)]
    pub enum Inspection {
        Missing,
        Regular {
            contents: Vec<u8>,
            identity: Identity,
        },
        Unsafe {
            detail: String,
        },
    }

    #[derive(Debug, Clone)]
    pub enum Expected {
        Missing,
        Regular { identity: Identity, digest: String },
        Unsafe,
    }

    impl Expected {
        pub fn identity_token(&self) -> Option<String> {
            match self {
                Self::Regular { identity, .. } => {
                    Some(format!("{:?}:{}", identity.device, identity.inode))
                }
                Self::Missing | Self::Unsafe => None,
            }
        }
    }

    #[derive(Debug)]
    pub struct WriteRequest {
        pub path: PathBuf,
        pub contents: Vec<u8>,
        pub expected: Expected,
    }

    pub fn inspect(root: &Path, relative: &Path) -> Result<Inspection> {
        validate_relative(relative)?;
        let root_fd = open(root, DIRECTORY_FLAGS, Mode::empty())
            .with_context(|| format!("open canonical project root {}", root.display()))?;
        let mut current = root_fd;
        let parent = relative.parent().unwrap_or_else(|| Path::new(""));
        for component in parent.components() {
            let Component::Normal(name) = component else {
                bail!("agent projection path must be normalized and project-relative");
            };
            match statat(&current, name, AtFlags::SYMLINK_NOFOLLOW) {
                Err(Errno::NOENT) => return Ok(Inspection::Missing),
                Err(error) => return Err(error.into()),
                Ok(stat) if rustix::fs::FileType::from_raw_mode(stat.st_mode).is_dir() => {}
                Ok(_) => {
                    return Ok(Inspection::Unsafe {
                        detail: format!(
                            "{} contains a symlink or non-directory path component",
                            relative.display()
                        ),
                    });
                }
            }
            current = match openat(&current, name, DIRECTORY_FLAGS, Mode::empty()) {
                Ok(directory) => directory,
                Err(error) => {
                    return Ok(Inspection::Unsafe {
                        detail: format!(
                            "{} changed while opening a path component: {error}",
                            relative.display()
                        ),
                    });
                }
            };
        }
        inspect_at(
            &current,
            relative.file_name().expect("validated path has a name"),
            relative,
        )
    }

    fn inspect_at(parent: &OwnedFd, name: &OsStr, path: &Path) -> Result<Inspection> {
        let stat = match statat(parent, name, AtFlags::SYMLINK_NOFOLLOW) {
            Ok(stat) => stat,
            Err(Errno::NOENT) => return Ok(Inspection::Missing),
            Err(error) => return Err(error.into()),
        };
        let file_type = rustix::fs::FileType::from_raw_mode(stat.st_mode);
        if !file_type.is_file() {
            return Ok(Inspection::Unsafe {
                detail: format!("{} contains a symlink or non-regular entry", path.display()),
            });
        }
        if stat.st_size < 0 || stat.st_size as u64 > MAX_MANAGED_BYTES {
            return Ok(Inspection::Unsafe {
                detail: format!("{} exceeds the managed file size limit", path.display()),
            });
        }
        let fd = match openat(parent, name, FILE_FLAGS, Mode::empty()) {
            Ok(fd) => fd,
            Err(error) => {
                return Ok(Inspection::Unsafe {
                    detail: format!("{} changed while opening: {error}", path.display()),
                });
            }
        };
        let opened = fstat(&fd)?;
        if (opened.st_dev, opened.st_ino) != (stat.st_dev, stat.st_ino) {
            return Ok(Inspection::Unsafe {
                detail: format!("{} changed identity while opening", path.display()),
            });
        }
        let mut contents = Vec::with_capacity(opened.st_size as usize);
        File::from(fd)
            .take(MAX_MANAGED_BYTES + 1)
            .read_to_end(&mut contents)?;
        if contents.len() as u64 > MAX_MANAGED_BYTES {
            return Ok(Inspection::Unsafe {
                detail: format!("{} changed size while reading", path.display()),
            });
        }
        Ok(Inspection::Regular {
            contents,
            identity: Identity {
                device: opened.st_dev,
                inode: opened.st_ino,
            },
        })
    }

    pub fn publish(root: &Path, requests: Vec<WriteRequest>) -> Result<()> {
        publish_inner(root, requests, || {})
    }

    #[cfg(test)]
    pub(super) fn publish_with_before_install<F>(
        root: &Path,
        requests: Vec<WriteRequest>,
        before_install: F,
    ) -> Result<()>
    where
        F: FnOnce(),
    {
        publish_inner(root, requests, before_install)
    }

    fn publish_inner<F>(root: &Path, requests: Vec<WriteRequest>, before_install: F) -> Result<()>
    where
        F: FnOnce(),
    {
        let root_fd = open(root, DIRECTORY_FLAGS, Mode::empty())
            .with_context(|| format!("open canonical project root {}", root.display()))?;
        let root_identity = identity_fd(&root_fd)?;
        let mut staged = Vec::with_capacity(requests.len());
        for request in requests {
            match stage(&root_fd, request) {
                Ok(entry) => staged.push(entry),
                Err(error) => {
                    cleanup(&mut staged, false);
                    return Err(error);
                }
            }
        }

        staged.sort_by_key(|entry| {
            let manifest = entry.path == Path::new(super::MANIFEST_PATH);
            let update = matches!(entry.expected, Expected::Regular { .. });
            (manifest, !update, entry.path.clone())
        });
        before_install();
        let result = install_all(root, &root_fd, root_identity, &mut staged);
        if result.is_err() {
            rollback(&mut staged);
        }
        cleanup(&mut staged, result.is_ok());
        result
    }

    fn stage(root: &OwnedFd, request: WriteRequest) -> Result<StagedWrite> {
        validate_relative(&request.path)?;
        if matches!(request.expected, Expected::Unsafe) {
            bail!("refuse unsafe agent projection {}", request.path.display());
        }
        let (parent, name) = open_or_create_parent(root, &request.path)?;
        let name = name.to_os_string();
        let parent_path = request
            .path
            .parent()
            .unwrap_or_else(|| Path::new(""))
            .to_path_buf();
        let parent_identity = identity_fd(&parent)?;
        verify_expected(&parent, &name, &request.path, &request.expected)?;
        let staging_name =
            OsString::from(format!(".minco-agent-{}.staging", Uuid::new_v4().simple()));
        let fd = openat(
            &parent,
            &staging_name,
            OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::RUSR | Mode::WUSR | Mode::RGRP | Mode::ROTH,
        )?;
        let created_stat = fstat(&fd)?;
        let created_identity = Identity {
            device: created_stat.st_dev,
            inode: created_stat.st_ino,
        };
        let stage_result: Result<Identity> = (|| {
            let mut file = File::from(fd);
            file.write_all(&request.contents)?;
            file.sync_all()?;
            let stage_stat = statat(&parent, &staging_name, AtFlags::SYMLINK_NOFOLLOW)?;
            let staged_identity = Identity {
                device: stage_stat.st_dev,
                inode: stage_stat.st_ino,
            };
            if staged_identity != created_identity {
                bail!(
                    "agent staging file for {} changed identity while writing",
                    request.path.display()
                );
            }
            Ok(staged_identity)
        })();
        let staged_identity = match stage_result {
            Ok(identity) => identity,
            Err(error) => {
                if identity_at(&parent, &staging_name) == Some(created_identity) {
                    let _ = unlinkat(&parent, &staging_name, AtFlags::empty());
                }
                return Err(error);
            }
        };
        Ok(StagedWrite {
            path: request.path,
            parent_path,
            parent,
            parent_identity,
            name,
            staging_name,
            staged_identity,
            expected: request.expected,
            state: InstallState::Staged,
        })
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum InstallState {
        Staged,
        Created,
        Exchanged,
        RestoreBlocked,
    }

    struct StagedWrite {
        path: PathBuf,
        parent_path: PathBuf,
        parent: OwnedFd,
        parent_identity: Identity,
        name: OsString,
        staging_name: OsString,
        staged_identity: Identity,
        expected: Expected,
        state: InstallState,
    }

    fn install_all(
        root_path: &Path,
        root: &OwnedFd,
        root_identity: Identity,
        staged: &mut [StagedWrite],
    ) -> Result<()> {
        for entry in staged {
            let reopened_root = open(root_path, DIRECTORY_FLAGS, Mode::empty())
                .with_context(|| format!("reopen project root {}", root_path.display()))?;
            if identity_fd(&reopened_root)? != root_identity {
                bail!("agent projection project root changed identity during publication");
            }
            let resolved_parent = open_parent(root, &entry.parent_path)?;
            if identity_fd(&resolved_parent)? != entry.parent_identity {
                bail!(
                    "agent projection parent {} changed identity during publication",
                    entry.parent_path.display()
                );
            }
            verify_expected(&entry.parent, &entry.name, &entry.path, &entry.expected)?;
            verify_identity(
                &entry.parent,
                &entry.staging_name,
                entry.staged_identity,
                &entry.path,
            )?;
            match entry.expected {
                Expected::Missing => {
                    renameat_with(
                        &entry.parent,
                        &entry.staging_name,
                        &entry.parent,
                        &entry.name,
                        RenameFlags::NOREPLACE,
                    )
                    .map_err(|error| anyhow::anyhow!(error))?;
                    entry.state = InstallState::Created;
                    verify_identity(
                        &entry.parent,
                        &entry.name,
                        entry.staged_identity,
                        &entry.path,
                    )?;
                }
                Expected::Regular { identity, .. } => {
                    renameat_with(
                        &entry.parent,
                        &entry.staging_name,
                        &entry.parent,
                        &entry.name,
                        RenameFlags::EXCHANGE,
                    )
                    .map_err(|error| anyhow::anyhow!(error))?;
                    entry.state = InstallState::Exchanged;
                    verify_identity(
                        &entry.parent,
                        &entry.name,
                        entry.staged_identity,
                        &entry.path,
                    )?;
                    verify_identity(&entry.parent, &entry.staging_name, identity, &entry.path)?;
                }
                Expected::Unsafe => unreachable!("unsafe writes are rejected before staging"),
            }
            fsync(&entry.parent)?;
        }
        Ok(())
    }

    fn rollback(staged: &mut [StagedWrite]) {
        for entry in staged.iter_mut().rev() {
            match entry.state {
                InstallState::Staged => {}
                InstallState::Created => {
                    if identity_at(&entry.parent, &entry.name) == Some(entry.staged_identity) {
                        if unlinkat(&entry.parent, &entry.name, AtFlags::empty()).is_ok() {
                            entry.state = InstallState::Staged;
                        } else {
                            entry.state = InstallState::RestoreBlocked;
                        }
                    } else if identity_at(&entry.parent, &entry.staging_name).is_none()
                        && renameat_with(
                            &entry.parent,
                            &entry.name,
                            &entry.parent,
                            &entry.staging_name,
                            RenameFlags::NOREPLACE,
                        )
                        .is_ok()
                    {
                        entry.state = InstallState::RestoreBlocked;
                    } else {
                        entry.state = InstallState::RestoreBlocked;
                    }
                }
                InstallState::Exchanged => {
                    let old_identity = match entry.expected {
                        Expected::Regular { identity, .. } => Some(identity),
                        Expected::Missing | Expected::Unsafe => None,
                    };
                    if old_identity.is_some_and(|identity| {
                        identity_at(&entry.parent, &entry.staging_name) == Some(identity)
                    }) {
                        if renameat_with(
                            &entry.parent,
                            &entry.staging_name,
                            &entry.parent,
                            &entry.name,
                            RenameFlags::EXCHANGE,
                        )
                        .is_ok()
                        {
                            entry.state = if identity_at(&entry.parent, &entry.staging_name)
                                == Some(entry.staged_identity)
                            {
                                InstallState::Staged
                            } else {
                                InstallState::RestoreBlocked
                            };
                        } else {
                            entry.state = InstallState::RestoreBlocked;
                        }
                    } else {
                        entry.state = InstallState::RestoreBlocked;
                    }
                }
                InstallState::RestoreBlocked => {}
            }
            let _ = fsync(&entry.parent);
        }
    }

    fn cleanup(staged: &mut [StagedWrite], committed: bool) {
        for entry in staged {
            if !committed && entry.state == InstallState::RestoreBlocked {
                continue;
            }
            let owned_identity = match entry.state {
                InstallState::Staged => Some(entry.staged_identity),
                InstallState::Exchanged if committed => match entry.expected {
                    Expected::Regular { identity, .. } => Some(identity),
                    Expected::Missing | Expected::Unsafe => None,
                },
                InstallState::Created | InstallState::Exchanged | InstallState::RestoreBlocked => {
                    None
                }
            };
            if owned_identity.is_some_and(|identity| {
                identity_at(&entry.parent, &entry.staging_name) == Some(identity)
            }) {
                let _ = unlinkat(&entry.parent, &entry.staging_name, AtFlags::empty());
                let _ = fsync(&entry.parent);
            }
        }
    }

    fn open_or_create_parent<'a>(
        root: &OwnedFd,
        relative: &'a Path,
    ) -> Result<(OwnedFd, &'a OsStr)> {
        let mut current = root.try_clone()?;
        for component in relative
            .parent()
            .unwrap_or_else(|| Path::new(""))
            .components()
        {
            let Component::Normal(name) = component else {
                bail!("agent projection path must be normalized and project-relative");
            };
            match mkdirat(
                &current,
                name,
                Mode::RUSR
                    | Mode::WUSR
                    | Mode::XUSR
                    | Mode::RGRP
                    | Mode::XGRP
                    | Mode::ROTH
                    | Mode::XOTH,
            ) {
                Ok(()) | Err(Errno::EXIST) => {}
                Err(error) => return Err(error.into()),
            }
            current = openat(&current, name, DIRECTORY_FLAGS, Mode::empty())
                .with_context(|| format!("open projection parent component {:?}", name))?;
        }
        Ok((
            current,
            relative.file_name().expect("validated path has a name"),
        ))
    }

    fn open_parent(root: &OwnedFd, relative: &Path) -> Result<OwnedFd> {
        let mut current = root.try_clone()?;
        for component in relative.components() {
            let Component::Normal(name) = component else {
                bail!("agent projection path must be normalized and project-relative");
            };
            current = openat(&current, name, DIRECTORY_FLAGS, Mode::empty())
                .with_context(|| format!("reopen projection parent component {:?}", name))?;
        }
        Ok(current)
    }

    fn verify_expected(
        parent: &OwnedFd,
        name: &OsStr,
        path: &Path,
        expected: &Expected,
    ) -> Result<()> {
        match expected {
            Expected::Missing => match statat(parent, name, AtFlags::SYMLINK_NOFOLLOW) {
                Err(Errno::NOENT) => Ok(()),
                Ok(_) => bail!("agent projection {} changed after planning", path.display()),
                Err(error) => Err(error.into()),
            },
            Expected::Regular {
                identity,
                digest: expected_digest,
            } => {
                verify_identity(parent, name, *identity, path)?;
                let inspection = inspect_at(parent, name, path)?;
                let Inspection::Regular { contents, .. } = inspection else {
                    bail!("agent projection {} changed after planning", path.display());
                };
                if digest(&contents) != *expected_digest {
                    bail!("agent projection {} changed after planning", path.display());
                }
                Ok(())
            }
            Expected::Unsafe => bail!("refuse unsafe agent projection {}", path.display()),
        }
    }

    fn verify_identity(
        parent: &OwnedFd,
        name: &OsStr,
        expected: Identity,
        path: &Path,
    ) -> Result<()> {
        if identity_at(parent, name) == Some(expected) {
            return Ok(());
        }
        bail!(
            "agent projection {} changed identity after planning",
            path.display()
        )
    }

    fn identity_at(parent: &OwnedFd, name: &OsStr) -> Option<Identity> {
        let stat = statat(parent, name, AtFlags::SYMLINK_NOFOLLOW).ok()?;
        Some(Identity {
            device: stat.st_dev,
            inode: stat.st_ino,
        })
    }

    fn identity_fd(fd: &OwnedFd) -> Result<Identity> {
        let stat = fstat(fd)?;
        Ok(Identity {
            device: stat.st_dev,
            inode: stat.st_ino,
        })
    }
}

#[cfg(not(any(target_os = "linux", target_vendor = "apple")))]
mod secure {
    use super::{Result, bail};
    use std::path::{Path, PathBuf};

    #[derive(Debug, Clone)]
    pub enum Inspection {
        Missing,
        Regular { contents: Vec<u8>, identity: () },
        Unsafe { detail: String },
    }

    #[derive(Debug, Clone)]
    pub enum Expected {
        Missing,
        Regular { identity: (), digest: String },
        Unsafe,
    }

    impl Expected {
        pub fn identity_token(&self) -> Option<String> {
            None
        }
    }

    #[derive(Debug)]
    pub struct WriteRequest {
        pub path: PathBuf,
        pub contents: Vec<u8>,
        pub expected: Expected,
    }

    pub fn inspect(_root: &Path, _relative: &Path) -> Result<Inspection> {
        bail!("safe agent projection is unsupported on this platform")
    }

    pub fn publish(_root: &Path, _requests: Vec<WriteRequest>) -> Result<()> {
        bail!("safe agent projection is unsupported on this platform")
    }
}

#[cfg(all(test, any(target_os = "linux", target_vendor = "apple")))]
mod tests {
    use super::secure::{self, Expected, WriteRequest};
    use std::{fs, path::PathBuf};

    fn request() -> WriteRequest {
        WriteRequest {
            path: PathBuf::from(".agents/skills/minco-review/SKILL.md"),
            contents: b"managed\n".to_vec(),
            expected: Expected::Missing,
        }
    }

    fn staging_files(root: &std::path::Path) -> Vec<PathBuf> {
        fn visit(current: &std::path::Path, output: &mut Vec<PathBuf>) {
            let Ok(entries) = fs::read_dir(current) else {
                return;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                let Ok(metadata) = fs::symlink_metadata(&path) else {
                    continue;
                };
                if metadata.is_dir() && !metadata.file_type().is_symlink() {
                    visit(&path, output);
                } else if path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| {
                        name.starts_with(".minco-agent-") && name.ends_with(".staging")
                    })
                {
                    output.push(path);
                }
            }
        }

        let mut output = Vec::new();
        visit(root, &mut output);
        output
    }

    #[test]
    fn concurrent_user_file_is_not_replaced() {
        let temporary = tempfile::tempdir().expect("temporary project");
        let root = temporary.path().canonicalize().expect("canonical project");
        let destination = root.join(".agents/skills/minco-review/SKILL.md");

        let error = secure::publish_with_before_install(&root, vec![request()], || {
            fs::write(&destination, "user race\n").expect("concurrent user file");
        })
        .expect_err("concurrent destination must fail closed");

        assert!(error.to_string().contains("changed after planning"));
        assert_eq!(
            fs::read_to_string(destination).expect("user file remains"),
            "user race\n"
        );
        assert!(staging_files(&root).is_empty());
    }

    #[test]
    fn replaced_staging_identity_is_neither_published_nor_deleted() {
        let temporary = tempfile::tempdir().expect("temporary project");
        let root = temporary.path().canonicalize().expect("canonical project");
        let destination = root.join(".agents/skills/minco-review/SKILL.md");

        let error = secure::publish_with_before_install(&root, vec![request()], || {
            let staging = staging_files(&root)
                .into_iter()
                .next()
                .expect("private staging file");
            fs::rename(&staging, staging.with_extension("saved")).expect("move Minco staging file");
            fs::write(&staging, "concurrent replacement\n").expect("replace staging identity");
        })
        .expect_err("changed staging identity must fail closed");

        assert!(error.to_string().contains("changed identity"));
        assert!(!destination.exists());
        let replacements = staging_files(&root);
        assert_eq!(replacements.len(), 1);
        assert_eq!(
            fs::read_to_string(&replacements[0]).expect("replacement remains"),
            "concurrent replacement\n"
        );
    }

    #[cfg(unix)]
    #[test]
    fn replaced_parent_identity_is_rejected_without_following_the_replacement() {
        use std::os::unix::fs::symlink;

        let temporary = tempfile::tempdir().expect("temporary project");
        let root = temporary.path().canonicalize().expect("canonical project");
        let outside = tempfile::tempdir().expect("outside directory");

        let error = secure::publish_with_before_install(&root, vec![request()], || {
            fs::rename(root.join(".agents"), root.join(".agents-moved"))
                .expect("move staged parent");
            symlink(outside.path(), root.join(".agents")).expect("replace parent with symlink");
        })
        .expect_err("changed parent identity must fail closed");

        assert!(error.to_string().contains("reopen projection parent"));
        assert!(!outside.path().join("skills").exists());
        assert!(
            !root
                .join(".agents-moved/skills/minco-review/SKILL.md")
                .exists()
        );
        assert!(staging_files(&root).is_empty());
    }
}
