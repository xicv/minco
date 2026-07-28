use anyhow::{Context, Result, bail};
use minco_config::ConfigurationField;
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
    pub configuration: ConfigurationManifest,
    #[serde(default)]
    pub architecture: ArchitectureManifest,
    #[serde(default)]
    pub operations: BTreeMap<String, OperationTrace>,
    #[serde(default)]
    pub migrations: MigrationManifest,
    #[serde(default)]
    pub seeds: SeedManifest,
    #[serde(default)]
    pub commands: CommandManifest,
    #[serde(default)]
    pub development: DevelopmentManifest,
    #[serde(default)]
    pub plugins: PluginSelectionFile,
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct ConfigurationManifest {
    pub root: PathBuf,
    pub default_file: String,
    pub local_override: String,
    pub environment_prefix: String,
    #[serde(default)]
    pub fields: Vec<ConfigurationField>,
}

impl Default for ConfigurationManifest {
    fn default() -> Self {
        Self {
            root: PathBuf::from("config"),
            default_file: "default.toml".into(),
            local_override: ".local.toml".into(),
            environment_prefix: "MINCO_CONFIG__".into(),
            fields: Vec::new(),
        }
    }
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
    /// Operation-specific contract for plugin-owned routes. Application
    /// operations fall back to the manifest-level contract.
    pub contract: Option<PathBuf>,
    /// Generated binding source when this operation has one. Plugin-owned
    /// contracts may intentionally map directly to hand-written typed routes.
    pub generated: Option<PathBuf>,
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
pub struct SeedManifest {
    #[serde(default)]
    pub roots: Vec<PathBuf>,
}

#[derive(Debug, Clone, Default, Deserialize, serde::Serialize)]
pub struct CommandManifest {
    #[serde(default)]
    pub package: Vec<String>,
    #[serde(default, rename = "test_unit")]
    pub unit: Vec<String>,
    #[serde(default, rename = "test_feature")]
    pub feature: Vec<String>,
    #[serde(default, rename = "test_e2e")]
    pub e2e: Vec<String>,
    #[serde(default, rename = "test_all")]
    pub all: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct DevelopmentManifest {
    pub default_environment: String,
    pub default_profile: String,
    pub compose_file: PathBuf,
    #[serde(default)]
    pub profiles: BTreeMap<String, DevelopmentProfile>,
    pub api: Option<minco_dev::ProcessConfig>,
    #[serde(default)]
    pub workers: Vec<minco_dev::ProcessConfig>,
    pub frontend: Option<minco_dev::ProcessConfig>,
}

impl Default for DevelopmentManifest {
    fn default() -> Self {
        Self {
            default_environment: String::new(),
            default_profile: String::new(),
            compose_file: PathBuf::from("infra/local/compose.yaml"),
            profiles: BTreeMap::new(),
            api: None,
            workers: Vec::new(),
            frontend: None,
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize, serde::Serialize)]
pub struct DevelopmentProfile {
    pub deployment_config: PathBuf,
    pub migration: Option<minco_dev::CommandSpec>,
    #[serde(default)]
    pub seeds: BTreeMap<String, minco_dev::CommandSpec>,
}

#[derive(Debug, Clone, Default, Deserialize, serde::Serialize)]
pub struct PluginSelectionFile {
    #[serde(default)]
    pub enabled: BTreeSet<String>,
    #[serde(default)]
    pub disabled: BTreeSet<String>,
    /// Plugin-specific values keyed by stable plugin ID. Secret values should
    /// be injected by the composition root rather than committed here.
    #[serde(default)]
    pub configuration: BTreeMap<String, toml::Value>,
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
