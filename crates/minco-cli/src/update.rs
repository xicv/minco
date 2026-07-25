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
        actions.push(checked(run_shell(root, "rustup check", false)?)?);
    } else {
        notes.push("rustup is not installed; the pinned Rust toolchain was not checked".into());
    }
    if command_available("cargo") {
        actions.push(checked(run_shell(root, "cargo update --dry-run", false)?)?);
        let mut metadata = checked(run_shell(
            root,
            "cargo metadata --locked --no-deps --format-version 1",
            false,
        )?)?;
        metadata.stdout.clear();
        actions.push(metadata);
        notes.push(
            "Cargo metadata resolved with --locked; verbose package JSON was omitted from the report."
                .into(),
        );
    } else {
        notes.push("cargo is not installed; dependency resolution was not checked".into());
    }
    if command_available("uv") {
        actions.push(checked(run_shell(root, "uv lock --check", false)?)?);
        actions.push(checked(run_shell(
            root,
            "uv lock --upgrade --dry-run",
            false,
        )?)?);
    } else {
        notes.push("uv is not installed; Python dependency resolution was not checked".into());
    }
    if command_available("jj") {
        actions.push(checked(run_shell(root, "jj version", false)?)?);
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
    if !toolchain && !dependencies && !run_checks {
        bail!("update apply requires at least one of --toolchain, --dependencies, or --run-checks");
    }
    let mut actions = Vec::new();
    let mut notes = Vec::new();
    match select_cleanliness_vcs(command_available("jj"), command_available("git"))? {
        CleanlinessVcs::Jj => {
            let diff = checked(run_shell(root, "jj diff --stat", false)?)?;
            if !diff.stdout.trim().is_empty() {
                bail!(
                    "the JJ working-copy commit has unreviewed changes; describe or split them before updating"
                );
            }
        }
        CleanlinessVcs::Git => {
            let status = checked(run_shell(root, "git status --porcelain", false)?)?;
            if !status.stdout.trim().is_empty() {
                bail!("the Git working tree has uncommitted changes; commit them before updating");
            }
            notes.push(
                "JJ was unavailable, so cleanliness was checked through the colocated Git repository."
                    .into(),
            );
        }
    }
    if toolchain {
        if !command_available("rustup") {
            bail!("rustup is required for --toolchain");
        }
        let result = checked(run_shell(
            root,
            "rustup toolchain install 1.97.1 --profile minimal --component rustfmt --component clippy --target aarch64-unknown-linux-gnu",
            true,
        )?)?;
        actions.push(result);
    }
    if dependencies {
        if !command_available("cargo") {
            bail!("cargo is required for --dependencies");
        }
        if !command_available("uv") {
            bail!("uv is required for --dependencies");
        }
        let result = checked(run_shell(root, "cargo update", true)?)?;
        actions.push(result);
        let result = checked(run_shell(root, "uv lock --upgrade", true)?)?;
        actions.push(result);
    }
    if run_checks {
        if !command_available("uv") {
            bail!("uv is required for --run-checks");
        }
        let static_result = checked(run_shell(
            root,
            "uv run --locked python scripts/validate_static.py",
            true,
        )?)?;
        actions.push(static_result);
        if command_available("cargo") {
            let cargo_result = checked(run_shell(root, "cargo minco check --with-cargo", true)?)?;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CleanlinessVcs {
    Jj,
    Git,
}

fn select_cleanliness_vcs(jj_available: bool, git_available: bool) -> Result<CleanlinessVcs> {
    if jj_available {
        Ok(CleanlinessVcs::Jj)
    } else if git_available {
        Ok(CleanlinessVcs::Git)
    } else {
        bail!("JJ or Git is required to prove the source workspace is clean before updating")
    }
}

fn checked(result: CommandResult) -> Result<CommandResult> {
    require_success(&result)?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn command_result(success: bool) -> CommandResult {
        CommandResult {
            command: "test command".into(),
            success,
            exit_code: Some(i32::from(!success)),
            stdout: String::new(),
            stderr: "failure".into(),
        }
    }

    #[test]
    fn check_actions_fail_the_command_instead_of_only_reporting_failure() {
        let error = checked(command_result(false)).unwrap_err().to_string();

        assert!(error.contains("command failed: test command"));
        assert!(error.contains("failure"));
    }

    #[test]
    fn update_apply_requires_a_version_control_cleanliness_proof() {
        let error = select_cleanliness_vcs(false, false)
            .unwrap_err()
            .to_string();

        assert!(error.contains("JJ or Git is required"));
        assert_eq!(
            select_cleanliness_vcs(true, true).unwrap(),
            CleanlinessVcs::Jj
        );
        assert_eq!(
            select_cleanliness_vcs(false, true).unwrap(),
            CleanlinessVcs::Git
        );
    }
}
