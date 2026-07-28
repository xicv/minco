//! Bounded `SQLx` `PostgreSQL` pools for local servers and serverless runtimes.
#![forbid(unsafe_code)]

use minco_db::{
    AppliedMigration, DatabaseBackend, MigrationSet, SeedPlan, SeedTransaction, SeedVerification,
    TargetState, resolve_seed_source, validate_seed_plan as validate_seed_model_plan,
};
use serde::{Deserialize, Serialize};
pub use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use thiserror::Error;

pub mod plugin_adapters;

const MINCO_PLAN_LOCK_ID: i64 = 0x4d49_4e43_4f5f_504c;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PostgresPoolConfig {
    pub url: String,
    pub max_connections: u32,
    pub acquire_timeout_seconds: u64,
    pub idle_timeout_seconds: u64,
}

impl std::fmt::Debug for PostgresPoolConfig {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PostgresPoolConfig")
            .field("url", &"[REDACTED DATABASE URL]")
            .field("max_connections", &self.max_connections)
            .field("acquire_timeout_seconds", &self.acquire_timeout_seconds)
            .field("idle_timeout_seconds", &self.idle_timeout_seconds)
            .finish()
    }
}

impl PostgresPoolConfig {
    pub fn serverless(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            max_connections: 2,
            acquire_timeout_seconds: 5,
            idle_timeout_seconds: 60,
        }
    }
    pub fn validate(&self) -> Result<(), PostgresError> {
        if self.url.trim().is_empty() {
            return Err(PostgresError::InvalidConfig("database URL is empty".into()));
        }
        if self.max_connections == 0 {
            return Err(PostgresError::InvalidConfig(
                "max_connections must be at least 1".into(),
            ));
        }
        Ok(())
    }
}

pub async fn connect(config: &PostgresPoolConfig) -> Result<PgPool, PostgresError> {
    config.validate()?;
    Ok(PgPoolOptions::new()
        .min_connections(0)
        .max_connections(config.max_connections)
        .acquire_timeout(Duration::from_secs(config.acquire_timeout_seconds))
        .idle_timeout(Some(Duration::from_secs(config.idle_timeout_seconds)))
        .connect(&config.url)
        .await?)
}

pub fn connect_lazy(config: &PostgresPoolConfig) -> Result<PgPool, PostgresError> {
    config.validate()?;
    Ok(PgPoolOptions::new()
        .min_connections(0)
        .max_connections(config.max_connections)
        .acquire_timeout(Duration::from_secs(config.acquire_timeout_seconds))
        .idle_timeout(Some(Duration::from_secs(config.idle_timeout_seconds)))
        .connect_lazy(&config.url)?)
}

pub async fn migrate(pool: &PgPool, path: impl AsRef<Path>) -> Result<(), PostgresError> {
    let migrator = sqlx::migrate::Migrator::new(path.as_ref()).await?;
    migrator.run(pool).await?;
    Ok(())
}

pub async fn migrate_with_history_table(
    pool: &PgPool,
    path: impl AsRef<Path>,
    history_table: &'static str,
) -> Result<(), PostgresError> {
    validate_identifier(history_table, "migration history table")?;
    let mut migrator = sqlx::migrate::Migrator::new(path.as_ref()).await?;
    migrator.dangerous_set_table_name(history_table);
    migrator.run(pool).await?;
    Ok(())
}

pub async fn migration_target_state(
    pool: &PgPool,
    set: &MigrationSet,
) -> Result<TargetState, PostgresError> {
    validate_set(set)?;
    if !table_exists(pool, &set.history_table).await? {
        return Ok(TargetState::default());
    }
    let dirty_query = format!(
        "SELECT version FROM {} WHERE success = false ORDER BY version LIMIT 1",
        set.history_table
    );
    // The only interpolated token was accepted by `validate_identifier`.
    let dirty_version = sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(dirty_query))
        .fetch_optional(pool)
        .await?;
    let applied_query = format!(
        "SELECT version, checksum FROM {} WHERE success = true ORDER BY version",
        set.history_table
    );
    // The only interpolated token was accepted by `validate_identifier`.
    let applied = sqlx::query_as::<_, (i64, Vec<u8>)>(sqlx::AssertSqlSafe(applied_query))
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|(version, checksum)| AppliedMigration {
            version,
            sqlx_checksum_sha384: hex(&checksum),
        })
        .collect();
    Ok(TargetState {
        dirty_version,
        applied,
    })
}

pub async fn verify_migration_tables(
    pool: &PgPool,
    set: &MigrationSet,
) -> Result<Vec<String>, PostgresError> {
    validate_set(set)?;
    let mut missing = Vec::new();
    for table in &set.verify_tables {
        if !table_exists(pool, table).await? {
            missing.push(table.clone());
        }
    }
    Ok(missing)
}

pub async fn apply_migration_set(
    pool: &PgPool,
    project_root: &Path,
    set: &MigrationSet,
) -> Result<(), PostgresError> {
    apply_migration_plan(pool, project_root, std::slice::from_ref(set)).await
}

pub async fn apply_migration_plan(
    pool: &PgPool,
    project_root: &Path,
    sets: &[MigrationSet],
) -> Result<(), PostgresError> {
    if sets.is_empty() {
        return Err(PostgresError::InvalidConfig(
            "migration plan contains no sets".into(),
        ));
    }
    let mut migrators = Vec::with_capacity(sets.len());
    for set in sets {
        validate_set(set)?;
        let root = migration_root(project_root, set)?;
        let mut migrator = sqlx::migrate::Migrator::new(root).await?;
        verify_resolved_migrations(&migrator, set)?;
        migrator.dangerous_set_table_name(set.history_table.clone());
        migrators.push(migrator);
    }

    let mut connection = pool.acquire().await?;
    // SQLx's migrator returns early on dirty/checksum/execution failures before
    // its normal unlock call. Closing this session on every exit guarantees
    // that PostgreSQL releases both the plan and SQLx advisory locks.
    connection.close_on_drop();
    sqlx::query("SELECT pg_advisory_lock($1)")
        .bind(MINCO_PLAN_LOCK_ID)
        .execute(&mut *connection)
        .await?;
    for migrator in migrators {
        migrator.run_direct(None, &mut *connection, false).await?;
    }
    let unlocked = sqlx::query_scalar::<_, bool>("SELECT pg_advisory_unlock($1)")
        .bind(MINCO_PLAN_LOCK_ID)
        .fetch_one(&mut *connection)
        .await?;
    if !unlocked {
        return Err(PostgresError::PlanLockLost);
    }
    Ok(())
}

pub async fn apply_seed_plan(
    pool: &PgPool,
    project_root: &Path,
    plan: &SeedPlan,
) -> Result<(), PostgresError> {
    validate_seed_plan(plan)?;
    let sources = plan
        .seeds
        .iter()
        .map(|seed| resolve_seed_source(project_root, seed))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| PostgresError::SeedSource(error.to_string()))?;
    match plan.seeds[0].transaction {
        SeedTransaction::Required => {
            let mut transaction = pool.begin().await?;
            for source in sources {
                sqlx::raw_sql(sqlx::AssertSqlSafe(source.apply_sql))
                    .execute(&mut *transaction)
                    .await?;
            }
            transaction.commit().await?;
        }
        SeedTransaction::Autocommit => {
            for source in sources {
                sqlx::raw_sql(sqlx::AssertSqlSafe(source.apply_sql))
                    .execute(pool)
                    .await?;
            }
        }
    }
    Ok(())
}

pub async fn verify_seed_plan(
    pool: &PgPool,
    project_root: &Path,
    plan: &SeedPlan,
) -> Result<Vec<SeedVerification>, PostgresError> {
    validate_seed_plan(plan)?;
    let mut transaction = pool.begin().await?;
    sqlx::query("SET TRANSACTION READ ONLY")
        .execute(&mut *transaction)
        .await?;
    let mut verification = Vec::with_capacity(plan.seeds.len());
    for seed in &plan.seeds {
        let source = resolve_seed_source(project_root, seed)
            .map_err(|error| PostgresError::SeedSource(error.to_string()))?;
        let rows = sqlx::query_scalar::<_, bool>(sqlx::AssertSqlSafe(source.verify_sql))
            .fetch_all(&mut *transaction)
            .await?;
        if rows.len() != 1 {
            return Err(PostgresError::InvalidConfig(format!(
                "seed {} verification must return exactly one boolean row",
                seed.id
            )));
        }
        verification.push(SeedVerification {
            seed_id: seed.id.clone(),
            verified: rows[0],
        });
    }
    transaction.rollback().await?;
    Ok(verification)
}

pub async fn ready(pool: &PgPool) -> bool {
    matches!(
        sqlx::query_scalar::<_, i32>("SELECT 1")
            .fetch_one(pool)
            .await,
        Ok(1)
    )
}

fn validate_seed_plan(plan: &SeedPlan) -> Result<(), PostgresError> {
    validate_seed_model_plan(plan).map_err(|error| PostgresError::SeedSource(error.to_string()))?;
    if plan.seeds.is_empty() {
        return Err(PostgresError::InvalidConfig(
            "seed plan contains no seeds".into(),
        ));
    }
    if plan
        .seeds
        .iter()
        .any(|seed| seed.backend != DatabaseBackend::Postgres)
    {
        return Err(PostgresError::InvalidConfig(
            "seed plan contains a non-PostgreSQL seed".into(),
        ));
    }
    if plan
        .seeds
        .iter()
        .any(|seed| seed.transaction != plan.seeds[0].transaction)
    {
        return Err(PostgresError::InvalidConfig(
            "seed plan mixes transaction behaviors".into(),
        ));
    }
    Ok(())
}

async fn table_exists(pool: &PgPool, table: &str) -> Result<bool, PostgresError> {
    validate_identifier(table, "table")?;
    Ok(sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS (
            SELECT 1
            FROM pg_catalog.pg_class
            WHERE oid = to_regclass($1)
              AND relkind IN ('r', 'p')
        )",
    )
    .bind(table)
    .fetch_one(pool)
    .await?)
}

fn validate_set(set: &MigrationSet) -> Result<(), PostgresError> {
    if set.backend != DatabaseBackend::Postgres {
        return Err(PostgresError::InvalidConfig(format!(
            "migration set {} targets a different database backend",
            set.id
        )));
    }
    validate_identifier(&set.history_table, "migration history table")?;
    for table in &set.verify_tables {
        validate_identifier(table, "verification table")?;
    }
    Ok(())
}

fn migration_root(project_root: &Path, set: &MigrationSet) -> Result<PathBuf, PostgresError> {
    let project_root = project_root.canonicalize().map_err(PostgresError::Io)?;
    if set.root.is_absolute() {
        return Err(PostgresError::InvalidConfig(format!(
            "migration set {} has an absolute source root",
            set.id
        )));
    }
    let root = project_root
        .join(&set.root)
        .canonicalize()
        .map_err(PostgresError::Io)?;
    if !root.starts_with(&project_root) {
        return Err(PostgresError::InvalidConfig(format!(
            "migration set {} source root escapes the project",
            set.id
        )));
    }
    Ok(root)
}

fn verify_resolved_migrations(
    migrator: &sqlx::migrate::Migrator,
    set: &MigrationSet,
) -> Result<(), PostgresError> {
    let resolved = migrator.iter().collect::<Vec<_>>();
    if resolved.len() != set.migrations.len() {
        return Err(PostgresError::SourceDrift(set.id.clone()));
    }
    for (resolved, expected) in resolved.iter().zip(&set.migrations) {
        if resolved.version != expected.version
            || hex(resolved.checksum.as_ref()) != expected.sqlx_checksum_sha384
        {
            return Err(PostgresError::SourceDrift(set.id.clone()));
        }
    }
    Ok(())
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(DIGITS[usize::from(byte >> 4)]));
        output.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    output
}

fn validate_identifier(value: &str, description: &str) -> Result<(), PostgresError> {
    let mut bytes = value.bytes();
    let valid_start = bytes
        .next()
        .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_');
    if !valid_start
        || value.len() > 63
        || !bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        return Err(PostgresError::InvalidConfig(format!(
            "{description} must be a PostgreSQL identifier of at most 63 ASCII characters"
        )));
    }
    Ok(())
}

#[derive(Debug, Error)]
pub enum PostgresError {
    #[error("invalid PostgreSQL configuration: {0}")]
    InvalidConfig(String),
    #[error("PostgreSQL error: {0}")]
    Sqlx(#[from] sqlx::Error),
    #[error("PostgreSQL migration error: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),
    #[error("PostgreSQL migration source changed after planning for set {0}")]
    SourceDrift(String),
    #[error("PostgreSQL migration plan advisory lock was not held at unlock")]
    PlanLockLost,
    #[error("PostgreSQL migration filesystem operation failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("PostgreSQL seed source validation failed: {0}")]
    SeedSource(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use minco_db::{MigrationState, compare_target, load_catalog};
    use std::fs;
    use tempfile::TempDir;
    use uuid::Uuid;

    #[test]
    fn serverless_defaults_bound_connection_pressure() {
        let config = PostgresPoolConfig::serverless("postgres://example.invalid/db");
        assert_eq!(config.max_connections, 2);
        assert_eq!(config.acquire_timeout_seconds, 5);
    }

    #[test]
    fn pool_configuration_debug_redacts_database_credentials() {
        let config =
            PostgresPoolConfig::serverless("postgres://minco:secret-password@example.invalid/db");
        let debug = format!("{config:?}");
        assert!(!debug.contains("secret-password"));
        assert!(!debug.contains("postgres://"));
    }

    #[tokio::test]
    async fn migration_history_table_rejects_dynamic_sql_tokens() {
        let config = PostgresPoolConfig::serverless("postgres://example.invalid/db");
        let pool = connect_lazy(&config).expect("lazy pool");
        let result =
            migrate_with_history_table(&pool, Path::new("missing"), "_migrations;DROP").await;
        assert!(matches!(result, Err(PostgresError::InvalidConfig(_))));
    }

    #[tokio::test]
    async fn lifecycle_rejects_a_set_for_another_backend_before_connecting() {
        let config = PostgresPoolConfig::serverless("postgres://example.invalid/db");
        let pool = connect_lazy(&config).expect("lazy pool");
        let set = MigrationSet {
            id: "wrong-backend".into(),
            owner: "application:test".into(),
            backend: DatabaseBackend::Sqlite,
            root: "migrations".into(),
            history_table: "_minco_test_migrations".into(),
            depends_on: Vec::new(),
            verify_tables: vec!["example".into()],
            digest: "digest".into(),
            migrations: Vec::new(),
        };

        let error = migration_target_state(&pool, &set)
            .await
            .expect_err("backend mismatch must fail before database access");
        assert!(matches!(error, PostgresError::InvalidConfig(_)));
    }

    #[tokio::test]
    async fn lifecycle_migration_is_behavioral_when_postgres_is_configured() {
        let Ok(url) = std::env::var("MINCO_TEST_POSTGRES_URL") else {
            eprintln!("MINCO_TEST_POSTGRES_URL not set; PostgreSQL lifecycle proof skipped");
            return;
        };
        let suffix = Uuid::new_v4().simple().to_string();
        let table = format!("minco_lifecycle_{suffix}");
        let history = format!("_minco_lifecycle_{suffix}");
        let project = TempDir::new().expect("temporary migration project");
        let migrations = project.path().join("migrations");
        fs::create_dir(&migrations).expect("create migration directory");
        fs::write(
            migrations.join("0001_example.sql"),
            format!("CREATE TABLE {table} (id BIGINT PRIMARY KEY);\n"),
        )
        .expect("write migration");
        fs::write(
            migrations.join(minco_db::MIGRATION_SET_MANIFEST),
            format!(
                concat!(
                    "schema = 1\n",
                    "id = \"test-postgres\"\n",
                    "owner = \"application:test\"\n",
                    "backend = \"postgres\"\n",
                    "history_table = \"{}\"\n",
                    "verify_tables = [\"{}\"]\n",
                    "\n",
                    "[[migration]]\n",
                    "version = 1\n",
                    "risk = \"additive\"\n",
                    "reversible = false\n",
                ),
                history, table
            ),
        )
        .expect("write lifecycle manifest");
        let set = load_catalog(project.path(), &[Path::new("migrations").to_path_buf()])
            .expect("load lifecycle catalog")
            .sets
            .into_iter()
            .next()
            .expect("migration set");
        let pool = connect(&PostgresPoolConfig::serverless(url))
            .await
            .expect("connect PostgreSQL");

        let before = migration_target_state(&pool, &set)
            .await
            .expect("read empty target state");
        assert!(before.applied.is_empty());
        let (first, second) = tokio::join!(
            apply_migration_set(&pool, project.path(), &set),
            apply_migration_set(&pool, project.path(), &set)
        );
        first.expect("apply first concurrent migration plan");
        second.expect("apply second concurrent migration plan");
        let after = migration_target_state(&pool, &set)
            .await
            .expect("read applied target state");
        assert_eq!(
            compare_target(&set, &after).entries[0].state,
            MigrationState::Applied
        );
        assert!(
            verify_migration_tables(&pool, &set)
                .await
                .expect("verify tables")
                .is_empty()
        );

        sqlx::query(sqlx::AssertSqlSafe(format!("DROP TABLE IF EXISTS {table}")))
            .execute(&pool)
            .await
            .expect("clean up verified table");
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "CREATE VIEW {table} AS SELECT 1::BIGINT AS id"
        )))
        .execute(&pool)
        .await
        .expect("replace verified table with a view");
        let missing_tables = verify_migration_tables(&pool, &set)
            .await
            .expect("reject view as a verification table");
        assert_eq!(missing_tables.as_slice(), std::slice::from_ref(&table));
        sqlx::query(sqlx::AssertSqlSafe(format!("DROP VIEW {table}")))
            .execute(&pool)
            .await
            .expect("clean up verified view");
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "DROP TABLE IF EXISTS {history}"
        )))
        .execute(&pool)
        .await
        .expect("clean up migration history");
    }
}
