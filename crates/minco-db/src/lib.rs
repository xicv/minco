//! Provider-neutral database migration and seed lifecycle models.
#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256, Sha384};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};
use thiserror::Error;

mod seed;

pub use seed::*;

pub const MIGRATION_SET_MANIFEST: &str = ".minco-migrations.toml";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DatabaseBackend {
    Postgres,
    Sqlite,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MigrationRisk {
    Additive,
    DataRewrite,
    Destructive,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MigrationCatalog {
    pub schema_version: u32,
    pub digest: String,
    pub sets: Vec<MigrationSet>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MigrationPlan {
    pub schema_version: u32,
    pub catalog_digest: String,
    pub selected_set: Option<String>,
    pub digest: String,
    pub sets: Vec<MigrationSet>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MigrationSet {
    pub id: String,
    pub owner: String,
    pub backend: DatabaseBackend,
    pub root: PathBuf,
    pub history_table: String,
    pub depends_on: Vec<String>,
    pub verify_tables: Vec<String>,
    pub digest: String,
    pub migrations: Vec<Migration>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Migration {
    pub id: String,
    pub version: i64,
    pub description: String,
    pub path: PathBuf,
    pub sha256: String,
    pub sqlx_checksum_sha384: String,
    pub risk: MigrationRisk,
    pub reversible: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppliedMigration {
    pub version: i64,
    pub sqlx_checksum_sha384: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TargetState {
    pub dirty_version: Option<i64>,
    pub applied: Vec<AppliedMigration>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MigrationState {
    Applied,
    Pending,
    Drift,
    MissingSource,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MigrationStatusEntry {
    pub id: String,
    pub version: i64,
    pub state: MigrationState,
    pub source_checksum_sha384: Option<String>,
    pub applied_checksum_sha384: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MigrationStatus {
    pub set_id: String,
    pub dirty_version: Option<i64>,
    pub entries: Vec<MigrationStatusEntry>,
}

#[derive(Debug, Error)]
pub enum DbLifecycleError {
    #[error("database lifecycle metadata is invalid: {0}")]
    Invalid(String),
    #[error("database lifecycle I/O failed at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("database lifecycle TOML failed at {path}: {source}")]
    Toml {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },
    #[error("database lifecycle serialization failed: {0}")]
    Json(#[from] serde_json::Error),
}

pub fn load_catalog(
    project_root: &Path,
    migration_roots: &[PathBuf],
) -> Result<MigrationCatalog, DbLifecycleError> {
    let project_root = canonicalize(project_root)?;
    let mut roots = BTreeSet::new();
    let mut sets = Vec::new();
    for configured_root in migration_roots {
        if configured_root.is_absolute() {
            return Err(DbLifecycleError::Invalid(format!(
                "migration root {} must be relative to the project",
                configured_root.display()
            )));
        }
        let root = canonicalize(&project_root.join(configured_root))?;
        if !root.starts_with(&project_root) {
            return Err(DbLifecycleError::Invalid(format!(
                "migration root {} escapes the project",
                configured_root.display()
            )));
        }
        let relative_root = root
            .strip_prefix(&project_root)
            .map_err(|_| {
                DbLifecycleError::Invalid(format!(
                    "migration root {} escapes the project",
                    configured_root.display()
                ))
            })?
            .to_path_buf();
        if !roots.insert(relative_root.clone()) {
            return Err(DbLifecycleError::Invalid(format!(
                "migration root {} is configured more than once",
                relative_root.display()
            )));
        }
        sets.push(load_set(&project_root, &root, relative_root)?);
    }
    sets.sort_by(|left, right| left.id.cmp(&right.id));
    validate_dependencies(&sets)?;
    let digest = sha256_hex(&serde_json::to_vec(&sets)?);
    Ok(MigrationCatalog {
        schema_version: 1,
        digest,
        sets,
    })
}

pub fn compare_target(set: &MigrationSet, target: &TargetState) -> MigrationStatus {
    let applied = target
        .applied
        .iter()
        .map(|migration| (migration.version, migration))
        .collect::<BTreeMap<_, _>>();
    let source_versions = set
        .migrations
        .iter()
        .map(|migration| migration.version)
        .collect::<BTreeSet<_>>();
    let mut entries = set
        .migrations
        .iter()
        .map(|migration| {
            let target = applied.get(&migration.version);
            let state = match target {
                None => MigrationState::Pending,
                Some(target) if target.sqlx_checksum_sha384 == migration.sqlx_checksum_sha384 => {
                    MigrationState::Applied
                }
                Some(_) => MigrationState::Drift,
            };
            MigrationStatusEntry {
                id: migration.id.clone(),
                version: migration.version,
                state,
                source_checksum_sha384: Some(migration.sqlx_checksum_sha384.clone()),
                applied_checksum_sha384: target
                    .map(|migration| migration.sqlx_checksum_sha384.clone()),
            }
        })
        .collect::<Vec<_>>();
    entries.extend(
        target
            .applied
            .iter()
            .filter(|migration| !source_versions.contains(&migration.version))
            .map(|migration| MigrationStatusEntry {
                id: format!("{}:{}", set.id, migration.version),
                version: migration.version,
                state: MigrationState::MissingSource,
                source_checksum_sha384: None,
                applied_checksum_sha384: Some(migration.sqlx_checksum_sha384.clone()),
            }),
    );
    entries.sort_by_key(|entry| entry.version);
    MigrationStatus {
        set_id: set.id.clone(),
        dirty_version: target.dirty_version,
        entries,
    }
}

pub fn build_plan(
    catalog: &MigrationCatalog,
    selected_set: Option<&str>,
) -> Result<MigrationPlan, DbLifecycleError> {
    let by_id = catalog
        .sets
        .iter()
        .map(|set| (set.id.as_str(), set))
        .collect::<BTreeMap<_, _>>();
    let mut visited = BTreeSet::new();
    let mut ordered = Vec::new();
    match selected_set {
        Some(id) => {
            if !by_id.contains_key(id) {
                return Err(DbLifecycleError::Invalid(format!(
                    "unknown migration set {id}"
                )));
            }
            collect_plan_set(id, &by_id, &mut visited, &mut ordered)?;
        }
        None => {
            for id in by_id.keys() {
                collect_plan_set(id, &by_id, &mut visited, &mut ordered)?;
            }
        }
    }
    let sets = ordered.into_iter().cloned().collect::<Vec<_>>();
    let selected_set = selected_set.map(str::to_owned);
    let digest_input = serde_json::to_vec(&(
        1_u32,
        catalog.digest.as_str(),
        selected_set.as_deref(),
        &sets,
    ))?;
    Ok(MigrationPlan {
        schema_version: 1,
        catalog_digest: catalog.digest.clone(),
        selected_set,
        digest: sha256_hex(&digest_input),
        sets,
    })
}

fn collect_plan_set<'a>(
    id: &'a str,
    sets: &BTreeMap<&'a str, &'a MigrationSet>,
    visited: &mut BTreeSet<&'a str>,
    ordered: &mut Vec<&'a MigrationSet>,
) -> Result<(), DbLifecycleError> {
    if !visited.insert(id) {
        return Ok(());
    }
    let set = sets
        .get(id)
        .ok_or_else(|| DbLifecycleError::Invalid(format!("unknown migration set {id}")))?;
    for dependency in &set.depends_on {
        collect_plan_set(dependency, sets, visited, ordered)?;
    }
    ordered.push(set);
    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MigrationSetManifest {
    schema: u32,
    id: String,
    owner: String,
    backend: DatabaseBackend,
    history_table: String,
    #[serde(default)]
    depends_on: Vec<String>,
    #[serde(default)]
    verify_tables: Vec<String>,
    #[serde(default)]
    migration: Vec<MigrationMetadata>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MigrationMetadata {
    version: i64,
    risk: MigrationRisk,
    reversible: bool,
}

fn metadata_by_version(
    manifest: &MigrationSetManifest,
) -> Result<BTreeMap<i64, &MigrationMetadata>, DbLifecycleError> {
    let mut metadata = BTreeMap::new();
    for migration in &manifest.migration {
        if migration.version <= 0 {
            return Err(DbLifecycleError::Invalid(format!(
                "migration set {} has non-positive metadata version {}",
                manifest.id, migration.version
            )));
        }
        if metadata.insert(migration.version, migration).is_some() {
            return Err(DbLifecycleError::Invalid(format!(
                "migration set {} repeats metadata for version {}",
                manifest.id, migration.version
            )));
        }
    }
    Ok(metadata)
}

fn load_set(
    project_root: &Path,
    root: &Path,
    relative_root: PathBuf,
) -> Result<MigrationSet, DbLifecycleError> {
    let manifest_path = canonicalize(&root.join(MIGRATION_SET_MANIFEST))?;
    if !manifest_path.starts_with(root) {
        return Err(DbLifecycleError::Invalid(format!(
            "migration metadata for root {} escapes its configured root",
            relative_root.display()
        )));
    }
    let manifest: MigrationSetManifest =
        toml::from_str(&read_to_string(&manifest_path)?).map_err(|source| {
            DbLifecycleError::Toml {
                path: manifest_path.clone(),
                source,
            }
        })?;
    if manifest.schema != 1 {
        return Err(DbLifecycleError::Invalid(format!(
            "migration set {} uses unsupported schema {}",
            manifest.id, manifest.schema
        )));
    }
    validate_stable_id(&manifest.id, "migration set ID")?;
    validate_owner(&manifest.owner)?;
    validate_identifier(&manifest.history_table, "migration history table")?;
    if manifest.verify_tables.is_empty() {
        return Err(DbLifecycleError::Invalid(format!(
            "migration set {} must declare at least one verification table",
            manifest.id
        )));
    }
    let mut verification_tables = BTreeSet::new();
    for table in &manifest.verify_tables {
        validate_identifier(table, "verification table")?;
        if !verification_tables.insert(table) {
            return Err(DbLifecycleError::Invalid(format!(
                "migration set {} repeats verification table {}",
                manifest.id, table
            )));
        }
    }
    let mut dependencies = BTreeSet::new();
    for dependency in &manifest.depends_on {
        validate_stable_id(dependency, "migration dependency")?;
        if dependency == &manifest.id {
            return Err(DbLifecycleError::Invalid(format!(
                "migration set {} cannot depend on itself",
                manifest.id
            )));
        }
        if !dependencies.insert(dependency) {
            return Err(DbLifecycleError::Invalid(format!(
                "migration set {} repeats dependency {}",
                manifest.id, dependency
            )));
        }
    }

    let metadata = metadata_by_version(&manifest)?;
    let mut seen_versions = BTreeSet::new();
    let mut migrations = Vec::new();
    for entry in fs::read_dir(root).map_err(|source| DbLifecycleError::Io {
        path: root.to_path_buf(),
        source,
    })? {
        let entry = entry.map_err(|source| DbLifecycleError::Io {
            path: root.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("sql") {
            continue;
        }
        let canonical_path = canonicalize(&path)?;
        if !canonical_path.starts_with(root) {
            return Err(DbLifecycleError::Invalid(format!(
                "migration file {} escapes its configured root",
                path.display()
            )));
        }
        let file_name = canonical_path
            .file_stem()
            .and_then(|value| value.to_str())
            .ok_or_else(|| {
                DbLifecycleError::Invalid(format!(
                    "migration file {} must have a UTF-8 name",
                    canonical_path.display()
                ))
            })?;
        let (version, description) = parse_migration_name(file_name)?;
        if !seen_versions.insert(version) {
            return Err(DbLifecycleError::Invalid(format!(
                "migration set {} repeats version {}",
                manifest.id, version
            )));
        }
        let migration_metadata = metadata.get(&version).ok_or_else(|| {
            DbLifecycleError::Invalid(format!(
                "migration set {} has no risk metadata for version {}",
                manifest.id, version
            ))
        })?;
        let sql = fs::read(&canonical_path).map_err(|source| DbLifecycleError::Io {
            path: canonical_path.clone(),
            source,
        })?;
        let sql_text = std::str::from_utf8(&sql).map_err(|_| {
            DbLifecycleError::Invalid(format!(
                "migration file {} is not UTF-8",
                canonical_path.display()
            ))
        })?;
        let relative_path = canonical_path
            .strip_prefix(project_root)
            .map_err(|_| {
                DbLifecycleError::Invalid(format!(
                    "migration file {} escapes the project",
                    canonical_path.display()
                ))
            })?
            .to_path_buf();
        migrations.push(Migration {
            id: format!("{}:{version}", manifest.id),
            version,
            description,
            path: relative_path,
            sha256: sha256_hex(&sql),
            sqlx_checksum_sha384: sha384_hex(sql_text.as_bytes()),
            risk: migration_metadata.risk,
            reversible: migration_metadata.reversible,
        });
    }
    migrations.sort_by_key(|migration| migration.version);
    if migrations.is_empty() {
        return Err(DbLifecycleError::Invalid(format!(
            "migration set {} contains no SQL migrations",
            manifest.id
        )));
    }
    for version in metadata.keys() {
        if !seen_versions.contains(version) {
            return Err(DbLifecycleError::Invalid(format!(
                "migration set {} has metadata for missing version {}",
                manifest.id, version
            )));
        }
    }
    let mut set = MigrationSet {
        id: manifest.id,
        owner: manifest.owner,
        backend: manifest.backend,
        root: relative_root,
        history_table: manifest.history_table,
        depends_on: manifest.depends_on,
        verify_tables: manifest.verify_tables,
        digest: String::new(),
        migrations,
    };
    set.depends_on.sort();
    set.verify_tables.sort();
    set.digest = sha256_hex(&serde_json::to_vec(&set)?);
    Ok(set)
}

fn validate_dependencies(sets: &[MigrationSet]) -> Result<(), DbLifecycleError> {
    let mut by_id = BTreeMap::new();
    for set in sets {
        if by_id.insert(set.id.as_str(), set).is_some() {
            return Err(DbLifecycleError::Invalid(format!(
                "migration catalog repeats migration set ID {}",
                set.id
            )));
        }
    }
    let mut history_owners = BTreeMap::new();
    for set in sets {
        let key = (set.backend, set.history_table.as_str());
        if let Some(previous) = history_owners.insert(key, set.id.as_str()) {
            return Err(DbLifecycleError::Invalid(format!(
                "migration set {} shares migration history table {} with set {}",
                set.id, set.history_table, previous
            )));
        }
    }
    for set in sets {
        for dependency in &set.depends_on {
            let dependency_set = by_id.get(dependency.as_str()).ok_or_else(|| {
                DbLifecycleError::Invalid(format!(
                    "migration set {} depends on unknown set {}",
                    set.id, dependency
                ))
            })?;
            if dependency_set.backend != set.backend {
                return Err(DbLifecycleError::Invalid(format!(
                    "migration set {} cannot depend on {} because their backends differ",
                    set.id, dependency
                )));
            }
        }
    }
    let mut visiting = BTreeSet::new();
    let mut visited = BTreeSet::new();
    for set in sets {
        visit_dependency(set.id.as_str(), &by_id, &mut visiting, &mut visited)?;
    }
    Ok(())
}

fn visit_dependency<'a>(
    id: &'a str,
    sets: &BTreeMap<&'a str, &'a MigrationSet>,
    visiting: &mut BTreeSet<&'a str>,
    visited: &mut BTreeSet<&'a str>,
) -> Result<(), DbLifecycleError> {
    if visited.contains(id) {
        return Ok(());
    }
    if !visiting.insert(id) {
        return Err(DbLifecycleError::Invalid(format!(
            "migration dependency cycle contains {id}"
        )));
    }
    let set = sets
        .get(id)
        .ok_or_else(|| DbLifecycleError::Invalid(format!("unknown migration set {id}")))?;
    for dependency in &set.depends_on {
        visit_dependency(dependency, sets, visiting, visited)?;
    }
    visiting.remove(id);
    visited.insert(id);
    Ok(())
}

fn parse_migration_name(file_stem: &str) -> Result<(i64, String), DbLifecycleError> {
    let (version, description) = file_stem.split_once('_').ok_or_else(|| {
        DbLifecycleError::Invalid(format!(
            "migration file {file_stem}.sql must use <version>_<description>.sql"
        ))
    })?;
    let version = version.parse::<i64>().map_err(|_| {
        DbLifecycleError::Invalid(format!(
            "migration file {file_stem}.sql has an invalid version"
        ))
    })?;
    if version <= 0 || description.is_empty() {
        return Err(DbLifecycleError::Invalid(format!(
            "migration file {file_stem}.sql has an invalid identity"
        )));
    }
    Ok((version, description.replace('_', " ")))
}

pub(crate) fn validate_stable_id(value: &str, description: &str) -> Result<(), DbLifecycleError> {
    let valid = !value.is_empty()
        && value.len() <= 96
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        && value.as_bytes().first().is_some_and(u8::is_ascii_lowercase)
        && value
            .as_bytes()
            .last()
            .is_some_and(u8::is_ascii_alphanumeric);
    if !valid {
        return Err(DbLifecycleError::Invalid(format!(
            "{description} {value:?} must be lower-kebab-case"
        )));
    }
    Ok(())
}

pub(crate) fn validate_owner(value: &str) -> Result<(), DbLifecycleError> {
    let Some((kind, id)) = value.split_once(':') else {
        return Err(DbLifecycleError::Invalid(format!(
            "migration owner {value:?} must be application:<id> or plugin:<id>"
        )));
    };
    if !matches!(kind, "application" | "plugin") {
        return Err(DbLifecycleError::Invalid(format!(
            "migration owner kind {kind:?} is unsupported"
        )));
    }
    validate_stable_id(id, "migration owner ID")
}

fn validate_identifier(value: &str, description: &str) -> Result<(), DbLifecycleError> {
    let mut bytes = value.bytes();
    let valid_start = bytes
        .next()
        .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_');
    if !valid_start
        || value.len() > 63
        || !bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        return Err(DbLifecycleError::Invalid(format!(
            "{description} {value:?} must be an ASCII SQL identifier of at most 63 characters"
        )));
    }
    Ok(())
}

pub(crate) fn canonicalize(path: &Path) -> Result<PathBuf, DbLifecycleError> {
    path.canonicalize().map_err(|source| DbLifecycleError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn read_to_string(path: &Path) -> Result<String, DbLifecycleError> {
    fs::read_to_string(path).map_err(|source| DbLifecycleError::Io {
        path: path.to_path_buf(),
        source,
    })
}

pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn sha384_hex(bytes: &[u8]) -> String {
    hex::encode(Sha384::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn write_set(directory: &Path, id: &str, depends_on: &[&str], include_metadata: bool) {
        fs::create_dir_all(directory).expect("create migration root");
        fs::write(
            directory.join("0001_foundation.sql"),
            "CREATE TABLE example (id INTEGER PRIMARY KEY);\n",
        )
        .expect("write migration");
        let dependencies = depends_on
            .iter()
            .map(|dependency| format!("\"{dependency}\""))
            .collect::<Vec<_>>()
            .join(", ");
        let metadata = if include_metadata {
            "\n[[migration]]\nversion = 1\nrisk = \"additive\"\nreversible = false\n"
        } else {
            ""
        };
        fs::write(
            directory.join(MIGRATION_SET_MANIFEST),
            format!(
                "schema = 1\nid = \"{id}\"\nowner = \"application:test\"\nbackend = \"sqlite\"\nhistory_table = \"_minco_{}_migrations\"\ndepends_on = [{dependencies}]\nverify_tables = [\"example\"]\n{metadata}",
                id.replace('-', "_")
            ),
        )
        .expect("write lifecycle metadata");
    }

    #[test]
    fn catalog_is_deterministic_and_carries_explicit_metadata() {
        let project = TempDir::new().expect("temporary project");
        write_set(&project.path().join("migrations"), "app", &[], true);

        let first =
            load_catalog(project.path(), &[PathBuf::from("migrations")]).expect("load catalog");
        let second = load_catalog(project.path(), &[PathBuf::from("migrations")])
            .expect("load catalog again");

        assert_eq!(first, second);
        assert_eq!(first.sets.len(), 1);
        assert_eq!(first.sets[0].id, "app");
        assert_eq!(first.sets[0].owner, "application:test");
        assert_eq!(first.sets[0].migrations[0].risk, MigrationRisk::Additive);
        assert_eq!(first.digest.len(), 64);
        assert_eq!(first.sets[0].migrations[0].sha256.len(), 64);
        assert_eq!(first.sets[0].migrations[0].sqlx_checksum_sha384.len(), 96);
    }

    #[test]
    fn every_sql_migration_requires_explicit_risk_metadata() {
        let project = TempDir::new().expect("temporary project");
        write_set(&project.path().join("migrations"), "app", &[], false);

        let error = load_catalog(project.path(), &[PathBuf::from("migrations")])
            .expect_err("missing risk metadata must fail");
        assert!(error.to_string().contains("version 1"));
    }

    #[test]
    fn dependency_cycles_fail_before_database_access() {
        let project = TempDir::new().expect("temporary project");
        write_set(&project.path().join("a"), "a", &["b"], true);
        write_set(&project.path().join("b"), "b", &["a"], true);

        let error = load_catalog(project.path(), &[PathBuf::from("a"), PathBuf::from("b")])
            .expect_err("cycle must fail");
        assert!(error.to_string().contains("cycle"));
    }

    #[test]
    fn duplicate_set_ids_fail_before_database_access() {
        let project = TempDir::new().expect("temporary project");
        write_set(&project.path().join("first"), "app", &[], true);
        write_set(&project.path().join("second"), "app", &[], true);

        let error = load_catalog(
            project.path(),
            &[PathBuf::from("first"), PathBuf::from("second")],
        )
        .expect_err("duplicate set IDs must fail");
        assert!(error.to_string().contains("repeats migration set ID app"));
    }

    #[test]
    fn migration_history_is_attributable_to_only_one_set_per_backend() {
        let project = TempDir::new().expect("temporary project");
        write_set(&project.path().join("first"), "first", &[], true);
        write_set(&project.path().join("second"), "second", &[], true);
        let second_manifest = project.path().join("second").join(MIGRATION_SET_MANIFEST);
        let manifest = fs::read_to_string(&second_manifest).expect("read second manifest");
        fs::write(
            &second_manifest,
            manifest.replace("_minco_second_migrations", "_minco_first_migrations"),
        )
        .expect("reuse history table");

        let error = load_catalog(
            project.path(),
            &[PathBuf::from("first"), PathBuf::from("second")],
        )
        .expect_err("shared history table must fail");
        assert!(error.to_string().contains("shares migration history table"));
    }

    #[cfg(unix)]
    #[test]
    fn migration_roots_cannot_escape_through_symlinks() {
        use std::os::unix::fs::symlink;

        let project = TempDir::new().expect("temporary project");
        let outside = TempDir::new().expect("outside directory");
        write_set(&outside.path().join("migrations"), "app", &[], true);
        symlink(
            outside.path().join("migrations"),
            project.path().join("migrations"),
        )
        .expect("create migration-root symlink");

        let error = load_catalog(project.path(), &[PathBuf::from("migrations")])
            .expect_err("symlink escape must fail");
        assert!(error.to_string().contains("escapes the project"));
    }

    #[test]
    fn dynamic_sql_identifiers_are_strictly_validated() {
        let project = TempDir::new().expect("temporary project");
        write_set(&project.path().join("migrations"), "app", &[], true);
        let manifest_path = project
            .path()
            .join("migrations")
            .join(MIGRATION_SET_MANIFEST);
        let manifest = fs::read_to_string(&manifest_path).expect("read manifest");
        fs::write(
            &manifest_path,
            manifest.replace(
                "history_table = \"_minco_app_migrations\"",
                "history_table = \"migrations; DROP TABLE example\"",
            ),
        )
        .expect("write malicious identifier");

        let error = load_catalog(project.path(), &[PathBuf::from("migrations")])
            .expect_err("unsafe SQL identifier must fail");
        assert!(error.to_string().contains("ASCII SQL identifier"));
    }

    #[test]
    fn selected_plan_orders_dependency_closure_and_has_a_stable_digest() {
        let project = TempDir::new().expect("temporary project");
        write_set(&project.path().join("foundation"), "foundation", &[], true);
        write_set(
            &project.path().join("application"),
            "application",
            &["foundation"],
            true,
        );
        write_set(&project.path().join("unrelated"), "unrelated", &[], true);
        let catalog = load_catalog(
            project.path(),
            &[
                PathBuf::from("application"),
                PathBuf::from("unrelated"),
                PathBuf::from("foundation"),
            ],
        )
        .expect("load catalog");

        let first = build_plan(&catalog, Some("application")).expect("build selected plan");
        let second = build_plan(&catalog, Some("application")).expect("build selected plan again");

        assert_eq!(first, second);
        assert_eq!(first.selected_set.as_deref(), Some("application"));
        assert_eq!(
            first
                .sets
                .iter()
                .map(|set| set.id.as_str())
                .collect::<Vec<_>>(),
            ["foundation", "application"]
        );
        assert_eq!(first.digest.len(), 64);
    }

    #[test]
    fn target_status_detects_checksum_drift_and_orphaned_history() {
        let set = MigrationSet {
            id: "app".into(),
            owner: "application:test".into(),
            backend: DatabaseBackend::Sqlite,
            root: "migrations".into(),
            history_table: "_minco_test_migrations".into(),
            depends_on: Vec::new(),
            verify_tables: vec!["example".into()],
            digest: "set-digest".into(),
            migrations: vec![Migration {
                id: "app:1".into(),
                version: 1,
                description: "foundation".into(),
                path: "migrations/0001_foundation.sql".into(),
                sha256: "source".into(),
                sqlx_checksum_sha384: "expected".into(),
                risk: MigrationRisk::Additive,
                reversible: false,
            }],
        };
        let status = compare_target(
            &set,
            &TargetState {
                dirty_version: None,
                applied: vec![
                    AppliedMigration {
                        version: 1,
                        sqlx_checksum_sha384: "changed".into(),
                    },
                    AppliedMigration {
                        version: 2,
                        sqlx_checksum_sha384: "orphan".into(),
                    },
                ],
            },
        );

        assert_eq!(status.entries[0].state, MigrationState::Drift);
        assert_eq!(status.entries[1].state, MigrationState::MissingSource);
    }
}
