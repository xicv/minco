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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    Jujutsu,
    Git,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SourceSnapshot {
    pub kind: SourceKind,
    pub change: String,
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

pub fn start_task(
    root: &Path,
    task_id: &str,
    destination: Option<PathBuf>,
) -> Result<WorkspaceResult> {
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
    let command = workspace_add_command(&workspace_name, &path);
    require_success(&run_shell(root, &command, true)?)?;
    let describe = format!(
        "jj describe -m {}",
        shell_word(&format!("task({task_id}): start"))
    );
    require_success(&run_shell(&path, &describe, true)?)?;
    Ok(WorkspaceResult {
        task_id: task_id.into(),
        workspace_name,
        path,
    })
}

pub fn finish_task(root: &Path, task_id: &str, message: &str, push: bool) -> Result<()> {
    if !command_available("jj") {
        bail!("Jujutsu (`jj`) is required");
    }
    validate_task_id(task_id)?;
    require_success(&run_shell(root, "cargo minco check --with-cargo", true)?)?;
    require_success(&run_shell(
        root,
        &format!("jj describe -m {}", shell_word(message)),
        true,
    )?)?;
    let bookmark = format!("task/{}", task_id.to_ascii_lowercase());
    require_success(&run_shell(
        root,
        &format!("jj bookmark set {} -r @", shell_word(&bookmark)),
        true,
    )?)?;
    if push {
        require_success(&run_shell(
            root,
            &format!("jj git push --bookmark {}", shell_word(&bookmark)),
            true,
        )?)?;
    }
    Ok(())
}

pub fn source_change(root: &Path) -> Result<String> {
    if command_available("jj") && root.join(".jj").exists() {
        return capture(
            root,
            "jj",
            &["log", "-r", "@", "--no-graph", "-T", "commit_id"],
        );
    }
    if command_available("git") && root.join(".git").exists() {
        return capture(root, "git", &["rev-parse", "HEAD"]);
    }
    Ok("unversioned-workspace".into())
}

pub fn source_snapshot(root: &Path) -> Result<SourceSnapshot> {
    if command_available("jj") && root.join(".jj").exists() {
        let conflicts = capture(
            root,
            "jj",
            &[
                "log",
                "-r",
                "@ & conflicts()",
                "--no-graph",
                "-T",
                "commit_id",
            ],
        )?;
        if !conflicts.is_empty() {
            bail!("refusing to package a conflicted Jujutsu working-copy commit");
        }
        let change = capture(
            root,
            "jj",
            &["log", "-r", "@", "--no-graph", "-T", "commit_id"],
        )?;
        if change.is_empty() {
            bail!("could not resolve the exact Jujutsu working-copy commit");
        }
        return Ok(SourceSnapshot {
            kind: SourceKind::Jujutsu,
            change,
        });
    }
    if command_available("git") {
        let inside = capture(root, "git", &["rev-parse", "--is-inside-work-tree"]);
        if matches!(inside.as_deref(), Ok("true")) {
            let status = capture(
                root,
                "git",
                &["status", "--porcelain=v1", "--untracked-files=all"],
            )?;
            if !status.is_empty() {
                bail!("refusing to package a dirty Git workspace");
            }
            let change = capture(root, "git", &["rev-parse", "HEAD"])?;
            return Ok(SourceSnapshot {
                kind: SourceKind::Git,
                change,
            });
        }
    }
    bail!("packaging requires an exact Jujutsu or Git source revision")
}

fn validate_task_id(value: &str) -> Result<()> {
    if value.is_empty()
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    {
        bail!("task ID may contain only letters, digits and hyphens");
    }
    Ok(())
}

fn shell_word(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn workspace_add_command(workspace_name: &str, path: &Path) -> String {
    format!(
        "jj workspace add --name {} -r @ {}",
        shell_word(workspace_name),
        shell_word(&path.display().to_string())
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn task_workspace_is_created_on_the_current_change() {
        assert_eq!(
            workspace_add_command("task-m6-t04", Path::new("/tmp/minco task")),
            "jj workspace add --name 'task-m6-t04' -r @ '/tmp/minco task'"
        );
    }

    #[test]
    fn git_source_snapshot_rejects_dirty_workspace() {
        let project = tempdir().expect("temporary Git project");
        require_success(
            &run_shell(project.path(), "git init --quiet", false).expect("initialize Git"),
        )
        .expect("Git initialization");
        fs::write(project.path().join("tracked.txt"), "baseline\n").expect("write tracked file");
        require_success(
            &run_shell(project.path(), "git add tracked.txt", false).expect("stage baseline"),
        )
        .expect("stage baseline");
        require_success(
            &run_shell(
                project.path(),
                "git -c user.name=Minco -c user.email=minco@example.invalid commit --quiet -m baseline",
                false,
            )
            .expect("commit baseline"),
        )
        .expect("commit baseline");

        let clean = source_snapshot(project.path()).expect("clean source snapshot");
        assert_eq!(clean.kind, SourceKind::Git);

        fs::write(project.path().join("tracked.txt"), "changed\n").expect("dirty tracked file");
        let error = source_snapshot(project.path()).expect_err("dirty source must be rejected");
        assert!(error.to_string().contains("dirty Git workspace"));
    }
}
