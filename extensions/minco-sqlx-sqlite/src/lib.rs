//! `SQLx` `SQLite` pools with explicit file-backed versus in-memory behavior.
#![forbid(unsafe_code)]

use minco_db::{AppliedMigration, DatabaseBackend, MigrationSet, TargetState};
use serde::{Deserialize, Serialize};
pub use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use std::{
    fs::{File, OpenOptions},
    path::{Path, PathBuf},
    str::FromStr,
    time::Duration,
};
use thiserror::Error;

pub mod plugin_adapters;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SqlitePoolConfig {
    pub url: String,
    pub max_connections: u32,
    pub acquire_timeout_seconds: u64,
}

impl SqlitePoolConfig {
    pub fn file(path: impl AsRef<Path>) -> Self {
        Self {
            url: format!("sqlite://{}", path.as_ref().display()),
            max_connections: 4,
            acquire_timeout_seconds: 5,
        }
    }
    pub fn memory() -> Self {
        Self {
            url: "sqlite::memory:".into(),
            max_connections: 1,
            acquire_timeout_seconds: 5,
        }
    }
    pub fn is_memory(&self) -> bool {
        self.url == "sqlite::memory:" || self.url.contains("mode=memory")
    }
    pub fn validate(&self) -> Result<(), SqliteError> {
        if self.url.trim().is_empty() {
            return Err(SqliteError::InvalidConfig("database URL is empty".into()));
        }
        if self.max_connections == 0 {
            return Err(SqliteError::InvalidConfig(
                "max_connections must be at least 1".into(),
            ));
        }
        if self.is_memory() && self.max_connections != 1 {
            return Err(SqliteError::InvalidConfig(
                "in-memory SQLite requires exactly one pooled connection".into(),
            ));
        }
        Ok(())
    }
}

pub async fn connect(config: &SqlitePoolConfig) -> Result<SqlitePool, SqliteError> {
    config.validate()?;
    let mut options = SqliteConnectOptions::from_str(&config.url)?
        .create_if_missing(!config.is_memory())
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(config.acquire_timeout_seconds));
    if !config.is_memory() {
        options = options.journal_mode(SqliteJournalMode::Wal);
    }
    Ok(SqlitePoolOptions::new()
        .max_connections(config.max_connections)
        .acquire_timeout(Duration::from_secs(config.acquire_timeout_seconds))
        .connect_with(options)
        .await?)
}

pub async fn migrate(pool: &SqlitePool, path: impl AsRef<Path>) -> Result<(), SqliteError> {
    let migrator = sqlx::migrate::Migrator::new(path.as_ref()).await?;
    migrator.run(pool).await?;
    Ok(())
}

pub async fn migrate_with_history_table(
    pool: &SqlitePool,
    path: impl AsRef<Path>,
    history_table: &'static str,
) -> Result<(), SqliteError> {
    validate_identifier(history_table, "migration history table")?;
    let mut migrator = sqlx::migrate::Migrator::new(path.as_ref()).await?;
    migrator.dangerous_set_table_name(history_table);
    migrator.run(pool).await?;
    Ok(())
}

pub async fn migration_target_state(
    pool: &SqlitePool,
    set: &MigrationSet,
) -> Result<TargetState, SqliteError> {
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
    pool: &SqlitePool,
    set: &MigrationSet,
) -> Result<Vec<String>, SqliteError> {
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
    pool: &SqlitePool,
    config: &SqlitePoolConfig,
    project_root: &Path,
    set: &MigrationSet,
) -> Result<(), SqliteError> {
    apply_migration_plan(pool, config, project_root, std::slice::from_ref(set)).await
}

pub async fn apply_migration_plan(
    pool: &SqlitePool,
    config: &SqlitePoolConfig,
    project_root: &Path,
    sets: &[MigrationSet],
) -> Result<(), SqliteError> {
    if sets.is_empty() {
        return Err(SqliteError::InvalidConfig(
            "migration plan contains no sets".into(),
        ));
    }
    config.validate()?;
    let mut migrators = Vec::with_capacity(sets.len());
    for set in sets {
        validate_set(set)?;
        let root = migration_root(project_root, set)?;
        let mut migrator = sqlx::migrate::Migrator::new(root).await?;
        verify_resolved_migrations(&migrator, set)?;
        migrator.dangerous_set_table_name(set.history_table.clone());
        migrators.push(migrator);
    }
    let _lock = acquire_migration_lock(config)?;
    for migrator in migrators {
        migrator.run(pool).await?;
    }
    Ok(())
}

pub async fn ready(pool: &SqlitePool) -> bool {
    matches!(
        sqlx::query_scalar::<_, i64>("SELECT 1")
            .fetch_one(pool)
            .await,
        Ok(1)
    )
}

async fn table_exists(pool: &SqlitePool, table: &str) -> Result<bool, SqliteError> {
    validate_identifier(table, "table")?;
    Ok(sqlx::query_scalar::<_, String>(
        "SELECT name FROM sqlite_master WHERE type = 'table' AND name = ?1",
    )
    .bind(table)
    .fetch_optional(pool)
    .await?
    .is_some())
}

fn validate_set(set: &MigrationSet) -> Result<(), SqliteError> {
    if set.backend != DatabaseBackend::Sqlite {
        return Err(SqliteError::InvalidConfig(format!(
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

fn migration_root(project_root: &Path, set: &MigrationSet) -> Result<PathBuf, SqliteError> {
    let project_root = project_root.canonicalize().map_err(SqliteError::Io)?;
    if set.root.is_absolute() {
        return Err(SqliteError::InvalidConfig(format!(
            "migration set {} has an absolute source root",
            set.id
        )));
    }
    let root = project_root
        .join(&set.root)
        .canonicalize()
        .map_err(SqliteError::Io)?;
    if !root.starts_with(&project_root) {
        return Err(SqliteError::InvalidConfig(format!(
            "migration set {} source root escapes the project",
            set.id
        )));
    }
    Ok(root)
}

fn verify_resolved_migrations(
    migrator: &sqlx::migrate::Migrator,
    set: &MigrationSet,
) -> Result<(), SqliteError> {
    let resolved = migrator.iter().collect::<Vec<_>>();
    if resolved.len() != set.migrations.len() {
        return Err(SqliteError::SourceDrift(set.id.clone()));
    }
    for (resolved, expected) in resolved.iter().zip(&set.migrations) {
        if resolved.version != expected.version
            || hex(resolved.checksum.as_ref()) != expected.sqlx_checksum_sha384
        {
            return Err(SqliteError::SourceDrift(set.id.clone()));
        }
    }
    Ok(())
}

fn acquire_migration_lock(config: &SqlitePoolConfig) -> Result<File, SqliteError> {
    if config.is_memory() {
        return Err(SqliteError::InvalidConfig(
            "migration execution requires file-backed SQLite".into(),
        ));
    }
    let options = SqliteConnectOptions::from_str(&config.url)?;
    let database = options
        .get_filename()
        .canonicalize()
        .map_err(SqliteError::Io)?;
    let mut lock_name = database.as_os_str().to_os_string();
    lock_name.push(".minco-migrate.lock");
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(PathBuf::from(lock_name))
        .map_err(SqliteError::Io)?;
    match lock.try_lock() {
        Ok(()) => {}
        Err(std::fs::TryLockError::WouldBlock) => {
            return Err(SqliteError::MigrationLockUnavailable);
        }
        Err(std::fs::TryLockError::Error(source)) => {
            return Err(SqliteError::Io(source));
        }
    }
    Ok(lock)
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

fn validate_identifier(value: &str, description: &str) -> Result<(), SqliteError> {
    let mut bytes = value.bytes();
    let valid_start = bytes
        .next()
        .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_');
    if !valid_start
        || value.len() > 63
        || !bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        return Err(SqliteError::InvalidConfig(format!(
            "{description} must be a SQLite identifier of at most 63 ASCII characters"
        )));
    }
    Ok(())
}

#[derive(Debug, Error)]
pub enum SqliteError {
    #[error("invalid SQLite configuration: {0}")]
    InvalidConfig(String),
    #[error("SQLite error: {0}")]
    Sqlx(#[from] sqlx::Error),
    #[error("SQLite migration error: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),
    #[error("SQLite migration source changed after planning for set {0}")]
    SourceDrift(String),
    #[error("another SQLite migration process holds the migration lock")]
    MigrationLockUnavailable,
    #[error("SQLite migration filesystem operation failed: {0}")]
    Io(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    use minco_db::{MigrationState, compare_target, load_catalog};
    use std::fs;
    use tempfile::TempDir;

    fn lifecycle_fixture() -> (TempDir, minco_db::MigrationSet) {
        let root = TempDir::new().expect("temporary migration project");
        let migrations = root.path().join("migrations");
        fs::create_dir(&migrations).expect("create migration directory");
        fs::write(
            migrations.join("0001_example.sql"),
            "CREATE TABLE example (id INTEGER PRIMARY KEY);\n",
        )
        .expect("write migration");
        fs::write(
            migrations.join(minco_db::MIGRATION_SET_MANIFEST),
            concat!(
                "schema = 1\n",
                "id = \"test-sqlite\"\n",
                "owner = \"application:test\"\n",
                "backend = \"sqlite\"\n",
                "history_table = \"_minco_test_migrations\"\n",
                "verify_tables = [\"example\"]\n",
                "\n",
                "[[migration]]\n",
                "version = 1\n",
                "risk = \"additive\"\n",
                "reversible = false\n",
            ),
        )
        .expect("write lifecycle manifest");
        let catalog = load_catalog(root.path(), &[Path::new("migrations").to_path_buf()])
            .expect("load lifecycle catalog");
        let set = catalog.sets.into_iter().next().expect("migration set");
        (root, set)
    }

    #[test]
    fn memory_profile_rejects_multiple_connections() {
        let mut config = SqlitePoolConfig::memory();
        config.max_connections = 2;
        assert!(config.validate().is_err());
    }

    #[tokio::test]
    async fn migration_history_table_rejects_dynamic_sql_tokens() {
        let pool = connect(&SqlitePoolConfig::memory())
            .await
            .expect("in-memory pool");
        let result =
            migrate_with_history_table(&pool, Path::new("missing"), "_migrations;DROP").await;
        assert!(matches!(result, Err(SqliteError::InvalidConfig(_))));
    }

    #[tokio::test]
    async fn lifecycle_migration_reports_state_and_verifies_expected_tables() {
        let (project, set) = lifecycle_fixture();
        let database = project.path().join("test.sqlite");
        let config = SqlitePoolConfig::file(&database);
        let pool = connect(&config).await.expect("connect SQLite");

        let before = migration_target_state(&pool, &set)
            .await
            .expect("read empty target state");
        assert!(before.applied.is_empty());

        apply_migration_set(&pool, &config, project.path(), &set)
            .await
            .expect("apply migration set");

        let after = migration_target_state(&pool, &set)
            .await
            .expect("read applied target state");
        let status = compare_target(&set, &after);
        assert_eq!(status.entries[0].state, MigrationState::Applied);
        assert!(
            verify_migration_tables(&pool, &set)
                .await
                .expect("verify migration tables")
                .is_empty()
        );
    }

    #[tokio::test]
    async fn lifecycle_migration_fails_closed_when_another_process_holds_the_file_lock() {
        let (project, set) = lifecycle_fixture();
        let config = SqlitePoolConfig::file(project.path().join("test.sqlite"));
        let pool = connect(&config).await.expect("connect SQLite");
        let _held_lock = acquire_migration_lock(&config).expect("hold migration lock");

        let error = apply_migration_set(&pool, &config, project.path(), &set)
            .await
            .expect_err("concurrent migration must fail");
        assert!(matches!(error, SqliteError::MigrationLockUnavailable));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn lifecycle_lock_cannot_be_bypassed_with_a_database_symlink() {
        use std::os::unix::fs::symlink;

        let (project, _) = lifecycle_fixture();
        let database = project.path().join("test.sqlite");
        let config = SqlitePoolConfig::file(&database);
        let pool = connect(&config).await.expect("connect SQLite");
        pool.close().await;
        let alias = project.path().join("database-alias.sqlite");
        symlink(&database, &alias).expect("create database symlink");
        let alias_config = SqlitePoolConfig::file(alias);

        let _held_lock = acquire_migration_lock(&config).expect("hold canonical migration lock");
        let error =
            acquire_migration_lock(&alias_config).expect_err("symlink alias must share the lock");
        assert!(matches!(error, SqliteError::MigrationLockUnavailable));
    }

    #[tokio::test]
    async fn lifecycle_migration_rejects_in_memory_targets() {
        let (project, set) = lifecycle_fixture();
        let config = SqlitePoolConfig::memory();
        let pool = connect(&config).await.expect("connect SQLite");

        let error = apply_migration_set(&pool, &config, project.path(), &set)
            .await
            .expect_err("in-memory migration target must fail");
        assert!(matches!(error, SqliteError::InvalidConfig(_)));
    }
}
