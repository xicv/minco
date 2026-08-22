//! Explicit typed job handler registration.
//!
//! Registration is static and explicit: no runtime scanning, no reflection,
//! no dynamic loading. Duplicate name/version registration fails, unknown
//! jobs and unsupported versions resolve deterministically.

use serde::Serialize;
use serde::de::DeserializeOwned;
use std::collections::BTreeMap;
use std::future::Future;
use std::sync::RwLock;

use crate::JobError;

/// A typed durable job command with a stable logical name and payload
/// version. The name and version are wire identity; the Rust type never
/// crosses a transport.
pub trait Job: Serialize + DeserializeOwned + Send + Sync + 'static {
    /// Stable logical job name (`[a-z0-9.-]`).
    const NAME: &'static str;
    /// Payload version this type encodes.
    const VERSION: u16;
}

/// Bounded execution context handed to a handler. Its `Debug` output shows
/// only structural information — metadata names and bounded key lengths —
/// never partition or metadata values.
#[derive(Clone)]
pub struct JobContext {
    pub job_id: uuid::Uuid,
    pub correlation_id: uuid::Uuid,
    pub causation_id: Option<uuid::Uuid>,
    pub attempt: u32,
    pub maximum_attempts: u32,
    pub deadline: Option<chrono::DateTime<chrono::Utc>>,
    pub partition: Option<String>,
    pub metadata: BTreeMap<String, String>,
}

impl std::fmt::Debug for JobContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JobContext")
            .field("job_id", &self.job_id)
            .field("correlation_id", &self.correlation_id)
            .field("causation_id", &self.causation_id)
            .field("attempt", &self.attempt)
            .field("maximum_attempts", &self.maximum_attempts)
            .field("deadline", &self.deadline)
            .field("partition", &self.partition.as_ref().map_or(0, String::len))
            .field("metadata_names", &self.metadata.keys().collect::<Vec<_>>())
            .finish_non_exhaustive()
    }
}

/// A handler failure with stable, public-safe classification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobExecutionFailure {
    kind: FailureKind,
    code: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureKind {
    /// The job should run again later under its retry policy.
    Retryable,
    /// The job must never run again automatically.
    Permanent,
}

impl JobExecutionFailure {
    #[must_use]
    pub fn retryable(code: impl Into<String>) -> Self {
        Self {
            kind: FailureKind::Retryable,
            code: sanitize_code(code),
        }
    }

    #[must_use]
    pub fn permanent(code: impl Into<String>) -> Self {
        Self {
            kind: FailureKind::Permanent,
            code: sanitize_code(code),
        }
    }

    #[must_use]
    pub const fn kind(&self) -> FailureKind {
        self.kind
    }

    #[must_use]
    pub fn code(&self) -> &str {
        &self.code
    }

    #[must_use]
    pub const fn is_permanent(&self) -> bool {
        matches!(self.kind, FailureKind::Permanent)
    }
}

/// Stable public failure codes used by the executor itself.
pub mod failure_codes {
    pub const HANDLER_TIMEOUT: &str = "JOBS-HANDLER-TIMEOUT";
    pub const DEADLINE_EXPIRED: &str = "JOBS-DEADLINE-EXPIRED";
    pub const UNKNOWN_JOB: &str = "JOBS-UNKNOWN-JOB";
    pub const UNSUPPORTED_VERSION: &str = "JOBS-UNSUPPORTED-VERSION";
    pub const RETRIES_EXHAUSTED: &str = "JOBS-RETRIES-EXHAUSTED";
    pub const OVERLAP_BUSY: &str = "JOBS-OVERLAP-BUSY";
    pub const PAYLOAD_DECODE: &str = "JOBS-PAYLOAD-DECODE";
    pub const TRANSPORT_INTEGRITY: &str = "JOBS-TRANSPORT-INTEGRITY";
}

fn sanitize_code(code: impl Into<String>) -> String {
    let code = code.into();
    let mut sanitized = String::with_capacity(code.len().min(64));
    let mut previous_hyphen = false;
    for character in code.chars() {
        if character.is_whitespace() {
            if !previous_hyphen && !sanitized.is_empty() {
                sanitized.push('-');
                previous_hyphen = true;
            }
            continue;
        }
        previous_hyphen = false;
        if character.is_ascii_lowercase()
            || character.is_ascii_digit()
            || character == '-'
            || character == '_'
            || character == '.'
        {
            sanitized.push(character);
        } else if character.is_ascii_uppercase() {
            sanitized.push(character.to_ascii_lowercase());
        }
    }
    let sanitized = sanitized.trim_matches('-').to_owned();
    if sanitized.is_empty() || sanitized.len() > 64 {
        "jobs-handler-failed".into()
    } else {
        sanitized
    }
}

/// An erased handler for one `(job name, version)` pair.
#[async_trait::async_trait]
pub trait JobHandler: Send + Sync {
    fn job_name(&self) -> &str;
    fn job_version(&self) -> u16;
    async fn execute(
        &self,
        payload: &serde_json::Value,
        context: JobContext,
    ) -> Result<(), JobExecutionFailure>;
}

/// Adapter wrapping a typed async closure for a specific [`Job`].
pub struct TypedJobHandler<J, F> {
    handler: F,
    name: &'static str,
    version: u16,
    marker: std::marker::PhantomData<fn(J)>,
}

impl<J, F> std::fmt::Debug for TypedJobHandler<J, F> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TypedJobHandler")
            .field("job_name", &self.name)
            .field("job_version", &self.version)
            .finish_non_exhaustive()
    }
}

/// Create a handler for [`Job`] `J` from an async closure receiving the
/// decoded typed payload and the bounded context.
pub fn typed<J, F, Fut>(handler: F) -> TypedJobHandler<J, F>
where
    J: Job,
    F: Fn(J, JobContext) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<(), JobExecutionFailure>> + Send,
{
    TypedJobHandler {
        handler,
        name: J::NAME,
        version: J::VERSION,
        marker: std::marker::PhantomData,
    }
}

#[async_trait::async_trait]
impl<J, F, Fut> JobHandler for TypedJobHandler<J, F>
where
    J: Job,
    F: Fn(J, JobContext) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<(), JobExecutionFailure>> + Send,
{
    fn job_name(&self) -> &str {
        self.name
    }

    fn job_version(&self) -> u16 {
        self.version
    }

    async fn execute(
        &self,
        payload: &serde_json::Value,
        context: JobContext,
    ) -> Result<(), JobExecutionFailure> {
        let decoded = serde_json::from_value::<J>(payload.clone()).map_err(|error| {
            JobExecutionFailure::permanent(format!("{}?{error}", failure_codes::PAYLOAD_DECODE))
        })?;
        (self.handler)(decoded, context).await
    }
}

/// A payload upcaster converts a previous payload version into the current
/// representation during the supported compatibility window.
pub type PayloadUpcaster =
    std::sync::Arc<dyn Fn(serde_json::Value) -> Result<serde_json::Value, String> + Send + Sync>;

#[derive(Default)]
struct RegistryState {
    handlers: BTreeMap<(String, u16), std::sync::Arc<dyn JobHandler>>,
    upcasters: BTreeMap<(String, u16), (u16, PayloadUpcaster)>,
}

/// The explicit, statically populated job handler registry.
#[derive(Default)]
pub struct JobHandlerRegistry {
    state: RwLock<RegistryState>,
}

impl std::fmt::Debug for JobHandlerRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let state = self.state.read().expect("job registry lock");
        f.debug_struct("JobHandlerRegistry")
            .field("handlers", &state.handlers.len())
            .field("upcasters", &state.upcasters.len())
            .finish()
    }
}

impl JobHandlerRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a handler. Duplicate name/version registration fails.
    pub fn register(&self, handler: std::sync::Arc<dyn JobHandler>) -> Result<(), JobError> {
        let key = (handler.job_name().to_owned(), handler.job_version());
        let mut state = self.state.write().expect("job registry lock");
        if state.handlers.contains_key(&key) {
            drop(state);
            return Err(JobError::DuplicateRegistration {
                job_name: key.0,
                job_version: key.1,
            });
        }
        state.handlers.insert(key, handler);
        drop(state);
        Ok(())
    }

    /// Register a typed closure for [`Job`] `J`.
    pub fn register_typed<J, F, Fut>(&self, handler: F) -> Result<(), JobError>
    where
        J: Job,
        F: Fn(J, JobContext) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<(), JobExecutionFailure>> + Send,
    {
        self.register(std::sync::Arc::new(typed::<J, F, Fut>(handler)))
    }

    /// Register an upcaster from `from_version` to `to_version` for a job
    /// name. Only one upcaster per source version is allowed.
    pub fn register_upcaster(
        &self,
        job_name: &str,
        from_version: u16,
        to_version: u16,
        upcaster: PayloadUpcaster,
    ) -> Result<(), JobError> {
        if from_version >= to_version {
            return Err(JobError::InvalidJob(
                "upcaster must upgrade from an older version".into(),
            ));
        }
        let mut state = self.state.write().expect("job registry lock");
        let duplicate = state
            .upcasters
            .insert((job_name.to_owned(), from_version), (to_version, upcaster))
            .is_some();
        drop(state);
        if duplicate {
            return Err(JobError::DuplicateRegistration {
                job_name: job_name.to_owned(),
                job_version: from_version,
            });
        }
        Ok(())
    }

    /// Resolve a handler for `(name, version)`, applying registered upcasters
    /// when the exact version is absent. Unknown jobs and versions without a
    /// compatible upcaster fail deterministically.
    pub fn resolve(&self, job_name: &str, job_version: u16) -> Result<ResolvedHandler, JobError> {
        let state = self.state.read().expect("job registry lock");
        if let Some(handler) = state.handlers.get(&(job_name.to_owned(), job_version)) {
            return Ok(ResolvedHandler {
                handler: handler.clone(),
                payload: None,
            });
        }
        let mut version = job_version;
        let mut applied = 0;
        let mut chain = Vec::new();
        while applied < 32 {
            let Some((to_version, upcaster)) = state
                .upcasters
                .get(&(job_name.to_owned(), version))
                .cloned()
            else {
                break;
            };
            chain.push((version, upcaster));
            version = to_version;
            applied += 1;
            if let Some(handler) = state.handlers.get(&(job_name.to_owned(), version)) {
                return Ok(ResolvedHandler {
                    handler: handler.clone(),
                    payload: Some(chain),
                });
            }
        }
        if state
            .handlers
            .range((job_name.to_owned(), 0)..(job_name.to_owned(), u16::MAX))
            .next()
            .is_some()
        {
            Err(JobError::UnsupportedJobVersion {
                job_name: job_name.to_owned(),
                job_version,
            })
        } else {
            Err(JobError::UnknownJob(job_name.to_owned()))
        }
    }

    /// Registered `(name, version)` pairs in deterministic order.
    pub fn registered(&self) -> Vec<(String, u16)> {
        let state = self.state.read().expect("job registry lock");
        state.handlers.keys().cloned().collect()
    }
}

/// A handler resolved for execution, plus any upcaster chain that must be
/// applied to the payload first.
pub struct ResolvedHandler {
    handler: std::sync::Arc<dyn JobHandler>,
    payload: Option<Vec<(u16, PayloadUpcaster)>>,
}

impl std::fmt::Debug for ResolvedHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResolvedHandler")
            .field("job_name", &self.handler.job_name())
            .field("job_version", &self.handler.job_version())
            .field("upcasters", &self.payload.as_ref().map_or(0, Vec::len))
            .finish()
    }
}

impl ResolvedHandler {
    #[must_use]
    pub fn job_name(&self) -> &str {
        self.handler.job_name()
    }

    #[must_use]
    pub fn job_version(&self) -> u16 {
        self.handler.job_version()
    }

    /// Apply the upcaster chain, if any, and execute the handler.
    pub async fn execute(
        &self,
        mut payload: serde_json::Value,
        context: JobContext,
    ) -> Result<(), JobExecutionFailure> {
        if let Some(chain) = &self.payload {
            for (from_version, upcaster) in chain {
                payload = upcaster(payload).map_err(|error| {
                    JobExecutionFailure::permanent(format!(
                        "{}?v{from_version}?{error}",
                        failure_codes::PAYLOAD_DECODE
                    ))
                })?;
            }
        }
        self.handler.execute(&payload, context).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(serde::Serialize, serde::Deserialize)]
    struct WelcomeEmail {
        order_id: String,
    }

    impl Job for WelcomeEmail {
        const NAME: &'static str = "orders.welcome-email";
        const VERSION: u16 = 2;
    }

    fn context() -> JobContext {
        JobContext {
            job_id: uuid::Uuid::now_v7(),
            correlation_id: uuid::Uuid::now_v7(),
            causation_id: None,
            attempt: 1,
            maximum_attempts: 5,
            deadline: None,
            partition: None,
            metadata: BTreeMap::new(),
        }
    }

    #[test]
    fn context_debug_excludes_partition_and_metadata_values() {
        let mut subject = context();
        subject.partition = Some("tenant-secret-partition".into());
        subject
            .metadata
            .insert("source".into(), "context-secret-value".into());
        let text = format!("{subject:?}");
        assert!(
            !text.contains("tenant-secret-partition"),
            "context debug leaked the partition value"
        );
        assert!(
            !text.contains("context-secret-value"),
            "context debug leaked a metadata value"
        );
        assert!(
            text.contains("metadata_names"),
            "context debug shows metadata names only"
        );
    }

    #[tokio::test]
    async fn typed_job_decodes_payload_once_and_runs() {
        let registry = JobHandlerRegistry::new();
        registry
            .register_typed::<WelcomeEmail, _, _>(|job: WelcomeEmail, _ctx| async move {
                assert_eq!(job.order_id, "o-1");
                Ok(())
            })
            .expect("register");
        let resolved = registry
            .resolve("orders.welcome-email", 2)
            .expect("resolve");
        resolved
            .execute(serde_json::json!({ "order_id": "o-1" }), context())
            .await
            .expect("execute");
    }

    #[test]
    fn duplicate_registration_fails() {
        let registry = JobHandlerRegistry::new();
        registry
            .register_typed::<WelcomeEmail, _, _>(|_: WelcomeEmail, _| async { Ok(()) })
            .expect("first registration");
        let error = registry
            .register_typed::<WelcomeEmail, _, _>(|_: WelcomeEmail, _| async { Ok(()) })
            .expect_err("duplicate");
        assert!(matches!(
            error,
            JobError::DuplicateRegistration { job_version: 2, .. }
        ));
    }

    #[test]
    fn unknown_job_fails_deterministically() {
        let registry = JobHandlerRegistry::new();
        let error = registry.resolve("nope.missing", 1).expect_err("unknown");
        assert!(matches!(error, JobError::UnknownJob(name) if name == "nope.missing"));
    }

    #[test]
    fn unsupported_version_fails_deterministically() {
        let registry = JobHandlerRegistry::new();
        registry
            .register_typed::<WelcomeEmail, _, _>(|_: WelcomeEmail, _| async { Ok(()) })
            .expect("register");
        let error = registry
            .resolve("orders.welcome-email", 9)
            .expect_err("unsupported");
        assert!(matches!(
            error,
            JobError::UnsupportedJobVersion { job_version: 9, .. }
        ));
    }

    #[tokio::test]
    async fn previous_version_upcasts_through_registered_adapter() {
        let registry = JobHandlerRegistry::new();
        registry
            .register_typed::<WelcomeEmail, _, _>(|job: WelcomeEmail, _| async move {
                assert_eq!(job.order_id, "o-1");
                Ok(())
            })
            .expect("register");
        registry
            .register_upcaster(
                "orders.welcome-email",
                1,
                2,
                std::sync::Arc::new(|mut payload| {
                    payload
                        .as_object_mut()
                        .ok_or_else(|| "expected object".to_owned())?
                        .insert("order_id".into(), serde_json::json!("o-1"));
                    Ok(payload)
                }),
            )
            .expect("upcaster");
        let resolved = registry
            .resolve("orders.welcome-email", 1)
            .expect("resolve via upcaster");
        assert_eq!(resolved.job_version(), 2);
        resolved
            .execute(serde_json::json!({ "orderId": "o-1" }), context())
            .await
            .expect("upcast payload executes");
    }

    #[test]
    fn failure_codes_are_sanitized() {
        assert_eq!(
            JobExecutionFailure::retryable("Notify Failed!").code(),
            "notify-failed"
        );
        assert_eq!(
            JobExecutionFailure::permanent("").code(),
            "jobs-handler-failed"
        );
        assert!(JobExecutionFailure::permanent("x").is_permanent());
    }
}
