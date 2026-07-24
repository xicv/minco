use anyhow::{Context, Result, bail};
use serde::Serialize;
use std::{
    path::Path,
    process::{Command, Stdio},
};

#[derive(Debug, Clone, Serialize)]
pub struct CommandResult {
    pub command: String,
    pub success: bool,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

pub fn run_shell(root: &Path, command: &str, stream: bool) -> Result<CommandResult> {
    let mut process = if cfg!(windows) {
        let mut command_process = Command::new("cmd");
        command_process.args(["/C", command]);
        command_process
    } else {
        let mut command_process = Command::new("sh");
        command_process.args(["-c", command]);
        command_process
    };
    process.current_dir(root);
    if stream {
        let status = process.status().with_context(|| format!("run {command}"))?;
        return Ok(CommandResult {
            command: command.into(),
            success: status.success(),
            exit_code: status.code(),
            stdout: String::new(),
            stderr: String::new(),
        });
    }
    let output = process.output().with_context(|| format!("run {command}"))?;
    Ok(CommandResult {
        command: command.into(),
        success: output.status.success(),
        exit_code: output.status.code(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

pub fn require_success(result: &CommandResult) -> Result<()> {
    if result.success {
        Ok(())
    } else {
        bail!(
            "command failed: {}\nstdout:\n{}\nstderr:\n{}",
            result.command,
            result.stdout,
            result.stderr
        )
    }
}

pub fn command_available(name: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|paths| {
        std::env::split_paths(&paths).any(|directory| {
            let candidate = directory.join(name);
            if candidate.is_file() {
                return true;
            }
            cfg!(windows) && directory.join(format!("{name}.exe")).is_file()
        })
    })
}

pub fn capture(root: &Path, program: &str, args: &[&str]) -> Result<String> {
    let output = Command::new(program)
        .args(args)
        .current_dir(root)
        .stdin(Stdio::null())
        .output()
        .with_context(|| format!("run {program}"))?;
    if !output.status.success() {
        bail!(
            "{program} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}
