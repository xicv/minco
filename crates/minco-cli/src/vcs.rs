use crate::process::{capture, command_available, require_success, run_shell};
use anyhow::{Context, Result, bail};
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize)]
pub struct WorkspaceResult {
    pub task_id: String,
    pub workspace_name: String,
    pub path: PathBuf,
}

pub fn initialize(root: &Path) -> Result<()> {
    if !command_available("jj") {
        bail!("Jujutsu (`jj`) is required; install it before running `minco vcs init`");
    }
    if !root.join(".jj").exists() {
        require_success(&run_shell(root, "jj git init .", true)?)?;
    }
    for command in [
        "jj config set --repo git.colocate true",
        "jj config set --repo git.push-new-bookmarks false",
        "jj config set --repo ui.default-command '[\"log\", \"--limit\", \"12\"]'",
    ] {
        require_success(&run_shell(root, command, true)?)?;
    }
    Ok(())
}

pub fn status(root: &Path) -> Result<String> {
    if !command_available("jj") {
        bail!("Jujutsu (`jj`) is required");
    }
    capture(root, "jj", &["status"])
}

pub fn start_task(root: &Path, task_id: &str, destination: Option<PathBuf>) -> Result<WorkspaceResult> {
    if !command_available("jj") {
        bail!("Jujutsu (`jj`) is required");
    }
    validate_task_id(task_id)?;
    let workspace_name = format!("task-{}", task_id.to_ascii_lowercase());
    let parent = root.parent().context("repository root has no parent")?;
    let path = destination.unwrap_or_else(|| parent.join(format!("minco-{workspace_name}")));
    if path.exists() {
        bail!("workspace destination {} already exists", path.display());
    }
    let command = format!(
        "jj workspace add --name {} {}",
        shell_word(&workspace_name),
        shell_word(&path.display().to_string())
    );
    require_success(&run_shell(root, &command, true)?)?;
    let describe = format!("jj describe -m {}", shell_word(&format!("task({task_id}): start")));
    require_success(&run_shell(&path, &describe, true)?)?;
    Ok(WorkspaceResult { task_id: task_id.into(), workspace_name, path })
}

pub fn finish_task(root: &Path, task_id: &str, message: &str, push: bool) -> Result<()> {
    if !command_available("jj") {
        bail!("Jujutsu (`jj`) is required");
    }
    validate_task_id(task_id)?;
    require_success(&run_shell(root, "cargo minco check --with-cargo", true)?)?;
    require_success(&run_shell(root, &format!("jj describe -m {}", shell_word(message)), true)?)?;
    let bookmark = format!("task/{}", task_id.to_ascii_lowercase());
    require_success(&run_shell(root, &format!("jj bookmark set {} -r @", shell_word(&bookmark)), true)?)?;
    if push {
        require_success(&run_shell(root, &format!("jj git push --bookmark {}", shell_word(&bookmark)), true)?)?;
    }
    Ok(())
}

pub fn source_change(root: &Path) -> Result<String> {
    if command_available("jj") && root.join(".jj").exists() {
        return capture(root, "jj", &["log", "-r", "@", "--no-graph", "-T", "commit_id"]);
    }
    if command_available("git") && root.join(".git").exists() {
        return capture(root, "git", &["rev-parse", "HEAD"]);
    }
    Ok("unversioned-workspace".into())
}

fn validate_task_id(value: &str) -> Result<()> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'-') {
        bail!("task ID may contain only letters, digits and hyphens");
    }
    Ok(())
}

fn shell_word(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}
