use crate::{
    delivery_evidence::{
        ExactInput, OutputSpec, VerifiedSourceManifest, inspect_exact, is_sha256,
        publish_create_only_guarded_checked, read_exact_input, reject_secret_text, sha256,
        validate_relative, verify_current_source_manifest, verify_exact_inputs,
    },
    feedback_cmd::{VerifiedFeedbackTaskReceipt, verify_feedback_task_receipt},
};
use anyhow::{Context, Result, bail};
use chrono::NaiveDate;
use clap::Args;
use minco_plan::{DeploymentPlan, estimate_runtime_cost};
use minco_release::{DeploymentOutcome, DeploymentReceipt, FileDigest, ReleaseManifest};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
};

const DEFAULT_JSON: &str = "verification/handover.json";
const DEFAULT_MARKDOWN: &str = "verification/handover.md";
const DEFAULT_FEEDBACK: &str = "verification/feedback-task-receipts";
const POLICY: &str = "verification/performance-policy.toml";
const BASELINE: &str = "verification/1.7-performance-baseline.json";
const PROVIDER: &str = "verification/provider-evidence.toml";
const CAPABILITIES: &str = "verification/aws-capability-candidates.toml";
const OPERATIONAL_VALIDATION: &str = "verification/operational-evidence-validation.json";
const REPOSITORY_TRUTH: &str = "verification/repository-truth.toml";
const DEPLOYMENT_ASSURANCE: &str = "verification/deployment-assurance.toml";
const DOCS_RELEASE: &str = "docs-site/release.json";

#[derive(Debug, Args)]
pub struct HandoverArgs {
    /// Exact independently verifiable release manifest.
    #[arg(long)]
    pub release_manifest: PathBuf,
    /// Exact successful deployment receipt for the release.
    #[arg(long)]
    pub deployment_receipt: PathBuf,
    /// Accountable client handover owner.
    #[arg(long)]
    pub owner: String,
    /// Canonical JSON output under verification/.
    #[arg(long, default_value = DEFAULT_JSON)]
    pub json_output: PathBuf,
    /// Human-readable Markdown output under verification/.
    #[arg(long, default_value = DEFAULT_MARKDOWN)]
    pub markdown_output: PathBuf,
    /// Optional directory under verification/ containing feedback-task receipts.
    #[arg(long, default_value = DEFAULT_FEEDBACK)]
    pub feedback_receipts: PathBuf,
    /// Apply only the exact previously printed plan digest. Omit for a read-only plan.
    #[arg(long)]
    pub approve_plan_digest: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct EvidenceFile {
    path: String,
    sha256: String,
    bytes: u64,
}

impl From<FileDigest> for EvidenceFile {
    fn from(value: FileDigest) -> Self {
        Self {
            path: value.path,
            sha256: value.sha256,
            bytes: value.bytes,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum TruthStatus {
    Proven,
    CurrentBounded,
    Stale,
    Deferred,
    Unsupported,
    NotRun,
    IncompleteRates,
    MissingLiveProviderEvidence,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct TruthClaim {
    id: String,
    status: TruthStatus,
    detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ReleaseIdentity {
    release_id: String,
    release_digest: String,
    source_change: String,
    manifest: EvidenceFile,
    application: String,
    environment: String,
    region: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct DeploymentIdentity {
    attempt_id: String,
    receipt_digest: String,
    receipt: EvidenceFile,
    outcome: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ProjectViewIdentity {
    schema_version: u32,
    digest: String,
    source_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct SourceIdentity {
    tree_sha256: String,
    manifest: EvidenceFile,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct FeedbackReceiptIdentity {
    feedback_id: String,
    task_id: String,
    receipt_digest: String,
    file: EvidenceFile,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct HandoverPlanPayload {
    schema_version: u32,
    owner: String,
    json_output: String,
    markdown_output: String,
    release: ReleaseIdentity,
    deployment: DeploymentIdentity,
    project_view: ProjectViewIdentity,
    source: SourceIdentity,
    performance_policy: EvidenceFile,
    performance_baseline: EvidenceFile,
    provider_evidence: EvidenceFile,
    capability_ledger: EvidenceFile,
    operational_validation: EvidenceFile,
    repository_truth: Vec<EvidenceFile>,
    feedback_receipts: Vec<FeedbackReceiptIdentity>,
    truth: Vec<TruthClaim>,
    limitations: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct HandoverPacket {
    plan_digest: String,
    #[serde(flatten)]
    payload: HandoverPlanPayload,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct HandoverPlanOutput {
    approval_required: bool,
    applied: bool,
    idempotent: bool,
    #[serde(flatten)]
    packet: HandoverPacket,
}

struct PlannedHandover {
    root: PathBuf,
    output: HandoverPlanOutput,
    json_relative: PathBuf,
    json_bytes: Vec<u8>,
    markdown_relative: PathBuf,
    markdown_bytes: Vec<u8>,
    inputs: Vec<ExactInput>,
    source_authority: Option<VerifiedSourceManifest>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct OperationalValidationReceipt {
    schema_version: u32,
    kind: String,
    status: String,
    source_tree_sha256: String,
    inputs: BTreeMap<String, String>,
    effective_date: Option<String>,
    effective_date_source: String,
    counts: OperationalValidationCounts,
    metrics: OperationalValidationMetrics,
    findings: Vec<OperationalValidationFinding>,
    receipt_digest: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct OperationalValidationCounts {
    errors: usize,
    warnings: usize,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct OperationalValidationMetrics {
    performance_status: String,
    current_provider_profiles: usize,
    capability_candidates: usize,
    project_lessons: usize,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct OperationalValidationFinding {
    code: String,
    severity: String,
    message: String,
    path: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PerformanceBaselineSummary {
    schema_version: u32,
    kind: String,
    status: String,
    candidate_version: String,
    source_tree_sha256: String,
    production_slo: bool,
    provider_contact: bool,
    #[serde(default)]
    reason: Option<String>,
    #[serde(default)]
    limitations: Vec<String>,
    #[serde(flatten)]
    other: BTreeMap<String, serde_json::Value>,
}

pub fn execute(root: &Path, args: &HandoverArgs, as_json: bool) -> Result<()> {
    let planned = plan_handover(root, args)?;
    if let Some(approval) = args.approve_plan_digest.as_deref() {
        let output = apply_handover(planned, approval)?;
        print(&output, as_json)
    } else {
        print(&planned.output, as_json)
    }
}

fn plan_handover(root: &Path, args: &HandoverArgs) -> Result<PlannedHandover> {
    let canonical_root = root.canonicalize().with_context(|| {
        format!(
            "MINCO-HANDOVER-001: resolve project root {}",
            root.display()
        )
    })?;
    validate_owner(&args.owner)?;
    let json_relative = confined_output(&args.json_output)?;
    let markdown_relative = confined_output(&args.markdown_output)?;
    if json_relative == markdown_relative {
        bail!("MINCO-HANDOVER-005: JSON and Markdown outputs must be distinct");
    }
    let feedback_relative = confined_feedback_directory(&args.feedback_receipts)?;
    if paths_overlap(&json_relative, &feedback_relative)
        || paths_overlap(&markdown_relative, &feedback_relative)
    {
        bail!("MINCO-HANDOVER-005: outputs must not overlap the feedback receipt directory");
    }

    let release_relative = confined_input(&canonical_root, &args.release_manifest)?;
    let deployment_relative = confined_input(&canonical_root, &args.deployment_receipt)?;
    let release_input = read_exact_input(&canonical_root, &release_relative)
        .context("MINCO-HANDOVER-001: read exact release manifest")?;
    let release: ReleaseManifest = serde_json::from_slice(&release_input.bytes)
        .context("MINCO-HANDOVER-001: parse exact release manifest")?;
    release
        .verify_at(&canonical_root)
        .context("MINCO-HANDOVER-001: verify exact release manifest")?;
    let deployment_input = read_exact_input(&canonical_root, &deployment_relative)
        .context("MINCO-HANDOVER-001: read exact deployment receipt")?;
    let deployment: DeploymentReceipt = serde_json::from_slice(&deployment_input.bytes)
        .context("MINCO-HANDOVER-001: parse exact deployment receipt")?;
    deployment
        .verify_at(&canonical_root)
        .context("MINCO-HANDOVER-001: verify exact deployment receipt")?;
    let release_file = release_input.file.clone();
    let deployment_file = deployment_input.file.clone();
    validate_delivery_binding(&release, &deployment, &release_file)?;
    let mut exact_inputs = vec![release_input, deployment_input];
    bind_verified_delivery_inputs(&canonical_root, &release, &deployment, &mut exact_inputs)?;

    let source = verify_current_source_manifest(&canonical_root)
        .context("MINCO-HANDOVER-002: verify current source-manifest authority")?;
    let source_manifest_input = read_exact_input(
        &canonical_root,
        Path::new("verification/source-manifest.json"),
    )
    .context("MINCO-HANDOVER-002: bind current source-manifest authority")?;
    if source_manifest_input.file != source.file {
        bail!("MINCO-HANDOVER-002: source manifest changed while planning handover");
    }
    exact_inputs.push(source_manifest_input);
    let view = minco_project_view::load_project_view(&canonical_root)
        .context("MINCO-HANDOVER-002: build current ProjectView")?;
    let view_bytes = serde_json::to_vec(&view)?;

    let policy = bind_file(&canonical_root, Path::new(POLICY), &mut exact_inputs)?;
    let baseline = bind_file(&canonical_root, Path::new(BASELINE), &mut exact_inputs)?;
    let provider = bind_file(&canonical_root, Path::new(PROVIDER), &mut exact_inputs)?;
    let capabilities = bind_file(&canonical_root, Path::new(CAPABILITIES), &mut exact_inputs)?;
    let operational_validation = bind_file(
        &canonical_root,
        Path::new(OPERATIONAL_VALIDATION),
        &mut exact_inputs,
    )?;
    let repository_truth = vec![
        bind_file(
            &canonical_root,
            Path::new(REPOSITORY_TRUTH),
            &mut exact_inputs,
        )?,
        bind_file(
            &canonical_root,
            Path::new(DEPLOYMENT_ASSURANCE),
            &mut exact_inputs,
        )?,
        bind_file(&canonical_root, Path::new(DOCS_RELEASE), &mut exact_inputs)?,
    ];

    let policy_value: toml::Value = toml::from_str(
        std::str::from_utf8(&policy.1).context("MINCO-HANDOVER-003: policy must be UTF-8")?,
    )
    .context("MINCO-HANDOVER-003: parse performance policy")?;
    validate_policy(&policy_value, &release, &source.source_tree_sha256)?;
    let baseline_summary: PerformanceBaselineSummary = serde_json::from_slice(&baseline.1)
        .context("MINCO-HANDOVER-003: parse performance baseline")?;
    validate_baseline(&baseline_summary, &release, &source.source_tree_sha256)?;
    let provider_value: toml::Value = toml::from_str(
        std::str::from_utf8(&provider.1)
            .context("MINCO-HANDOVER-003: provider ledger must be UTF-8")?,
    )
    .context("MINCO-HANDOVER-003: parse provider evidence")?;
    let provider_states = validate_provider_ledger(&provider_value, &source.source_tree_sha256)?;
    let capability_value: toml::Value = toml::from_str(
        std::str::from_utf8(&capabilities.1)
            .context("MINCO-HANDOVER-003: capability ledger must be UTF-8")?,
    )
    .context("MINCO-HANDOVER-003: parse capability ledger")?;
    let capability_states = validate_capability_ledger(&capability_value)?;
    let validation_receipt: OperationalValidationReceipt =
        serde_json::from_slice(&operational_validation.1)
            .context("MINCO-HANDOVER-003: parse operational validation receipt")?;
    validate_operational_receipt(
        &canonical_root,
        &validation_receipt,
        &source.source_tree_sha256,
        [&policy.0, &baseline.0, &provider.0, &capabilities.0],
        &mut exact_inputs,
    )?;
    if validation_receipt.metrics.performance_status != baseline_summary.status
        || validation_receipt.metrics.current_provider_profiles
            != provider_states.get("current").copied().unwrap_or(0)
        || validation_receipt.metrics.capability_candidates
            != capability_states.values().sum::<usize>()
        || validation_receipt.metrics.project_lessons == 0
    {
        bail!("MINCO-HANDOVER-003: operational validation metrics contradict bound evidence");
    }

    let deployment_plan_input =
        read_exact_input(&canonical_root, Path::new(&release.deployment_plan.path))
            .context("MINCO-HANDOVER-003: read release deployment Plan IR")?;
    if deployment_plan_input.file != release.deployment_plan {
        bail!("MINCO-HANDOVER-003: release deployment Plan IR binding changed");
    }
    let deployment_plan: DeploymentPlan = serde_json::from_slice(&deployment_plan_input.bytes)
        .context("MINCO-HANDOVER-003: parse release deployment Plan IR")?;
    exact_inputs.push(deployment_plan_input);
    let runtime_cost = estimate_runtime_cost(&deployment_plan);

    let feedback = load_feedback_receipts(
        &canonical_root,
        &feedback_relative,
        &release,
        &deployment,
        &mut exact_inputs,
    )?;

    let mut truth = vec![
        TruthClaim {
            id: "release_manifest".into(),
            status: TruthStatus::Proven,
            detail: "The release manifest independently verifies all bound source and artifact digests.".into(),
        },
        TruthClaim {
            id: "successful_deployment_receipt".into(),
            status: TruthStatus::Proven,
            detail: "The deployment receipt independently verifies, is terminal succeeded, and binds the exact release-manifest bytes.".into(),
        },
        TruthClaim {
            id: "current_source_manifest".into(),
            status: TruthStatus::Proven,
            detail: "The complete checked-in source manifest matches direct current-tree authority.".into(),
        },
        TruthClaim {
            id: "source_relationship".into(),
            status: TruthStatus::CurrentBounded,
            detail: "The deployed release source revision and current post-feedback repository tree are separately bound authorities; current implementation evidence may postdate the deployed review release.".into(),
        },
        TruthClaim {
            id: "project_view".into(),
            status: TruthStatus::CurrentBounded,
            detail: "ProjectView is a current bounded repository projection, not deployment or production proof.".into(),
        },
    ];
    truth.push(performance_claim(&baseline_summary));
    truth.push(provider_claim(
        &provider_states,
        validation_receipt.metrics.current_provider_profiles,
    ));
    truth.push(capability_claim(&capability_states));
    truth.push(if runtime_cost.complete {
        TruthClaim {
            id: "runtime_cost_rates".into(),
            status: TruthStatus::CurrentBounded,
            detail:
                "The selected deployment topology has no unresolved rate dimensions in Plan IR."
                    .into(),
        }
    } else {
        TruthClaim {
            id: "runtime_cost_rates".into(),
            status: TruthStatus::IncompleteRates,
            detail: format!(
                "Unresolved rate dimensions: {}.",
                runtime_cost.missing_rates.join(", ")
            ),
        }
    });
    truth.push(TruthClaim {
        id: "production_slo".into(),
        status: TruthStatus::Unsupported,
        detail: "Local or hosted candidate qualification is explicitly not a production SLO."
            .into(),
    });
    truth.sort_by(|left, right| left.id.cmp(&right.id));

    let payload = HandoverPlanPayload {
        schema_version: 1,
        owner: args.owner.clone(),
        json_output: path_string(&json_relative)?,
        markdown_output: path_string(&markdown_relative)?,
        release: ReleaseIdentity {
            release_id: release.release_id.clone(),
            release_digest: release.release_digest.clone(),
            source_change: release.source_change.clone(),
            manifest: release_file.into(),
            application: release.environment.application.clone(),
            environment: release.environment.environment.clone(),
            region: release.environment.region,
        },
        deployment: DeploymentIdentity {
            attempt_id: deployment.attempt_id.clone(),
            receipt_digest: deployment.receipt_digest,
            receipt: deployment_file.into(),
            outcome: "succeeded".into(),
        },
        project_view: ProjectViewIdentity {
            schema_version: view.schema_version,
            digest: sha256(&view_bytes),
            source_digest: view.project.source_digest,
        },
        source: SourceIdentity {
            tree_sha256: source.source_tree_sha256.clone(),
            manifest: source.file.clone().into(),
        },
        performance_policy: policy.0,
        performance_baseline: baseline.0,
        provider_evidence: provider.0,
        capability_ledger: capabilities.0,
        operational_validation: operational_validation.0,
        repository_truth: repository_truth.into_iter().map(|(binding, _)| binding).collect(),
        feedback_receipts: feedback,
        truth,
        limitations: vec![
            "This packet contains no customer feedback body, attachment, private log, environment-variable value, token, credential or database URL.".into(),
            "It does not authorize implementation, deployment, promotion, publication or production acceptance.".into(),
            "Accepted operational evidence retains its recorded stale, deferred, unsupported, incomplete-rates, missing-live-provider or NOT RUN status; positive current/PASS claims fail closed until the compiled verifier covers their complete provenance.".into(),
        ],
    };
    let plan_digest = sha256(&serde_json::to_vec(&payload)?);
    let packet = HandoverPacket {
        plan_digest,
        payload,
    };
    let mut json_bytes = serde_json::to_vec_pretty(&packet)?;
    json_bytes.push(b'\n');
    let markdown_bytes = render_markdown(&packet)?.into_bytes();
    let json_text = std::str::from_utf8(&json_bytes)?;
    let markdown_text = std::str::from_utf8(&markdown_bytes)?;
    reject_secret_text("MINCO-HANDOVER-007: JSON packet", json_text, &[])?;
    reject_secret_text("MINCO-HANDOVER-007: Markdown packet", markdown_text, &[])?;
    let json_existing = inspect_exact(&canonical_root, &json_relative, &json_bytes)?;
    let markdown_existing = inspect_exact(&canonical_root, &markdown_relative, &markdown_bytes)?;
    Ok(PlannedHandover {
        root: canonical_root,
        output: HandoverPlanOutput {
            approval_required: true,
            applied: false,
            idempotent: json_existing && markdown_existing,
            packet,
        },
        json_relative,
        json_bytes,
        markdown_relative,
        markdown_bytes,
        inputs: exact_inputs,
        source_authority: Some(source),
    })
}

fn apply_handover(planned: PlannedHandover, approval: &str) -> Result<HandoverPlanOutput> {
    if !is_sha256(approval) {
        bail!("MINCO-HANDOVER-006: --approve-plan-digest must be one lowercase SHA-256 digest");
    }
    if approval != planned.output.packet.plan_digest {
        bail!("MINCO-HANDOVER-006: approved plan digest differs from the current handover plan");
    }
    verify_exact_inputs(&planned.root, &planned.inputs)
        .context("MINCO-HANDOVER-006: handover input changed after planning")?;
    if let Some(expected_source) = &planned.source_authority {
        let current_source = verify_current_source_manifest(&planned.root)
            .context("MINCO-HANDOVER-006: source authority changed after planning")?;
        if current_source != *expected_source {
            bail!("MINCO-HANDOVER-006: source authority changed after planning");
        }
    }
    let source_root = planned.root.clone();
    let source_authority = planned.source_authority.clone();
    let publication = publish_create_only_guarded_checked(
        &planned.root,
        &planned.inputs,
        vec![
            OutputSpec {
                relative: planned.json_relative,
                contents: planned.json_bytes,
            },
            OutputSpec {
                relative: planned.markdown_relative,
                contents: planned.markdown_bytes,
            },
        ],
        move || {
            if let Some(expected) = &source_authority {
                let current = verify_current_source_manifest(&source_root)?;
                if &current != expected {
                    bail!("MINCO-HANDOVER-006: source authority changed during publication");
                }
            }
            Ok(())
        },
    )?;
    let mut output = planned.output;
    output.applied = true;
    output.idempotent = publication.created.iter().all(|created| !created);
    Ok(output)
}

fn validate_owner(owner: &str) -> Result<()> {
    if owner.trim() != owner
        || owner.is_empty()
        || owner.chars().count() > 200
        || owner.chars().any(char::is_control)
    {
        bail!("MINCO-HANDOVER-005: owner must contain 1-200 trimmed visible characters");
    }
    reject_secret_text("MINCO-HANDOVER-007: owner", owner, &[])
}

fn validate_delivery_binding(
    release: &ReleaseManifest,
    deployment: &DeploymentReceipt,
    release_file: &FileDigest,
) -> Result<()> {
    if deployment.outcome() != DeploymentOutcome::Succeeded {
        bail!("MINCO-HANDOVER-001: deployment receipt is not successful");
    }
    if deployment.release_id != release.release_id
        || deployment.release_digest != release.release_digest
        || deployment.environment != release.environment
        || deployment.release_manifest != *release_file
    {
        bail!("MINCO-HANDOVER-001: release and deployment bindings disagree");
    }
    Ok(())
}

fn bind_verified_delivery_inputs(
    root: &Path,
    release: &ReleaseManifest,
    deployment: &DeploymentReceipt,
    inputs: &mut Vec<ExactInput>,
) -> Result<()> {
    for file in release
        .artifacts
        .iter()
        .map(|artifact| &artifact.file)
        .chain(std::iter::once(&release.contract))
        .chain(std::iter::once(&release.deployment_plan))
        .chain(std::iter::once(&release.deployment_template))
        .chain(release.cargo_lock.iter())
        .chain(release.attestations.iter())
        .chain(deployment.database_plans.iter().map(|plan| &plan.file))
        .chain(deployment.attestations.iter())
        .chain(
            deployment
                .verification()
                .iter()
                .map(|evidence| &evidence.file),
        )
    {
        let input = read_exact_input(root, Path::new(&file.path))
            .context("MINCO-HANDOVER-001: bind verified delivery artifact")?;
        if input.file != *file {
            bail!("MINCO-HANDOVER-001: verified delivery artifact changed while planning");
        }
        inputs.push(input);
    }
    Ok(())
}

fn confined_output(path: &Path) -> Result<PathBuf> {
    validate_relative(path).context("MINCO-HANDOVER-005: invalid output path")?;
    let is_default = path == Path::new(DEFAULT_JSON) || path == Path::new(DEFAULT_MARKDOWN);
    let is_custom = path.starts_with("verification/handover") && path.components().count() >= 3;
    if !is_default && !is_custom {
        bail!(
            "MINCO-HANDOVER-005: outputs must use the defaults or stay under verification/handover/"
        );
    }
    Ok(path.to_path_buf())
}

fn confined_feedback_directory(path: &Path) -> Result<PathBuf> {
    validate_relative(path).context("MINCO-HANDOVER-004: invalid feedback receipt directory")?;
    if !path.starts_with("verification") {
        bail!("MINCO-HANDOVER-004: feedback receipts must stay under verification/");
    }
    Ok(path.to_path_buf())
}

fn confined_input(root: &Path, requested: &Path) -> Result<PathBuf> {
    let relative = if requested.is_absolute() {
        requested
            .strip_prefix(root)
            .context("MINCO-HANDOVER-001: absolute input escapes the project root")?
            .to_path_buf()
    } else {
        requested.to_path_buf()
    };
    validate_relative(&relative).context("MINCO-HANDOVER-001: invalid input path")?;
    Ok(relative)
}

fn paths_overlap(left: &Path, right: &Path) -> bool {
    left == right || left.starts_with(right) || right.starts_with(left)
}

fn reject_symlink_components(root: &Path, candidate: &Path) -> Result<()> {
    let mut current = Some(candidate);
    while let Some(path) = current {
        match fs::symlink_metadata(path) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                bail!(
                    "MINCO-HANDOVER-001: input path refuses symbolic links: {}",
                    path.display()
                );
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        if path == root {
            return Ok(());
        }
        current = path.parent();
    }
    bail!("MINCO-HANDOVER-001: input path escapes the project root")
}

fn bind_file(
    root: &Path,
    relative: &Path,
    inputs: &mut Vec<ExactInput>,
) -> Result<(EvidenceFile, Vec<u8>)> {
    let input = read_exact_input(root, relative)
        .with_context(|| format!("MINCO-HANDOVER-003: read {}", relative.display()))?;
    let binding = EvidenceFile::from(input.file.clone());
    let bytes = input.bytes.clone();
    inputs.push(input);
    Ok((binding, bytes))
}

fn validate_operational_receipt(
    root: &Path,
    receipt: &OperationalValidationReceipt,
    source_digest: &str,
    evidence: [&EvidenceFile; 4],
    exact_inputs: &mut Vec<ExactInput>,
) -> Result<()> {
    validate_operational_receipt_seal(receipt)?;
    if receipt.schema_version != 1
        || receipt.kind != "minco.operational-evidence-validation.v1"
        || receipt.status != "PASS"
        || receipt.source_tree_sha256 != source_digest
        || receipt.counts.errors != 0
        || receipt
            .effective_date
            .as_deref()
            .is_none_or(|value| NaiveDate::parse_from_str(value, "%Y-%m-%d").is_err())
        || !matches!(
            receipt.effective_date_source.as_str(),
            "repository_truth" | "cli"
        )
    {
        bail!("MINCO-HANDOVER-003: operational validation receipt is stale or contradictory");
    }
    if receipt.findings.iter().any(|finding| {
        finding.severity == "error"
            || !matches!(finding.severity.as_str(), "warning")
            || finding.code.is_empty()
            || finding.message.is_empty()
            || finding.path.as_deref().is_some_and(str::is_empty)
    }) || receipt.counts.warnings
        != receipt
            .findings
            .iter()
            .filter(|finding| finding.severity == "warning")
            .count()
    {
        bail!("MINCO-HANDOVER-003: operational validation findings contradict PASS");
    }

    let required = [
        ("verification/source-manifest.json", None),
        (POLICY, Some(evidence[0])),
        (BASELINE, Some(evidence[1])),
        (PROVIDER, Some(evidence[2])),
        (CAPABILITIES, Some(evidence[3])),
        (REPOSITORY_TRUTH, None),
    ];
    for (path, expected) in required {
        let digest = receipt.inputs.get(path).with_context(|| {
            format!("MINCO-HANDOVER-003: validation receipt does not bind {path}")
        })?;
        if expected.is_some_and(|file| file.sha256 != *digest) {
            bail!("MINCO-HANDOVER-003: validation receipt evidence digest mismatch");
        }
    }
    for (path, expected_digest) in &receipt.inputs {
        if !is_sha256(expected_digest) {
            bail!("MINCO-HANDOVER-003: validation receipt input digest is malformed");
        }
        let input = read_exact_input(root, Path::new(path))
            .with_context(|| format!("MINCO-HANDOVER-003: verify validated input {path}"))?;
        if input.file.sha256 != *expected_digest {
            bail!("MINCO-HANDOVER-003: validated operational input changed");
        }
        exact_inputs.push(input);
    }
    Ok(())
}

fn validate_operational_receipt_seal(receipt: &OperationalValidationReceipt) -> Result<()> {
    let mut sealed_payload = serde_json::to_value(receipt)?;
    sealed_payload
        .as_object_mut()
        .context("MINCO-HANDOVER-003: validation receipt must be one JSON object")?
        .remove("receipt_digest");
    if !is_sha256(&receipt.receipt_digest)
        || sha256(&serde_json::to_vec(&sealed_payload)?) != receipt.receipt_digest
    {
        bail!("MINCO-HANDOVER-003: operational validation receipt seal is invalid");
    }
    Ok(())
}

fn validate_policy(
    value: &toml::Value,
    release: &ReleaseManifest,
    _source_digest: &str,
) -> Result<()> {
    let workspace_version = env!("CARGO_PKG_VERSION");
    if value.get("schema").and_then(toml::Value::as_integer) != Some(1)
        || value.get("kind").and_then(toml::Value::as_str) != Some("minco.performance-policy.v1")
        || value.get("candidate_version").and_then(toml::Value::as_str) != Some(workspace_version)
        || value
            .get("candidate_release_state")
            .and_then(toml::Value::as_str)
            != Some("candidate")
        || value.get("production_slo").and_then(toml::Value::as_bool) != Some(false)
        || value.get("provider_contact").and_then(toml::Value::as_bool) != Some(false)
    {
        bail!("MINCO-HANDOVER-003: performance policy contradicts candidate truth");
    }
    if release.source_change.is_empty() {
        bail!("MINCO-HANDOVER-003: release source change is missing");
    }
    Ok(())
}

fn validate_baseline(
    value: &PerformanceBaselineSummary,
    _release: &ReleaseManifest,
    source_digest: &str,
) -> Result<()> {
    if value.schema_version != 1
        || value.kind != "minco.performance-baseline.v1"
        || value.candidate_version != env!("CARGO_PKG_VERSION")
        || value.source_tree_sha256 != source_digest
        || value.production_slo
        || value.provider_contact
        || !matches!(value.status.as_str(), "PASS" | "NOT RUN")
    {
        bail!("MINCO-HANDOVER-003: performance baseline is stale or contradictory");
    }
    if value.status == "NOT RUN"
        && (value.reason.as_deref().is_none_or(str::is_empty) || value.limitations.is_empty())
    {
        bail!("MINCO-HANDOVER-003: NOT RUN baseline requires reason and limitations");
    }
    if value.status == "NOT RUN" && !value.other.is_empty() {
        bail!("MINCO-HANDOVER-003: NOT RUN baseline cannot contain measurement claims");
    }
    if value.status == "PASS" {
        bail!(
            "MINCO-HANDOVER-003: positive performance evidence is unsupported until the compiled handover verifier covers its complete provenance and measurement policy"
        );
    }
    Ok(())
}

fn validate_provider_ledger(
    value: &toml::Value,
    _source_digest: &str,
) -> Result<BTreeMap<String, usize>> {
    if value.get("schema").and_then(toml::Value::as_integer) != Some(1)
        || value.get("kind").and_then(toml::Value::as_str) != Some("minco.provider-evidence.v1")
    {
        bail!("MINCO-HANDOVER-003: provider evidence schema is invalid");
    }
    let profiles = value
        .get("profile")
        .and_then(toml::Value::as_array)
        .context("MINCO-HANDOVER-003: provider evidence profiles are missing")?;
    let mut states = BTreeMap::new();
    for profile in profiles {
        let state = profile
            .get("evidence_state")
            .and_then(toml::Value::as_str)
            .context("MINCO-HANDOVER-003: provider evidence state is missing")?;
        if !matches!(state, "current" | "stale" | "not_run") {
            bail!("MINCO-HANDOVER-003: provider evidence state is invalid");
        }
        if state == "current" {
            bail!(
                "MINCO-HANDOVER-003: current provider evidence is unsupported until the compiled handover verifier covers the sealed receipt, freshness, artifacts and cleanup proof"
            );
        }
        *states.entry(state.to_owned()).or_insert(0) += 1;
    }
    Ok(states)
}

fn validate_capability_ledger(value: &toml::Value) -> Result<BTreeMap<String, usize>> {
    if value.get("schema").and_then(toml::Value::as_integer) != Some(1)
        || value.get("kind").and_then(toml::Value::as_str)
            != Some("minco.aws-capability-candidates.v1")
    {
        bail!("MINCO-HANDOVER-003: capability ledger schema is invalid");
    }
    let candidates = value
        .get("candidate")
        .and_then(toml::Value::as_array)
        .context("MINCO-HANDOVER-003: capability candidates are missing")?;
    let mut states = BTreeMap::new();
    for candidate in candidates {
        let state = candidate
            .get("support_state")
            .and_then(toml::Value::as_str)
            .context("MINCO-HANDOVER-003: capability state is missing")?;
        if !matches!(
            state,
            "supported" | "declared" | "research" | "deferred" | "rejected"
        ) {
            bail!("MINCO-HANDOVER-003: capability state is invalid");
        }
        if state == "supported" {
            bail!(
                "MINCO-HANDOVER-003: supported capability claims are unavailable without compiled live-provider and implementation-evidence verification"
            );
        }
        *states.entry(state.to_owned()).or_insert(0) += 1;
    }
    Ok(states)
}

fn load_feedback_receipts(
    root: &Path,
    directory: &Path,
    release: &ReleaseManifest,
    deployment: &DeploymentReceipt,
    exact_inputs: &mut Vec<ExactInput>,
) -> Result<Vec<FeedbackReceiptIdentity>> {
    let absolute = root.join(directory);
    if !absolute.exists() {
        return Ok(Vec::new());
    }
    reject_symlink_components(root, &absolute)?;
    if !absolute.is_dir() {
        bail!("MINCO-HANDOVER-004: feedback receipt path is not a directory");
    }
    let mut entries = fs::read_dir(&absolute)?.collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by_key(fs::DirEntry::file_name);
    let mut matching = Vec::new();
    for entry in entries {
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            bail!("MINCO-HANDOVER-004: feedback receipt entries must be real regular files");
        }
        if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
            continue;
        }
        let relative = directory.join(entry.file_name());
        let input = read_exact_input(root, &relative)
            .context("MINCO-HANDOVER-004: securely read feedback task receipt")?;
        let verified = verify_feedback_task_receipt(root, &input.bytes, &[])
            .context("MINCO-HANDOVER-004: verify feedback task receipt")?;
        if receipt_matches(&verified, release, deployment) {
            matching.push(FeedbackReceiptIdentity {
                feedback_id: verified.feedback_id,
                task_id: verified.task_id,
                receipt_digest: verified.receipt_digest,
                file: input.file.clone().into(),
            });
        }
        exact_inputs.push(input);
    }
    matching.sort_by(|left, right| {
        left.feedback_id
            .cmp(&right.feedback_id)
            .then_with(|| left.task_id.cmp(&right.task_id))
            .then_with(|| left.receipt_digest.cmp(&right.receipt_digest))
    });
    Ok(matching)
}

fn receipt_matches(
    receipt: &VerifiedFeedbackTaskReceipt,
    release: &ReleaseManifest,
    deployment: &DeploymentReceipt,
) -> bool {
    let release_matches = receipt.release_id == release.release_id
        && receipt.release_digest == release.release_digest;
    let deployment_matches = receipt.deployment_attempt_id == deployment.attempt_id
        && receipt.deployment_receipt_digest == deployment.receipt_digest;
    release_matches && deployment_matches
}

fn performance_claim(baseline: &PerformanceBaselineSummary) -> TruthClaim {
    if baseline.status == "PASS" {
        TruthClaim {
            id: "performance_candidate".into(),
            status: TruthStatus::CurrentBounded,
            detail: "Exact-source hosted candidate performance evidence passed; it remains explicitly non-production.".into(),
        }
    } else {
        TruthClaim {
            id: "performance_candidate".into(),
            status: TruthStatus::NotRun,
            detail: baseline
                .reason
                .clone()
                .unwrap_or_else(|| "Performance evidence is NOT RUN.".into()),
        }
    }
}

fn provider_claim(states: &BTreeMap<String, usize>, validated_current: usize) -> TruthClaim {
    if validated_current > 0 && states.get("current").copied() == Some(validated_current) {
        TruthClaim {
            id: "live_provider_evidence".into(),
            status: TruthStatus::CurrentBounded,
            detail: "At least one exact-source provider profile records current bounded evidence."
                .into(),
        }
    } else {
        TruthClaim {
            id: "live_provider_evidence".into(),
            status: TruthStatus::MissingLiveProviderEvidence,
            detail: format!(
                "No current exact-source provider proof; stale={}, not_run={}.",
                states.get("stale").copied().unwrap_or(0),
                states.get("not_run").copied().unwrap_or(0)
            ),
        }
    }
}

fn capability_claim(states: &BTreeMap<String, usize>) -> TruthClaim {
    let supported = states.get("supported").copied().unwrap_or(0);
    TruthClaim {
        id: "aws_capability_candidates".into(),
        status: if supported == 0 {
            TruthStatus::Deferred
        } else {
            TruthStatus::CurrentBounded
        },
        detail: format!(
            "supported={supported}, declared={}, research={}, deferred={}, rejected={}",
            states.get("declared").copied().unwrap_or(0),
            states.get("research").copied().unwrap_or(0),
            states.get("deferred").copied().unwrap_or(0),
            states.get("rejected").copied().unwrap_or(0),
        ),
    }
}

fn render_markdown(packet: &HandoverPacket) -> Result<String> {
    let mut output = format!(
        "# Minco client handover\n\nPlan digest: `{}`\n\nOwner: {}\n\n",
        packet.plan_digest, packet.payload.owner
    );
    output.push_str("## Exact delivery binding\n\n");
    writeln!(
        output,
        "- Release `{}` digest `{}` from source revision `{}`\n- Deployment `{}` receipt `{}` (`succeeded`)\n- Application `{}` environment `{}` Region `{}`\n- Current post-feedback repository source tree `{}`\n- ProjectView `{}`",
        packet.payload.release.release_id,
        packet.payload.release.release_digest,
        packet.payload.release.source_change,
        packet.payload.deployment.attempt_id,
        packet.payload.deployment.receipt_digest,
        packet.payload.release.application,
        packet.payload.release.environment,
        packet.payload.release.region,
        packet.payload.source.tree_sha256,
        packet.payload.project_view.digest,
    )?;
    output.push_str("\n## Evidence truth\n\n| Evidence | Status | Detail |\n|---|---|---|\n");
    for claim in &packet.payload.truth {
        writeln!(
            output,
            "| `{}` | `{:?}` | {} |",
            claim.id,
            claim.status,
            markdown_cell(&claim.detail)
        )?;
    }
    output.push_str("\n## Release-bound feedback tasks\n\n");
    if packet.payload.feedback_receipts.is_empty() {
        output.push_str("- No matching feedback-task receipts.\n");
    } else {
        for receipt in &packet.payload.feedback_receipts {
            writeln!(
                output,
                "- Feedback `{}` -> task `{}`; receipt `{}`",
                receipt.feedback_id, receipt.task_id, receipt.receipt_digest
            )?;
        }
    }
    output.push_str("\n## Limitations and authority\n\n");
    for limitation in &packet.payload.limitations {
        writeln!(output, "- {limitation}")?;
    }
    Ok(output)
}

fn markdown_cell(value: &str) -> String {
    value.replace('|', "\\|").replace(['\r', '\n'], " ")
}

fn path_string(path: &Path) -> Result<String> {
    path.to_str()
        .map(|value| value.replace('\\', "/"))
        .context("handover paths must be UTF-8")
}

fn print<T: Serialize + ?Sized>(value: &T, _as_json: bool) -> Result<()> {
    let serialized = serde_json::to_value(value)?;
    println!("{}", serde_json::to_string_pretty(&serialized)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use minco_release::{
        DatabaseSourceDigests, DeploymentReceiptInput, FunctionArtifact, ReleaseEnvironment,
        ReleaseManifestInput, ToolchainIdentity, VerificationEvidence,
    };

    fn sample_payload() -> HandoverPlanPayload {
        let file = EvidenceFile {
            path: "verification/evidence.json".into(),
            sha256: "1".repeat(64),
            bytes: 10,
        };
        HandoverPlanPayload {
            schema_version: 1,
            owner: "Application owner".into(),
            json_output: DEFAULT_JSON.into(),
            markdown_output: DEFAULT_MARKDOWN.into(),
            release: ReleaseIdentity {
                release_id: "minco.release".into(),
                release_digest: "2".repeat(64),
                source_change: "a".repeat(40),
                manifest: file.clone(),
                application: "orders".into(),
                environment: "review".into(),
                region: "ap-southeast-2".into(),
            },
            deployment: DeploymentIdentity {
                attempt_id: "attempt-1".into(),
                receipt_digest: "3".repeat(64),
                receipt: file.clone(),
                outcome: "succeeded".into(),
            },
            project_view: ProjectViewIdentity {
                schema_version: 1,
                digest: "4".repeat(64),
                source_digest: "5".repeat(64),
            },
            source: SourceIdentity {
                tree_sha256: "6".repeat(64),
                manifest: file.clone(),
            },
            performance_policy: file.clone(),
            performance_baseline: file.clone(),
            provider_evidence: file.clone(),
            capability_ledger: file.clone(),
            operational_validation: file.clone(),
            repository_truth: vec![file],
            feedback_receipts: Vec::new(),
            truth: vec![TruthClaim {
                id: "performance_candidate".into(),
                status: TruthStatus::NotRun,
                detail: "No hosted evidence.".into(),
            }],
            limitations: vec!["No production acceptance claim.".into()],
        }
    }

    fn planned(root: &Path) -> PlannedHandover {
        let payload = sample_payload();
        let plan_digest = sha256(&serde_json::to_vec(&payload).unwrap());
        let packet = HandoverPacket {
            plan_digest,
            payload,
        };
        let mut json_bytes = serde_json::to_vec_pretty(&packet).unwrap();
        json_bytes.push(b'\n');
        let markdown_bytes = render_markdown(&packet).unwrap().into_bytes();
        PlannedHandover {
            root: root.to_path_buf(),
            output: HandoverPlanOutput {
                approval_required: true,
                applied: false,
                idempotent: false,
                packet,
            },
            json_relative: PathBuf::from(DEFAULT_JSON),
            json_bytes,
            markdown_relative: PathBuf::from(DEFAULT_MARKDOWN),
            markdown_bytes,
            inputs: Vec::new(),
            source_authority: None,
        }
    }

    #[test]
    fn plan_digest_and_rendered_bytes_are_deterministic() {
        let first = planned(Path::new("/tmp/unused"));
        let second = planned(Path::new("/tmp/unused"));
        assert_eq!(
            first.output.packet.plan_digest,
            second.output.packet.plan_digest
        );
        assert_eq!(first.json_bytes, second.json_bytes);
        assert_eq!(first.markdown_bytes, second.markdown_bytes);
    }

    #[test]
    fn plan_only_and_bad_digest_write_nothing() {
        let temporary = tempfile::tempdir().unwrap();
        let planned = planned(temporary.path());
        assert!(!temporary.path().join(DEFAULT_JSON).exists());
        assert!(apply_handover(planned, &"0".repeat(64)).is_err());
        assert!(!temporary.path().join(DEFAULT_JSON).exists());
        assert!(!temporary.path().join(DEFAULT_MARKDOWN).exists());
    }

    #[test]
    fn exact_apply_creates_both_outputs_and_is_idempotent() {
        let temporary = tempfile::tempdir().unwrap();
        let first = planned(temporary.path());
        let digest = first.output.packet.plan_digest.clone();
        let result = apply_handover(first, &digest).unwrap();
        assert!(result.applied);
        assert!(!result.idempotent);
        assert!(temporary.path().join(DEFAULT_JSON).is_file());
        assert!(temporary.path().join(DEFAULT_MARKDOWN).is_file());

        let second = planned(temporary.path());
        let digest = second.output.packet.plan_digest.clone();
        assert!(apply_handover(second, &digest).unwrap().idempotent);
    }

    #[test]
    fn conflicting_or_symlinked_outputs_fail_closed() {
        let temporary = tempfile::tempdir().unwrap();
        fs::create_dir_all(temporary.path().join("verification")).unwrap();
        fs::write(temporary.path().join(DEFAULT_JSON), b"conflict\n").unwrap();
        let first = planned(temporary.path());
        let digest = first.output.packet.plan_digest.clone();
        assert!(apply_handover(first, &digest).is_err());
        assert!(!temporary.path().join(DEFAULT_MARKDOWN).exists());

        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            fs::remove_file(temporary.path().join(DEFAULT_JSON)).unwrap();
            fs::write(temporary.path().join("target.json"), b"target").unwrap();
            symlink(
                temporary.path().join("target.json"),
                temporary.path().join(DEFAULT_JSON),
            )
            .unwrap();
            let second = planned(temporary.path());
            let digest = second.output.packet.plan_digest.clone();
            assert!(apply_handover(second, &digest).is_err());
        }
    }

    #[test]
    fn paths_and_secret_like_owners_fail_closed() {
        assert!(confined_output(Path::new("verification/handover/custom.json")).is_ok());
        assert!(confined_output(Path::new("verification/custom.json")).is_err());
        assert!(confined_output(Path::new("../escape.json")).is_err());
        assert!(confined_output(Path::new("target/handover.json")).is_err());
        assert!(paths_overlap(
            Path::new("verification/receipts/handover.json"),
            Path::new("verification/receipts")
        ));
        assert!(validate_owner("postgres://user:password@example.test/db").is_err());
        assert!(validate_owner("Application owner").is_ok());
    }

    #[test]
    fn changed_exact_input_blocks_apply_without_writing() {
        let temporary = tempfile::tempdir().unwrap();
        fs::create_dir_all(temporary.path().join("verification")).unwrap();
        let input_path = Path::new("verification/input.json");
        fs::write(temporary.path().join(input_path), b"first\n").unwrap();
        let mut handover = planned(temporary.path());
        handover.inputs = vec![read_exact_input(temporary.path(), input_path).unwrap()];
        fs::write(temporary.path().join(input_path), b"second\n").unwrap();
        let digest = handover.output.packet.plan_digest.clone();
        assert!(apply_handover(handover, &digest).is_err());
        assert!(!temporary.path().join(DEFAULT_JSON).exists());
        assert!(!temporary.path().join(DEFAULT_MARKDOWN).exists());
    }

    #[test]
    fn stale_provider_and_not_run_performance_render_truthfully() {
        let provider = provider_claim(
            &BTreeMap::from([("stale".into(), 1), ("not_run".into(), 1)]),
            0,
        );
        assert_eq!(provider.status, TruthStatus::MissingLiveProviderEvidence);
        let baseline = PerformanceBaselineSummary {
            schema_version: 1,
            kind: "minco.performance-baseline.v1".into(),
            status: "NOT RUN".into(),
            candidate_version: env!("CARGO_PKG_VERSION").into(),
            source_tree_sha256: "1".repeat(64),
            production_slo: false,
            provider_contact: false,
            reason: Some("not run".into()),
            limitations: vec!["local is not hosted".into()],
            other: BTreeMap::new(),
        };
        assert_eq!(performance_claim(&baseline).status, TruthStatus::NotRun);
    }

    #[test]
    fn operational_validation_receipt_seal_fails_closed() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let bytes = fs::read(root.join(OPERATIONAL_VALIDATION)).unwrap();
        let mut receipt: OperationalValidationReceipt = serde_json::from_slice(&bytes).unwrap();
        assert!(validate_operational_receipt_seal(&receipt).is_ok());
        receipt.receipt_digest = "0".repeat(64);
        assert!(validate_operational_receipt_seal(&receipt).is_err());
    }

    #[test]
    fn positive_operational_claims_fail_closed_in_the_compiled_verifier() {
        let baseline = PerformanceBaselineSummary {
            schema_version: 1,
            kind: "minco.performance-baseline.v1".into(),
            status: "PASS".into(),
            candidate_version: env!("CARGO_PKG_VERSION").into(),
            source_tree_sha256: "a".repeat(64),
            production_slo: false,
            provider_contact: false,
            reason: None,
            limitations: vec!["bounded".into()],
            other: BTreeMap::new(),
        };
        let release = release_and_deployment(&tempfile::tempdir().unwrap().path().join("release"));
        assert!(validate_baseline(&baseline, &release.0, &"a".repeat(64)).is_err());

        let provider: toml::Value = toml::from_str(
            "schema = 1\nkind = \"minco.provider-evidence.v1\"\n[[profile]]\nevidence_state = \"current\"\n",
        )
        .unwrap();
        assert!(validate_provider_ledger(&provider, &"a".repeat(64)).is_err());

        let capabilities: toml::Value = toml::from_str(
            "schema = 1\nkind = \"minco.aws-capability-candidates.v1\"\n[[candidate]]\nsupport_state = \"supported\"\n",
        )
        .unwrap();
        assert!(validate_capability_ledger(&capabilities).is_err());
    }

    fn release_and_deployment(root: &Path) -> (ReleaseManifest, DeploymentReceipt, FileDigest) {
        fs::create_dir_all(root.join("verification")).unwrap();
        for (path, bytes) in [
            ("artifact.zip", b"artifact".as_slice()),
            ("contract.json", b"contract".as_slice()),
            ("plan.json", b"{}".as_slice()),
            ("template.yaml", b"template".as_slice()),
            ("verification/hosted.json", b"hosted".as_slice()),
        ] {
            fs::write(root.join(path), bytes).unwrap();
        }
        let release = ReleaseManifest::seal(ReleaseManifestInput {
            source_change: "d".repeat(40),
            environment: ReleaseEnvironment {
                application: "orders".into(),
                environment: "review".into(),
                region: "ap-southeast-2".into(),
            },
            toolchain: ToolchainIdentity {
                rustc: "rustc-test".into(),
                cargo_minco: env!("CARGO_PKG_VERSION").into(),
                artifact_builder: None,
            },
            artifacts: vec![FunctionArtifact {
                function_id: "api".into(),
                file: FileDigest::from_rooted_path(root, root.join("artifact.zip")).unwrap(),
            }],
            contract: FileDigest::from_rooted_path(root, root.join("contract.json")).unwrap(),
            configuration_digest: "a".repeat(64),
            database_sources: DatabaseSourceDigests {
                migration_catalog: "b".repeat(64),
                seed_catalog: "c".repeat(64),
            },
            cargo_lock: None,
            deployment_plan: FileDigest::from_rooted_path(root, root.join("plan.json")).unwrap(),
            deployment_template: FileDigest::from_rooted_path(root, root.join("template.yaml"))
                .unwrap(),
            attestations: Vec::new(),
        })
        .unwrap();
        let release_path = root.join("verification/release.json");
        release.write_json(&release_path).unwrap();
        let release_file = FileDigest::from_rooted_path(root, &release_path).unwrap();
        let deployment = DeploymentReceipt::start(DeploymentReceiptInput {
            attempt_id: "attempt-1".into(),
            release_manifest: release_file.clone(),
            release_id: release.release_id.clone(),
            release_digest: release.release_digest.clone(),
            environment: release.environment.clone(),
            configuration_digest: release.configuration_digest.clone(),
            database_plans: Vec::new(),
            attestations: Vec::new(),
        })
        .unwrap();
        (release, deployment, release_file)
    }

    #[test]
    fn unsuccessful_or_mismatched_deployment_fails_closed() {
        let temporary = tempfile::tempdir().unwrap();
        let (release, mut deployment, mut release_file) = release_and_deployment(temporary.path());
        assert!(validate_delivery_binding(&release, &deployment, &release_file).is_err());

        deployment
            .succeed(vec![VerificationEvidence {
                kind: "hosted".into(),
                file: FileDigest::from_rooted_path(
                    temporary.path(),
                    temporary.path().join("verification/hosted.json"),
                )
                .unwrap(),
            }])
            .unwrap();
        assert!(validate_delivery_binding(&release, &deployment, &release_file).is_ok());

        release_file.sha256 = "f".repeat(64);
        assert!(validate_delivery_binding(&release, &deployment, &release_file).is_err());
    }

    #[test]
    fn actual_handover_plan_is_read_only_and_deterministic() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .unwrap();
        let scratch_root = root.join("target/minco");
        fs::create_dir_all(&scratch_root).unwrap();
        let temporary = tempfile::tempdir_in(&scratch_root).unwrap();
        for (name, bytes) in [
            ("artifact.zip", b"artifact".as_slice()),
            ("contract.json", b"contract".as_slice()),
            ("template.yaml", b"template".as_slice()),
            ("hosted.json", b"hosted".as_slice()),
        ] {
            fs::write(temporary.path().join(name), bytes).unwrap();
        }
        let release = ReleaseManifest::seal(ReleaseManifestInput {
            source_change: "d".repeat(40),
            environment: ReleaseEnvironment {
                application: "orders".into(),
                environment: "review".into(),
                region: "ap-southeast-2".into(),
            },
            toolchain: ToolchainIdentity {
                rustc: "rustc-test".into(),
                cargo_minco: env!("CARGO_PKG_VERSION").into(),
                artifact_builder: None,
            },
            artifacts: vec![FunctionArtifact {
                function_id: "api".into(),
                file: FileDigest::from_rooted_path(&root, temporary.path().join("artifact.zip"))
                    .unwrap(),
            }],
            contract: FileDigest::from_rooted_path(&root, temporary.path().join("contract.json"))
                .unwrap(),
            configuration_digest: "a".repeat(64),
            database_sources: DatabaseSourceDigests {
                migration_catalog: "b".repeat(64),
                seed_catalog: "c".repeat(64),
            },
            cargo_lock: None,
            deployment_plan: FileDigest::from_rooted_path(
                &root,
                root.join("infra/aws/generated/plan.json"),
            )
            .unwrap(),
            deployment_template: FileDigest::from_rooted_path(
                &root,
                temporary.path().join("template.yaml"),
            )
            .unwrap(),
            attestations: Vec::new(),
        })
        .unwrap();
        let release_path = temporary.path().join("release.json");
        release.write_json(&release_path).unwrap();
        let release_file = FileDigest::from_rooted_path(&root, &release_path).unwrap();
        let mut deployment = DeploymentReceipt::start(DeploymentReceiptInput {
            attempt_id: "attempt-actual-plan".into(),
            release_manifest: release_file,
            release_id: release.release_id.clone(),
            release_digest: release.release_digest.clone(),
            environment: release.environment.clone(),
            configuration_digest: release.configuration_digest,
            database_plans: Vec::new(),
            attestations: Vec::new(),
        })
        .unwrap();
        deployment
            .succeed(vec![VerificationEvidence {
                kind: "hosted".into(),
                file: FileDigest::from_rooted_path(&root, temporary.path().join("hosted.json"))
                    .unwrap(),
            }])
            .unwrap();
        let deployment_path = temporary.path().join("deployment.json");
        deployment.write_json(&deployment_path).unwrap();
        let suffix = temporary.path().file_name().unwrap().to_string_lossy();
        let args = HandoverArgs {
            release_manifest: release_path.strip_prefix(&root).unwrap().to_path_buf(),
            deployment_receipt: deployment_path.strip_prefix(&root).unwrap().to_path_buf(),
            owner: "Application owner".into(),
            json_output: PathBuf::from(format!("verification/handover/{suffix}.json")),
            markdown_output: PathBuf::from(format!("verification/handover/{suffix}.md")),
            feedback_receipts: PathBuf::from(DEFAULT_FEEDBACK),
            approve_plan_digest: None,
        };
        let first = plan_handover(&root, &args).unwrap();
        let second = plan_handover(&root, &args).unwrap();
        assert_eq!(
            first.output.packet.plan_digest,
            second.output.packet.plan_digest
        );
        assert_eq!(first.json_bytes, second.json_bytes);
        assert_eq!(first.markdown_bytes, second.markdown_bytes);
        assert!(!root.join(&args.json_output).exists());
        assert!(!root.join(&args.markdown_output).exists());

        let digest = first.output.packet.plan_digest.clone();
        let applied = apply_handover(first, &digest).unwrap();
        assert!(applied.applied);
        assert!(!applied.idempotent);
        assert!(root.join(&args.json_output).is_file());
        assert!(root.join(&args.markdown_output).is_file());

        let idempotent = plan_handover(&root, &args).unwrap();
        assert_eq!(idempotent.output.packet.plan_digest, digest);
        assert!(idempotent.output.idempotent);
        assert!(apply_handover(idempotent, &digest).unwrap().idempotent);

        fs::remove_file(root.join(&args.json_output)).unwrap();
        fs::remove_file(root.join(&args.markdown_output)).unwrap();
    }
}
