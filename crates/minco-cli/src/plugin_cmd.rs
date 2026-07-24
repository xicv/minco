use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, path::{Path, PathBuf}};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PluginCatalog {
    pub schema: u32,
    #[serde(default)]
    pub plugin: Vec<PluginCatalogEntry>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PluginCatalogEntry {
    pub id: String,
    #[serde(rename = "crate")]
    pub crate_name: String,
    /// Repository-relative path for a workspace plugin. Omit for a registry dependency.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    pub default_enabled: bool,
    pub stability: String,
    pub description: String,
}

pub fn load_catalog(root: &Path, relative: &Path) -> Result<PluginCatalog> {
    let path = root.join(relative);
    let catalog: PluginCatalog = toml::from_str(&std::fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?)?;
    if catalog.schema != 1 {
        bail!("unsupported plugin catalog schema {}", catalog.schema);
    }
    Ok(catalog)
}

pub fn set_plugin_state(root: &Path, plugin_id: &str, enabled: bool) -> Result<()> {
    validate_plugin_id(plugin_id)?;
    let path = root.join("minco.toml");
    let mut document: toml::Value = toml::from_str(&std::fs::read_to_string(&path)?)?;
    let plugins = document
        .as_table_mut()
        .context("minco.toml root must be a table")?
        .entry("plugins")
        .or_insert_with(|| toml::Value::Table(toml::map::Map::new()))
        .as_table_mut()
        .context("plugins must be a table")?;
    let mut enabled_set = read_string_set(plugins.get("enabled"))?;
    let mut disabled_set = read_string_set(plugins.get("disabled"))?;
    if enabled {
        enabled_set.insert(plugin_id.into());
        disabled_set.remove(plugin_id);
    } else {
        disabled_set.insert(plugin_id.into());
        enabled_set.remove(plugin_id);
    }
    plugins.insert("enabled".into(), string_array(enabled_set));
    plugins.insert("disabled".into(), string_array(disabled_set));
    std::fs::write(path, toml::to_string_pretty(&document)?)?;
    Ok(())
}

pub fn scaffold_plugin(root: &Path, plugin_id: &str) -> Result<()> {
    validate_plugin_id(plugin_id)?;
    let crate_name = format!("minco-plugin-{plugin_id}");
    let directory = root.join("plugins").join(&crate_name);
    if directory.exists() {
        bail!("plugin directory {} already exists", directory.display());
    }
    std::fs::create_dir_all(directory.join("src"))?;
    std::fs::write(
        directory.join("Cargo.toml"),
        format!(
            "[package]\nname = \"{crate_name}\"\nversion.workspace = true\nedition.workspace = true\nrust-version.workspace = true\nlicense.workspace = true\nrepository.workspace = true\npublish = false\n\n[dependencies]\nminco-core.workspace = true\nsemver.workspace = true\n\n[lints]\nworkspace = true\n"
        ),
    )?;
    let type_name = plugin_id
        .split('-')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            chars.next().map_or_else(String::new, |first| first.to_ascii_uppercase().to_string() + chars.as_str())
        })
        .collect::<String>();
    std::fs::write(
        directory.join("src/lib.rs"),
        format!(
            "//! Minco plugin `{plugin_id}`.\n#![forbid(unsafe_code)]\n\nuse minco_core::{{Plugin, PluginContext, PluginDescriptor, PluginError, PluginId}};\nuse semver::Version;\n\n#[derive(Debug, Clone, Default)]\npub struct {type_name}Plugin;\n\nimpl Plugin for {type_name}Plugin {{\n    fn descriptor(&self) -> PluginDescriptor {{\n        PluginDescriptor::new(\n            PluginId::new(\"{plugin_id}\").expect(\"static plugin ID\"),\n            Version::new(0, 1, 0),\n            \"Describe the plugin capability\",\n        )\n    }}\n\n    fn install(&self, _context: &mut PluginContext<'_>) -> Result<(), PluginError> {{\n        Ok(())\n    }}\n}}\n\n#[cfg(test)]\nmod tests {{\n    use super::*;\n\n    #[test]\n    fn descriptor_has_the_expected_identity() {{\n        assert_eq!({type_name}Plugin.descriptor().id.as_str(), \"{plugin_id}\");\n    }}\n}}\n"
        ),
    )?;
    add_workspace_member(root, &format!("plugins/{crate_name}"), &crate_name)?;
    add_catalog_entry(root, plugin_id, &crate_name)?;
    Ok(())
}

pub fn validate_catalog(root: &Path, catalog: &PluginCatalog) -> Result<Vec<String>> {
    let mut findings = Vec::new();
    let mut ids = BTreeSet::new();
    for plugin in &catalog.plugin {
        validate_plugin_id(&plugin.id)?;
        if !ids.insert(&plugin.id) {
            findings.push(format!("duplicate plugin ID {}", plugin.id));
        }
        if let Some(relative) = &plugin.path {
            let manifest = root.join(relative).join("Cargo.toml");
            if !manifest.is_file() {
                findings.push(format!(
                    "plugin {} references missing local manifest {}",
                    plugin.id,
                    manifest.display()
                ));
            } else if let Ok(source) = std::fs::read_to_string(&manifest) {
                match toml::from_str::<toml::Value>(&source) {
                    Ok(document) => {
                        let package_name = document
                            .get("package")
                            .and_then(|value| value.get("name"))
                            .and_then(toml::Value::as_str);
                        if package_name != Some(plugin.crate_name.as_str()) {
                            findings.push(format!(
                                "plugin {} path {} declares package {:?}, expected {}",
                                plugin.id,
                                relative.display(),
                                package_name,
                                plugin.crate_name
                            ));
                        }
                    }
                    Err(error) => findings.push(format!(
                        "plugin {} local manifest {} is invalid TOML: {error}",
                        plugin.id,
                        manifest.display()
                    )),
                }
            }
        }
    }
    Ok(findings)
}

fn add_workspace_member(root: &Path, member: &str, crate_name: &str) -> Result<()> {
    let path = root.join("Cargo.toml");
    let mut document: toml::Value = toml::from_str(&std::fs::read_to_string(&path)?)?;
    let root_table = document.as_table_mut().context("Cargo root")?;
    let workspace = root_table
        .get_mut("workspace")
        .and_then(toml::Value::as_table_mut)
        .context("workspace table")?;
    for key in ["members", "default-members"] {
        let values = workspace
            .get_mut(key)
            .and_then(toml::Value::as_array_mut)
            .context("workspace member list")?;
        if !values.iter().any(|value| value.as_str() == Some(member)) {
            values.push(toml::Value::String(member.into()));
        }
    }
    let dependencies = workspace
        .entry("dependencies")
        .or_insert_with(|| toml::Value::Table(toml::map::Map::new()))
        .as_table_mut()
        .context("workspace.dependencies table")?;
    dependencies.insert(
        crate_name.into(),
        toml::Value::Table(toml::map::Map::from_iter([(
            String::from("path"),
            toml::Value::String(member.into()),
        )])),
    );
    std::fs::write(path, toml::to_string_pretty(&document)?)?;
    Ok(())
}

fn add_catalog_entry(root: &Path, id: &str, crate_name: &str) -> Result<()> {
    let path = root.join("plugins/catalog.toml");
    let mut catalog: PluginCatalog = toml::from_str(&std::fs::read_to_string(&path)?)?;
    catalog.plugin.push(PluginCatalogEntry {
        id: id.into(),
        crate_name: crate_name.into(),
        path: Some(std::path::PathBuf::from(format!("plugins/{crate_name}"))),
        default_enabled: false,
        stability: "experimental".into(),
        description: "User-defined Minco plugin.".into(),
    });
    catalog.plugin.sort_by(|left, right| left.id.cmp(&right.id));
    std::fs::write(path, toml::to_string_pretty(&catalog)?)?;
    Ok(())
}

fn read_string_set(value: Option<&toml::Value>) -> Result<BTreeSet<String>> {
    value
        .and_then(toml::Value::as_array)
        .into_iter()
        .flatten()
        .map(|value| value.as_str().map(str::to_owned).context("plugin selection values must be strings"))
        .collect()
}

fn string_array(values: BTreeSet<String>) -> toml::Value {
    toml::Value::Array(values.into_iter().map(toml::Value::String).collect())
}

fn validate_plugin_id(value: &str) -> Result<()> {
    if value.is_empty()
        || !value.as_bytes().first().is_some_and(|byte| byte.is_ascii_lowercase())
        || value.starts_with('-')
        || value.ends_with('-')
        || value.contains("--")
        || !value.bytes().all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        bail!("plugin ID must be lower-kebab-case");
    }
    Ok(())
}
