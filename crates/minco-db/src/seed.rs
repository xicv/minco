use crate::{
    DatabaseBackend, DbLifecycleError, canonicalize, sha256_hex, validate_owner, validate_stable_id,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Component, Path, PathBuf},
};

pub const SEED_SET_MANIFEST: &str = ".minco-seeds.toml";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SeedClass {
    Reference,
    Demo,
    Test,
    Bootstrap,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SeedEnvironment {
    Local,
    Development,
    Test,
    Staging,
    Production,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SeedIdempotency {
    InsertOnce,
    Upsert,
    Reconcile,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SeedMutableState {
    None,
    OwnedRows,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SeedRisk {
    NonDestructive,
    ReplacesOwnedRows,
    Destructive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SeedTransaction {
    Required,
    Autocommit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SeedPreservation {
    PreserveAllExisting,
    PreserveUnownedRows,
    ReplaceOwnedRows,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeedCatalog {
    pub schema_version: u32,
    pub digest: String,
    pub sets: Vec<SeedSet>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeedSet {
    pub id: String,
    pub owner: String,
    pub backend: DatabaseBackend,
    pub root: PathBuf,
    pub digest: String,
    pub seeds: Vec<Seed>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeedPlan {
    pub schema_version: u32,
    pub catalog_digest: String,
    pub profile: SeedClass,
    pub environment: SeedEnvironment,
    pub selected_set: Option<String>,
    pub digest: String,
    pub seeds: Vec<Seed>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Seed {
    pub id: String,
    pub version: u32,
    pub set_id: String,
    pub root: PathBuf,
    pub owner: String,
    pub backend: DatabaseBackend,
    pub class: SeedClass,
    pub source: PathBuf,
    pub source_sha256: String,
    pub verify: PathBuf,
    pub verify_sha256: String,
    pub depends_on: Vec<String>,
    pub environments: Vec<SeedEnvironment>,
    pub idempotency: SeedIdempotency,
    pub mutable_state: SeedMutableState,
    pub risk: SeedRisk,
    pub transaction: SeedTransaction,
    pub preservation: SeedPreservation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedSeedSource {
    pub apply_sql: String,
    pub verify_sql: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeedVerification {
    pub seed_id: String,
    pub verified: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SeedSetManifest {
    schema: u32,
    id: String,
    owner: String,
    backend: DatabaseBackend,
    seed: Vec<SeedManifest>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SeedManifest {
    id: String,
    version: u32,
    class: SeedClass,
    source: PathBuf,
    verify: PathBuf,
    #[serde(default)]
    depends_on: Vec<String>,
    environments: Vec<SeedEnvironment>,
    idempotency: SeedIdempotency,
    mutable_state: SeedMutableState,
    risk: SeedRisk,
    transaction: SeedTransaction,
    preservation: SeedPreservation,
}

pub fn load_seed_catalog(
    project_root: &Path,
    seed_roots: &[PathBuf],
) -> Result<SeedCatalog, DbLifecycleError> {
    let project_root = canonicalize(project_root)?;
    let mut configured_roots = BTreeSet::new();
    let mut sets = Vec::new();
    for configured_root in seed_roots {
        if configured_root.is_absolute() {
            return Err(DbLifecycleError::Invalid(format!(
                "seed root {} must be relative to the project",
                configured_root.display()
            )));
        }
        let root = canonicalize(&project_root.join(configured_root))?;
        if !root.starts_with(&project_root) {
            return Err(DbLifecycleError::Invalid(format!(
                "seed root {} escapes the project",
                configured_root.display()
            )));
        }
        let relative_root = root
            .strip_prefix(&project_root)
            .map_err(|_| {
                DbLifecycleError::Invalid(format!(
                    "seed root {} escapes the project",
                    configured_root.display()
                ))
            })?
            .to_path_buf();
        if !configured_roots.insert(relative_root.clone()) {
            return Err(DbLifecycleError::Invalid(format!(
                "seed root {} is configured more than once",
                relative_root.display()
            )));
        }
        sets.push(load_seed_set(&project_root, &root, relative_root)?);
    }
    sets.sort_by(|left, right| left.id.cmp(&right.id));
    let mut set_ids = BTreeSet::new();
    let mut seed_ids = BTreeSet::new();
    for set in &sets {
        if !set_ids.insert(set.id.as_str()) {
            return Err(DbLifecycleError::Invalid(format!(
                "seed catalog repeats seed set ID {}",
                set.id
            )));
        }
        for seed in &set.seeds {
            if !seed_ids.insert(seed.id.as_str()) {
                return Err(DbLifecycleError::Invalid(format!(
                    "seed catalog repeats seed ID {}",
                    seed.id
                )));
            }
        }
    }
    validate_seed_dependencies(&sets)?;
    let digest = sha256_hex(&serde_json::to_vec(&sets)?);
    Ok(SeedCatalog {
        schema_version: 1,
        digest,
        sets,
    })
}

fn load_seed_set(
    project_root: &Path,
    root: &Path,
    relative_root: PathBuf,
) -> Result<SeedSet, DbLifecycleError> {
    let manifest_path = canonicalize(&root.join(SEED_SET_MANIFEST))?;
    if !manifest_path.starts_with(root) {
        return Err(DbLifecycleError::Invalid(format!(
            "seed metadata for root {} escapes its configured root",
            relative_root.display()
        )));
    }
    let manifest: SeedSetManifest =
        toml::from_str(&fs::read_to_string(&manifest_path).map_err(|source| {
            DbLifecycleError::Io {
                path: manifest_path.clone(),
                source,
            }
        })?)
        .map_err(|source| DbLifecycleError::Toml {
            path: manifest_path,
            source,
        })?;
    if manifest.schema != 1 {
        return Err(DbLifecycleError::Invalid(format!(
            "seed set {} uses unsupported schema {}",
            manifest.id, manifest.schema
        )));
    }
    validate_stable_id(&manifest.id, "seed set ID")?;
    validate_owner(&manifest.owner)?;
    if manifest.seed.is_empty() {
        return Err(DbLifecycleError::Invalid(format!(
            "seed set {} contains no seeds",
            manifest.id
        )));
    }
    let mut seeds = Vec::with_capacity(manifest.seed.len());
    let mut local_ids = BTreeSet::new();
    for entry in manifest.seed {
        validate_stable_id(&entry.id, "seed ID")?;
        if !local_ids.insert(entry.id.clone()) {
            return Err(DbLifecycleError::Invalid(format!(
                "seed set {} repeats seed ID {}",
                manifest.id, entry.id
            )));
        }
        if entry.version == 0 {
            return Err(DbLifecycleError::Invalid(format!(
                "seed {} has a zero version",
                entry.id
            )));
        }
        if entry.environments.is_empty() {
            return Err(DbLifecycleError::Invalid(format!(
                "seed {} has an empty environment allowlist",
                entry.id
            )));
        }
        let source = bounded_seed_file(project_root, root, &entry.source, "source")?;
        let verify = bounded_seed_file(project_root, root, &entry.verify, "verification")?;
        let source_bytes =
            fs::read(project_root.join(&source)).map_err(|source_error| DbLifecycleError::Io {
                path: source.clone(),
                source: source_error,
            })?;
        let verify_bytes =
            fs::read(project_root.join(&verify)).map_err(|source_error| DbLifecycleError::Io {
                path: verify.clone(),
                source: source_error,
            })?;
        let mut dependencies = BTreeSet::new();
        for dependency in entry.depends_on {
            validate_stable_id(&dependency, "seed dependency")?;
            if dependency == entry.id {
                return Err(DbLifecycleError::Invalid(format!(
                    "seed {} cannot depend on itself",
                    entry.id
                )));
            }
            if !dependencies.insert(dependency.clone()) {
                return Err(DbLifecycleError::Invalid(format!(
                    "seed {} repeats dependency {}",
                    entry.id, dependency
                )));
            }
        }
        let environment_count = entry.environments.len();
        let environments = entry.environments.into_iter().collect::<BTreeSet<_>>();
        if environments.len() != environment_count {
            return Err(DbLifecycleError::Invalid(format!(
                "seed {} repeats an environment allowlist entry",
                entry.id
            )));
        }
        seeds.push(Seed {
            id: entry.id,
            version: entry.version,
            set_id: manifest.id.clone(),
            root: relative_root.clone(),
            owner: manifest.owner.clone(),
            backend: manifest.backend,
            class: entry.class,
            source,
            source_sha256: sha256_hex(&source_bytes),
            verify,
            verify_sha256: sha256_hex(&verify_bytes),
            depends_on: dependencies.into_iter().collect(),
            environments: environments.into_iter().collect(),
            idempotency: entry.idempotency,
            mutable_state: entry.mutable_state,
            risk: entry.risk,
            transaction: entry.transaction,
            preservation: entry.preservation,
        });
    }
    seeds.sort_by(|left, right| left.id.cmp(&right.id));
    let mut set = SeedSet {
        id: manifest.id,
        owner: manifest.owner,
        backend: manifest.backend,
        root: relative_root,
        digest: String::new(),
        seeds,
    };
    set.digest = sha256_hex(&serde_json::to_vec(&set)?);
    Ok(set)
}

pub fn resolve_seed_source(
    project_root: &Path,
    seed: &Seed,
) -> Result<ResolvedSeedSource, DbLifecycleError> {
    let project_root = canonicalize(project_root)?;
    let root = canonicalize(&project_root.join(&seed.root))?;
    if !root.starts_with(&project_root) {
        return Err(DbLifecycleError::Invalid(format!(
            "seed {} source root escapes the project",
            seed.id
        )));
    }
    let apply_sql = read_planned_seed_file(
        &project_root,
        &root,
        &seed.source,
        &seed.source_sha256,
        &seed.id,
        "source",
    )?;
    let verify_sql = read_planned_seed_file(
        &project_root,
        &root,
        &seed.verify,
        &seed.verify_sha256,
        &seed.id,
        "verification",
    )?;
    Ok(ResolvedSeedSource {
        apply_sql,
        verify_sql,
    })
}

pub fn build_seed_plan(
    catalog: &SeedCatalog,
    profile: SeedClass,
    environment: SeedEnvironment,
    selected_set: Option<&str>,
) -> Result<SeedPlan, DbLifecycleError> {
    if environment == SeedEnvironment::Production
        && matches!(profile, SeedClass::Demo | SeedClass::Test)
    {
        let profile_name = match profile {
            SeedClass::Demo => "demo",
            SeedClass::Test => "test",
            SeedClass::Reference | SeedClass::Bootstrap => unreachable!(),
        };
        return Err(DbLifecycleError::Invalid(format!(
            "{profile_name} seeds are forbidden in production"
        )));
    }
    if let Some(selected_set) = selected_set
        && !catalog.sets.iter().any(|set| set.id == selected_set)
    {
        return Err(DbLifecycleError::Invalid(format!(
            "unknown seed set {selected_set}"
        )));
    }
    let by_id = catalog
        .sets
        .iter()
        .flat_map(|set| set.seeds.iter())
        .map(|seed| (seed.id.as_str(), seed))
        .collect::<BTreeMap<_, _>>();
    let roots = by_id
        .values()
        .filter(|seed| seed.class == profile)
        .filter(|seed| selected_set.is_none_or(|set| seed.set_id == set))
        .map(|seed| seed.id.as_str())
        .collect::<Vec<_>>();
    if roots.is_empty() {
        return Err(DbLifecycleError::Invalid(format!(
            "seed profile {profile:?} selects no seeds"
        )));
    }
    let mut visiting = BTreeSet::new();
    let mut visited = BTreeSet::new();
    let mut ordered = Vec::new();
    for id in roots {
        collect_seed(
            id,
            &by_id,
            environment,
            &mut visiting,
            &mut visited,
            &mut ordered,
        )?;
    }
    let seeds = ordered.into_iter().cloned().collect::<Vec<_>>();
    if seeds
        .iter()
        .any(|seed| seed.transaction != seeds[0].transaction)
    {
        return Err(DbLifecycleError::Invalid(
            "an executable seed plan cannot mix transaction behaviors".into(),
        ));
    }
    let selected_set = selected_set.map(str::to_owned);
    let digest_input = serde_json::to_vec(&(
        1_u32,
        catalog.digest.as_str(),
        profile,
        environment,
        selected_set.as_deref(),
        &seeds,
    ))?;
    let plan = SeedPlan {
        schema_version: 1,
        catalog_digest: catalog.digest.clone(),
        profile,
        environment,
        selected_set,
        digest: sha256_hex(&digest_input),
        seeds,
    };
    validate_seed_plan(&plan)?;
    Ok(plan)
}

pub fn validate_seed_plan(plan: &SeedPlan) -> Result<(), DbLifecycleError> {
    if plan.schema_version != 1 {
        return Err(DbLifecycleError::Invalid(format!(
            "seed plan uses unsupported schema {}",
            plan.schema_version
        )));
    }
    if plan.seeds.is_empty() {
        return Err(DbLifecycleError::Invalid(
            "seed plan contains no seeds".into(),
        ));
    }
    let mut ordered_ids = BTreeSet::new();
    for seed in &plan.seeds {
        if ordered_ids.contains(seed.id.as_str()) {
            return Err(DbLifecycleError::Invalid(format!(
                "seed plan repeats seed ID {}",
                seed.id
            )));
        }
        if plan.environment == SeedEnvironment::Production
            && matches!(seed.class, SeedClass::Demo | SeedClass::Test)
        {
            return Err(DbLifecycleError::Invalid(format!(
                "seed {} is forbidden in production because it is classified as {:?}",
                seed.id, seed.class
            )));
        }
        if !seed.environments.contains(&plan.environment) {
            return Err(DbLifecycleError::Invalid(format!(
                "seed {} is not allowed in environment {:?}",
                seed.id, plan.environment
            )));
        }
        for dependency in &seed.depends_on {
            if !ordered_ids.contains(dependency.as_str()) {
                return Err(DbLifecycleError::Invalid(format!(
                    "seed {} dependency {} is absent or ordered after it",
                    seed.id, dependency
                )));
            }
        }
        ordered_ids.insert(seed.id.as_str());
    }
    if plan
        .seeds
        .iter()
        .any(|seed| seed.transaction != plan.seeds[0].transaction)
    {
        return Err(DbLifecycleError::Invalid(
            "an executable seed plan cannot mix transaction behaviors".into(),
        ));
    }
    let digest_input = serde_json::to_vec(&(
        1_u32,
        plan.catalog_digest.as_str(),
        plan.profile,
        plan.environment,
        plan.selected_set.as_deref(),
        &plan.seeds,
    ))?;
    if plan.digest != sha256_hex(&digest_input) {
        return Err(DbLifecycleError::Invalid(
            "seed plan digest does not match its contents".into(),
        ));
    }
    Ok(())
}

fn collect_seed<'a>(
    id: &'a str,
    seeds: &BTreeMap<&'a str, &'a Seed>,
    environment: SeedEnvironment,
    visiting: &mut BTreeSet<&'a str>,
    visited: &mut BTreeSet<&'a str>,
    ordered: &mut Vec<&'a Seed>,
) -> Result<(), DbLifecycleError> {
    if visited.contains(id) {
        return Ok(());
    }
    if !visiting.insert(id) {
        return Err(DbLifecycleError::Invalid(format!(
            "seed dependency cycle contains {id}"
        )));
    }
    let seed = seeds
        .get(id)
        .ok_or_else(|| DbLifecycleError::Invalid(format!("unknown seed {id}")))?;
    if environment == SeedEnvironment::Production
        && matches!(seed.class, SeedClass::Demo | SeedClass::Test)
    {
        return Err(DbLifecycleError::Invalid(format!(
            "seed {} is forbidden in production because it is classified as {:?}",
            seed.id, seed.class
        )));
    }
    if !seed.environments.contains(&environment) {
        return Err(DbLifecycleError::Invalid(format!(
            "seed {} is not allowed in environment {environment:?}",
            seed.id
        )));
    }
    for dependency in &seed.depends_on {
        let dependency_seed = seeds.get(dependency.as_str()).ok_or_else(|| {
            DbLifecycleError::Invalid(format!(
                "seed {} depends on unknown seed {}",
                seed.id, dependency
            ))
        })?;
        if dependency_seed.backend != seed.backend {
            return Err(DbLifecycleError::Invalid(format!(
                "seed {} cannot depend on {} because their backends differ",
                seed.id, dependency
            )));
        }
        collect_seed(dependency, seeds, environment, visiting, visited, ordered)?;
    }
    visiting.remove(id);
    visited.insert(id);
    ordered.push(seed);
    Ok(())
}

fn validate_seed_dependencies(sets: &[SeedSet]) -> Result<(), DbLifecycleError> {
    let seeds = sets
        .iter()
        .flat_map(|set| set.seeds.iter())
        .map(|seed| (seed.id.as_str(), seed))
        .collect::<BTreeMap<_, _>>();
    for seed in seeds.values() {
        for dependency in &seed.depends_on {
            let dependency_seed = seeds.get(dependency.as_str()).ok_or_else(|| {
                DbLifecycleError::Invalid(format!(
                    "seed {} depends on unknown seed {}",
                    seed.id, dependency
                ))
            })?;
            if dependency_seed.backend != seed.backend {
                return Err(DbLifecycleError::Invalid(format!(
                    "seed {} cannot depend on {} because their backends differ",
                    seed.id, dependency
                )));
            }
        }
    }
    let mut visiting = BTreeSet::new();
    let mut visited = BTreeSet::new();
    for id in seeds.keys().copied() {
        visit_seed_dependency(id, &seeds, &mut visiting, &mut visited)?;
    }
    Ok(())
}

fn visit_seed_dependency<'a>(
    id: &'a str,
    seeds: &BTreeMap<&'a str, &'a Seed>,
    visiting: &mut BTreeSet<&'a str>,
    visited: &mut BTreeSet<&'a str>,
) -> Result<(), DbLifecycleError> {
    if visited.contains(id) {
        return Ok(());
    }
    if !visiting.insert(id) {
        return Err(DbLifecycleError::Invalid(format!(
            "seed dependency cycle contains {id}"
        )));
    }
    let seed = seeds
        .get(id)
        .ok_or_else(|| DbLifecycleError::Invalid(format!("unknown seed {id}")))?;
    for dependency in &seed.depends_on {
        visit_seed_dependency(dependency, seeds, visiting, visited)?;
    }
    visiting.remove(id);
    visited.insert(id);
    Ok(())
}

fn bounded_seed_file(
    project_root: &Path,
    root: &Path,
    relative: &Path,
    kind: &str,
) -> Result<PathBuf, DbLifecycleError> {
    if relative.as_os_str().is_empty()
        || relative.is_absolute()
        || relative.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
        || relative.extension().and_then(|value| value.to_str()) != Some("sql")
    {
        return Err(DbLifecycleError::Invalid(format!(
            "seed {kind} path {} must be a relative SQL file",
            relative.display()
        )));
    }
    let path = canonicalize(&root.join(relative))?;
    if !path.starts_with(root) {
        return Err(DbLifecycleError::Invalid(format!(
            "seed {kind} path {} escapes its configured root",
            relative.display()
        )));
    }
    path.strip_prefix(project_root)
        .map(Path::to_path_buf)
        .map_err(|_| {
            DbLifecycleError::Invalid(format!(
                "seed {kind} path {} escapes the project",
                relative.display()
            ))
        })
}

fn read_planned_seed_file(
    project_root: &Path,
    root: &Path,
    relative: &Path,
    expected_digest: &str,
    seed_id: &str,
    kind: &str,
) -> Result<String, DbLifecycleError> {
    if relative.is_absolute() {
        return Err(DbLifecycleError::Invalid(format!(
            "seed {seed_id} {kind} path must be project-relative"
        )));
    }
    let path = canonicalize(&project_root.join(relative))?;
    if !path.starts_with(root) {
        return Err(DbLifecycleError::Invalid(format!(
            "seed {seed_id} {kind} path escapes its configured root"
        )));
    }
    let bytes = fs::read(&path).map_err(|source| DbLifecycleError::Io {
        path: path.clone(),
        source,
    })?;
    if sha256_hex(&bytes) != expected_digest {
        return Err(DbLifecycleError::Invalid(format!(
            "seed {seed_id} {kind} changed after planning"
        )));
    }
    String::from_utf8(bytes)
        .map_err(|_| DbLifecycleError::Invalid(format!("seed {seed_id} {kind} is not UTF-8")))
}
