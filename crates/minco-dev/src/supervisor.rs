use crate::{
    CommandSpec, DevPlan, ProcessPlan, ReadinessProbe, ServicePlan, is_sensitive_environment_name,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    future::{Future, pending},
    path::{Path, PathBuf},
    pin::Pin,
    process::{ExitStatus, Stdio},
    time::Duration,
};
use thiserror::Error;
use tokio::{
    io::{AsyncBufReadExt, AsyncRead, BufReader},
    process::Child,
    sync::mpsc,
    task::JoinHandle,
    time,
};

struct ManagedChild {
    id: String,
    child: Child,
    log_tasks: Vec<JoinHandle<()>>,
    #[cfg(unix)]
    process_group: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CompletedCommand {
    Finished,
    Shutdown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DevStream {
    Stdout,
    Stderr,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DevEvent {
    Starting {
        id: String,
    },
    Ready {
        id: String,
    },
    Log {
        id: String,
        stream: DevStream,
        line: String,
    },
    Stopping {
        id: String,
    },
    Stopped {
        id: String,
    },
    Failed {
        id: String,
    },
}

#[derive(Debug, Clone)]
pub struct Supervisor {
    root: PathBuf,
    poll_interval: Duration,
    readiness_timeout: Duration,
    shutdown_grace: Duration,
}

impl Supervisor {
    #[must_use]
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
            poll_interval: Duration::from_millis(50),
            readiness_timeout: Duration::from_secs(30),
            shutdown_grace: Duration::from_secs(3),
        }
    }

    #[must_use]
    pub const fn with_poll_interval(mut self, poll_interval: Duration) -> Self {
        self.poll_interval = poll_interval;
        self
    }

    #[must_use]
    pub const fn with_shutdown_grace(mut self, shutdown_grace: Duration) -> Self {
        self.shutdown_grace = shutdown_grace;
        self
    }

    #[must_use]
    pub const fn with_readiness_timeout(mut self, readiness_timeout: Duration) -> Self {
        self.readiness_timeout = readiness_timeout;
        self
    }

    pub async fn run_until<S>(
        &self,
        plan: &DevPlan,
        runtime_environment: &BTreeMap<String, String>,
        shutdown: S,
        events: mpsc::UnboundedSender<DevEvent>,
    ) -> Result<(), SupervisorError>
    where
        S: Future<Output = ()>,
    {
        tokio::pin!(shutdown);
        let mut started_services = Vec::new();
        for service in &plan.services {
            if let Some(start) = &service.start {
                events.send(DevEvent::Starting {
                    id: service.id.clone(),
                })?;
                let result = self
                    .run_completed_until(
                        &service.id,
                        start,
                        runtime_environment,
                        &events,
                        shutdown.as_mut(),
                    )
                    .await;
                match result {
                    Ok(CompletedCommand::Finished) => {
                        started_services.push(service);
                    }
                    Ok(CompletedCommand::Shutdown) => {
                        started_services.push(service);
                        return self
                            .stop_services(&started_services, runtime_environment, &events)
                            .await;
                    }
                    Err(error) => {
                        if !matches!(error, SupervisorError::Spawn { .. }) {
                            started_services.push(service);
                        }
                        return match self
                            .stop_services(&started_services, runtime_environment, &events)
                            .await
                        {
                            Ok(()) => Err(error),
                            Err(cleanup) => Err(cleanup),
                        };
                    }
                }
                events.send(DevEvent::Ready {
                    id: service.id.clone(),
                })?;
            }
        }

        for lifecycle in &plan.lifecycle {
            events.send(DevEvent::Starting {
                id: lifecycle.id.clone(),
            })?;
            match self
                .run_completed_until(
                    &lifecycle.id,
                    &lifecycle.command,
                    runtime_environment,
                    &events,
                    shutdown.as_mut(),
                )
                .await
            {
                Ok(CompletedCommand::Finished) => {}
                Ok(CompletedCommand::Shutdown) => {
                    return self
                        .stop_services(&started_services, runtime_environment, &events)
                        .await;
                }
                Err(error) => {
                    return match self
                        .stop_services(&started_services, runtime_environment, &events)
                        .await
                    {
                        Ok(()) => Err(error),
                        Err(cleanup) => Err(cleanup),
                    };
                }
            }
            events.send(DevEvent::Ready {
                id: lifecycle.id.clone(),
            })?;
        }

        let mut children = Vec::new();
        for process in &plan.processes {
            events.send(DevEvent::Starting {
                id: process.id.clone(),
            })?;
            match self.spawn_process(process, runtime_environment, &events) {
                Ok(child) => {
                    children.push(child);
                    let readiness = tokio::select! {
                        result = self.wait_for_readiness(
                            process,
                            children.last_mut().expect("child was just inserted"),
                            &events,
                        ) => Some(result),
                        () = &mut shutdown => None,
                    };
                    let Some(readiness) = readiness else {
                        return self
                            .stop_topology(
                                &mut children,
                                &started_services,
                                runtime_environment,
                                &events,
                            )
                            .await;
                    };
                    if let Err(error) = readiness {
                        return match self
                            .stop_topology(
                                &mut children,
                                &started_services,
                                runtime_environment,
                                &events,
                            )
                            .await
                        {
                            Ok(()) => Err(error),
                            Err(cleanup) => Err(cleanup),
                        };
                    }
                }
                Err(error) => {
                    return match self
                        .stop_topology(
                            &mut children,
                            &started_services,
                            runtime_environment,
                            &events,
                        )
                        .await
                    {
                        Ok(()) => Err(error),
                        Err(cleanup) => Err(cleanup),
                    };
                }
            }
        }

        let mut ticker = time::interval(self.poll_interval);
        let outcome = loop {
            tokio::select! {
                () = &mut shutdown => break Ok(()),
                _ = ticker.tick() => {
                    let mut exited = None;
                    for managed in &mut children {
                        if let Some(status) = managed.child.try_wait().map_err(|source| {
                            SupervisorError::Inspect {
                                id: managed.id.clone(),
                                source,
                            }
                        })? {
                            exited = Some((managed.id.clone(), status));
                            break;
                        }
                    }
                    if let Some((id, status)) = exited {
                        break Err(SupervisorError::ProcessExited { id, status });
                    }
                }
            }
        };

        match self
            .stop_topology(
                &mut children,
                &started_services,
                runtime_environment,
                &events,
            )
            .await
        {
            Ok(()) => outcome,
            Err(cleanup) => Err(cleanup),
        }
    }

    async fn wait_for_readiness(
        &self,
        process: &ProcessPlan,
        managed: &mut ManagedChild,
        events: &mpsc::UnboundedSender<DevEvent>,
    ) -> Result<(), SupervisorError> {
        let ReadinessProbe::Http { url } = &process.readiness else {
            events.send(DevEvent::Ready {
                id: process.id.clone(),
            })?;
            return Ok(());
        };
        let url = local_readiness_url(url, &process.id)?;
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|source| SupervisorError::HttpClient {
                id: process.id.clone(),
                source,
            })?;
        let deadline = time::Instant::now() + self.readiness_timeout;
        loop {
            if let Some(status) =
                managed
                    .child
                    .try_wait()
                    .map_err(|source| SupervisorError::Inspect {
                        id: process.id.clone(),
                        source,
                    })?
            {
                return Err(SupervisorError::ProcessExited {
                    id: process.id.clone(),
                    status,
                });
            }
            let remaining = deadline.saturating_duration_since(time::Instant::now());
            if remaining.is_zero() {
                return Err(SupervisorError::ReadinessTimeout {
                    id: process.id.clone(),
                });
            }
            let request_timeout = remaining.min(Duration::from_secs(1));
            let response = time::timeout(request_timeout, client.get(url.clone()).send()).await;
            if matches!(response, Ok(Ok(response)) if response.status().is_success()) {
                events.send(DevEvent::Ready {
                    id: process.id.clone(),
                })?;
                return Ok(());
            }
            time::sleep(self.poll_interval.min(remaining)).await;
        }
    }

    async fn run_completed_until<S>(
        &self,
        id: &str,
        command: &CommandSpec,
        runtime_environment: &BTreeMap<String, String>,
        events: &mpsc::UnboundedSender<DevEvent>,
        mut shutdown: Pin<&mut S>,
    ) -> Result<CompletedCommand, SupervisorError>
    where
        S: Future<Output = ()>,
    {
        let mut managed = self.spawn_command(id, command, runtime_environment, events)?;
        let status = tokio::select! {
            result = managed.child.wait() => Some(result),
            () = shutdown.as_mut() => None,
        };
        let Some(status) = status else {
            self.terminate_managed(&mut managed).await?;
            return Ok(CompletedCommand::Shutdown);
        };
        self.finish_log_tasks(&mut managed).await;
        let status = status.map_err(|source| SupervisorError::Inspect {
            id: id.into(),
            source,
        })?;
        if status.success() {
            Ok(CompletedCommand::Finished)
        } else {
            Err(SupervisorError::CommandFailed {
                id: id.into(),
                status,
            })
        }
    }

    async fn run_completed(
        &self,
        id: &str,
        command: &CommandSpec,
        runtime_environment: &BTreeMap<String, String>,
        events: &mpsc::UnboundedSender<DevEvent>,
    ) -> Result<(), SupervisorError> {
        let never = pending();
        tokio::pin!(never);
        match self
            .run_completed_until(id, command, runtime_environment, events, never.as_mut())
            .await?
        {
            CompletedCommand::Finished => Ok(()),
            CompletedCommand::Shutdown => unreachable!("pending shutdown cannot resolve"),
        }
    }

    fn spawn_command(
        &self,
        id: &str,
        command: &CommandSpec,
        runtime_environment: &BTreeMap<String, String>,
        events: &mpsc::UnboundedSender<DevEvent>,
    ) -> Result<ManagedChild, SupervisorError> {
        let mut configured = configured_command(&self.root, command, runtime_environment);
        configured.stdout(Stdio::piped()).stderr(Stdio::piped());
        #[cfg(unix)]
        configured.process_group(0);
        let mut child = configured
            .spawn()
            .map_err(|source| SupervisorError::Spawn {
                id: id.into(),
                source,
            })?;
        #[cfg(unix)]
        let process_group = child
            .id()
            .ok_or_else(|| SupervisorError::MissingProcessId { id: id.into() })?;
        let redactions = redaction_values(runtime_environment, &command.environment);
        let mut log_tasks = Vec::new();
        if let Some(stdout) = child.stdout.take() {
            log_tasks.push(spawn_log_reader(
                stdout,
                id.into(),
                DevStream::Stdout,
                redactions.clone(),
                events.clone(),
            ));
        }
        if let Some(stderr) = child.stderr.take() {
            log_tasks.push(spawn_log_reader(
                stderr,
                id.into(),
                DevStream::Stderr,
                redactions,
                events.clone(),
            ));
        }
        Ok(ManagedChild {
            id: id.into(),
            child,
            log_tasks,
            #[cfg(unix)]
            process_group,
        })
    }

    fn spawn_process(
        &self,
        process: &ProcessPlan,
        runtime_environment: &BTreeMap<String, String>,
        events: &mpsc::UnboundedSender<DevEvent>,
    ) -> Result<ManagedChild, SupervisorError> {
        self.spawn_command(&process.id, &process.command, runtime_environment, events)
    }

    async fn terminate_managed(&self, managed: &mut ManagedChild) -> Result<(), SupervisorError> {
        #[cfg(unix)]
        signal_process_group(managed.process_group, rustix::process::Signal::TERM);
        #[cfg(not(unix))]
        let _ = managed.child.start_kill();
        self.reap_managed_until(managed, time::Instant::now() + self.shutdown_grace)
            .await
    }

    async fn reap_managed_until(
        &self,
        managed: &mut ManagedChild,
        deadline: time::Instant,
    ) -> Result<(), SupervisorError> {
        let wait = time::timeout_at(deadline, managed.child.wait()).await;
        let result = if let Ok(result) = wait {
            result
        } else {
            #[cfg(unix)]
            signal_process_group(managed.process_group, rustix::process::Signal::KILL);
            let _ = managed.child.start_kill();
            if let Ok(result) = time::timeout(self.shutdown_grace, managed.child.wait()).await {
                result
            } else {
                for task in managed.log_tasks.drain(..) {
                    task.abort();
                }
                return Err(SupervisorError::ShutdownTimeout {
                    id: managed.id.clone(),
                });
            }
        };
        self.finish_log_tasks(managed).await;
        result
            .map(|_| ())
            .map_err(|source| SupervisorError::Inspect {
                id: managed.id.clone(),
                source,
            })
    }

    async fn finish_log_tasks(&self, managed: &mut ManagedChild) {
        #[cfg(unix)]
        signal_process_group(managed.process_group, rustix::process::Signal::KILL);
        for task in managed.log_tasks.drain(..) {
            let _ = task.await;
        }
    }

    async fn stop_children(
        &self,
        children: &mut [ManagedChild],
        events: &mpsc::UnboundedSender<DevEvent>,
    ) -> Result<(), SupervisorError> {
        for managed in children.iter_mut() {
            let _ = events.send(DevEvent::Stopping {
                id: managed.id.clone(),
            });
            #[cfg(unix)]
            signal_process_group(managed.process_group, rustix::process::Signal::TERM);
            #[cfg(not(unix))]
            let _ = managed.child.start_kill();
        }
        let deadline = time::Instant::now() + self.shutdown_grace;
        let mut first_error = None;
        for managed in children {
            match self.reap_managed_until(managed, deadline).await {
                Ok(()) => {
                    let _ = events.send(DevEvent::Stopped {
                        id: managed.id.clone(),
                    });
                }
                Err(error) => {
                    let _ = events.send(DevEvent::Failed {
                        id: managed.id.clone(),
                    });
                    if first_error.is_none() {
                        first_error = Some(error);
                    }
                }
            }
        }
        match first_error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    async fn stop_topology(
        &self,
        children: &mut [ManagedChild],
        services: &[&ServicePlan],
        runtime_environment: &BTreeMap<String, String>,
        events: &mpsc::UnboundedSender<DevEvent>,
    ) -> Result<(), SupervisorError> {
        let child_result = self.stop_children(children, events).await;
        let service_result = self
            .stop_services(services, runtime_environment, events)
            .await;
        match (child_result, service_result) {
            (Err(error), _) | (Ok(()), Err(error)) => Err(error),
            (Ok(()), Ok(())) => Ok(()),
        }
    }

    async fn stop_services(
        &self,
        services: &[&ServicePlan],
        runtime_environment: &BTreeMap<String, String>,
        events: &mpsc::UnboundedSender<DevEvent>,
    ) -> Result<(), SupervisorError> {
        let mut first_error = None;
        for service in services.iter().rev() {
            let Some(stop) = &service.stop else {
                continue;
            };
            let _ = events.send(DevEvent::Stopping {
                id: service.id.clone(),
            });
            let result = self
                .run_completed(&service.id, stop, runtime_environment, events)
                .await;
            match result {
                Ok(()) => {
                    let _ = events.send(DevEvent::Stopped {
                        id: service.id.clone(),
                    });
                }
                Err(error) => {
                    let _ = events.send(DevEvent::Failed {
                        id: service.id.clone(),
                    });
                    if first_error.is_none() {
                        first_error = Some(error);
                    }
                }
            }
        }
        match first_error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}

fn spawn_log_reader<R>(
    reader: R,
    id: String,
    stream: DevStream,
    redactions: Vec<String>,
    events: mpsc::UnboundedSender<DevEvent>,
) -> JoinHandle<()>
where
    R: AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        let mut lines = BufReader::new(reader).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let line = redact_line(line, &redactions);
            if events
                .send(DevEvent::Log {
                    id: id.clone(),
                    stream,
                    line,
                })
                .is_err()
            {
                break;
            }
        }
    })
}

fn redaction_values(
    runtime_environment: &BTreeMap<String, String>,
    command_environment: &BTreeMap<String, String>,
) -> Vec<String> {
    let mut values = runtime_environment
        .iter()
        .chain(command_environment.iter())
        .filter(|(name, value)| is_sensitive_environment_name(name) && !value.is_empty())
        .map(|(_, value)| value.clone())
        .collect::<Vec<_>>();
    values.sort_by_key(|value| std::cmp::Reverse(value.len()));
    values.dedup();
    values
}

fn redact_line(mut line: String, redactions: &[String]) -> String {
    for value in redactions {
        line = line.replace(value, "<redacted>");
    }
    line
}

fn local_readiness_url(value: &str, id: &str) -> Result<reqwest::Url, SupervisorError> {
    let url = reqwest::Url::parse(value)
        .map_err(|_| SupervisorError::InvalidReadinessUrl { id: id.into() })?;
    let local_host = match url.host_str() {
        Some("localhost") => true,
        Some(host) => host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|address| address.is_loopback()),
        None => false,
    };
    if url.scheme() != "http"
        || !local_host
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(SupervisorError::InvalidReadinessUrl { id: id.into() });
    }
    Ok(url)
}

fn configured_command(
    root: &Path,
    command: &CommandSpec,
    runtime_environment: &BTreeMap<String, String>,
) -> tokio::process::Command {
    let mut configured = tokio::process::Command::new(&command.program);
    configured
        .args(&command.arguments)
        .current_dir(root)
        .envs(runtime_environment)
        .envs(&command.environment)
        .kill_on_drop(true);
    configured
}

#[cfg(unix)]
fn signal_process_group(process_group: u32, signal: rustix::process::Signal) {
    let Ok(raw) = i32::try_from(process_group) else {
        return;
    };
    let Some(process_group) = rustix::process::Pid::from_raw(raw) else {
        return;
    };
    let _ = rustix::process::kill_process_group(process_group, signal);
}

#[derive(Debug, Error)]
pub enum SupervisorError {
    #[error("development event receiver closed")]
    EventReceiverClosed,
    #[error("failed to start `{id}`: {source}")]
    Spawn {
        id: String,
        #[source]
        source: std::io::Error,
    },
    #[error("development command `{id}` exited with {status}")]
    CommandFailed { id: String, status: ExitStatus },
    #[error("development process `{id}` exited unexpectedly with {status}")]
    ProcessExited { id: String, status: ExitStatus },
    #[error("failed to inspect development process `{id}`: {source}")]
    Inspect {
        id: String,
        #[source]
        source: std::io::Error,
    },
    #[error("development process `{id}` did not expose a process identifier")]
    MissingProcessId { id: String },
    #[error("development process `{id}` could not be reaped before the shutdown timeout")]
    ShutdownTimeout { id: String },
    #[error("development process `{id}` has a non-local or invalid readiness URL")]
    InvalidReadinessUrl { id: String },
    #[error("failed to create the readiness client for `{id}`: {source}")]
    HttpClient {
        id: String,
        #[source]
        source: reqwest::Error,
    },
    #[error("development process `{id}` did not become ready before the timeout")]
    ReadinessTimeout { id: String },
}

impl From<mpsc::error::SendError<DevEvent>> for SupervisorError {
    fn from(_: mpsc::error::SendError<DevEvent>) -> Self {
        Self::EventReceiverClosed
    }
}
