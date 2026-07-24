use anyhow::{Context, Result};
use serde::Serialize;
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

use crate::config::ArchitectureManifest;

#[derive(Debug, Clone, Serialize)]
pub struct ArchitectureFinding {
    pub code: &'static str,
    pub layer: &'static str,
    pub manifest: PathBuf,
    pub dependency: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ArchitectureReport {
    pub status: &'static str,
    pub inspected_manifests: Vec<PathBuf>,
    pub findings: Vec<ArchitectureFinding>,
}

pub fn validate_architecture(
    root: &Path,
    architecture: &ArchitectureManifest,
) -> Result<ArchitectureReport> {
    let mut inspected = BTreeSet::new();
    let mut findings = Vec::new();
    inspect_layer(
        root,
        "domain",
        &architecture.domain_roots,
        &[
            "axum",
            "http",
            "tower",
            "sqlx",
            "aws-",
            "lambda-",
            "lambda_",
            "minco-contract",
            "minco-http",
            "minco-sqlx",
            "minco-aws",
        ],
        &mut inspected,
        &mut findings,
    )?;
    inspect_layer(
        root,
        "application",
        &architecture.application_roots,
        &[
            "axum",
            "http",
            "tower",
            "sqlx",
            "aws-",
            "lambda-",
            "lambda_",
            "minco-contract",
            "minco-http",
            "minco-sqlx",
            "minco-aws",
        ],
        &mut inspected,
        &mut findings,
    )?;
    inspect_layer(
        root,
        "api",
        &architecture.api_roots,
        &[
            "sqlx",
            "aws-sdk-",
            "aws-config",
            "lambda-",
            "lambda_",
            "minco-sqlx",
            "minco-aws",
        ],
        &mut inspected,
        &mut findings,
    )?;

    let inspected_manifests = inspected.into_iter().collect::<Vec<_>>();
    Ok(ArchitectureReport {
        status: if findings.is_empty() { "ok" } else { "error" },
        inspected_manifests,
        findings,
    })
}

fn inspect_layer(
    root: &Path,
    layer: &'static str,
    roots: &[PathBuf],
    forbidden: &[&str],
    inspected: &mut BTreeSet<PathBuf>,
    findings: &mut Vec<ArchitectureFinding>,
) -> Result<()> {
    for relative in roots {
        let absolute = root.join(relative);
        for manifest in find_manifests(&absolute)? {
            let source = fs::read_to_string(&manifest)
                .with_context(|| format!("read architecture manifest {}", manifest.display()))?;
            let document: toml::Value = toml::from_str(&source)
                .with_context(|| format!("parse architecture manifest {}", manifest.display()))?;
            let display_path = manifest
                .strip_prefix(root)
                .unwrap_or(&manifest)
                .to_path_buf();
            inspected.insert(display_path.clone());
            for section in ["dependencies", "build-dependencies"] {
                let Some(dependencies) = document.get(section).and_then(toml::Value::as_table)
                else {
                    continue;
                };
                for (alias, specification) in dependencies {
                    let package = specification
                        .as_table()
                        .and_then(|table| table.get("package"))
                        .and_then(toml::Value::as_str)
                        .unwrap_or(alias);
                    if let Some(rule) = forbidden.iter().find(|rule| matches_rule(package, rule)) {
                        findings.push(ArchitectureFinding {
                            code: match layer {
                                "domain" => "MINCO-ARCH-001",
                                "application" => "MINCO-ARCH-002",
                                _ => "MINCO-ARCH-003",
                            },
                            layer,
                            manifest: display_path.clone(),
                            dependency: package.to_owned(),
                            message: format!(
                                "{layer} package depends on forbidden boundary `{package}` (rule `{rule}`)"
                            ),
                        });
                    }
                }
            }
        }
    }
    Ok(())
}

fn matches_rule(package: &str, rule: &str) -> bool {
    if rule.ends_with('-') || rule.ends_with('_') {
        package.starts_with(rule)
    } else {
        package == rule || package.starts_with(&format!("{rule}-"))
    }
}

fn find_manifests(root: &Path) -> Result<Vec<PathBuf>> {
    if root.is_file() {
        return Ok(
            (root.file_name().and_then(|name| name.to_str()) == Some("Cargo.toml"))
                .then(|| root.to_path_buf())
                .into_iter()
                .collect(),
        );
    }
    if !root.exists() {
        return Ok(Vec::new());
    }
    let direct = root.join("Cargo.toml");
    if direct.is_file() {
        return Ok(vec![direct]);
    }
    let mut output = Vec::new();
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            output.extend(find_manifests(&path)?);
        }
    }
    output.sort();
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefix_rules_cover_provider_crates() {
        assert!(matches_rule("aws-sdk-s3", "aws-"));
        assert!(matches_rule("minco-sqlx-postgres", "minco-sqlx"));
        assert!(!matches_rule("serde", "sqlx"));
    }
}
