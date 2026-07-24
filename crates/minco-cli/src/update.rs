use crate::process::{CommandResult, command_available, require_success, run_shell};
use anyhow::{Result, bail};
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Clone, Serialize)]
pub struct UpdateReport {
    pub mode: String,
    pub actions: Vec<CommandResult>,
    pub notes: Vec<String>,
}

pub fn check(root: &Path) -> Result<UpdateReport> {
    let mut actions = Vec::new();
    let mut notes = Vec::new();
    if command_available("rustup") {
        actions.push(run_shell(root, "rustup check", false)?);
    } else {
        notes.push("rustup is not installed; the pinned Rust toolchain was not checked".into());
    }
    if command_available("cargo") {
        actions.push(run_shell(root, "cargo update --dry-run", false)?);
        actions.push(run_shell(
            root,
            "cargo metadata --locked --no-deps --format-version 1",
            false,
        )?);
    } else {
        notes.push("cargo is not installed; dependency resolution was not checked".into());
    }
    if command_available("jj") {
        actions.push(run_shell(root, "jj version", false)?);
    } else {
        notes.push("jj is not installed; version and workspace state were not checked".into());
    }
    notes.push("Minco 0.1 updates source workspaces; binary self-update from a release registry is intentionally deferred until signed releases exist.".into());
    Ok(UpdateReport {
        mode: "check".into(),
        actions,
        notes,
    })
}

// The booleans map one-to-one to explicit CLI flags; grouping them would only
// move the same independent switches into another internal type.
#[allow(clippy::fn_params_excessive_bools)]
pub fn apply(
    root: &Path,
    yes: bool,
    toolchain: bool,
    dependencies: bool,
    run_checks: bool,
) -> Result<UpdateReport> {
    if !yes {
        bail!("update apply requires --yes because it mutates toolchains and/or Cargo.lock");
    }
    let mut actions = Vec::new();
    let mut notes = Vec::new();
    if command_available("jj") {
        let diff = run_shell(root, "jj diff --stat", false)?;
        require_success(&diff)?;
        if !diff.stdout.trim().is_empty() {
            bail!(
                "the JJ working-copy commit has unreviewed changes; describe or split them before updating"
            );
        }
    } else if command_available("git") {
        let status = run_shell(root, "git status --porcelain", false)?;
        require_success(&status)?;
        if !status.stdout.trim().is_empty() {
            bail!("the Git working tree has uncommitted changes; commit them before updating");
        }
        notes.push(
            "JJ was unavailable, so cleanliness was checked through the colocated Git repository."
                .into(),
        );
    }
    if toolchain {
        if !command_available("rustup") {
            bail!("rustup is required for --toolchain");
        }
        let result = run_shell(
            root,
            "rustup toolchain install 1.97.1 --profile minimal --component rustfmt --component clippy --target aarch64-unknown-linux-gnu",
            true,
        )?;
        require_success(&result)?;
        actions.push(result);
    }
    if dependencies {
        if !command_available("cargo") {
            bail!("cargo is required for --dependencies");
        }
        let result = run_shell(root, "cargo update", true)?;
        require_success(&result)?;
        actions.push(result);
    }
    if run_checks {
        let static_result = run_shell(root, "python3 scripts/validate_static.py", true)?;
        require_success(&static_result)?;
        actions.push(static_result);
        if command_available("cargo") {
            let cargo_result = run_shell(root, "cargo minco check --with-cargo", true)?;
            require_success(&cargo_result)?;
            actions.push(cargo_result);
        } else {
            notes.push("cargo checks were skipped because Cargo is unavailable".into());
        }
    }
    Ok(UpdateReport {
        mode: "apply".into(),
        actions,
        notes,
    })
}
