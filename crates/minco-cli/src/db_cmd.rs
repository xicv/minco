use super::{
    DbCommand, DbMigrateArgs, DbSeedArgs, DbTargetArgs, MincoManifest, canonical_json, print_value,
    vcs,
};
use anyhow::{Context, Result, anyhow, bail};
use chrono::{DateTime, Utc};
use minco_db::{
    DatabaseBackend, MigrationCatalog, MigrationPlan, MigrationRisk, MigrationSet, MigrationState,
    MigrationStatus, SeedClass, SeedEnvironment, SeedPlan, SeedRisk, SeedTransaction,
    SeedVerification, build_plan, build_seed_plan, compare_target, load_catalog, load_seed_catalog,
};
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::{Seek, SeekFrom, Write},
    path::{Component, Path, PathBuf},
};
use uuid::Uuid;

#[derive(Debug, Serialize)]
struct SourceStatus<'a> {
    schema_version: u32,
    plan_digest: &'a str,
    source_catalog_digest: &'a str,
    source_sets: &'a [MigrationSet],
    target_inspected: bool,
}

#[derive(Debug, Serialize)]
struct SourceVerification<'a> {
    schema_version: u32,
    plan_digest: &'a str,
    source_catalog_digest: &'a str,
    source_verified: bool,
    target_inspected: bool,
    target_verified: Option<bool>,
}

#[derive(Debug, Serialize)]
struct SeedSourceVerification<'a> {
    schema_version: u32,
    catalog_digest: &'a str,
    source_verified: bool,
    target_inspected: bool,
    target_verified: Option<bool>,
}

#[derive(Debug, Serialize)]
struct SeedTargetVerification {
    schema_version: u32,
    catalog_digest: String,
    plan_digest: String,
    selected_set: String,
    target_inspected: bool,
    target_verified: bool,
    verification: Vec<SeedVerification>,
}

#[derive(Debug, Clone, Serialize)]
struct SetStatus {
    set_id: String,
    owner: String,
    status: MigrationStatus,
}

#[derive(Debug, Clone, Serialize)]
struct TargetStatus {
    schema_version: u32,
    plan_digest: String,
    source_catalog_digest: String,
    selected_set: String,
    target_inspected: bool,
    sets: Vec<SetStatus>,
}

#[derive(Debug, Clone, Serialize)]
struct SetVerification {
    set_id: String,
    expected_tables: Vec<String>,
    missing_tables: Vec<String>,
    verified: bool,
}

#[derive(Debug, Clone, Serialize)]
struct TargetVerification {
    schema_version: u32,
    plan_digest: String,
    selected_set: String,
    target_inspected: bool,
    target_verified: bool,
    status: Vec<SetStatus>,
    tables: Vec<SetVerification>,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum ReceiptOutcome {
    Started,
    Succeeded,
    Failed,
}

#[derive(Debug, Clone, Serialize)]
struct MigrationReceipt {
    schema_version: u32,
    receipt_id: String,
    created_at: DateTime<Utc>,
    source_change: String,
    catalog_digest: String,
    plan_digest: String,
    selected_set: String,
    backend: DatabaseBackend,
    outcome: ReceiptOutcome,
    failure_code: Option<String>,
    became_applied_versions: BTreeMap<String, Vec<i64>>,
    before: Vec<SetStatus>,
    after: Vec<SetStatus>,
    verification: Vec<SetVerification>,
}

#[derive(Debug, Clone, Serialize)]
struct SeedReceipt {
    schema_version: u32,
    receipt_id: String,
    created_at: DateTime<Utc>,
    source_change: String,
    catalog_digest: String,
    plan_digest: String,
    selected_set: String,
    backend: DatabaseBackend,
    profile: SeedClass,
    environment: SeedEnvironment,
    transaction: SeedTransaction,
    destructive_authorized: bool,
    bootstrap_authorized: bool,
    seed_ids: Vec<String>,
    outcome: ReceiptOutcome,
    failure_code: Option<String>,
    target_verified: bool,
    verification: Vec<SeedVerification>,
}

enum DatabaseTarget {
    Postgres(minco_sqlx_postgres::PgPool),
    Sqlite {
        pool: minco_sqlx_sqlite::SqlitePool,
        config: minco_sqlx_sqlite::SqlitePoolConfig,
    },
}

impl DatabaseTarget {
    async fn connect(backend: DatabaseBackend, database_url: String) -> Result<Self> {
        match backend {
            DatabaseBackend::Postgres => {
                if !database_url.starts_with("postgres://")
                    && !database_url.starts_with("postgresql://")
                {
                    bail!("configured database target is not a PostgreSQL URL");
                }
                let config = minco_sqlx_postgres::PostgresPoolConfig {
                    url: database_url,
                    max_connections: 1,
                    acquire_timeout_seconds: 10,
                    idle_timeout_seconds: 60,
                };
                let pool = minco_sqlx_postgres::connect(&config)
                    .await
                    .map_err(|_| anyhow!("could not connect to the PostgreSQL database target"))?;
                Ok(Self::Postgres(pool))
            }
            DatabaseBackend::Sqlite => {
                if !database_url.starts_with("sqlite:") {
                    bail!("configured database target is not a SQLite URL");
                }
                let config = minco_sqlx_sqlite::SqlitePoolConfig {
                    url: database_url,
                    max_connections: 1,
                    acquire_timeout_seconds: 10,
                };
                let pool = minco_sqlx_sqlite::connect(&config)
                    .await
                    .map_err(|_| anyhow!("could not connect to the SQLite database target"))?;
                Ok(Self::Sqlite { pool, config })
            }
        }
    }

    async fn state(&self, set: &MigrationSet) -> Result<minco_db::TargetState> {
        match self {
            Self::Postgres(pool) => minco_sqlx_postgres::migration_target_state(pool, set)
                .await
                .map_err(Into::into),
            Self::Sqlite { pool, .. } => minco_sqlx_sqlite::migration_target_state(pool, set)
                .await
                .map_err(Into::into),
        }
    }

    async fn missing_tables(&self, set: &MigrationSet) -> Result<Vec<String>> {
        match self {
            Self::Postgres(pool) => minco_sqlx_postgres::verify_migration_tables(pool, set)
                .await
                .map_err(Into::into),
            Self::Sqlite { pool, .. } => minco_sqlx_sqlite::verify_migration_tables(pool, set)
                .await
                .map_err(Into::into),
        }
    }

    async fn migrate_plan(&self, root: &Path, sets: &[MigrationSet]) -> Result<()> {
        match self {
            Self::Postgres(pool) => minco_sqlx_postgres::apply_migration_plan(pool, root, sets)
                .await
                .map_err(Into::into),
            Self::Sqlite { pool, config } => {
                minco_sqlx_sqlite::apply_migration_plan(pool, config, root, sets)
                    .await
                    .map_err(Into::into)
            }
        }
    }

    async fn apply_seed_plan(&self, root: &Path, plan: &SeedPlan) -> Result<()> {
        match self {
            Self::Postgres(pool) => minco_sqlx_postgres::apply_seed_plan(pool, root, plan)
                .await
                .map_err(Into::into),
            Self::Sqlite { pool, .. } => minco_sqlx_sqlite::apply_seed_plan(pool, root, plan)
                .await
                .map_err(Into::into),
        }
    }

    async fn verify_seed_plan(
        &self,
        root: &Path,
        plan: &SeedPlan,
    ) -> Result<Vec<SeedVerification>> {
        match self {
            Self::Postgres(pool) => minco_sqlx_postgres::verify_seed_plan(pool, root, plan)
                .await
                .map_err(Into::into),
            Self::Sqlite { pool, .. } => minco_sqlx_sqlite::verify_seed_plan(pool, root, plan)
                .await
                .map_err(Into::into),
        }
    }
}

pub async fn execute(
    root: &Path,
    manifest: &MincoManifest,
    command: DbCommand,
    as_json: bool,
) -> Result<()> {
    let catalog = load_catalog(root, &manifest.migrations.roots)?;
    match command {
        DbCommand::Plan { set } => {
            let plan = build_plan(&catalog, set.as_deref())?;
            print_value(&plan, as_json)
        }
        DbCommand::Status(args) => status(&catalog, args, as_json).await,
        DbCommand::Verify(args) => verify(&catalog, args, as_json).await,
        DbCommand::Migrate(args) => migrate(root, &catalog, args, as_json).await,
        DbCommand::Seed(args) => seed(root, manifest, args, as_json).await,
    }
}

async fn seed(
    root: &Path,
    manifest: &MincoManifest,
    args: DbSeedArgs,
    as_json: bool,
) -> Result<()> {
    let catalog = load_seed_catalog(root, &manifest.seeds.roots)?;
    if args.verify && args.database_url_env.is_none() {
        if args.profile.is_some()
            || args.environment.is_some()
            || args.set.is_some()
            || args.expected_plan_digest.is_some()
            || args.receipt.is_some()
            || args.dry_run
            || args.allow_destructive
            || args.authorize_bootstrap.is_some()
        {
            bail!("source seed verification cannot include planning or execution arguments");
        }
        return print_value(
            &SeedSourceVerification {
                schema_version: 1,
                catalog_digest: &catalog.digest,
                source_verified: true,
                target_inspected: false,
                target_verified: None,
            },
            as_json,
        );
    }
    if args.verify {
        if args.expected_plan_digest.is_some()
            || args.receipt.is_some()
            || args.dry_run
            || args.allow_destructive
            || args.authorize_bootstrap.is_some()
        {
            bail!("target seed verification cannot include execution arguments");
        }
        let profile = parse_seed_class(
            args.profile
                .as_deref()
                .context("target seed verification requires --profile")?,
        )?;
        let environment = requested_seed_environment(&args)?;
        let selected_set = args
            .set
            .as_deref()
            .context("target seed verification requires --set")?;
        let plan = build_seed_plan(&catalog, profile, environment, Some(selected_set))?;
        let backend = seed_plan_backend(&plan)?;
        let database_url_env = args
            .database_url_env
            .as_deref()
            .context("target seed verification requires --database-url-env")?;
        let database_url = database_url_from_environment(database_url_env)?;
        let target = DatabaseTarget::connect(backend, database_url).await?;
        let verification = target.verify_seed_plan(root, &plan).await?;
        let target_verified = verification.iter().all(|entry| entry.verified);
        let report = SeedTargetVerification {
            schema_version: 1,
            catalog_digest: plan.catalog_digest.clone(),
            plan_digest: plan.digest.clone(),
            selected_set: selected_set.to_owned(),
            target_inspected: true,
            target_verified,
            verification,
        };
        print_value(&report, as_json)?;
        if !target_verified {
            bail!("database seed verification failed for set {selected_set}");
        }
        return Ok(());
    }
    if args.dry_run {
        if args.database_url_env.is_some()
            || args.expected_plan_digest.is_some()
            || args.receipt.is_some()
            || args.allow_destructive
            || args.authorize_bootstrap.is_some()
        {
            bail!("seed dry-run cannot include execution or verification arguments");
        }
        let profile = parse_seed_class(
            args.profile
                .as_deref()
                .context("seed dry-run requires --profile")?,
        )?;
        let environment = requested_seed_environment(&args)?;
        let plan = build_seed_plan(&catalog, profile, environment, args.set.as_deref())?;
        return print_value(&plan, as_json);
    }
    let profile = parse_seed_class(
        args.profile
            .as_deref()
            .context("seed execution requires --profile")?,
    )?;
    let environment = requested_seed_environment(&args)?;
    let selected_set = args
        .set
        .as_deref()
        .context("seed execution requires --set")?;
    let plan = build_seed_plan(&catalog, profile, environment, Some(selected_set))?;
    let expected_digest = args
        .expected_plan_digest
        .as_deref()
        .context("seed execution requires --expected-plan-digest")?;
    if plan.digest != expected_digest {
        bail!(
            "seed plan digest changed; rerun `minco db seed --profile {} --environment {} --set {} --dry-run`",
            seed_class_name(profile),
            seed_environment_name(environment),
            selected_set
        );
    }
    let database_url_env = args
        .database_url_env
        .as_deref()
        .context("seed execution requires --database-url-env")?;
    let receipt_path = args
        .receipt
        .as_deref()
        .context("seed execution requires --receipt")?;
    let contains_destructive = plan
        .seeds
        .iter()
        .any(|seed| seed.risk == SeedRisk::Destructive);
    if contains_destructive && !args.allow_destructive {
        bail!("seed plan contains a destructive seed and requires --allow-destructive");
    }
    let contains_bootstrap = plan
        .seeds
        .iter()
        .any(|seed| seed.class == SeedClass::Bootstrap);
    let bootstrap_authorized = if contains_bootstrap {
        let environment_name = seed_environment_name(environment);
        if args.authorize_bootstrap.as_deref() != Some(environment_name) {
            bail!("bootstrap seed execution requires --authorize-bootstrap {environment_name}");
        }
        true
    } else {
        if args.authorize_bootstrap.is_some() {
            bail!("--authorize-bootstrap is valid only for the bootstrap seed profile");
        }
        false
    };
    let backend = seed_plan_backend(&plan)?;
    let database_url = database_url_from_environment(database_url_env)?;
    let target = DatabaseTarget::connect(backend, database_url).await?;
    let mut receipt = SeedReceipt {
        schema_version: 1,
        receipt_id: Uuid::now_v7().to_string(),
        created_at: Utc::now(),
        source_change: vcs::source_change(root)?,
        catalog_digest: plan.catalog_digest.clone(),
        plan_digest: plan.digest.clone(),
        selected_set: selected_set.to_owned(),
        backend,
        profile,
        environment,
        transaction: plan.seeds[0].transaction,
        destructive_authorized: args.allow_destructive,
        bootstrap_authorized,
        seed_ids: plan.seeds.iter().map(|seed| seed.id.clone()).collect(),
        outcome: ReceiptOutcome::Started,
        failure_code: None,
        target_verified: false,
        verification: Vec::new(),
    };
    let mut writer = ReceiptWriter::reserve(root, receipt_path, &receipt)?;
    if let Err(error) = target.apply_seed_plan(root, &plan).await {
        receipt.outcome = ReceiptOutcome::Failed;
        receipt.failure_code = Some("seed_execution_failed".into());
        writer.update(&receipt)?;
        return Err(error).context("apply database seed plan");
    }
    let verification = match target.verify_seed_plan(root, &plan).await {
        Ok(verification) => verification,
        Err(error) => {
            receipt.outcome = ReceiptOutcome::Failed;
            receipt.failure_code = Some("seed_verification_failed".into());
            writer.update(&receipt)?;
            return Err(error).context("verify database seed plan");
        }
    };
    receipt.target_verified = verification.iter().all(|entry| entry.verified);
    receipt.verification = verification;
    if receipt.target_verified {
        receipt.outcome = ReceiptOutcome::Succeeded;
    } else {
        receipt.outcome = ReceiptOutcome::Failed;
        receipt.failure_code = Some("seed_verification_failed".into());
    }
    writer.update(&receipt)?;
    print_value(&receipt, as_json)?;
    if !receipt.target_verified {
        bail!("database seed verification failed for set {selected_set}");
    }
    Ok(())
}

fn seed_plan_backend(plan: &SeedPlan) -> Result<DatabaseBackend> {
    let Some(first) = plan.seeds.first() else {
        bail!("seed plan contains no seeds");
    };
    if plan.seeds.iter().any(|seed| seed.backend != first.backend) {
        bail!("an executable seed plan cannot mix database backends");
    }
    Ok(first.backend)
}

fn parse_seed_class(value: &str) -> Result<SeedClass> {
    match value {
        "reference" => Ok(SeedClass::Reference),
        "demo" => Ok(SeedClass::Demo),
        "test" => Ok(SeedClass::Test),
        "bootstrap" => Ok(SeedClass::Bootstrap),
        _ => bail!("seed profile must be reference, demo, test, or bootstrap"),
    }
}

fn parse_seed_environment(value: &str) -> Result<SeedEnvironment> {
    match value {
        "local" => Ok(SeedEnvironment::Local),
        "development" => Ok(SeedEnvironment::Development),
        "test" => Ok(SeedEnvironment::Test),
        "staging" => Ok(SeedEnvironment::Staging),
        "production" => Ok(SeedEnvironment::Production),
        _ => bail!("seed environment must be local, development, test, staging, or production"),
    }
}

fn requested_seed_environment(args: &DbSeedArgs) -> Result<SeedEnvironment> {
    parse_seed_environment(args.environment.as_deref().unwrap_or("local"))
}

const fn seed_class_name(value: SeedClass) -> &'static str {
    match value {
        SeedClass::Reference => "reference",
        SeedClass::Demo => "demo",
        SeedClass::Test => "test",
        SeedClass::Bootstrap => "bootstrap",
    }
}

const fn seed_environment_name(value: SeedEnvironment) -> &'static str {
    match value {
        SeedEnvironment::Local => "local",
        SeedEnvironment::Development => "development",
        SeedEnvironment::Test => "test",
        SeedEnvironment::Staging => "staging",
        SeedEnvironment::Production => "production",
    }
}

async fn status(catalog: &MigrationCatalog, args: DbTargetArgs, as_json: bool) -> Result<()> {
    let plan = build_plan(catalog, args.set.as_deref())?;
    let Some(environment_name) = args.database_url_env else {
        return print_value(
            &SourceStatus {
                schema_version: 1,
                plan_digest: &plan.digest,
                source_catalog_digest: &plan.catalog_digest,
                source_sets: &plan.sets,
                target_inspected: false,
            },
            as_json,
        );
    };
    let selected_set = require_selected_set(&plan)?;
    let target = connect_target(&plan, &environment_name).await?;
    let report = inspect_target(&target, &plan, selected_set).await?;
    print_value(&report, as_json)
}

async fn verify(catalog: &MigrationCatalog, args: DbTargetArgs, as_json: bool) -> Result<()> {
    let plan = build_plan(catalog, args.set.as_deref())?;
    let Some(environment_name) = args.database_url_env else {
        return print_value(
            &SourceVerification {
                schema_version: 1,
                plan_digest: &plan.digest,
                source_catalog_digest: &plan.catalog_digest,
                source_verified: true,
                target_inspected: false,
                target_verified: None,
            },
            as_json,
        );
    };
    let selected_set = require_selected_set(&plan)?;
    let target = connect_target(&plan, &environment_name).await?;
    let status = inspect_target(&target, &plan, selected_set).await?;
    let verification = verify_target(&target, &plan, &status).await?;
    print_value(&verification, as_json)?;
    if !verification.target_verified {
        bail!("database target verification failed for set {selected_set}");
    }
    Ok(())
}

async fn migrate(
    root: &Path,
    catalog: &MigrationCatalog,
    args: DbMigrateArgs,
    as_json: bool,
) -> Result<()> {
    let plan = build_plan(catalog, Some(&args.set))?;
    if plan.digest != args.expected_plan_digest {
        bail!(
            "migration plan digest changed; rerun `minco db plan --set {}`",
            args.set
        );
    }
    let selected_set = require_selected_set(&plan)?;
    let target = connect_target(&plan, &args.database_url_env).await?;
    let before = inspect_target(&target, &plan, selected_set).await?;
    reject_unsafe_target_state(&before)?;
    if !args.allow_destructive {
        let risky = pending_risky_migrations(&plan, &before);
        if !risky.is_empty() {
            bail!(
                "migration plan contains gated data-rewrite or destructive migrations: {}",
                risky.join(", ")
            );
        }
    }

    let mut receipt = MigrationReceipt {
        schema_version: 1,
        receipt_id: Uuid::now_v7().to_string(),
        created_at: Utc::now(),
        source_change: vcs::source_change(root)?,
        catalog_digest: plan.catalog_digest.clone(),
        plan_digest: plan.digest.clone(),
        selected_set: selected_set.to_owned(),
        backend: plan_backend(&plan)?,
        outcome: ReceiptOutcome::Started,
        failure_code: None,
        became_applied_versions: BTreeMap::new(),
        before: before.sets.clone(),
        after: Vec::new(),
        verification: Vec::new(),
    };
    let mut writer = ReceiptWriter::reserve(root, &args.receipt, &receipt)?;

    if let Err(error) = target.migrate_plan(root, &plan.sets).await {
        receipt.outcome = ReceiptOutcome::Failed;
        receipt.failure_code = Some("migration_execution_failed".into());
        if let Ok(after) = inspect_target(&target, &plan, selected_set).await {
            receipt.after = after.sets;
            receipt.became_applied_versions =
                became_applied_versions(&receipt.before, &receipt.after);
        }
        writer.update(&receipt)?;
        return Err(error).context("migrate database plan");
    }

    let after = match inspect_target(&target, &plan, selected_set).await {
        Ok(after) => after,
        Err(error) => {
            receipt.outcome = ReceiptOutcome::Failed;
            receipt.failure_code = Some("post_migration_status_failed".into());
            writer.update(&receipt)?;
            return Err(error).context("inspect post-migration target state");
        }
    };
    receipt.after = after.sets.clone();
    receipt.became_applied_versions = became_applied_versions(&receipt.before, &receipt.after);
    let verification = match verify_target(&target, &plan, &after).await {
        Ok(verification) => verification,
        Err(error) => {
            receipt.outcome = ReceiptOutcome::Failed;
            receipt.failure_code = Some("post_migration_verification_failed".into());
            writer.update(&receipt)?;
            return Err(error).context("verify post-migration target state");
        }
    };
    receipt.verification = verification.tables;
    if verification.target_verified {
        receipt.outcome = ReceiptOutcome::Succeeded;
    } else {
        receipt.outcome = ReceiptOutcome::Failed;
        receipt.failure_code = Some("post_migration_verification_failed".into());
    }
    writer.update(&receipt)?;
    print_value(&receipt, as_json)?;
    if !verification.target_verified {
        bail!("post-migration verification failed for set {selected_set}");
    }
    Ok(())
}

async fn connect_target(plan: &MigrationPlan, environment_name: &str) -> Result<DatabaseTarget> {
    let backend = plan_backend(plan)?;
    let database_url = database_url_from_environment(environment_name)?;
    DatabaseTarget::connect(backend, database_url).await
}

fn plan_backend(plan: &MigrationPlan) -> Result<DatabaseBackend> {
    let Some(first) = plan.sets.first() else {
        bail!("migration plan contains no sets");
    };
    if plan.sets.iter().any(|set| set.backend != first.backend) {
        bail!("an executable migration plan cannot mix database backends");
    }
    Ok(first.backend)
}

fn require_selected_set(plan: &MigrationPlan) -> Result<&str> {
    plan.selected_set
        .as_deref()
        .context("database target operations require --set")
}

fn database_url_from_environment(name: &str) -> Result<String> {
    let mut bytes = name.bytes();
    let valid = bytes
        .next()
        .is_some_and(|byte| byte.is_ascii_uppercase() || byte == b'_')
        && bytes.all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_');
    if !valid || name.len() > 128 {
        bail!("database URL environment variable name is invalid");
    }
    std::env::var(name)
        .map_err(|_| anyhow!("database URL environment variable {name} is unavailable"))
}

async fn inspect_target(
    target: &DatabaseTarget,
    plan: &MigrationPlan,
    selected_set: &str,
) -> Result<TargetStatus> {
    let mut sets = Vec::new();
    for set in &plan.sets {
        let state = target
            .state(set)
            .await
            .with_context(|| format!("inspect migration state for set {}", set.id))?;
        sets.push(SetStatus {
            set_id: set.id.clone(),
            owner: set.owner.clone(),
            status: compare_target(set, &state),
        });
    }
    Ok(TargetStatus {
        schema_version: 1,
        plan_digest: plan.digest.clone(),
        source_catalog_digest: plan.catalog_digest.clone(),
        selected_set: selected_set.to_owned(),
        target_inspected: true,
        sets,
    })
}

async fn verify_target(
    target: &DatabaseTarget,
    plan: &MigrationPlan,
    status: &TargetStatus,
) -> Result<TargetVerification> {
    let mut tables = Vec::new();
    for (set, set_status) in plan.sets.iter().zip(&status.sets) {
        let missing_tables = target
            .missing_tables(set)
            .await
            .with_context(|| format!("verify tables for migration set {}", set.id))?;
        let state_verified = status_is_fully_applied(&set_status.status);
        tables.push(SetVerification {
            set_id: set.id.clone(),
            expected_tables: set.verify_tables.clone(),
            verified: state_verified && missing_tables.is_empty(),
            missing_tables,
        });
    }
    let target_verified = tables.iter().all(|set| set.verified);
    Ok(TargetVerification {
        schema_version: 1,
        plan_digest: plan.digest.clone(),
        selected_set: status.selected_set.clone(),
        target_inspected: true,
        target_verified,
        status: status.sets.clone(),
        tables,
    })
}

fn status_is_fully_applied(status: &MigrationStatus) -> bool {
    status.dirty_version.is_none()
        && status
            .entries
            .iter()
            .all(|entry| entry.state == MigrationState::Applied)
}

fn reject_unsafe_target_state(status: &TargetStatus) -> Result<()> {
    for set in &status.sets {
        if let Some(version) = set.status.dirty_version {
            bail!(
                "migration set {} has dirty migration version {version}",
                set.set_id
            );
        }
        if set.status.entries.iter().any(|entry| {
            matches!(
                entry.state,
                MigrationState::Drift | MigrationState::MissingSource
            )
        }) {
            bail!(
                "migration set {} has checksum drift or missing source history",
                set.set_id
            );
        }
    }
    Ok(())
}

fn pending_risky_migrations(plan: &MigrationPlan, status: &TargetStatus) -> Vec<String> {
    plan.sets
        .iter()
        .zip(&status.sets)
        .flat_map(|(set, set_status)| {
            let pending = set_status
                .status
                .entries
                .iter()
                .filter(|entry| entry.state == MigrationState::Pending)
                .map(|entry| entry.version)
                .collect::<BTreeSet<_>>();
            set.migrations
                .iter()
                .filter(move |migration| {
                    pending.contains(&migration.version)
                        && matches!(
                            migration.risk,
                            MigrationRisk::DataRewrite | MigrationRisk::Destructive
                        )
                })
                .map(|migration| migration.id.clone())
        })
        .collect()
}

fn became_applied_versions(
    before: &[SetStatus],
    after: &[SetStatus],
) -> BTreeMap<String, Vec<i64>> {
    let before_applied = before
        .iter()
        .map(|set| {
            (
                set.set_id.as_str(),
                set.status
                    .entries
                    .iter()
                    .filter(|entry| entry.state == MigrationState::Applied)
                    .map(|entry| entry.version)
                    .collect::<BTreeSet<_>>(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    after
        .iter()
        .filter_map(|set| {
            let previous = before_applied
                .get(set.set_id.as_str())
                .cloned()
                .unwrap_or_default();
            let applied = set
                .status
                .entries
                .iter()
                .filter(|entry| {
                    entry.state == MigrationState::Applied && !previous.contains(&entry.version)
                })
                .map(|entry| entry.version)
                .collect::<Vec<_>>();
            (!applied.is_empty()).then(|| (set.set_id.clone(), applied))
        })
        .collect()
}

struct ReceiptWriter {
    file: File,
}

impl ReceiptWriter {
    fn reserve<T: Serialize>(root: &Path, relative_path: &Path, receipt: &T) -> Result<Self> {
        let destination = receipt_destination(root, relative_path)?;
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&destination)
            .with_context(|| format!("create new database receipt {}", relative_path.display()))?;
        file.write_all(&canonical_json(receipt)?)?;
        file.sync_all()?;
        Ok(Self { file })
    }

    fn update<T: Serialize>(&mut self, receipt: &T) -> Result<()> {
        self.file.seek(SeekFrom::Start(0))?;
        self.file.set_len(0)?;
        self.file.write_all(&canonical_json(receipt)?)?;
        self.file.sync_all()?;
        Ok(())
    }
}

fn receipt_destination(root: &Path, relative_path: &Path) -> Result<PathBuf> {
    if relative_path.as_os_str().is_empty()
        || relative_path.is_absolute()
        || relative_path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        bail!("database receipt path must stay within the project");
    }
    let root = root.canonicalize().context("resolve project root")?;
    let destination = root.join(relative_path);
    let parent = destination
        .parent()
        .context("database receipt path has no parent")?;
    let mut existing_ancestor = parent;
    while !existing_ancestor.exists() {
        existing_ancestor = existing_ancestor
            .parent()
            .context("database receipt path has no existing project ancestor")?;
    }
    let existing_ancestor = existing_ancestor
        .canonicalize()
        .context("resolve database receipt ancestor")?;
    if !existing_ancestor.starts_with(&root) {
        bail!("database receipt path escapes the project");
    }
    fs::create_dir_all(parent).context("create database receipt directory")?;
    let parent = parent
        .canonicalize()
        .context("resolve database receipt directory")?;
    if !parent.starts_with(&root) {
        bail!("database receipt path escapes the project");
    }
    let file_name = destination
        .file_name()
        .context("database receipt path has no file name")?;
    Ok(parent.join(file_name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn receipt_destination_rejects_escape_and_existing_receipts() {
        let root = TempDir::new().expect("temporary project");
        assert!(receipt_destination(root.path(), Path::new("../receipt.json")).is_err());
        assert!(receipt_destination(root.path(), Path::new("/tmp/receipt.json")).is_err());

        let receipt_path = Path::new("target/minco/receipt.json");
        let destination =
            receipt_destination(root.path(), receipt_path).expect("bounded receipt destination");
        fs::write(&destination, "{}").expect("existing receipt");
        assert!(
            OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(destination)
                .is_err()
        );
    }

    #[cfg(unix)]
    #[test]
    fn receipt_destination_rejects_an_existing_parent_symlink_escape() {
        use std::os::unix::fs::symlink;

        let root = TempDir::new().expect("temporary project");
        let outside = TempDir::new().expect("outside directory");
        symlink(outside.path(), root.path().join("receipts")).expect("create escaping symlink");

        assert!(receipt_destination(root.path(), Path::new("receipts/result.json")).is_err());
        assert!(!outside.path().join("result.json").exists());
    }

    #[test]
    fn environment_variable_names_are_strict_and_values_are_not_accepted_as_names() {
        assert!(database_url_from_environment("database-url").is_err());
        assert!(database_url_from_environment("sqlite://secret.db").is_err());
    }
}
