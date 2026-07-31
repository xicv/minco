use crate::{config::MincoManifest, print_value};
use anyhow::{Context, Result, bail};
use clap::{Args, Subcommand};
use minco_contract::{
    OwnedOperation, OwnedResourceOperation, ResourceAction, Severity, load_contract,
};
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, OpenOptions},
    io::Write,
    path::{Component, Path, PathBuf},
};

#[derive(Debug, Subcommand)]
pub enum MakeCommand {
    /// Generate domain and application module boundaries without business rules.
    Module(NamedArgs),
    /// Generate failing application and HTTP specifications for one contract operation.
    Operation(OperationArgs),
    /// Generate failing specifications for one reviewed five-action resource contract.
    Resource(NamedArgs),
    /// Generate an empty, explicitly classified SQL migration.
    Migration(NamedArgs),
    /// Generate an empty seeder with a fail-closed verification query.
    Seeder(NamedArgs),
    /// Generate a disabled worker entrypoint and failing specification.
    Worker(NamedArgs),
    /// Generate an infrastructure adapter boundary and failing behavioral specification.
    Adapter(NamedArgs),
    /// Generate only the failing specifications for one contract operation.
    Test(OperationArgs),
    /// Generate an application-owned statically linked plugin crate.
    Plugin(NamedArgs),
}

#[derive(Debug, Subcommand)]
pub enum StubsCommand {
    /// Publish framework generator stubs into `stubs/minco` for app-owned customization.
    Publish(DryRunArgs),
}

#[derive(Debug, Clone, Args)]
pub struct NamedArgs {
    /// Lower-kebab-case generator name.
    pub name: String,
    /// Print the deterministic edit plan without changing application files.
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Debug, Clone, Copy, Args)]
pub struct DryRunArgs {
    /// Print the deterministic edit plan without changing application files.
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Debug, Clone, Args)]
pub struct OperationArgs {
    /// Existing lowerCamelCase `OpenAPI` operationId.
    pub operation_id: String,
    /// Print the deterministic edit plan without changing application files.
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Debug, Clone, Serialize)]
struct GenerationPlan {
    schema_version: u32,
    generator: &'static str,
    name: String,
    dry_run: bool,
    applied: bool,
    contract: Option<ContractSelection>,
    #[serde(skip_serializing_if = "Option::is_none")]
    resource: Option<ResourceSelection>,
    changes: Vec<PlannedChange>,
    #[serde(skip)]
    edits: Vec<PlannedEdit>,
}

#[derive(Debug, Clone, Serialize)]
struct ContractSelection {
    operation_id: String,
    method: String,
    path: String,
    contract_sha256: String,
}

#[derive(Debug, Clone, Serialize)]
struct ResourceSelection {
    name: String,
    contract_sha256: String,
    operations: Vec<ResourceOperationSelection>,
}

#[derive(Debug, Clone, Serialize)]
struct ResourceOperationSelection {
    action: ResourceAction,
    operation_id: String,
    method: String,
    path: String,
}

#[derive(Debug, Clone, Serialize)]
struct PlannedChange {
    path: String,
    action: ChangeAction,
    format: FileFormat,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum ChangeAction {
    Create,
    Update,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum FileFormat {
    Json,
    Markdown,
    Rust,
    Sql,
    Template,
    Toml,
}

#[derive(Debug, Clone)]
struct PlannedEdit {
    path: PathBuf,
    before: Option<Vec<u8>>,
    after: Vec<u8>,
}

impl PlannedEdit {
    fn create(root: &Path, path: impl Into<PathBuf>, after: impl Into<Vec<u8>>) -> Result<Self> {
        let path = path.into();
        validate_relative_path(&path)?;
        reject_symlink_ancestors(root, &path)?;
        match fs::symlink_metadata(root.join(&path)) {
            Ok(_) => {
                bail!(
                    "generator refuses to overwrite existing path {}",
                    path.display()
                );
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("inspect generator target {}", path.display()));
            }
        }
        Ok(Self {
            path,
            before: None,
            after: after.into(),
        })
    }

    fn update(root: &Path, path: impl Into<PathBuf>, after: impl Into<Vec<u8>>) -> Result<Self> {
        let path = path.into();
        validate_relative_path(&path)?;
        reject_symlink_ancestors(root, &path)?;
        let target = root.join(&path);
        let metadata = fs::symlink_metadata(&target)
            .with_context(|| format!("inspect generator input {}", path.display()))?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            bail!("generator input {} must be a regular file", path.display());
        }
        let before = fs::read(&target)
            .with_context(|| format!("read generator input {}", path.display()))?;
        Ok(Self {
            path,
            before: Some(before),
            after: after.into(),
        })
    }

    fn summary(&self) -> PlannedChange {
        PlannedChange {
            path: slash_path(&self.path),
            action: if self.before.is_some() {
                ChangeAction::Update
            } else {
                ChangeAction::Create
            },
            format: match self.path.extension().and_then(|value| value.to_str()) {
                Some("json") => FileFormat::Json,
                Some("md") => FileFormat::Markdown,
                Some("rs") => FileFormat::Rust,
                Some("sql") => FileFormat::Sql,
                Some("tmpl") => FileFormat::Template,
                Some("toml") => FileFormat::Toml,
                _ => unreachable!("generator plans only known file formats"),
            },
        }
    }
}

pub fn execute(
    root: &Path,
    manifest: &MincoManifest,
    command: MakeCommand,
    as_json: bool,
) -> Result<()> {
    let mut plan = match command {
        MakeCommand::Module(args) => module_plan(root, &args)?,
        MakeCommand::Operation(args) => operation_plan(root, manifest, &args)?,
        MakeCommand::Resource(args) => resource_plan(root, manifest, &args)?,
        MakeCommand::Migration(args) => migration_plan(root, manifest, &args)?,
        MakeCommand::Seeder(args) => seeder_plan(root, manifest, &args)?,
        MakeCommand::Worker(args) => worker_plan(root, manifest, &args)?,
        MakeCommand::Adapter(args) => adapter_plan(root, &args)?,
        MakeCommand::Test(args) => test_plan(root, manifest, &args)?,
        MakeCommand::Plugin(args) => plugin_plan(root, manifest, &args)?,
    };
    if !plan.dry_run {
        print_pre_write_plan(&plan)?;
        apply(root, &plan.edits)?;
        plan.applied = true;
    }
    print_value(&plan, as_json)
}

pub fn execute_stubs(root: &Path, command: &StubsCommand, as_json: bool) -> Result<()> {
    let mut plan = match command {
        StubsCommand::Publish(args) => stubs_plan(root, *args)?,
    };
    if !plan.dry_run {
        print_pre_write_plan(&plan)?;
        apply(root, &plan.edits)?;
        plan.applied = true;
    }
    print_value(&plan, as_json)
}

fn print_pre_write_plan(plan: &GenerationPlan) -> Result<()> {
    let rendered =
        serde_json::to_string_pretty(plan).context("render pre-write generation plan")?;
    let mut stderr = std::io::stderr().lock();
    writeln!(stderr, "Generation plan (before write):\n{rendered}")
        .context("print pre-write generation plan")?;
    stderr.flush().context("flush pre-write generation plan")
}

fn operation_plan(
    root: &Path,
    manifest: &MincoManifest,
    args: &OperationArgs,
) -> Result<GenerationPlan> {
    let contract_path = root.join(&manifest.contract);
    let report = load_contract(&contract_path)
        .with_context(|| format!("load OpenAPI contract {}", manifest.contract.display()))?;
    let errors = report
        .findings
        .iter()
        .filter(|finding| finding.severity == Severity::Error)
        .collect::<Vec<_>>();
    if !errors.is_empty() {
        bail!(
            "OpenAPI contract is invalid; fix it before generating code: {}",
            serde_json::to_string(&errors)?
        );
    }
    let operation = report
        .document
        .operations
        .iter()
        .find(|operation| operation.operation_id == args.operation_id)
        .with_context(|| {
            format!(
                "operationId {} is not present in {}; add and review the OpenAPI operation first",
                args.operation_id,
                manifest.contract.display()
            )
        })?;
    let snake_name = lower_camel_to_snake(&operation.operation_id)?;
    let type_name = lower_camel_to_pascal(&operation.operation_id)?;

    let application_test = PathBuf::from(format!("crates/application/tests/{snake_name}.rs"));
    let http_test = PathBuf::from(format!("crates/api/tests/{snake_name}.rs"));
    let documentation = PathBuf::from(format!("docs/generated/operations/{snake_name}.md"));

    let mut edits = vec![
        PlannedEdit::create(
            root,
            &application_test,
            render_operation_stub(root, Stub::ApplicationTest, operation, &type_name)?,
        )?,
        PlannedEdit::create(
            root,
            &http_test,
            render_operation_stub(root, Stub::HttpTest, operation, &type_name)?,
        )?,
        PlannedEdit::create(
            root,
            &documentation,
            render_operation_stub(root, Stub::OperationDocumentation, operation, &type_name)?,
        )?,
    ];
    edits.push(plan_operation_manifest_update(
        root,
        operation,
        &application_test,
        &http_test,
        &snake_name,
        &type_name,
    )?);
    edits.sort_by(|left, right| left.path.cmp(&right.path));

    Ok(GenerationPlan {
        schema_version: 1,
        generator: "operation",
        name: operation.operation_id.clone(),
        dry_run: args.dry_run,
        applied: false,
        contract: Some(ContractSelection {
            operation_id: operation.operation_id.clone(),
            method: operation.method.as_str().to_ascii_lowercase(),
            path: operation.path.clone(),
            contract_sha256: report.document.sha256,
        }),
        resource: None,
        changes: edits.iter().map(PlannedEdit::summary).collect(),
        edits,
    })
}

fn resource_plan(
    root: &Path,
    manifest: &MincoManifest,
    args: &NamedArgs,
) -> Result<GenerationPlan> {
    let names = GeneratorNames::new(&args.name)?;
    let contract_path = root.join(&manifest.contract);
    let report = load_contract(&contract_path)
        .with_context(|| format!("load OpenAPI contract {}", manifest.contract.display()))?;
    let errors = report
        .findings
        .iter()
        .filter(|finding| finding.severity == Severity::Error)
        .collect::<Vec<_>>();
    if !errors.is_empty() {
        bail!(
            "OpenAPI contract is invalid; fix it before generating code: {}",
            serde_json::to_string(&errors)?
        );
    }

    let selected = report
        .document
        .resource_operations()
        .into_iter()
        .filter(|operation| operation.name == names.kebab)
        .collect::<Vec<_>>();
    let by_action = selected
        .iter()
        .map(|operation| (operation.action, operation))
        .collect::<BTreeMap<_, _>>();
    let required = [
        ResourceAction::Create,
        ResourceAction::List,
        ResourceAction::Read,
        ResourceAction::Update,
        ResourceAction::Delete,
    ];
    let missing = required
        .into_iter()
        .filter(|action| !by_action.contains_key(action))
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        bail!(
            "resource {} is not a complete reviewed contract family; missing actions: {}",
            names.kebab,
            missing
                .iter()
                .map(|action| format!("{action:?}").to_ascii_lowercase())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }

    let operations_by_id = report
        .document
        .operations
        .iter()
        .map(|operation| (operation.operation_id.as_str(), operation))
        .collect::<BTreeMap<_, _>>();
    let mut operations = Vec::new();
    let mut edits = Vec::new();
    for action in required {
        let resource_operation = *by_action
            .get(&action)
            .expect("complete resource family was checked");
        let operation = operations_by_id
            .get(resource_operation.operation_id.as_str())
            .expect("resource metadata refers to a loaded operation");
        let snake_name = lower_camel_to_snake(&operation.operation_id)?;
        let type_name = lower_camel_to_pascal(&operation.operation_id)?;
        let application_test = PathBuf::from(format!("crates/application/tests/{snake_name}.rs"));
        let http_test = PathBuf::from(format!("crates/api/tests/{snake_name}.rs"));
        let documentation = PathBuf::from(format!("docs/generated/operations/{snake_name}.md"));
        edits.extend([
            PlannedEdit::create(
                root,
                &application_test,
                render_operation_stub(root, Stub::ApplicationTest, operation, &type_name)?,
            )?,
            PlannedEdit::create(
                root,
                &http_test,
                render_operation_stub(root, Stub::HttpTest, operation, &type_name)?,
            )?,
            PlannedEdit::create(
                root,
                &documentation,
                render_operation_stub(root, Stub::OperationDocumentation, operation, &type_name)?,
            )?,
        ]);
        operations.push((
            resource_operation,
            *operation,
            application_test,
            http_test,
            snake_name,
            type_name,
        ));
    }
    edits.push(plan_resource_manifest_update(root, &operations)?);
    edits.sort_by(|left, right| left.path.cmp(&right.path));

    Ok(GenerationPlan {
        schema_version: 1,
        generator: "resource",
        name: names.kebab.clone(),
        dry_run: args.dry_run,
        applied: false,
        contract: None,
        resource: Some(ResourceSelection {
            name: names.kebab,
            contract_sha256: report.document.sha256,
            operations: operations
                .iter()
                .map(
                    |(resource, operation, _, _, _, _)| ResourceOperationSelection {
                        action: resource.action,
                        operation_id: operation.operation_id.clone(),
                        method: operation.method.as_str().to_ascii_lowercase(),
                        path: operation.path.clone(),
                    },
                )
                .collect(),
        }),
        changes: edits.iter().map(PlannedEdit::summary).collect(),
        edits,
    })
}

fn render_operation_stub(
    root: &Path,
    stub: Stub,
    operation: &OwnedOperation,
    type_name: &str,
) -> Result<Vec<u8>> {
    let rust_path_literal =
        serde_json::to_string(&operation.path).context("escape contract path for Rust stub")?;
    render_stub(
        root,
        stub,
        &[
            ("OPERATION_ID", operation.operation_id.as_str()),
            (
                "SNAKE_NAME",
                &lower_camel_to_snake(&operation.operation_id)?,
            ),
            ("PASCAL_NAME", type_name),
            ("METHOD", operation.method.as_str()),
            ("PATH", operation.path.as_str()),
            ("RUST_PATH_LITERAL", rust_path_literal.as_str()),
        ],
    )
}

fn plan_operation_manifest_update(
    root: &Path,
    operation: &OwnedOperation,
    application_test: &Path,
    http_test: &Path,
    snake_name: &str,
    type_name: &str,
) -> Result<PlannedEdit> {
    let path = Path::new("minco.toml");
    let source = fs::read_to_string(root.join(path)).context("read minco.toml")?;
    let mut document: toml::Value = toml::from_str(&source).context("parse minco.toml")?;
    add_operation_trace(
        &mut document,
        operation,
        application_test,
        http_test,
        snake_name,
        type_name,
    )?;
    let rendered = toml::to_string_pretty(&document).context("render minco.toml")?;
    PlannedEdit::update(root, path, rendered.into_bytes())
}

type ResourcePlanOperation<'a> = (
    &'a OwnedResourceOperation,
    &'a OwnedOperation,
    PathBuf,
    PathBuf,
    String,
    String,
);

fn plan_resource_manifest_update(
    root: &Path,
    operations: &[ResourcePlanOperation<'_>],
) -> Result<PlannedEdit> {
    let path = Path::new("minco.toml");
    let source = fs::read_to_string(root.join(path)).context("read minco.toml")?;
    let mut document: toml::Value = toml::from_str(&source).context("parse minco.toml")?;
    for (_, operation, application_test, http_test, snake_name, type_name) in operations {
        add_operation_trace(
            &mut document,
            operation,
            application_test,
            http_test,
            snake_name,
            type_name,
        )?;
    }
    let rendered = toml::to_string_pretty(&document).context("render minco.toml")?;
    PlannedEdit::update(root, path, rendered.into_bytes())
}

fn add_operation_trace(
    document: &mut toml::Value,
    operation: &OwnedOperation,
    application_test: &Path,
    http_test: &Path,
    snake_name: &str,
    type_name: &str,
) -> Result<()> {
    let root_table = document
        .as_table_mut()
        .context("minco.toml root must be a TOML table")?;
    let operations = root_table
        .entry("operations")
        .or_insert_with(|| toml::Value::Table(toml::map::Map::new()))
        .as_table_mut()
        .context("minco.toml operations must be a TOML table")?;
    let trace = operations
        .entry(&operation.operation_id)
        .or_insert_with(|| toml::Value::Table(toml::map::Map::new()))
        .as_table_mut()
        .with_context(|| {
            format!(
                "minco.toml operation {} must be a TOML table",
                operation.operation_id
            )
        })?;
    trace
        .entry("handler")
        .or_insert_with(|| toml::Value::String(format!("crates/api/src/lib.rs#{snake_name}")));
    trace.entry("application").or_insert_with(|| {
        toml::Value::String(format!("crates/application/src/lib.rs#{type_name}"))
    });
    let mut tests = trace
        .get("tests")
        .and_then(toml::Value::as_array)
        .into_iter()
        .flatten()
        .map(|value| {
            value
                .as_str()
                .map(str::to_owned)
                .context("operation test traces must be strings")
        })
        .collect::<Result<BTreeSet<_>>>()?;
    tests.insert(slash_path(application_test));
    tests.insert(slash_path(http_test));
    trace.insert(
        "tests".into(),
        toml::Value::Array(tests.into_iter().map(toml::Value::String).collect()),
    );
    Ok(())
}

fn module_plan(root: &Path, args: &NamedArgs) -> Result<GenerationPlan> {
    let names = GeneratorNames::new(&args.name)?;
    let replacements = names.replacements();
    let domain_test_replacements = [
        ("NAME", names.kebab.as_str()),
        ("SNAKE_NAME", names.snake.as_str()),
        ("PASCAL_NAME", names.pascal.as_str()),
        ("LAYER", "domain"),
    ];
    let application_test_replacements = [
        ("NAME", names.kebab.as_str()),
        ("SNAKE_NAME", names.snake.as_str()),
        ("PASCAL_NAME", names.pascal.as_str()),
        ("LAYER", "application"),
    ];
    let edits = vec![
        PlannedEdit::create(
            root,
            format!("crates/application/src/modules/{}.rs", names.snake),
            render_stub(root, Stub::ModuleApplication, &replacements)?,
        )?,
        PlannedEdit::create(
            root,
            format!("crates/domain/src/modules/{}.rs", names.snake),
            render_stub(root, Stub::ModuleDomain, &replacements)?,
        )?,
        PlannedEdit::create(
            root,
            format!("crates/application/tests/{}_module.rs", names.snake),
            render_stub(root, Stub::ModuleTest, &application_test_replacements)?,
        )?,
        PlannedEdit::create(
            root,
            format!("crates/domain/tests/{}_module.rs", names.snake),
            render_stub(root, Stub::ModuleTest, &domain_test_replacements)?,
        )?,
        PlannedEdit::create(
            root,
            format!("docs/generated/modules/{}.md", names.snake),
            render_stub(root, Stub::ModuleDocumentation, &replacements)?,
        )?,
    ];
    generation_plan("module", &args.name, args.dry_run, None, edits)
}

fn migration_plan(
    root: &Path,
    manifest: &MincoManifest,
    args: &NamedArgs,
) -> Result<GenerationPlan> {
    let names = GeneratorNames::new(&args.name)?;
    let migration_root = exactly_one_root("migration", &manifest.migrations.roots)?;
    let metadata_path = migration_root.join(".minco-migrations.toml");
    let source = fs::read_to_string(root.join(&metadata_path))
        .with_context(|| format!("read {}", metadata_path.display()))?;
    let mut metadata: toml::Value =
        toml::from_str(&source).with_context(|| format!("parse {}", metadata_path.display()))?;
    let table = metadata
        .as_table_mut()
        .context("migration metadata must be a TOML table")?;
    let migrations = table
        .entry("migration")
        .or_insert_with(|| toml::Value::Array(Vec::new()))
        .as_array_mut()
        .context("migration metadata entries must be an array")?;
    let next_version = migrations
        .iter()
        .filter_map(|migration| migration.get("version"))
        .filter_map(toml::Value::as_integer)
        .max()
        .unwrap_or(0)
        .checked_add(1)
        .context("migration version overflow")?;
    migrations.push(toml::Value::Table(toml::map::Map::from_iter([
        ("version".into(), toml::Value::Integer(next_version)),
        ("risk".into(), toml::Value::String("destructive".into())),
        ("reversible".into(), toml::Value::Boolean(false)),
    ])));
    migrations.sort_by_key(|migration| {
        migration
            .get("version")
            .and_then(toml::Value::as_integer)
            .unwrap_or(i64::MAX)
    });
    let metadata_after = toml::to_string_pretty(&metadata).context("render migration metadata")?;
    let version = format!("{next_version:04}");
    let replacements = [
        ("NAME", names.kebab.as_str()),
        ("SNAKE_NAME", names.snake.as_str()),
        ("PASCAL_NAME", names.pascal.as_str()),
        ("VERSION", version.as_str()),
    ];
    let edits = vec![
        PlannedEdit::create(
            root,
            migration_root.join(format!("{version}_{}.sql", names.snake)),
            render_stub(root, Stub::MigrationSql, &replacements)?,
        )?,
        PlannedEdit::create(
            root,
            format!("docs/generated/migrations/{}.md", names.snake),
            render_stub(root, Stub::MigrationDocumentation, &replacements)?,
        )?,
        PlannedEdit::update(root, metadata_path, metadata_after.into_bytes())?,
    ];
    generation_plan("migration", &args.name, args.dry_run, None, edits)
}

fn seeder_plan(root: &Path, manifest: &MincoManifest, args: &NamedArgs) -> Result<GenerationPlan> {
    let names = GeneratorNames::new(&args.name)?;
    let seed_root = exactly_one_root("seed", &manifest.seeds.roots)?;
    let metadata_path = seed_root.join(".minco-seeds.toml");
    let source = fs::read_to_string(root.join(&metadata_path))
        .with_context(|| format!("read {}", metadata_path.display()))?;
    let mut metadata: toml::Value =
        toml::from_str(&source).with_context(|| format!("parse {}", metadata_path.display()))?;
    let table = metadata
        .as_table_mut()
        .context("seed metadata must be a TOML table")?;
    let set_id = table
        .get("id")
        .and_then(toml::Value::as_str)
        .context("seed metadata requires a string id")?
        .to_owned();
    let seeds = table
        .entry("seed")
        .or_insert_with(|| toml::Value::Array(Vec::new()))
        .as_array_mut()
        .context("seed metadata entries must be an array")?;
    let next_version = seeds
        .iter()
        .filter_map(|seed| seed.get("version"))
        .filter_map(toml::Value::as_integer)
        .max()
        .unwrap_or(0)
        .checked_add(1)
        .context("seed version overflow")?;
    let seed_id = format!("{set_id}-{}", names.kebab);
    if seeds
        .iter()
        .any(|seed| seed.get("id").and_then(toml::Value::as_str) == Some(seed_id.as_str()))
    {
        bail!("seed metadata already contains {seed_id}");
    }
    seeds.push(toml::Value::Table(toml::map::Map::from_iter([
        ("id".into(), toml::Value::String(seed_id)),
        ("version".into(), toml::Value::Integer(next_version)),
        ("class".into(), toml::Value::String("demo".into())),
        (
            "source".into(),
            toml::Value::String(format!("{}.sql", names.snake)),
        ),
        (
            "verify".into(),
            toml::Value::String(format!("{}.verify.sql", names.snake)),
        ),
        ("depends_on".into(), toml::Value::Array(Vec::new())),
        (
            "environments".into(),
            toml::Value::Array(
                ["local", "development"]
                    .into_iter()
                    .map(|value| toml::Value::String(value.into()))
                    .collect(),
            ),
        ),
        (
            "idempotency".into(),
            toml::Value::String("insert_once".into()),
        ),
        ("mutable_state".into(), toml::Value::String("none".into())),
        ("risk".into(), toml::Value::String("non_destructive".into())),
        ("transaction".into(), toml::Value::String("required".into())),
        (
            "preservation".into(),
            toml::Value::String("preserve_all_existing".into()),
        ),
    ])));
    seeds.sort_by(|left, right| {
        left.get("id")
            .and_then(toml::Value::as_str)
            .cmp(&right.get("id").and_then(toml::Value::as_str))
    });
    let metadata_after = toml::to_string_pretty(&metadata).context("render seed metadata")?;
    let replacements = names.replacements();
    let edits = vec![
        PlannedEdit::create(
            root,
            seed_root.join(format!("{}.sql", names.snake)),
            render_stub(root, Stub::SeederSql, &replacements)?,
        )?,
        PlannedEdit::create(
            root,
            seed_root.join(format!("{}.verify.sql", names.snake)),
            render_stub(root, Stub::SeederVerifySql, &replacements)?,
        )?,
        PlannedEdit::create(
            root,
            format!("docs/generated/seeders/{}.md", names.snake),
            render_stub(root, Stub::SeederDocumentation, &replacements)?,
        )?,
        PlannedEdit::update(root, metadata_path, metadata_after.into_bytes())?,
    ];
    generation_plan("seeder", &args.name, args.dry_run, None, edits)
}

fn worker_plan(root: &Path, manifest: &MincoManifest, args: &NamedArgs) -> Result<GenerationPlan> {
    let names = GeneratorNames::new(&args.name)?;
    let replacements = names.replacements();
    let worker_source = PathBuf::from(format!("services/app/src/bin/{}.rs", names.kebab));
    let worker_test = PathBuf::from(format!("services/app/tests/{}_worker.rs", names.snake));
    let documentation = PathBuf::from(format!("docs/generated/workers/{}.md", names.snake));

    let manifest_path = Path::new("minco.toml");
    let source = fs::read_to_string(root.join(manifest_path)).context("read minco.toml")?;
    let mut document: toml::Value = toml::from_str(&source).context("parse minco.toml")?;
    let root_table = document
        .as_table_mut()
        .context("minco.toml root must be a TOML table")?;
    let development = root_table
        .entry("development")
        .or_insert_with(|| toml::Value::Table(toml::map::Map::new()))
        .as_table_mut()
        .context("minco.toml development must be a TOML table")?;
    let workers = development
        .entry("workers")
        .or_insert_with(|| toml::Value::Array(Vec::new()))
        .as_array_mut()
        .context("development.workers must be an array")?;
    if workers
        .iter()
        .any(|worker| worker.get("id").and_then(toml::Value::as_str) == Some(names.kebab.as_str()))
    {
        bail!("development worker {} already exists", names.kebab);
    }
    let command = toml::Value::Table(toml::map::Map::from_iter([
        ("program".into(), toml::Value::String("cargo".into())),
        (
            "arguments".into(),
            toml::Value::Array(
                [
                    "run".to_owned(),
                    "-p".to_owned(),
                    format!("{}-service", manifest.name),
                    "--bin".to_owned(),
                    names.kebab.clone(),
                ]
                .into_iter()
                .map(toml::Value::String)
                .collect(),
            ),
        ),
    ]));
    workers.push(toml::Value::Table(toml::map::Map::from_iter([
        ("id".into(), toml::Value::String(names.kebab.clone())),
        ("default_enabled".into(), toml::Value::Boolean(false)),
        ("command".into(), command),
        (
            "readiness".into(),
            toml::Value::Table(toml::map::Map::from_iter([(
                "kind".into(),
                toml::Value::String("process".into()),
            )])),
        ),
    ])));
    workers.sort_by(|left, right| {
        left.get("id")
            .and_then(toml::Value::as_str)
            .cmp(&right.get("id").and_then(toml::Value::as_str))
    });
    let manifest_after = toml::to_string_pretty(&document).context("render minco.toml")?;
    let edits = vec![
        PlannedEdit::create(
            root,
            worker_source,
            render_stub(root, Stub::WorkerSource, &replacements)?,
        )?,
        PlannedEdit::create(
            root,
            worker_test,
            render_stub(root, Stub::WorkerTest, &replacements)?,
        )?,
        PlannedEdit::create(
            root,
            documentation,
            render_stub(root, Stub::WorkerDocumentation, &replacements)?,
        )?,
        PlannedEdit::update(root, manifest_path, manifest_after.into_bytes())?,
    ];
    generation_plan("worker", &args.name, args.dry_run, None, edits)
}

fn adapter_plan(root: &Path, args: &NamedArgs) -> Result<GenerationPlan> {
    let names = GeneratorNames::new(&args.name)?;
    let replacements = names.replacements();
    let edits = vec![
        PlannedEdit::create(
            root,
            format!("crates/adapters/src/{}.rs", names.snake),
            render_stub(root, Stub::AdapterSource, &replacements)?,
        )?,
        PlannedEdit::create(
            root,
            format!("crates/adapters/tests/{}.rs", names.snake),
            render_stub(root, Stub::AdapterTest, &replacements)?,
        )?,
        PlannedEdit::create(
            root,
            format!("docs/generated/adapters/{}.md", names.snake),
            render_stub(root, Stub::AdapterDocumentation, &replacements)?,
        )?,
    ];
    generation_plan("adapter", &args.name, args.dry_run, None, edits)
}

fn test_plan(
    root: &Path,
    manifest: &MincoManifest,
    args: &OperationArgs,
) -> Result<GenerationPlan> {
    let mut plan = operation_plan(root, manifest, args)?;
    plan.generator = "test";
    plan.edits
        .retain(|edit| edit.path.extension().and_then(|value| value.to_str()) != Some("md"));
    plan.changes = plan.edits.iter().map(PlannedEdit::summary).collect();
    Ok(plan)
}

fn plugin_plan(root: &Path, manifest: &MincoManifest, args: &NamedArgs) -> Result<GenerationPlan> {
    let names = GeneratorNames::new(&args.name)?;
    let crate_name = format!("minco-plugin-{}", names.kebab);
    let member = format!("plugins/{crate_name}");
    let replacements = names.replacements();
    let plugin_manifest = format!(
        "[package]\n\
         name = \"{crate_name}\"\n\
         version.workspace = true\n\
         edition.workspace = true\n\
         rust-version.workspace = true\n\
         license.workspace = true\n\
         publish = false\n\
         include = [\"src/**\", \"Cargo.toml\", \"minco-plugin.json\"]\n\n\
         [package.metadata.minco]\n\
         plugin = \"minco-plugin.json\"\n\n\
         [dependencies]\n\
         minco-core.workspace = true\n\
         semver.workspace = true\n\n\
         [lints]\n\
         workspace = true\n"
    );

    let workspace_path = Path::new("Cargo.toml");
    let workspace_source =
        fs::read_to_string(root.join(workspace_path)).context("read workspace Cargo.toml")?;
    let mut workspace: toml::Value =
        toml::from_str(&workspace_source).context("parse workspace Cargo.toml")?;
    add_workspace_member_to_document(&mut workspace, &member, &crate_name)?;

    let catalog_path = &manifest.plugin_catalog;
    let catalog_source = fs::read_to_string(root.join(catalog_path))
        .with_context(|| format!("read {}", catalog_path.display()))?;
    let mut catalog: toml::Value = toml::from_str(&catalog_source)
        .with_context(|| format!("parse {}", catalog_path.display()))?;
    add_plugin_catalog_entry(&mut catalog, &names.kebab, &crate_name, &member)?;

    let edits = vec![
        PlannedEdit::update(
            root,
            workspace_path,
            toml::to_string_pretty(&workspace)
                .context("render workspace Cargo.toml")?
                .into_bytes(),
        )?,
        PlannedEdit::create(
            root,
            format!("{member}/Cargo.toml"),
            plugin_manifest.into_bytes(),
        )?,
        PlannedEdit::create(
            root,
            format!("{member}/README.md"),
            render_stub(root, Stub::PluginReadme, &replacements)?,
        )?,
        PlannedEdit::create(
            root,
            format!("{member}/minco-plugin.json"),
            crate::plugin_cmd::default_distribution_record(&names.kebab, &crate_name)?,
        )?,
        PlannedEdit::create(
            root,
            format!("{member}/src/lib.rs"),
            render_stub(root, Stub::PluginSource, &replacements)?,
        )?,
        PlannedEdit::update(
            root,
            catalog_path,
            toml::to_string_pretty(&catalog)
                .context("render plugin catalog")?
                .into_bytes(),
        )?,
    ];
    generation_plan("plugin", &args.name, args.dry_run, None, edits)
}

fn stubs_plan(root: &Path, args: DryRunArgs) -> Result<GenerationPlan> {
    let edits = Stub::all()
        .iter()
        .map(|stub| {
            PlannedEdit::create(
                root,
                Path::new("stubs/minco").join(stub.file_name()),
                stub.default().as_bytes().to_vec(),
            )
        })
        .collect::<Result<Vec<_>>>()?;
    generation_plan("stubs", "defaults", args.dry_run, None, edits)
}

fn generation_plan(
    generator: &'static str,
    name: &str,
    dry_run: bool,
    contract: Option<ContractSelection>,
    mut edits: Vec<PlannedEdit>,
) -> Result<GenerationPlan> {
    edits.sort_by(|left, right| left.path.cmp(&right.path));
    let mut seen = BTreeSet::new();
    for edit in &edits {
        if !seen.insert(edit.path.clone()) {
            bail!("generator planned duplicate target {}", edit.path.display());
        }
    }
    Ok(GenerationPlan {
        schema_version: 1,
        generator,
        name: name.to_owned(),
        dry_run,
        applied: false,
        contract,
        resource: None,
        changes: edits.iter().map(PlannedEdit::summary).collect(),
        edits,
    })
}

fn exactly_one_root<'a>(kind: &str, roots: &'a [PathBuf]) -> Result<&'a Path> {
    match roots {
        [root] => {
            validate_relative_path(root)?;
            Ok(root)
        }
        [] => bail!("make {kind} requires one configured {kind} root"),
        _ => bail!(
            "make {kind} requires exactly one configured {kind} root; select a set explicitly after multi-root generation is reviewed"
        ),
    }
}

fn add_workspace_member_to_document(
    document: &mut toml::Value,
    member: &str,
    crate_name: &str,
) -> Result<()> {
    let root = document
        .as_table_mut()
        .context("Cargo.toml root must be a TOML table")?;
    let workspace = root
        .get_mut("workspace")
        .and_then(toml::Value::as_table_mut)
        .context("Cargo.toml requires a workspace table")?;
    for key in ["members", "default-members"] {
        let values = workspace
            .get_mut(key)
            .and_then(toml::Value::as_array_mut)
            .with_context(|| format!("workspace.{key} must be an array"))?;
        if values.iter().any(|value| value.as_str() == Some(member)) {
            bail!("workspace already contains {member}");
        }
        values.push(toml::Value::String(member.into()));
        values.sort_by(|left, right| left.as_str().cmp(&right.as_str()));
    }
    let dependencies = workspace
        .get_mut("dependencies")
        .and_then(toml::Value::as_table_mut)
        .context("workspace.dependencies must be a TOML table")?;
    if dependencies.contains_key(crate_name) {
        bail!("workspace dependency {crate_name} already exists");
    }
    dependencies.insert(
        crate_name.into(),
        toml::Value::Table(toml::map::Map::from_iter([(
            "path".into(),
            toml::Value::String(member.into()),
        )])),
    );
    Ok(())
}

fn add_plugin_catalog_entry(
    document: &mut toml::Value,
    id: &str,
    crate_name: &str,
    member: &str,
) -> Result<()> {
    let root = document
        .as_table_mut()
        .context("plugin catalog root must be a TOML table")?;
    let plugins = root
        .entry("plugin")
        .or_insert_with(|| toml::Value::Array(Vec::new()))
        .as_array_mut()
        .context("plugin catalog entries must be an array")?;
    if plugins
        .iter()
        .any(|plugin| plugin.get("id").and_then(toml::Value::as_str) == Some(id))
    {
        bail!("plugin catalog already contains {id}");
    }
    plugins.push(toml::Value::Table(toml::map::Map::from_iter([
        ("id".into(), toml::Value::String(id.into())),
        ("crate".into(), toml::Value::String(crate_name.into())),
        ("path".into(), toml::Value::String(member.into())),
        ("kind".into(), toml::Value::String("plugin".into())),
        (
            "feature".into(),
            toml::Value::String(format!("plugin-{id}")),
        ),
        ("default_enabled".into(), toml::Value::Boolean(false)),
        (
            "stability".into(),
            toml::Value::String("experimental".into()),
        ),
        (
            "description".into(),
            toml::Value::String("Application-owned Minco plugin.".into()),
        ),
    ])));
    plugins.sort_by(|left, right| {
        left.get("id")
            .and_then(toml::Value::as_str)
            .cmp(&right.get("id").and_then(toml::Value::as_str))
    });
    Ok(())
}

fn apply(root: &Path, edits: &[PlannedEdit]) -> Result<()> {
    let root = root
        .canonicalize()
        .with_context(|| format!("resolve project root {}", root.display()))?;
    preflight(&root, edits)?;

    for edit in edits {
        if let Some(parent) = root.join(&edit.path).parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("create generator directory {}", parent.display()))?;
        }
    }
    preflight(&root, edits)?;

    let mut temporary_paths = Vec::with_capacity(edits.len());
    for (index, edit) in edits.iter().enumerate() {
        let target = root.join(&edit.path);
        let file_name = target
            .file_name()
            .and_then(|value| value.to_str())
            .context("generator target filename must be UTF-8")?;
        let temporary = target.with_file_name(format!(
            ".{file_name}.minco-{}-{index}.tmp",
            std::process::id()
        ));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .with_context(|| format!("reserve generator temporary file {}", temporary.display()))?;
        if let Err(error) = file.write_all(&edit.after).and_then(|()| file.sync_all()) {
            let _ = fs::remove_file(&temporary);
            cleanup_temporary_files(&temporary_paths);
            return Err(error).context("write generated content");
        }
        temporary_paths.push(temporary);
    }

    if let Err(error) = preflight(&root, edits) {
        cleanup_temporary_files(&temporary_paths);
        return Err(error);
    }
    let mut install_order = (0..edits.len()).collect::<Vec<_>>();
    install_order.sort_by_key(|index| edits[*index].before.is_some());
    let mut installed = Vec::new();
    for index in install_order {
        let edit = &edits[index];
        let target = root.join(&edit.path);
        let result = if edit.before.is_none() {
            install_create_without_clobber(&temporary_paths[index], &target)
        } else {
            fs::rename(&temporary_paths[index], &target)
                .with_context(|| format!("replace reviewed input {}", edit.path.display()))
        };
        if let Err(error) = result {
            cleanup_temporary_files(&temporary_paths);
            let rollback = rollback_installed(&root, edits, &installed);
            return match rollback {
                Ok(()) => Err(error)
                    .with_context(|| format!("install generated file {}", edit.path.display())),
                Err(rollback_error) => bail!(
                    "failed to install generated file {}: {error}; rollback also failed: {rollback_error:#}",
                    edit.path.display()
                ),
            };
        }
        installed.push(index);
    }
    Ok(())
}

fn install_create_without_clobber(source: &Path, target: &Path) -> Result<()> {
    fs::hard_link(source, target).with_context(|| {
        format!(
            "install generated file without overwriting {}",
            target.display()
        )
    })?;
    if let Err(error) = fs::remove_file(source) {
        return match fs::remove_file(target) {
            Ok(()) => Err(error)
                .with_context(|| format!("remove generator temporary file {}", source.display())),
            Err(rollback_error) => bail!(
                "failed to remove generator temporary file {}: {error}; rollback of {} also failed: {rollback_error}",
                source.display(),
                target.display()
            ),
        };
    }
    Ok(())
}

fn preflight(root: &Path, edits: &[PlannedEdit]) -> Result<()> {
    for edit in edits {
        validate_relative_path(&edit.path)?;
        reject_symlink_ancestors(root, &edit.path)?;
        let target = root.join(&edit.path);
        match &edit.before {
            None => match fs::symlink_metadata(&target) {
                Ok(_) => {
                    bail!(
                        "generator refuses to overwrite existing path {}",
                        edit.path.display()
                    );
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(error).with_context(|| {
                        format!("inspect generator target {}", edit.path.display())
                    });
                }
            },
            Some(expected) => {
                let metadata = fs::symlink_metadata(&target).with_context(|| {
                    format!("generator input {} disappeared", edit.path.display())
                })?;
                if metadata.file_type().is_symlink() || !metadata.is_file() {
                    bail!(
                        "generator input {} is no longer a regular file",
                        edit.path.display()
                    );
                }
                let actual = fs::read(&target)
                    .with_context(|| format!("re-read generator input {}", edit.path.display()))?;
                if &actual != expected {
                    bail!(
                        "generator input {} changed after planning; rerun the command",
                        edit.path.display()
                    );
                }
            }
        }
    }
    Ok(())
}

fn cleanup_temporary_files(paths: &[PathBuf]) {
    for path in paths {
        let _ = fs::remove_file(path);
    }
}

fn rollback_installed(root: &Path, edits: &[PlannedEdit], installed: &[usize]) -> Result<()> {
    for index in installed.iter().rev() {
        let edit = &edits[*index];
        let target = root.join(&edit.path);
        match &edit.before {
            None => fs::remove_file(&target)
                .with_context(|| format!("roll back generated file {}", edit.path.display()))?,
            Some(before) => fs::write(&target, before)
                .with_context(|| format!("restore generator input {}", edit.path.display()))?,
        }
    }
    Ok(())
}

fn validate_relative_path(path: &Path) -> Result<()> {
    if path.as_os_str().is_empty()
        || path.is_absolute()
        || !path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
    {
        bail!(
            "generator target must be a normalized project-relative path: {}",
            path.display()
        );
    }
    Ok(())
}

fn reject_symlink_ancestors(root: &Path, relative: &Path) -> Result<()> {
    let mut current = root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(component) = component else {
            unreachable!("relative path was validated");
        };
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                bail!(
                    "generator refuses symlinked path component {}",
                    current.display()
                );
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("inspect generator path {}", current.display()));
            }
        }
    }
    Ok(())
}

fn lower_camel_to_snake(value: &str) -> Result<String> {
    validate_operation_id(value)?;
    let mut output = String::with_capacity(value.len() + 4);
    for byte in value.bytes() {
        if byte.is_ascii_uppercase() {
            output.push('_');
            output.push(char::from(byte.to_ascii_lowercase()));
        } else {
            output.push(char::from(byte));
        }
    }
    Ok(output)
}

fn lower_camel_to_pascal(value: &str) -> Result<String> {
    validate_operation_id(value)?;
    let mut bytes = value.as_bytes().to_vec();
    bytes[0] = bytes[0].to_ascii_uppercase();
    String::from_utf8(bytes).context("operationId must be ASCII")
}

fn validate_operation_id(value: &str) -> Result<()> {
    if value.is_empty()
        || !value.as_bytes()[0].is_ascii_lowercase()
        || !value.bytes().all(|byte| byte.is_ascii_alphanumeric())
    {
        bail!("operationId must be lowerCamelCase ASCII");
    }
    Ok(())
}

fn slash_path(path: &Path) -> String {
    path.iter()
        .map(|component| component.to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

#[derive(Debug, Clone)]
struct GeneratorNames {
    kebab: String,
    snake: String,
    pascal: String,
}

impl GeneratorNames {
    fn new(value: &str) -> Result<Self> {
        validate_kebab_name(value)?;
        let snake = value.replace('-', "_");
        let pascal = value
            .split('-')
            .map(|part| {
                let mut bytes = part.as_bytes().to_vec();
                bytes[0] = bytes[0].to_ascii_uppercase();
                String::from_utf8(bytes).expect("validated generator names are ASCII")
            })
            .collect::<String>();
        Ok(Self {
            kebab: value.to_owned(),
            snake,
            pascal,
        })
    }

    const fn replacements(&self) -> [(&str, &str); 3] {
        [
            ("NAME", self.kebab.as_str()),
            ("SNAKE_NAME", self.snake.as_str()),
            ("PASCAL_NAME", self.pascal.as_str()),
        ]
    }
}

fn validate_kebab_name(value: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 64
        || !value.as_bytes()[0].is_ascii_lowercase()
        || value.ends_with('-')
        || value.contains("--")
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        bail!("generator name must be lower-kebab-case ASCII and at most 64 bytes");
    }
    Ok(())
}

#[derive(Debug, Clone, Copy)]
enum Stub {
    ApplicationTest,
    HttpTest,
    OperationDocumentation,
    ModuleDomain,
    ModuleApplication,
    ModuleDocumentation,
    ModuleTest,
    MigrationSql,
    MigrationDocumentation,
    SeederSql,
    SeederVerifySql,
    SeederDocumentation,
    WorkerSource,
    WorkerTest,
    WorkerDocumentation,
    AdapterSource,
    AdapterTest,
    AdapterDocumentation,
    PluginSource,
    PluginReadme,
}

impl Stub {
    const fn all() -> &'static [Self] {
        &[
            Self::AdapterDocumentation,
            Self::AdapterSource,
            Self::AdapterTest,
            Self::ApplicationTest,
            Self::HttpTest,
            Self::MigrationDocumentation,
            Self::MigrationSql,
            Self::ModuleApplication,
            Self::ModuleDocumentation,
            Self::ModuleDomain,
            Self::ModuleTest,
            Self::OperationDocumentation,
            Self::PluginReadme,
            Self::PluginSource,
            Self::SeederDocumentation,
            Self::SeederSql,
            Self::SeederVerifySql,
            Self::WorkerDocumentation,
            Self::WorkerSource,
            Self::WorkerTest,
        ]
    }

    const fn file_name(self) -> &'static str {
        match self {
            Self::ApplicationTest => "application-test.rs.tmpl",
            Self::HttpTest => "http-test.rs.tmpl",
            Self::OperationDocumentation => "operation.md.tmpl",
            Self::ModuleDomain => "module-domain.rs.tmpl",
            Self::ModuleApplication => "module-application.rs.tmpl",
            Self::ModuleDocumentation => "module.md.tmpl",
            Self::ModuleTest => "module-test.rs.tmpl",
            Self::MigrationSql => "migration.sql.tmpl",
            Self::MigrationDocumentation => "migration.md.tmpl",
            Self::SeederSql => "seeder.sql.tmpl",
            Self::SeederVerifySql => "seeder-verify.sql.tmpl",
            Self::SeederDocumentation => "seeder.md.tmpl",
            Self::WorkerSource => "worker.rs.tmpl",
            Self::WorkerTest => "worker-test.rs.tmpl",
            Self::WorkerDocumentation => "worker.md.tmpl",
            Self::AdapterSource => "adapter.rs.tmpl",
            Self::AdapterTest => "adapter-test.rs.tmpl",
            Self::AdapterDocumentation => "adapter.md.tmpl",
            Self::PluginSource => "plugin-lib.rs.tmpl",
            Self::PluginReadme => "plugin-readme.md.tmpl",
        }
    }

    const fn default(self) -> &'static str {
        match self {
            Self::ApplicationTest => {
                include_str!("../templates/generator/application-test.rs.tmpl")
            }
            Self::HttpTest => include_str!("../templates/generator/http-test.rs.tmpl"),
            Self::OperationDocumentation => {
                include_str!("../templates/generator/operation.md.tmpl")
            }
            Self::ModuleDomain => {
                include_str!("../templates/generator/module-domain.rs.tmpl")
            }
            Self::ModuleApplication => {
                include_str!("../templates/generator/module-application.rs.tmpl")
            }
            Self::ModuleDocumentation => {
                include_str!("../templates/generator/module.md.tmpl")
            }
            Self::ModuleTest => include_str!("../templates/generator/module-test.rs.tmpl"),
            Self::MigrationSql => include_str!("../templates/generator/migration.sql.tmpl"),
            Self::MigrationDocumentation => {
                include_str!("../templates/generator/migration.md.tmpl")
            }
            Self::SeederSql => include_str!("../templates/generator/seeder.sql.tmpl"),
            Self::SeederVerifySql => {
                include_str!("../templates/generator/seeder-verify.sql.tmpl")
            }
            Self::SeederDocumentation => {
                include_str!("../templates/generator/seeder.md.tmpl")
            }
            Self::WorkerSource => include_str!("../templates/generator/worker.rs.tmpl"),
            Self::WorkerTest => include_str!("../templates/generator/worker-test.rs.tmpl"),
            Self::WorkerDocumentation => {
                include_str!("../templates/generator/worker.md.tmpl")
            }
            Self::AdapterSource => include_str!("../templates/generator/adapter.rs.tmpl"),
            Self::AdapterTest => include_str!("../templates/generator/adapter-test.rs.tmpl"),
            Self::AdapterDocumentation => {
                include_str!("../templates/generator/adapter.md.tmpl")
            }
            Self::PluginSource => include_str!("../templates/generator/plugin-lib.rs.tmpl"),
            Self::PluginReadme => {
                include_str!("../templates/generator/plugin-readme.md.tmpl")
            }
        }
    }
}

fn render_stub(root: &Path, stub: Stub, replacements: &[(&str, &str)]) -> Result<Vec<u8>> {
    let custom_relative = Path::new("stubs/minco").join(stub.file_name());
    reject_symlink_ancestors(root, &custom_relative)?;
    let custom_path = root.join(&custom_relative);
    let mut source = match fs::symlink_metadata(&custom_path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                bail!(
                    "custom generator stub {} must be a regular file",
                    custom_path.display()
                );
            }
            fs::read_to_string(&custom_path)
                .with_context(|| format!("read custom generator stub {}", custom_path.display()))?
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => stub.default().to_owned(),
        Err(error) => {
            return Err(error).with_context(|| {
                format!("inspect custom generator stub {}", custom_path.display())
            });
        }
    };
    for (name, value) in replacements {
        source = source.replace(&format!("{{{{{name}}}}}"), value);
    }
    if source.contains("{{") || source.contains("}}") {
        bail!(
            "generator stub {} contains an unknown or unresolved placeholder",
            stub.file_name()
        );
    }
    Ok(source.into_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operation_names_are_converted_without_accepting_path_syntax() {
        assert_eq!(lower_camel_to_snake("placeOrder").unwrap(), "place_order");
        assert_eq!(lower_camel_to_pascal("placeOrder").unwrap(), "PlaceOrder");
        assert!(lower_camel_to_snake("../placeOrder").is_err());
    }

    #[test]
    fn http_stub_uses_an_escaped_literal_for_parameterized_contract_paths() {
        let temporary = tempfile::tempdir().unwrap();
        let operation = OwnedOperation {
            operation_id: "getWidget".into(),
            method: minco_contract::HttpMethod::Get,
            path: "/widgets/{id}/\"quoted\"".into(),
            authenticated: false,
            idempotent: false,
        };

        let rendered =
            render_operation_stub(temporary.path(), Stub::HttpTest, &operation, "GetWidget")
                .unwrap();
        let rendered = String::from_utf8(rendered).unwrap();

        assert!(rendered.contains(r#"let path = "/widgets/{id}/\"quoted\"";"#));
        assert!(rendered.contains("exercise {method} {path} through the Axum router"));
        assert!(!rendered.contains("exercise GET /widgets/{id}"));
    }

    #[test]
    fn changed_update_input_prevents_every_planned_create() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path();
        fs::write(root.join("inventory.toml"), b"schema = 1\n").unwrap();
        let edits = vec![
            PlannedEdit::create(root, "generated.rs", b"generated".to_vec()).unwrap(),
            PlannedEdit::update(root, "inventory.toml", b"schema = 2\n".to_vec()).unwrap(),
        ];
        fs::write(root.join("inventory.toml"), b"schema = 3\n").unwrap();

        let error = apply(root, &edits).unwrap_err();

        assert!(error.to_string().contains("changed after planning"));
        assert!(!root.join("generated.rs").exists());
        assert_eq!(
            fs::read(root.join("inventory.toml")).unwrap(),
            b"schema = 3\n"
        );
    }

    #[test]
    fn create_install_never_replaces_a_concurrent_target() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path();
        let source = root.join(".generated.tmp");
        let target = root.join("generated.rs");
        fs::write(&source, b"planned").unwrap();
        fs::write(&target, b"concurrent").unwrap();

        let error = install_create_without_clobber(&source, &target).unwrap_err();

        assert_eq!(fs::read(&source).unwrap(), b"planned");
        assert_eq!(fs::read(&target).unwrap(), b"concurrent");
        assert!(
            error
                .to_string()
                .contains("install generated file without overwriting")
        );
    }
}
