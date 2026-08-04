use minco_db::{MigrationCatalog, SeedCatalog};
use minco_plan::{DatabaseCostEstimate, DeploymentPlan, PlanDiagnostic, RuntimeCostEstimate};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, path::PathBuf};
use thiserror::Error;

pub const PROJECT_VIEW_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewLimits {
    pub max_files: usize,
    pub max_file_bytes: usize,
    pub max_total_input_bytes: usize,
    pub max_text_bytes: usize,
    pub max_nodes: usize,
    pub max_edges: usize,
    pub max_response_bytes: usize,
}

impl Default for ViewLimits {
    fn default() -> Self {
        Self {
            max_files: 1_024,
            max_file_bytes: 2 * 1_024 * 1_024,
            max_total_input_bytes: 16 * 1_024 * 1_024,
            max_text_bytes: 16 * 1_024,
            max_nodes: 4_096,
            max_edges: 8_192,
            max_response_bytes: 2 * 1_024 * 1_024,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectIdentity {
    pub name: String,
    pub source_digest: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    Manifest,
    Contract,
    Deployment,
    Roadmap,
    Task,
    PluginCatalog,
    QualityContract,
    GeneratedBinding,
    Migration,
    Seed,
    Verification,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceProvenance {
    pub kind: SourceKind,
    pub path: PathBuf,
    pub sha256: String,
    pub bytes: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    Project,
    Architecture,
    Resource,
    Operation,
    Milestone,
    Task,
    Feature,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SemanticStatus {
    NotStarted,
    Active,
    Blocked,
    Complete,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectNode {
    pub id: String,
    pub kind: NodeKind,
    pub label: String,
    pub description: Option<String>,
    pub raw_status: Option<String>,
    pub semantic_status: Option<SemanticStatus>,
    pub source: PathBuf,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub properties: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeKind {
    Contains,
    DependsOn,
    BelongsTo,
    Implements,
    Exposes,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectEdge {
    pub from: String,
    pub to: String,
    pub kind: EdgeKind,
    pub source: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatusMapping {
    pub vocabulary: String,
    pub raw: String,
    pub semantic: SemanticStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceLane {
    Source,
    LocalVerification,
    HostedVerification,
    Deployment,
    Runtime,
    Review,
}

impl EvidenceLane {
    pub const ALL: [Self; 6] = [
        Self::Source,
        Self::LocalVerification,
        Self::HostedVerification,
        Self::Deployment,
        Self::Runtime,
        Self::Review,
    ];
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceFreshness {
    pub basis: String,
    pub observed_at: Option<String>,
    pub limitation: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceItem {
    pub subject: String,
    pub state: String,
    pub source: String,
    pub exact_subject: Option<String>,
    pub freshness: EvidenceFreshness,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigurationFieldView {
    pub key: String,
    pub kind: String,
    pub required: bool,
    pub secret: bool,
    pub description: String,
    pub value: ConfigurationValue,
    pub source: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", content = "value", rename_all = "snake_case")]
pub enum ConfigurationValue {
    Declared(Value),
    Redacted,
    Absent,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeploymentProjection {
    pub plan: DeploymentPlan,
    pub diagnostics: Vec<PlanDiagnostic>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CostProjection {
    pub database: DatabaseCostEstimate,
    pub runtime: RuntimeCostEstimate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskReadiness {
    pub id: String,
    pub raw_status: String,
    pub dependencies_complete: bool,
    pub ready: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeedbackContext {
    pub feature_declared: bool,
    pub enabled: bool,
    pub operation_ids: Vec<String>,
    pub limitation: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticSeverity {
    Information,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectDiagnostic {
    pub code: String,
    pub severity: DiagnosticSeverity,
    pub message: String,
    pub source: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InputUsage {
    pub files: usize,
    pub bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DerivedSummary {
    pub derived: bool,
    pub node_count: usize,
    pub edge_count: usize,
    pub denominator: usize,
    pub task_status_counts: BTreeMap<String, usize>,
    pub ready_task_ids: Vec<String>,
    pub evidence_item_counts: BTreeMap<EvidenceLane, usize>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectView {
    pub schema_version: u32,
    pub project: ProjectIdentity,
    pub limits: ViewLimits,
    pub input_usage: InputUsage,
    pub provenance: Vec<SourceProvenance>,
    pub nodes: Vec<ProjectNode>,
    pub edges: Vec<ProjectEdge>,
    pub status_mappings: Vec<StatusMapping>,
    pub evidence: BTreeMap<EvidenceLane, Vec<EvidenceItem>>,
    pub configuration: Vec<ConfigurationFieldView>,
    pub migrations: MigrationCatalog,
    pub seeds: SeedCatalog,
    pub deployment: DeploymentProjection,
    pub costs: CostProjection,
    pub task_readiness: Vec<TaskReadiness>,
    pub feedback: FeedbackContext,
    pub summary: DerivedSummary,
    pub diagnostics: Vec<ProjectDiagnostic>,
}

impl ProjectView {
    #[must_use]
    pub fn operation(&self, operation_id: &str) -> Option<&ProjectNode> {
        self.nodes.iter().find(|node| {
            node.kind == NodeKind::Operation
                && node.properties.get("operation_id").and_then(Value::as_str) == Some(operation_id)
        })
    }

    #[must_use]
    pub fn task(&self, task_id: &str) -> Option<&TaskReadiness> {
        self.task_readiness.iter().find(|task| task.id == task_id)
    }
}

#[derive(Debug, Error)]
pub enum ProjectViewError {
    #[error("project root must be an explicit canonical absolute directory: {0}")]
    NonCanonicalRoot(PathBuf),
    #[error("declared project path is not a normalized relative path: {0}")]
    InvalidDeclaredPath(PathBuf),
    #[error("declared project path crosses a symbolic link: {0}")]
    SymbolicLink(PathBuf),
    #[error("declared project path is missing or has the wrong type: {0}")]
    InvalidPathType(PathBuf),
    #[error("project view input exceeds {limit_name}={limit} at {path}")]
    LimitExceeded {
        limit_name: &'static str,
        limit: usize,
        path: PathBuf,
    },
    #[error("project view source I/O failed at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("project view source is invalid at {path}: {message}")]
    InvalidSource { path: PathBuf, message: String },
}
