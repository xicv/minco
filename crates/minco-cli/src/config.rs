use anyhow::{Context, Result, bail};
use serde::Deserialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct MincoManifest {
    pub schema: u32,
    pub name: String,
    pub contract: PathBuf,
    pub generated: PathBuf,
    pub deployment_config: PathBuf,
    pub roadmap: PathBuf,
    pub tasks: PathBuf,
    pub plugin_catalog: PathBuf,
    pub quality: PathBuf,
    #[serde(default)]
    pub architecture: ArchitectureManifest,
    #[serde(default)]
    pub operations: BTreeMap<String, OperationTrace>,
    #[serde(default)]
    pub migrations: MigrationManifest,
    #[serde(default)]
    pub commands: CommandManifest,
    #[serde(default)]
    pub plugins: PluginSelectionFile,
}

#[derive(Debug, Clone, Default, Deserialize, serde::Serialize)]
// These names mirror the stable `[architecture]` keys in minco.toml.
#[allow(clippy::struct_field_names)]
pub struct ArchitectureManifest {
    #[serde(default)]
    pub domain_roots: Vec<PathBuf>,
    #[serde(default)]
    pub application_roots: Vec<PathBuf>,
    #[serde(default)]
    pub api_roots: Vec<PathBuf>,
}

#[derive(Debug, Clone, Default, Deserialize, serde::Serialize)]
pub struct OperationTrace {
    pub handler: Option<String>,
    pub application: Option<String>,
    #[serde(default)]
    pub adapters: Vec<String>,
    #[serde(default)]
    pub tests: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize, serde::Serialize)]
pub struct MigrationManifest {
    #[serde(default)]
    pub roots: Vec<PathBuf>,
}

#[derive(Debug, Clone, Default, Deserialize, serde::Serialize)]
pub struct CommandManifest {
    pub database_migrate: Option<String>,
    #[serde(default)]
    pub test_unit: Vec<String>,
    #[serde(default)]
    pub test_feature: Vec<String>,
    #[serde(default)]
    pub test_e2e: Vec<String>,
    #[serde(default)]
    pub test_all: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize, serde::Serialize)]
pub struct PluginSelectionFile {
    #[serde(default)]
    pub enabled: BTreeSet<String>,
    #[serde(default)]
    pub disabled: BTreeSet<String>,
}

impl MincoManifest {
    pub fn load(root: &Path) -> Result<Self> {
        let path = root.join("minco.toml");
        let value: Self = toml::from_str(
            &std::fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?,
        )
        .with_context(|| format!("parse {}", path.display()))?;
        if value.schema != 1 {
            bail!("unsupported minco.toml schema {}", value.schema);
        }
        Ok(value)
    }
}

pub fn discover_root(explicit: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(root) = explicit {
        let root = root
            .canonicalize()
            .with_context(|| format!("resolve {}", root.display()))?;
        if !root.join("minco.toml").is_file() {
            bail!("{} does not contain minco.toml", root.display());
        }
        return Ok(root);
    }
    let mut current = std::env::current_dir()?;
    loop {
        if current.join("minco.toml").is_file() {
            return Ok(current);
        }
        if !current.pop() {
            bail!("could not find minco.toml in the current directory or any parent");
        }
    }
}
