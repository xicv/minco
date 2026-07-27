use anyhow::{Context, Result, bail};
use clap::ValueEnum;
use minco_contract::{generate_rust, load_contract};
use serde::Serialize;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use crate::process::command_available;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum DatabaseChoice {
    Postgres,
    Sqlite,
}

impl DatabaseChoice {
    const fn feature(self) -> &'static str {
        match self {
            Self::Postgres => "sqlx-postgres",
            Self::Sqlite => "sqlx-sqlite",
        }
    }

    const fn migration_directory(self) -> &'static str {
        match self {
            Self::Postgres => "migrations/postgres",
            Self::Sqlite => "migrations/sqlite",
        }
    }

    const fn environment_template(self) -> &'static str {
        match self {
            Self::Postgres => include_str!("../templates/app/environments/dev-postgres.toml.tmpl"),
            Self::Sqlite => include_str!("../templates/app/environments/dev-sqlite.toml.tmpl"),
        }
    }

    const fn adapter_template(self) -> &'static str {
        match self {
            Self::Postgres => {
                include_str!("../templates/app/crates/adapters/src/lib-postgres.rs.tmpl")
            }
            Self::Sqlite => include_str!("../templates/app/crates/adapters/src/lib-sqlite.rs.tmpl"),
        }
    }

    const fn migration_template(self) -> &'static str {
        match self {
            Self::Postgres => {
                include_str!("../templates/app/migrations/postgres/0001_foundation.sql.tmpl")
            }
            Self::Sqlite => {
                include_str!("../templates/app/migrations/sqlite/0001_foundation.sql.tmpl")
            }
        }
    }

    const fn database_env(self) -> &'static str {
        match self {
            Self::Postgres => {
                "DATABASE_URL=postgresql://postgres:postgres@127.0.0.1:5432/app\nDATABASE_MIGRATION_URL=postgresql://postgres:postgres@127.0.0.1:5432/app"
            }
            Self::Sqlite => "DATABASE_PATH=var/app.db",
        }
    }

    const fn database_secret_reference(self) -> &'static str {
        match self {
            Self::Postgres => "env:DATABASE_URL",
            Self::Sqlite => "env:DATABASE_PATH",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum VcsChoice {
    Jj,
    None,
}

#[derive(Debug, Clone)]
pub struct NewProjectOptions {
    pub name: String,
    pub directory: Option<PathBuf>,
    pub database: DatabaseChoice,
    pub vcs: VcsChoice,
}

#[derive(Debug, Clone, Serialize)]
pub struct NewProjectReport {
    pub name: String,
    pub directory: PathBuf,
    pub database: String,
    pub vcs: String,
    pub generated_contract: PathBuf,
    pub next_commands: Vec<String>,
}

struct Template {
    path: &'static str,
    source: &'static str,
}

const COMMON_TEMPLATES: &[Template] = &[
    Template {
        path: "Cargo.toml",
        source: include_str!("../templates/app/Cargo.toml.tmpl"),
    },
    Template {
        path: "README.md",
        source: include_str!("../templates/app/README.md.tmpl"),
    },
    Template {
        path: "AGENTS.md",
        source: include_str!("../templates/app/AGENTS.md.tmpl"),
    },
    Template {
        path: ".gitignore",
        source: include_str!("../templates/app/.gitignore.tmpl"),
    },
    Template {
        path: ".env.example",
        source: include_str!("../templates/app/.env.example.tmpl"),
    },
    Template {
        path: "rust-toolchain.toml",
        source: include_str!("../templates/app/rust-toolchain.toml.tmpl"),
    },
    Template {
        path: "minco.toml",
        source: include_str!("../templates/app/minco.toml.tmpl"),
    },
    Template {
        path: "config/environments/default.toml",
        source: include_str!("../templates/app/config/environments/default.toml.tmpl"),
    },
    Template {
        path: "config/environments/dev.toml",
        source: include_str!("../templates/app/config/environments/dev.toml.tmpl"),
    },
    Template {
        path: "quality.toml",
        source: include_str!("../templates/app/quality.toml.tmpl"),
    },
    Template {
        path: "plugins/catalog.toml",
        source: include_str!("../templates/app/plugins/catalog.toml.tmpl"),
    },
    Template {
        path: "roadmap/roadmap.yaml",
        source: include_str!("../templates/app/roadmap/roadmap.yaml.tmpl"),
    },
    Template {
        path: "tasks/M0/M0-T01-foundation.md",
        source: include_str!("../templates/app/tasks/M0/M0-T01-foundation.md.tmpl"),
    },
    Template {
        path: "openapi/openapi.yaml",
        source: include_str!("../templates/app/openapi/openapi.yaml.tmpl"),
    },
    Template {
        path: "crates/domain/Cargo.toml",
        source: include_str!("../templates/app/crates/domain/Cargo.toml.tmpl"),
    },
    Template {
        path: "crates/domain/src/lib.rs",
        source: include_str!("../templates/app/crates/domain/src/lib.rs.tmpl"),
    },
    Template {
        path: "crates/application/Cargo.toml",
        source: include_str!("../templates/app/crates/application/Cargo.toml.tmpl"),
    },
    Template {
        path: "crates/application/src/lib.rs",
        source: include_str!("../templates/app/crates/application/src/lib.rs.tmpl"),
    },
    Template {
        path: "crates/adapters/Cargo.toml",
        source: include_str!("../templates/app/crates/adapters/Cargo.toml.tmpl"),
    },
    Template {
        path: "crates/api/Cargo.toml",
        source: include_str!("../templates/app/crates/api/Cargo.toml.tmpl"),
    },
    Template {
        path: "crates/api/src/lib.rs",
        source: include_str!("../templates/app/crates/api/src/lib.rs.tmpl"),
    },
    Template {
        path: "services/app/Cargo.toml",
        source: include_str!("../templates/app/services/app/Cargo.toml.tmpl"),
    },
    Template {
        path: "services/app/src/lib.rs",
        source: include_str!("../templates/app/services/app/src/lib.rs.tmpl"),
    },
    Template {
        path: "services/app/src/main.rs",
        source: include_str!("../templates/app/services/app/src/main.rs.tmpl"),
    },
    Template {
        path: "services/app/src/bin/lambda.rs",
        source: include_str!("../templates/app/services/app/src/bin/lambda.rs.tmpl"),
    },
    Template {
        path: "services/app/src/bin/migrate.rs",
        source: include_str!("../templates/app/services/app/src/bin/migrate.rs.tmpl"),
    },
];

pub fn create_project(options: &NewProjectOptions) -> Result<NewProjectReport> {
    validate_package_name(&options.name)?;
    if options.vcs == VcsChoice::Jj && !command_available("jj") {
        bail!("JJ is required by the default VCS profile; install `jj` or pass `--vcs none`");
    }

    let directory = options
        .directory
        .clone()
        .unwrap_or_else(|| PathBuf::from(&options.name));
    let directory = if directory.is_absolute() {
        directory
    } else {
        std::env::current_dir()?.join(directory)
    };
    if directory.exists() {
        if !directory.is_dir() {
            bail!("{} exists and is not a directory", directory.display());
        }
        if fs::read_dir(&directory)?.next().is_some() {
            bail!("{} exists and is not empty", directory.display());
        }
    } else {
        fs::create_dir_all(&directory)?;
    }

    if options.vcs == VcsChoice::Jj {
        let status = Command::new("jj")
            .args(["git", "init", "--colocate", "."])
            .current_dir(&directory)
            .status()
            .context("initialize colocated JJ/Git repository")?;
        if !status.success() {
            bail!(
                "`jj git init --colocate .` failed in {}",
                directory.display()
            );
        }
    }

    let crate_name = options.name.replace('-', "_");
    let title = title_case(&options.name);
    let replacements = BTreeMap::from([
        ("{{PACKAGE}}", options.name.as_str()),
        ("{{CRATE}}", crate_name.as_str()),
        ("{{TITLE}}", title.as_str()),
        ("{{MINCO_VERSION}}", env!("CARGO_PKG_VERSION")),
        ("{{DB_FEATURE}}", options.database.feature()),
        ("{{MIGRATION_DIR}}", options.database.migration_directory()),
        ("{{DATABASE_ENV}}", options.database.database_env()),
        (
            "{{DATABASE_SECRET_REFERENCE}}",
            options.database.database_secret_reference(),
        ),
    ]);

    for template in COMMON_TEMPLATES {
        write_rendered(&directory, template.path, template.source, &replacements)?;
    }
    write_rendered(
        &directory,
        "crates/adapters/src/lib.rs",
        options.database.adapter_template(),
        &replacements,
    )?;
    write_rendered(
        &directory,
        "environments/dev.toml",
        options.database.environment_template(),
        &replacements,
    )?;
    write_rendered(
        &directory,
        &format!(
            "{}/0001_foundation.sql",
            options.database.migration_directory()
        ),
        options.database.migration_template(),
        &replacements,
    )?;

    let contract = load_contract(directory.join("openapi/openapi.yaml"))?;
    if !contract.is_valid() {
        bail!(
            "generated OpenAPI contract failed validation: {}",
            serde_json::to_string_pretty(&contract.findings)?
        );
    }
    let generated_path = directory.join("crates/api/src/generated.rs");
    fs::write(&generated_path, generate_rust(&contract.document))?;

    if options.vcs == VcsChoice::Jj {
        let status = Command::new("jj")
            .args([
                "describe",
                "-m",
                &format!("chore: initialize {} with Minco", options.name),
            ])
            .current_dir(&directory)
            .status()
            .context("describe initial JJ change")?;
        if !status.success() {
            bail!("`jj describe` failed in {}", directory.display());
        }
    }

    Ok(NewProjectReport {
        name: options.name.clone(),
        directory: directory.clone(),
        database: format!("{:?}", options.database).to_ascii_lowercase(),
        vcs: format!("{:?}", options.vcs).to_ascii_lowercase(),
        generated_contract: generated_path,
        next_commands: vec![
            format!("cd {}", directory.display()),
            "cp .env.example .env".into(),
            "cargo minco doctor".into(),
            "cargo minco config check".into(),
            "cargo minco contract sync --check".into(),
            "cargo minco check --with-cargo".into(),
            format!(
                "cargo run -p {}-service --bin {}-migrate",
                options.name, options.name
            ),
            format!(
                "cargo run -p {}-service --bin {}-local",
                options.name, options.name
            ),
        ],
    })
}

fn write_rendered(
    root: &Path,
    relative: &str,
    source: &str,
    replacements: &BTreeMap<&str, &str>,
) -> Result<()> {
    let mut rendered = source.to_owned();
    for (needle, replacement) in replacements {
        rendered = rendered.replace(*needle, replacement);
    }
    if rendered.contains("{{") {
        bail!("template {relative} contains an unresolved placeholder");
    }
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, rendered).with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

fn validate_package_name(value: &str) -> Result<()> {
    let bytes = value.as_bytes();
    if bytes.is_empty()
        || !bytes[0].is_ascii_lowercase()
        || value.starts_with('-')
        || value.ends_with('-')
        || value.contains("--")
        || !bytes
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
    {
        bail!("application name must be lower-kebab-case");
    }
    Ok(())
}

fn title_case(value: &str) -> String {
    value
        .split('-')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut characters = part.chars();
            characters.next().map_or_else(String::new, |first| {
                first.to_ascii_uppercase().to_string() + characters.as_str()
            })
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_validated_before_writing() {
        assert!(validate_package_name("example-api").is_ok());
        assert!(validate_package_name("ExampleApi").is_err());
        assert!(validate_package_name("example_api").is_err());
    }

    #[test]
    fn titles_are_human_readable() {
        assert_eq!(title_case("example-api"), "Example Api");
    }

    #[test]
    fn scaffold_writes_a_layered_contract_first_workspace() {
        let temporary = tempfile::tempdir().unwrap();
        let destination = temporary.path().join("example-api");
        let report = create_project(&NewProjectOptions {
            name: "example-api".into(),
            directory: Some(destination.clone()),
            database: DatabaseChoice::Postgres,
            vcs: VcsChoice::None,
        })
        .unwrap();

        assert_eq!(report.directory, destination);
        assert!(report.generated_contract.is_file());
        assert!(destination.join("crates/domain/Cargo.toml").is_file());
        assert!(destination.join("crates/application/Cargo.toml").is_file());
        assert!(destination.join("crates/adapters/Cargo.toml").is_file());
        assert!(destination.join("crates/api/Cargo.toml").is_file());
        assert!(destination.join("services/app/Cargo.toml").is_file());
        assert!(
            load_contract(destination.join("openapi/openapi.yaml"))
                .unwrap()
                .is_valid()
        );
        let manifest: toml::Value =
            toml::from_str(&std::fs::read_to_string(destination.join("minco.toml")).unwrap())
                .unwrap();
        assert_eq!(manifest["name"].as_str(), Some("example-api"));
    }
}
