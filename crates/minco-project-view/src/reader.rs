use crate::model::{
    ConfigurationFieldView, ConfigurationValue, CostProjection, DeploymentProjection,
    DerivedSummary, DiagnosticSeverity, EdgeKind, EvidenceFreshness, EvidenceItem, EvidenceLane,
    FeedbackContext, InputUsage, NodeKind, PROJECT_VIEW_SCHEMA_VERSION, ProjectDiagnostic,
    ProjectEdge, ProjectIdentity, ProjectNode, ProjectView, ProjectViewError, SemanticStatus,
    SourceKind, SourceProvenance, StatusMapping, TaskReadiness, ViewLimits,
};
use minco_contract::load_contract_source;
use minco_plan::{DeploymentConfig, estimate_deployment_database_cost, estimate_runtime_cost};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write as _,
    fs,
    path::{Component, Path, PathBuf},
};

#[derive(Debug, Deserialize)]
struct Manifest {
    schema: u32,
    name: String,
    contract: PathBuf,
    generated: PathBuf,
    deployment_config: PathBuf,
    roadmap: PathBuf,
    tasks: PathBuf,
    plugin_catalog: PathBuf,
    quality: PathBuf,
    #[serde(default)]
    configuration: ConfigurationManifest,
    #[serde(default)]
    architecture: ArchitectureManifest,
    #[serde(default)]
    operations: BTreeMap<String, OperationTrace>,
    #[serde(default)]
    migrations: RootManifest,
    #[serde(default)]
    seeds: RootManifest,
    #[serde(default)]
    plugins: PluginSelection,
}

#[derive(Debug, Default, Deserialize)]
struct ConfigurationManifest {
    #[serde(default)]
    fields: Vec<ConfigurationField>,
}

#[derive(Debug, Deserialize)]
struct ConfigurationField {
    key: String,
    kind: String,
    required: bool,
    secret: bool,
    description: String,
    default: Option<toml::Value>,
}

#[derive(Debug, Default, Deserialize)]
#[allow(clippy::struct_field_names)]
struct ArchitectureManifest {
    #[serde(default)]
    domain_roots: Vec<PathBuf>,
    #[serde(default)]
    application_roots: Vec<PathBuf>,
    #[serde(default)]
    api_roots: Vec<PathBuf>,
}

#[derive(Debug, Default, Deserialize)]
struct OperationTrace {
    contract: Option<PathBuf>,
    generated: Option<PathBuf>,
    handler: Option<String>,
    application: Option<String>,
    #[serde(default)]
    adapters: Vec<String>,
    #[serde(default)]
    tests: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
struct RootManifest {
    #[serde(default)]
    roots: Vec<PathBuf>,
}

#[derive(Debug, Default, Deserialize)]
struct PluginSelection {
    #[serde(default)]
    enabled: BTreeSet<String>,
    #[serde(default)]
    disabled: BTreeSet<String>,
}

#[derive(Debug, Deserialize)]
struct Roadmap {
    schema: u32,
    product: String,
    milestones: Vec<Milestone>,
}

#[derive(Debug, Deserialize)]
struct Milestone {
    id: String,
    name: String,
    status: String,
    #[serde(default)]
    depends_on: Vec<String>,
    outcome: String,
}

#[derive(Debug, Deserialize)]
struct Task {
    id: String,
    title: String,
    milestone: String,
    status: String,
    priority: String,
    area: String,
    #[serde(default)]
    depends_on: Vec<String>,
    #[serde(default)]
    operations: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct PluginCatalog {
    schema: u32,
    #[serde(default)]
    plugin: Vec<PluginEntry>,
}

#[derive(Debug, Deserialize)]
struct PluginEntry {
    id: String,
    #[serde(rename = "crate")]
    crate_name: String,
    #[serde(default)]
    path: Option<PathBuf>,
    kind: String,
    feature: String,
    default_enabled: bool,
    stability: String,
    description: String,
}

struct BoundedReader {
    root: PathBuf,
    limits: ViewLimits,
    cache: BTreeMap<PathBuf, Vec<u8>>,
    kinds: BTreeMap<PathBuf, SourceKind>,
    total_bytes: usize,
    scanned_entries: usize,
}

impl BoundedReader {
    fn new(root: &Path, limits: ViewLimits) -> Result<Self, ProjectViewError> {
        if !root.is_absolute()
            || root.canonicalize().ok().as_deref() != Some(root)
            || !root.is_dir()
        {
            return Err(ProjectViewError::NonCanonicalRoot(root.to_path_buf()));
        }
        let metadata = fs::symlink_metadata(root).map_err(|source| ProjectViewError::Io {
            path: root.to_path_buf(),
            source,
        })?;
        if metadata.file_type().is_symlink() {
            return Err(ProjectViewError::SymbolicLink(root.to_path_buf()));
        }
        Ok(Self {
            root: root.to_path_buf(),
            limits,
            cache: BTreeMap::new(),
            kinds: BTreeMap::new(),
            total_bytes: 0,
            scanned_entries: 0,
        })
    }

    fn validate_relative(relative: &Path) -> Result<(), ProjectViewError> {
        if relative.as_os_str().is_empty()
            || relative.is_absolute()
            || !relative
                .components()
                .all(|component| matches!(component, Component::Normal(_)))
        {
            return Err(ProjectViewError::InvalidDeclaredPath(
                relative.to_path_buf(),
            ));
        }
        Ok(())
    }

    fn checked_path(&self, relative: &Path) -> Result<PathBuf, ProjectViewError> {
        Self::validate_relative(relative)?;
        let mut current = self.root.clone();
        for component in relative.components() {
            let Component::Normal(part) = component else {
                return Err(ProjectViewError::InvalidDeclaredPath(
                    relative.to_path_buf(),
                ));
            };
            current.push(part);
            let metadata = fs::symlink_metadata(&current).map_err(|source| {
                if source.kind() == std::io::ErrorKind::NotFound {
                    ProjectViewError::InvalidPathType(relative.to_path_buf())
                } else {
                    ProjectViewError::Io {
                        path: relative.to_path_buf(),
                        source,
                    }
                }
            })?;
            if metadata.file_type().is_symlink() {
                return Err(ProjectViewError::SymbolicLink(relative.to_path_buf()));
            }
        }
        Ok(current)
    }

    fn validate_dir(&self, relative: &Path) -> Result<PathBuf, ProjectViewError> {
        let path = self.checked_path(relative)?;
        if !path.is_dir() {
            return Err(ProjectViewError::InvalidPathType(relative.to_path_buf()));
        }
        Ok(path)
    }

    fn read(&mut self, relative: &Path, kind: SourceKind) -> Result<Vec<u8>, ProjectViewError> {
        if let Some(source) = self.cache.get(relative) {
            return Ok(source.clone());
        }
        let path = self.checked_path(relative)?;
        let metadata = fs::metadata(&path).map_err(|source| ProjectViewError::Io {
            path: relative.to_path_buf(),
            source,
        })?;
        if !metadata.is_file() {
            return Err(ProjectViewError::InvalidPathType(relative.to_path_buf()));
        }
        let bytes = usize::try_from(metadata.len()).unwrap_or(usize::MAX);
        if bytes > self.limits.max_file_bytes {
            return Err(ProjectViewError::LimitExceeded {
                limit_name: "max_file_bytes",
                limit: self.limits.max_file_bytes,
                path: relative.to_path_buf(),
            });
        }
        if self.cache.len() >= self.limits.max_files {
            return Err(ProjectViewError::LimitExceeded {
                limit_name: "max_files",
                limit: self.limits.max_files,
                path: relative.to_path_buf(),
            });
        }
        if self.total_bytes.saturating_add(bytes) > self.limits.max_total_input_bytes {
            return Err(ProjectViewError::LimitExceeded {
                limit_name: "max_total_input_bytes",
                limit: self.limits.max_total_input_bytes,
                path: relative.to_path_buf(),
            });
        }
        let source = fs::read(&path).map_err(|source| ProjectViewError::Io {
            path: relative.to_path_buf(),
            source,
        })?;
        self.total_bytes += source.len();
        self.kinds.insert(relative.to_path_buf(), kind);
        self.cache.insert(relative.to_path_buf(), source.clone());
        Ok(source)
    }

    fn optional_read(
        &mut self,
        relative: &Path,
        kind: SourceKind,
    ) -> Result<Option<Vec<u8>>, ProjectViewError> {
        Self::validate_relative(relative)?;
        let mut current = self.root.clone();
        for component in relative.components() {
            let Component::Normal(part) = component else {
                return Err(ProjectViewError::InvalidDeclaredPath(
                    relative.to_path_buf(),
                ));
            };
            current.push(part);
            match fs::symlink_metadata(&current) {
                Ok(metadata) if metadata.file_type().is_symlink() => {
                    return Err(ProjectViewError::SymbolicLink(relative.to_path_buf()));
                }
                Ok(_) => {}
                Err(source) if source.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                Err(source) => {
                    return Err(ProjectViewError::Io {
                        path: relative.to_path_buf(),
                        source,
                    });
                }
            }
        }
        self.read(relative, kind).map(Some)
    }

    fn collect_files(
        &mut self,
        relative: &Path,
        extension: Option<&str>,
    ) -> Result<Vec<PathBuf>, ProjectViewError> {
        let root = self.validate_dir(relative)?;
        let mut pending = vec![root];
        let mut files = Vec::new();
        while let Some(directory) = pending.pop() {
            let mut entries = fs::read_dir(&directory)
                .map_err(|source| ProjectViewError::Io {
                    path: directory
                        .strip_prefix(&self.root)
                        .unwrap_or(&directory)
                        .to_path_buf(),
                    source,
                })?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|source| ProjectViewError::Io {
                    path: directory
                        .strip_prefix(&self.root)
                        .unwrap_or(&directory)
                        .to_path_buf(),
                    source,
                })?;
            entries.sort_by_key(fs::DirEntry::file_name);
            for entry in entries {
                let path = entry.path();
                let relative_path = path
                    .strip_prefix(&self.root)
                    .map_err(|_| ProjectViewError::InvalidDeclaredPath(path.clone()))?
                    .to_path_buf();
                self.scanned_entries = self.scanned_entries.saturating_add(1);
                if self.scanned_entries > self.limits.max_files {
                    return Err(ProjectViewError::LimitExceeded {
                        limit_name: "max_files",
                        limit: self.limits.max_files,
                        path: relative_path,
                    });
                }
                let metadata =
                    fs::symlink_metadata(&path).map_err(|source| ProjectViewError::Io {
                        path: relative_path.clone(),
                        source,
                    })?;
                if metadata.file_type().is_symlink() {
                    return Err(ProjectViewError::SymbolicLink(relative_path));
                }
                if metadata.is_dir() {
                    pending.push(path);
                } else if metadata.is_file()
                    && extension.is_none_or(|expected| {
                        path.extension().and_then(|value| value.to_str()) == Some(expected)
                    })
                {
                    files.push(relative_path);
                }
            }
        }
        files.sort();
        Ok(files)
    }

    fn provenance(&self) -> Vec<SourceProvenance> {
        self.cache
            .iter()
            .map(|(path, source)| SourceProvenance {
                kind: self.kinds[path],
                path: path.clone(),
                sha256: sha256(source),
                bytes: source.len(),
            })
            .collect()
    }
}

pub fn load_project_view(root: &Path) -> Result<ProjectView, ProjectViewError> {
    load_project_view_with_limits(root, ViewLimits::default())
}

pub fn load_project_view_with_limits(
    root: &Path,
    limits: ViewLimits,
) -> Result<ProjectView, ProjectViewError> {
    let mut reader = BoundedReader::new(root, limits)?;
    let manifest_path = Path::new("minco.toml");
    let manifest_source = reader.read(manifest_path, SourceKind::Manifest)?;
    let manifest: Manifest = parse_toml(manifest_path, &manifest_source)?;
    if manifest.schema != 1 {
        return invalid(
            manifest_path,
            format!("unsupported manifest schema {}", manifest.schema),
        );
    }

    let contract_source = reader.read(&manifest.contract, SourceKind::Contract)?;
    let contract_text = utf8(&manifest.contract, &contract_source)?;
    let contract = load_contract_source(manifest.contract.display().to_string(), contract_text)
        .map_err(|error| ProjectViewError::InvalidSource {
            path: manifest.contract.clone(),
            message: error.to_string(),
        })?;
    let mut contracts = BTreeMap::from([(manifest.contract.clone(), contract.document.clone())]);
    let secondary_contracts = manifest
        .operations
        .values()
        .filter_map(|trace| trace.contract.as_ref())
        .filter(|path| *path != &manifest.contract)
        .cloned()
        .collect::<BTreeSet<_>>();
    for contract_path in secondary_contracts {
        let source = reader.read(&contract_path, SourceKind::Contract)?;
        let source_text = utf8(&contract_path, &source)?;
        let report = load_contract_source(contract_path.display().to_string(), source_text)
            .map_err(|error| ProjectViewError::InvalidSource {
                path: contract_path.clone(),
                message: error.to_string(),
            })?;
        contracts.insert(contract_path, report.document);
    }

    let deployment_source = reader.read(&manifest.deployment_config, SourceKind::Deployment)?;
    let deployment_config: DeploymentConfig =
        parse_toml(&manifest.deployment_config, &deployment_source)?;
    let deployment_plan = deployment_config.into_plan(&contract.document);
    let deployment_diagnostics = deployment_plan.validate();
    let costs = CostProjection {
        database: estimate_deployment_database_cost(&deployment_plan),
        runtime: estimate_runtime_cost(&deployment_plan),
    };

    let roadmap_source = reader.read(&manifest.roadmap, SourceKind::Roadmap)?;
    let roadmap: Roadmap = parse_yaml(&manifest.roadmap, &roadmap_source)?;
    if roadmap.schema != 1 {
        return invalid(
            &manifest.roadmap,
            format!("unsupported roadmap schema {}", roadmap.schema),
        );
    }
    let task_paths = reader.collect_files(&manifest.tasks, Some("md"))?;
    let mut tasks = Vec::new();
    for task_path in &task_paths {
        let source = reader.read(task_path, SourceKind::Task)?;
        tasks.push(parse_task(task_path, utf8(task_path, &source)?)?);
    }
    tasks.sort_by(|left, right| left.id.cmp(&right.id));

    let catalog_source = reader.read(&manifest.plugin_catalog, SourceKind::PluginCatalog)?;
    let mut catalog: PluginCatalog = parse_toml(&manifest.plugin_catalog, &catalog_source)?;
    if catalog.schema != 1 {
        return invalid(
            &manifest.plugin_catalog,
            format!("unsupported plugin catalog schema {}", catalog.schema),
        );
    }
    catalog.plugin.sort_by(|left, right| left.id.cmp(&right.id));
    let _quality = reader.read(&manifest.quality, SourceKind::QualityContract)?;
    let _generated = reader.read(&manifest.generated, SourceKind::GeneratedBinding)?;

    validate_declared_roots(&reader, &manifest.architecture)?;
    for migration_root in &manifest.migrations.roots {
        for source_path in reader.collect_files(migration_root, None)? {
            let _ = reader.read(&source_path, SourceKind::Migration)?;
        }
    }
    for seed_root in &manifest.seeds.roots {
        for source_path in reader.collect_files(seed_root, None)? {
            let _ = reader.read(&source_path, SourceKind::Seed)?;
        }
    }
    let migrations = minco_db::load_catalog(root, &manifest.migrations.roots).map_err(|error| {
        ProjectViewError::InvalidSource {
            path: PathBuf::from("minco.toml"),
            message: error.to_string(),
        }
    })?;
    let seeds = minco_db::load_seed_catalog(root, &manifest.seeds.roots).map_err(|error| {
        ProjectViewError::InvalidSource {
            path: PathBuf::from("minco.toml"),
            message: error.to_string(),
        }
    })?;

    let mut diagnostics = Vec::new();
    let status_mappings = status_mappings();
    let (mut nodes, mut edges) = graph(
        &manifest,
        &roadmap,
        &tasks,
        &catalog,
        &contracts,
        &status_mappings,
        &mut diagnostics,
        limits.max_text_bytes,
    );
    nodes.sort_by(|left, right| left.id.cmp(&right.id));
    edges.sort_by(|left, right| {
        (&left.from, &left.to, left.kind).cmp(&(&right.from, &right.to, right.kind))
    });
    if nodes.len() > limits.max_nodes {
        return Err(ProjectViewError::LimitExceeded {
            limit_name: "max_nodes",
            limit: limits.max_nodes,
            path: manifest_path.to_path_buf(),
        });
    }
    if edges.len() > limits.max_edges {
        return Err(ProjectViewError::LimitExceeded {
            limit_name: "max_edges",
            limit: limits.max_edges,
            path: manifest_path.to_path_buf(),
        });
    }

    let task_readiness = task_readiness(&tasks);
    let configuration = configuration(
        &manifest.configuration,
        limits.max_text_bytes,
        &mut diagnostics,
    );
    let mut evidence = evidence(&mut reader, &mut diagnostics)?;
    let provenance = reader.provenance();
    let source_digest = aggregate_source_digest(&provenance);
    evidence
        .get_mut(&EvidenceLane::Source)
        .expect("source lane")
        .extend(provenance.iter().map(|source| EvidenceItem {
            subject: source.path.display().to_string(),
            state: "present".into(),
            source: source.path.display().to_string(),
            exact_subject: Some(format!("sha256:{}", source.sha256)),
            freshness: snapshot_freshness(),
        }));
    let summary = summary(&nodes, &edges, &tasks, &task_readiness, &evidence);
    let feedback = feedback_context(&manifest, &catalog);
    let input_usage = InputUsage {
        files: reader.cache.len(),
        bytes: reader.total_bytes,
    };
    let mut view = ProjectView {
        schema_version: PROJECT_VIEW_SCHEMA_VERSION,
        project: ProjectIdentity {
            name: manifest.name,
            source_digest,
        },
        limits,
        input_usage,
        provenance: reader.provenance(),
        nodes,
        edges,
        status_mappings,
        evidence,
        configuration,
        migrations,
        seeds,
        deployment: DeploymentProjection {
            plan: deployment_plan,
            diagnostics: deployment_diagnostics,
        },
        costs,
        task_readiness,
        feedback,
        summary,
        diagnostics,
    };
    view.provenance
        .sort_by(|left, right| left.path.cmp(&right.path));
    let response_size = serde_json::to_vec(&view)
        .map_err(|error| ProjectViewError::InvalidSource {
            path: manifest_path.to_path_buf(),
            message: error.to_string(),
        })?
        .len();
    if response_size > limits.max_response_bytes {
        return Err(ProjectViewError::LimitExceeded {
            limit_name: "max_response_bytes",
            limit: limits.max_response_bytes,
            path: manifest_path.to_path_buf(),
        });
    }
    Ok(view)
}

fn evidence(
    reader: &mut BoundedReader,
    diagnostics: &mut Vec<ProjectDiagnostic>,
) -> Result<BTreeMap<EvidenceLane, Vec<EvidenceItem>>, ProjectViewError> {
    let mut lanes = EvidenceLane::ALL
        .into_iter()
        .map(|lane| (lane, Vec::new()))
        .collect::<BTreeMap<_, _>>();
    for relative in [
        "verification/static-validation.json",
        "verification/deep-review.json",
        "verification/publish-validation.json",
        "verification/source-manifest.json",
        "verification/adoption-measurements.json",
    ] {
        let path = Path::new(relative);
        let Some(source) = reader.optional_read(path, SourceKind::Verification)? else {
            continue;
        };
        match serde_json::from_slice::<Value>(&source) {
            Ok(value) => {
                let state = value
                    .get("status")
                    .and_then(Value::as_str)
                    .unwrap_or("recorded")
                    .to_owned();
                let exact_subject = value
                    .get("source_tree_sha256")
                    .and_then(Value::as_str)
                    .map(|digest| format!("source-tree-sha256:{digest}"))
                    .or_else(|| {
                        value
                            .pointer("/candidate/revision")
                            .and_then(Value::as_str)
                            .map(str::to_owned)
                    });
                lanes
                    .get_mut(&EvidenceLane::LocalVerification)
                    .expect("local lane")
                    .push(EvidenceItem {
                        subject: relative.trim_start_matches("verification/").to_owned(),
                        state,
                        source: relative.to_owned(),
                        exact_subject,
                        freshness: snapshot_freshness(),
                    });
            }
            Err(error) => diagnostics.push(ProjectDiagnostic {
                code: "PROJECT_VIEW_EVIDENCE_INVALID".into(),
                severity: DiagnosticSeverity::Warning,
                message: format!("bounded evidence report could not be parsed: {error}"),
                source: Some(path.to_path_buf()),
            }),
        }
    }
    for lane in [
        EvidenceLane::LocalVerification,
        EvidenceLane::HostedVerification,
        EvidenceLane::Deployment,
        EvidenceLane::Runtime,
        EvidenceLane::Review,
    ] {
        let items = lanes.get_mut(&lane).expect("evidence lane");
        if items.is_empty() {
            items.push(EvidenceItem {
                subject: format!("{lane:?}").to_ascii_lowercase(),
                state: "absent".into(),
                source: "not_configured".into(),
                exact_subject: None,
                freshness: EvidenceFreshness {
                    basis: "not_observed".into(),
                    observed_at: None,
                    limitation: Some("absence does not imply failure or success".into()),
                },
            });
        }
    }
    Ok(lanes)
}

#[allow(clippy::too_many_arguments)]
fn graph(
    manifest: &Manifest,
    roadmap: &Roadmap,
    tasks: &[Task],
    catalog: &PluginCatalog,
    contracts: &BTreeMap<PathBuf, minco_contract::ContractDocument>,
    mappings: &[StatusMapping],
    diagnostics: &mut Vec<ProjectDiagnostic>,
    max_text_bytes: usize,
) -> (Vec<ProjectNode>, Vec<ProjectEdge>) {
    let mut nodes = vec![ProjectNode {
        id: "project:root".into(),
        kind: NodeKind::Project,
        label: bounded_text(
            &manifest.name,
            max_text_bytes,
            Path::new("minco.toml"),
            diagnostics,
        ),
        description: Some(bounded_text(
            &roadmap.product,
            max_text_bytes,
            &manifest.roadmap,
            diagnostics,
        )),
        raw_status: None,
        semantic_status: None,
        source: PathBuf::from("minco.toml"),
        properties: BTreeMap::new(),
    }];
    let mut edges = Vec::new();
    for (layer, roots) in [
        ("domain", &manifest.architecture.domain_roots),
        ("application", &manifest.architecture.application_roots),
        ("api", &manifest.architecture.api_roots),
    ] {
        for root in roots {
            let id = format!("architecture:{layer}:{}", root.display());
            nodes.push(ProjectNode {
                id: id.clone(),
                kind: NodeKind::Architecture,
                label: root.display().to_string(),
                description: Some(format!("Declared {layer} architecture root")),
                raw_status: None,
                semantic_status: None,
                source: PathBuf::from("minco.toml"),
                properties: BTreeMap::from([("layer".into(), json!(layer))]),
            });
            edges.push(edge("project:root", &id, EdgeKind::Contains, "minco.toml"));
        }
    }
    for milestone in &roadmap.milestones {
        let id = format!("milestone:{}", milestone.id);
        let semantic = semantic_status(
            "minco_progress",
            &milestone.status,
            mappings,
            diagnostics,
            &manifest.roadmap,
        );
        nodes.push(ProjectNode {
            id: id.clone(),
            kind: NodeKind::Milestone,
            label: bounded_text(
                &milestone.name,
                max_text_bytes,
                &manifest.roadmap,
                diagnostics,
            ),
            description: Some(bounded_text(
                &milestone.outcome,
                max_text_bytes,
                &manifest.roadmap,
                diagnostics,
            )),
            raw_status: Some(milestone.status.clone()),
            semantic_status: Some(semantic),
            source: manifest.roadmap.clone(),
            properties: BTreeMap::from([("milestone_id".into(), json!(milestone.id))]),
        });
        edges.push(edge(
            "project:root",
            &id,
            EdgeKind::Contains,
            &manifest.roadmap,
        ));
        for dependency in &milestone.depends_on {
            edges.push(edge(
                &id,
                &format!("milestone:{dependency}"),
                EdgeKind::DependsOn,
                &manifest.roadmap,
            ));
        }
    }
    for task in tasks {
        let id = format!("task:{}", task.id);
        let semantic = semantic_status(
            "minco_progress",
            &task.status,
            mappings,
            diagnostics,
            &manifest.tasks,
        );
        nodes.push(ProjectNode {
            id: id.clone(),
            kind: NodeKind::Task,
            label: bounded_text(&task.title, max_text_bytes, &manifest.tasks, diagnostics),
            description: None,
            raw_status: Some(task.status.clone()),
            semantic_status: Some(semantic),
            source: manifest.tasks.clone(),
            properties: BTreeMap::from([
                ("task_id".into(), json!(task.id)),
                ("priority".into(), json!(task.priority)),
                ("area".into(), json!(task.area)),
            ]),
        });
        edges.push(edge(
            &id,
            &format!("milestone:{}", task.milestone),
            EdgeKind::BelongsTo,
            &manifest.tasks,
        ));
        for dependency in &task.depends_on {
            edges.push(edge(
                &id,
                &format!("task:{dependency}"),
                EdgeKind::DependsOn,
                &manifest.tasks,
            ));
        }
        for operation in &task.operations {
            edges.push(edge(
                &id,
                &format!("operation:{operation}"),
                EdgeKind::Implements,
                &manifest.tasks,
            ));
        }
    }
    let mut resources = BTreeSet::new();
    for (contract_path, contract) in contracts {
        for resource in contract.resource_operations() {
            resources.insert((resource.name.clone(), contract_path.clone()));
            edges.push(edge(
                &format!("resource:{}", resource.name),
                &format!("operation:{}", resource.operation_id),
                EdgeKind::Exposes,
                contract_path,
            ));
        }
    }
    for (resource, contract_path) in resources {
        let id = format!("resource:{resource}");
        nodes.push(ProjectNode {
            id: id.clone(),
            kind: NodeKind::Resource,
            label: resource,
            description: None,
            raw_status: None,
            semantic_status: None,
            source: contract_path.clone(),
            properties: BTreeMap::new(),
        });
        edges.push(edge("project:root", &id, EdgeKind::Contains, contract_path));
    }
    let mut seen_operations = BTreeSet::new();
    for (contract_path, contract) in contracts {
        for operation in &contract.operations {
            if !seen_operations.insert(operation.operation_id.clone()) {
                diagnostics.push(ProjectDiagnostic {
                    code: "PROJECT_VIEW_OPERATION_DUPLICATE".into(),
                    severity: DiagnosticSeverity::Error,
                    message: format!(
                        "operation ID {:?} appears in more than one declared contract",
                        operation.operation_id
                    ),
                    source: Some(contract_path.clone()),
                });
                continue;
            }
            let id = format!("operation:{}", operation.operation_id);
            let trace = manifest.operations.get(&operation.operation_id);
            let mut properties = BTreeMap::from([
                ("operation_id".into(), json!(operation.operation_id)),
                ("method".into(), json!(operation.method.as_str())),
                ("path".into(), json!(operation.path)),
                ("authenticated".into(), json!(operation.authenticated)),
                ("idempotent".into(), json!(operation.idempotent)),
                ("contract_source".into(), json!(contract_path)),
            ]);
            if let Some(trace) = trace {
                properties.insert("handler".into(), json!(trace.handler));
                properties.insert("application".into(), json!(trace.application));
                properties.insert("adapters".into(), json!(trace.adapters));
                properties.insert("tests".into(), json!(trace.tests));
                properties.insert("generated_source".into(), json!(trace.generated));
            }
            nodes.push(ProjectNode {
                id: id.clone(),
                kind: NodeKind::Operation,
                label: operation.operation_id.clone(),
                description: Some(format!("{} {}", operation.method.as_str(), operation.path)),
                raw_status: None,
                semantic_status: None,
                source: contract_path.clone(),
                properties,
            });
            edges.push(edge("project:root", &id, EdgeKind::Contains, contract_path));
        }
    }
    for plugin in &catalog.plugin {
        let id = format!("feature:{}", plugin.id);
        let enabled = manifest.plugins.enabled.contains(&plugin.id)
            || (plugin.default_enabled && !manifest.plugins.disabled.contains(&plugin.id));
        nodes.push(ProjectNode {
            id: id.clone(),
            kind: NodeKind::Feature,
            label: plugin.id.clone(),
            description: Some(bounded_text(
                &plugin.description,
                max_text_bytes,
                &manifest.plugin_catalog,
                diagnostics,
            )),
            raw_status: None,
            semantic_status: None,
            source: manifest.plugin_catalog.clone(),
            properties: BTreeMap::from([
                ("crate".into(), json!(plugin.crate_name)),
                ("path".into(), json!(plugin.path)),
                ("kind".into(), json!(plugin.kind)),
                ("feature".into(), json!(plugin.feature)),
                ("enabled".into(), json!(enabled)),
                ("stability".into(), json!(plugin.stability)),
            ]),
        });
        edges.push(edge(
            "project:root",
            &id,
            EdgeKind::Contains,
            &manifest.plugin_catalog,
        ));
    }
    (nodes, edges)
}

fn status_mappings() -> Vec<StatusMapping> {
    [
        ("planned", SemanticStatus::NotStarted),
        ("ready", SemanticStatus::NotStarted),
        ("active", SemanticStatus::Active),
        ("in_progress", SemanticStatus::Active),
        ("blocked", SemanticStatus::Blocked),
        ("complete", SemanticStatus::Complete),
    ]
    .into_iter()
    .map(|(raw, semantic)| StatusMapping {
        vocabulary: "minco_progress".into(),
        raw: raw.into(),
        semantic,
    })
    .collect()
}

fn semantic_status(
    vocabulary: &str,
    raw: &str,
    mappings: &[StatusMapping],
    diagnostics: &mut Vec<ProjectDiagnostic>,
    source: &Path,
) -> SemanticStatus {
    mappings
        .iter()
        .find(|mapping| mapping.vocabulary == vocabulary && mapping.raw == raw)
        .map_or_else(
            || {
                diagnostics.push(ProjectDiagnostic {
                    code: "PROJECT_VIEW_STATUS_UNMAPPED".into(),
                    severity: DiagnosticSeverity::Warning,
                    message: format!("unmapped raw status {raw:?} in vocabulary {vocabulary}"),
                    source: Some(source.to_path_buf()),
                });
                SemanticStatus::Unknown
            },
            |mapping| mapping.semantic,
        )
}

fn task_readiness(tasks: &[Task]) -> Vec<TaskReadiness> {
    let complete = tasks
        .iter()
        .filter(|task| task.status == "complete")
        .map(|task| task.id.as_str())
        .collect::<BTreeSet<_>>();
    tasks
        .iter()
        .map(|task| {
            let dependencies_complete = task
                .depends_on
                .iter()
                .all(|dependency| complete.contains(dependency.as_str()));
            TaskReadiness {
                id: task.id.clone(),
                raw_status: task.status.clone(),
                dependencies_complete,
                ready: dependencies_complete
                    && matches!(task.status.as_str(), "ready" | "active" | "in_progress"),
            }
        })
        .collect()
}

fn configuration(
    manifest: &ConfigurationManifest,
    max_text_bytes: usize,
    diagnostics: &mut Vec<ProjectDiagnostic>,
) -> Vec<ConfigurationFieldView> {
    let mut fields = manifest
        .fields
        .iter()
        .map(|field| ConfigurationFieldView {
            key: field.key.clone(),
            kind: field.kind.clone(),
            required: field.required,
            secret: field.secret,
            description: bounded_text(
                &field.description,
                max_text_bytes,
                Path::new("minco.toml"),
                diagnostics,
            ),
            value: if field.secret {
                ConfigurationValue::Redacted
            } else {
                field
                    .default
                    .as_ref()
                    .map_or(ConfigurationValue::Absent, |value| {
                        ConfigurationValue::Declared(
                            serde_json::to_value(value).unwrap_or(Value::Null),
                        )
                    })
            },
            source: PathBuf::from("minco.toml"),
        })
        .collect::<Vec<_>>();
    fields.sort_by(|left, right| left.key.cmp(&right.key));
    fields
}

fn feedback_context(manifest: &Manifest, catalog: &PluginCatalog) -> FeedbackContext {
    let feature_declared = catalog.plugin.iter().any(|plugin| plugin.id == "feedback");
    let enabled = manifest.plugins.enabled.contains("feedback")
        || catalog.plugin.iter().any(|plugin| {
            plugin.id == "feedback"
                && plugin.default_enabled
                && !manifest.plugins.disabled.contains("feedback")
        });
    let mut operation_ids = manifest
        .operations
        .iter()
        .filter(|(_, trace)| {
            trace
                .contract
                .as_ref()
                .is_some_and(|path| path.to_string_lossy().contains("feedback"))
                || trace
                    .handler
                    .as_deref()
                    .is_some_and(|handler| handler.contains("feedback"))
        })
        .map(|(operation_id, _)| operation_id.clone())
        .collect::<Vec<_>>();
    operation_ids.sort();
    FeedbackContext {
        feature_declared,
        enabled,
        operation_ids,
        limitation: "Capability metadata only; feedback instances, attachments, credentials and service clients are not read.".into(),
    }
}

fn summary(
    nodes: &[ProjectNode],
    edges: &[ProjectEdge],
    tasks: &[Task],
    readiness: &[TaskReadiness],
    evidence: &BTreeMap<EvidenceLane, Vec<EvidenceItem>>,
) -> DerivedSummary {
    let mut task_status_counts = BTreeMap::new();
    for task in tasks {
        *task_status_counts.entry(task.status.clone()).or_insert(0) += 1;
    }
    DerivedSummary {
        derived: true,
        node_count: nodes.len(),
        edge_count: edges.len(),
        denominator: tasks.len(),
        task_status_counts,
        ready_task_ids: readiness
            .iter()
            .filter(|task| task.ready)
            .map(|task| task.id.clone())
            .collect(),
        evidence_item_counts: evidence
            .iter()
            .map(|(lane, items)| (*lane, items.len()))
            .collect(),
    }
}

fn validate_declared_roots(
    reader: &BoundedReader,
    architecture: &ArchitectureManifest,
) -> Result<(), ProjectViewError> {
    for root in architecture
        .domain_roots
        .iter()
        .chain(&architecture.application_roots)
        .chain(&architecture.api_roots)
    {
        let _ = reader.validate_dir(root)?;
    }
    Ok(())
}

fn bounded_text(
    source: &str,
    max_bytes: usize,
    path: &Path,
    diagnostics: &mut Vec<ProjectDiagnostic>,
) -> String {
    if source.len() <= max_bytes {
        return source.to_owned();
    }
    let mut boundary = max_bytes;
    while !source.is_char_boundary(boundary) {
        boundary = boundary.saturating_sub(1);
    }
    diagnostics.push(ProjectDiagnostic {
        code: "PROJECT_VIEW_TEXT_TRUNCATED".into(),
        severity: DiagnosticSeverity::Warning,
        message: format!("text exceeded max_text_bytes={max_bytes} and was truncated"),
        source: Some(path.to_path_buf()),
    });
    source[..boundary].to_owned()
}

fn parse_task(path: &Path, source: &str) -> Result<Task, ProjectViewError> {
    let rest = source
        .strip_prefix("---\n")
        .ok_or_else(|| ProjectViewError::InvalidSource {
            path: path.to_path_buf(),
            message: "task has no YAML front matter".into(),
        })?;
    let (frontmatter, _) =
        rest.split_once("\n---\n")
            .ok_or_else(|| ProjectViewError::InvalidSource {
                path: path.to_path_buf(),
                message: "task has unterminated YAML front matter".into(),
            })?;
    serde_yaml_ng::from_str(frontmatter).map_err(|error| ProjectViewError::InvalidSource {
        path: path.to_path_buf(),
        message: error.to_string(),
    })
}

fn parse_toml<T: for<'de> Deserialize<'de>>(
    path: &Path,
    source: &[u8],
) -> Result<T, ProjectViewError> {
    toml::from_str(utf8(path, source)?).map_err(|error| ProjectViewError::InvalidSource {
        path: path.to_path_buf(),
        message: error.to_string(),
    })
}

fn parse_yaml<T: for<'de> Deserialize<'de>>(
    path: &Path,
    source: &[u8],
) -> Result<T, ProjectViewError> {
    serde_yaml_ng::from_str(utf8(path, source)?).map_err(|error| ProjectViewError::InvalidSource {
        path: path.to_path_buf(),
        message: error.to_string(),
    })
}

fn utf8<'a>(path: &Path, source: &'a [u8]) -> Result<&'a str, ProjectViewError> {
    std::str::from_utf8(source).map_err(|error| ProjectViewError::InvalidSource {
        path: path.to_path_buf(),
        message: error.to_string(),
    })
}

fn invalid<T>(path: &Path, message: String) -> Result<T, ProjectViewError> {
    Err(ProjectViewError::InvalidSource {
        path: path.to_path_buf(),
        message,
    })
}

fn edge(from: &str, to: &str, kind: EdgeKind, source: impl AsRef<Path>) -> ProjectEdge {
    ProjectEdge {
        from: from.to_owned(),
        to: to.to_owned(),
        kind,
        source: source.as_ref().to_path_buf(),
    }
}

fn sha256(source: &[u8]) -> String {
    hex::encode(Sha256::digest(source))
}

fn aggregate_source_digest(provenance: &[SourceProvenance]) -> String {
    let mut canonical = String::new();
    for source in provenance {
        writeln!(
            &mut canonical,
            "{}\0{}",
            source.path.display(),
            source.sha256
        )
        .expect("writing to a String is infallible");
    }
    sha256(canonical.as_bytes())
}

fn snapshot_freshness() -> EvidenceFreshness {
    EvidenceFreshness {
        basis: "repository_snapshot".into(),
        observed_at: None,
        limitation: Some("No wall-clock freshness is inferred from repository content.".into()),
    }
}
