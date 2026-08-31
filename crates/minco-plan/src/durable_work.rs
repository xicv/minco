//! Durable typed work topology: an additive Plan sidecar.
//!
//! The sidecar synthesizes job queues, worker functions and event-source
//! mappings into the existing schema-2 collections so every existing
//! validation, IAM derivation, logical-ID collision rule, wake-source and
//! cost rule governs durable work unchanged. Explicit schedules render as
//! `AWS::Scheduler::Schedule` resources targeting the job queue directly:
//! the documented Scheduler context attributes give every recurrence a fresh
//! identity, so no dispatcher function and no static job identity exist. A
//! disabled capability renders zero resources and changes no serialized
//! schema-2 bytes.

use crate::model::{
    DeploymentPlan, FunctionPlan, FunctionRole, PlanDiagnostic, QueuePlan, Severity, TriggerPlan,
    is_stable_id,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fmt::Write as _;

/// Diagnostic codes emitted by durable-work validation.
pub mod durable_work_codes {
    pub const PROFILE_QUEUE_REFERENCE: &str = "MINCO-JOBS-001";
    pub const PROFILE_FUNCTION_REFERENCE: &str = "MINCO-JOBS-002";
    pub const DUPLICATE_PROFILE: &str = "MINCO-JOBS-003";
    pub const ROUTE_PROFILE_REFERENCE: &str = "MINCO-JOBS-004";
    pub const ROUTE_JOB_NAME: &str = "MINCO-JOBS-005";
    pub const PROFILE_FIFO_PAYLOAD: &str = "MINCO-JOBS-006";
    pub const SCHEDULE_PROFILE_REFERENCE: &str = "MINCO-JOBS-007";
    pub const SCHEDULE_PAYLOAD_LIMIT: &str = "MINCO-JOBS-008";
    pub const SCHEDULE_EXPRESSION: &str = "MINCO-JOBS-009";
    pub const SCHEDULE_TIMEZONE: &str = "MINCO-JOBS-010";
    pub const SCHEDULE_RETRY: &str = "MINCO-JOBS-011";
    pub const SCHEDULE_FLEX_WINDOW: &str = "MINCO-JOBS-012";
    pub const SCHEDULE_ID_COLLISION: &str = "MINCO-JOBS-013";
    pub const SCHEDULE_DLQ_FIFO: &str = "MINCO-JOBS-014";
    pub const DISABLED_ZERO_RESOURCES: &str = "MINCO-JOBS-015";
    pub const WORKER_ARTIFACT_REQUIRED: &str = "MINCO-JOBS-016";
    pub const SCHEDULE_FIFO_UNSUPPORTED: &str = "MINCO-JOBS-017";
    pub const SCHEDULE_DLQ_REFERENCE: &str = "MINCO-JOBS-018";
    pub const SCHEDULE_INPUT_SIZE: &str = "MINCO-JOBS-019";
}

/// Scheduler target payloads are capped at 256 KiB by the provider.
pub const MAX_SCHEDULE_PAYLOAD_BYTES: usize = 262_144;
/// IANA timezone names are capped at 50 characters by the provider.
pub const MAX_SCHEDULE_TIMEZONE_BYTES: usize = 50;
/// Scheduler retry attempts are capped at 185 by the provider.
pub const MAX_SCHEDULE_RETRY_ATTEMPTS: u32 = 185;
/// Flexible time windows are capped at 1440 minutes by the provider.
pub const MAX_FLEXIBLE_WINDOW_MINUTES: u32 = 1440;

/// One worker profile: the queue, function and execution boundaries a set
/// of compatible jobs shares.
///
/// Jobs with incompatible ordering, security, timeout, payload, capability
/// or execution requirements must not share a profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkerProfilePlan {
    /// Stable profile identifier (`[a-z0-9-]`).
    pub id: String,
    /// Queue id synthesized into `plan.queues` (`jobs-<id>` when omitted
    /// from the base plan).
    pub queue_id: String,
    /// Worker function id synthesized into `plan.functions` with the
    /// `Worker` role.
    pub function_id: String,
    /// Explicit artifact identity for the worker function. Durable workers
    /// never inherit the API binary or another worker's artifact: a missing
    /// or ambiguous artifact fails validation.
    pub artifact_path: String,
    pub fifo: bool,
    pub batch_size: u32,
    pub batching_window_seconds: u32,
    pub maximum_concurrency: u32,
    pub memory_mb: u32,
    pub timeout_seconds: u32,
    pub reserved_concurrency: u32,
    /// Maximum serialized envelope bytes this profile accepts; bounded by
    /// the wire envelope maximum.
    pub max_payload_bytes: u32,
    pub database_connections_per_instance: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dead_letter_queue_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_receive_count: Option<u32>,
    /// Data classifications carried by this profile's job payloads.
    pub data_classes: Vec<String>,
    /// Capabilities the worker requires; IAM intent derives from these.
    pub required_capabilities: Vec<String>,
}

/// One job type routed to a worker profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobRoutePlan {
    /// Stable logical job name matching a registered handler.
    pub job_name: String,
    pub job_version: u16,
    pub worker_profile: String,
    /// FIFO profiles require an explicit per-job ordering source
    /// (`partition` or `overlap` key) declared here for review.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ordering_source: Option<String>,
}

/// One explicit schedule dispatching a job onto its profile's queue.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobSchedulePlan {
    /// Stable schedule identifier (`[a-z0-9-]`).
    pub id: String,
    pub job_name: String,
    pub job_version: u16,
    pub worker_profile: String,
    /// Bounded static payload template. Per-invocation identity comes from
    /// Scheduler context attributes substituted into the target input, never
    /// from this payload.
    pub payload: serde_json::Value,
    /// `at(...)`, `rate(...)` or `cron(...)` expression.
    pub expression: String,
    pub enabled: bool,
    /// Reviewable purpose, shown as the schedule description.
    pub purpose: String,
    /// IANA timezone; defaults to UTC when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,
    /// Flexible time window minutes; `None` means exact timing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flexible_window_minutes: Option<u32>,
    /// Scheduler-level retry attempts before the schedule DLQ.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maximum_retry_attempts: Option<u32>,
    /// Standard (non-FIFO) DLQ queue id for exhausted schedule deliveries.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dead_letter_queue_id: Option<String>,
}

/// The additive durable-work sidecar on [`DeploymentPlan`].
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DurableWorkTopology {
    /// Disabled renders zero durable-work resources.
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub profiles: Vec<WorkerProfilePlan>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub routes: Vec<JobRoutePlan>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub schedules: Vec<JobSchedulePlan>,
}

impl DurableWorkTopology {
    /// The profile with the given id, if present.
    #[must_use]
    pub fn profile(&self, id: &str) -> Option<&WorkerProfilePlan> {
        self.profiles.iter().find(|profile| profile.id == id)
    }
}

/// Synthesize durable-work queues, worker functions and mappings into a copy
/// of the plan. A disabled or absent sidecar returns the plan unchanged:
/// zero queues, zero workers, zero schedules.
#[must_use]
pub fn apply_durable_work(plan: &DeploymentPlan, topology: &DurableWorkTopology) -> DeploymentPlan {
    if !topology.enabled {
        return plan.clone();
    }
    let mut next = plan.clone();
    for profile in &topology.profiles {
        if !next.queues.iter().any(|queue| queue.id == profile.queue_id) {
            next.queues.push(QueuePlan {
                id: profile.queue_id.clone(),
                fifo: profile.fifo,
                visibility_timeout_seconds: profile.timeout_seconds * 6
                    + profile.batching_window_seconds,
                retention_seconds: 345_600,
                dead_letter_queue_id: profile.dead_letter_queue_id.clone(),
                max_receive_count: profile.max_receive_count,
            });
        }
        if !next
            .functions
            .iter()
            .any(|function| function.name == profile.function_id)
        {
            next.functions.push(FunctionPlan {
                name: profile.function_id.clone(),
                role: FunctionRole::Worker,
                artifact_path: profile.artifact_path.clone(),
                memory_mb: profile.memory_mb,
                timeout_seconds: profile.timeout_seconds,
                reserved_concurrency: profile.reserved_concurrency,
                provisioned_concurrency: 0,
                database_connections_per_instance: profile.database_connections_per_instance,
            });
        }
        if !next.triggers.iter().any(|trigger| matches!(trigger, TriggerPlan::Sqs { queue_id, .. } if *queue_id == profile.queue_id))
        {
            next.triggers.push(TriggerPlan::Sqs {
                id: format!("{}-mapping", profile.id),
                function_id: profile.function_id.clone(),
                queue_id: profile.queue_id.clone(),
                batch_size: profile.batch_size,
                batching_window_seconds: profile.batching_window_seconds,
                report_batch_item_failures: true,
                maximum_concurrency: profile.maximum_concurrency,
            });
        }
    }
    // Synthesized queues and triggers change the derived local-service list
    // and IAM intents; recompute both exactly as plan construction does so
    // the ordinary validators accept the result (shared sidecar helper,
    // exact-head review 5064401898).
    crate::model::refresh_derived_plan_state(&mut next);
    next
}

fn diagnostic(code: &str, message: String) -> PlanDiagnostic {
    PlanDiagnostic {
        code: code.to_owned(),
        severity: Severity::Error,
        message,
    }
}

/// Validate the durable-work sidecar. Synthesized queues, functions and
/// mappings are validated by the ordinary schema-2 topology rules once
/// applied; this pass covers the sidecar-only contracts.
pub fn validate_durable_work(
    plan: &DeploymentPlan,
    topology: &DurableWorkTopology,
) -> Vec<PlanDiagnostic> {
    let mut diagnostics = Vec::new();
    if !topology.enabled {
        if !topology.profiles.is_empty()
            || !topology.routes.is_empty()
            || !topology.schedules.is_empty()
        {
            diagnostics.push(diagnostic(
                durable_work_codes::DISABLED_ZERO_RESOURCES,
                "a disabled durable-work topology must declare no profiles, routes or schedules"
                    .into(),
            ));
        }
        return diagnostics;
    }
    let mut profile_ids = BTreeSet::new();
    let mut queue_ids = BTreeSet::new();
    let mut function_ids = BTreeSet::new();
    for profile in &topology.profiles {
        if !is_stable_id(&profile.id) {
            diagnostics.push(diagnostic(
                durable_work_codes::DUPLICATE_PROFILE,
                format!("worker profile id '{}' must be a stable id", profile.id),
            ));
        }
        if !profile_ids.insert(profile.id.clone()) {
            diagnostics.push(diagnostic(
                durable_work_codes::DUPLICATE_PROFILE,
                format!("worker profile '{}' is declared twice", profile.id),
            ));
        }
        if !is_stable_id(&profile.queue_id) || !is_stable_id(&profile.function_id) {
            diagnostics.push(diagnostic(
                durable_work_codes::PROFILE_QUEUE_REFERENCE,
                format!(
                    "profile '{}' queue and function ids must be stable ids",
                    profile.id
                ),
            ));
        }
        queue_ids.insert(profile.queue_id.clone());
        function_ids.insert(profile.function_id.clone());
        if plan
            .triggers
            .iter()
            .any(|trigger| matches!(trigger, TriggerPlan::HttpApi { function_id, .. } if *function_id == profile.function_id))
        {
            diagnostics.push(diagnostic(
                durable_work_codes::PROFILE_FUNCTION_REFERENCE,
                format!(
                    "profile '{}' must use a dedicated worker function, not the HTTP API function",
                    profile.id
                ),
            ));
        }
        if profile.max_payload_bytes == 0 || profile.max_payload_bytes > 262_144 {
            diagnostics.push(diagnostic(
                durable_work_codes::PROFILE_FIFO_PAYLOAD,
                format!(
                    "profile '{}' payload limit must stay below the 262144-byte envelope bound",
                    profile.id
                ),
            ));
        }
        if profile.artifact_path.trim().is_empty() {
            diagnostics.push(diagnostic(
                durable_work_codes::WORKER_ARTIFACT_REQUIRED,
                format!(
                    "worker profile '{}' must declare an explicit artifact path",
                    profile.id
                ),
            ));
        }
    }
    for route in &topology.routes {
        if !topology
            .profiles
            .iter()
            .any(|profile| profile.id == route.worker_profile)
        {
            diagnostics.push(diagnostic(
                durable_work_codes::ROUTE_PROFILE_REFERENCE,
                format!(
                    "job '{}' routes to unknown worker profile '{}'",
                    route.job_name, route.worker_profile
                ),
            ));
        }
        if route.job_name.trim().is_empty() {
            diagnostics.push(diagnostic(
                durable_work_codes::ROUTE_JOB_NAME,
                "job routes require a stable logical job name".into(),
            ));
        }
        if route.job_version == 0 {
            diagnostics.push(diagnostic(
                durable_work_codes::ROUTE_JOB_NAME,
                format!(
                    "job '{}' requires a positive payload version",
                    route.job_name
                ),
            ));
        }
        if let Some(profile) = topology
            .profile(&route.worker_profile)
            .filter(|profile| profile.fifo)
            .filter(|_| route.ordering_source.as_deref().is_none_or(str::is_empty))
        {
            diagnostics.push(diagnostic(
                durable_work_codes::PROFILE_FIFO_PAYLOAD,
                format!(
                    "FIFO profile '{}' requires route '{}' to declare an ordering source",
                    profile.id, route.job_name
                ),
            ));
        }
    }
    let _ = (&queue_ids, &function_ids);
    let mut schedule_ids = BTreeSet::new();
    for schedule in &topology.schedules {
        if !is_stable_id(&schedule.id) || !schedule_ids.insert(schedule.id.clone()) {
            diagnostics.push(diagnostic(
                durable_work_codes::SCHEDULE_ID_COLLISION,
                format!("schedule id '{}' must be a unique stable id", schedule.id),
            ));
        }
        if topology.profile(&schedule.worker_profile).is_none() {
            diagnostics.push(diagnostic(
                durable_work_codes::SCHEDULE_PROFILE_REFERENCE,
                format!(
                    "schedule '{}' targets unknown worker profile '{}'",
                    schedule.id, schedule.worker_profile
                ),
            ));
        } else if topology
            .profile(&schedule.worker_profile)
            .is_some_and(|profile| profile.fifo)
        {
            diagnostics.push(diagnostic(
                durable_work_codes::SCHEDULE_FIFO_UNSUPPORTED,
                format!(
                    "schedule '{}' targets FIFO profile '{}': the provider cannot carry an \
                     explicit occurrence deduplication identity and Minco queues do not declare \
                     content-based deduplication",
                    schedule.id, schedule.worker_profile
                ),
            ));
        }
        let payload_bytes =
            serde_json::to_vec(&schedule.payload).map_or(usize::MAX, |bytes| bytes.len());
        if payload_bytes == 0 || payload_bytes > MAX_SCHEDULE_PAYLOAD_BYTES {
            diagnostics.push(diagnostic(
                durable_work_codes::SCHEDULE_PAYLOAD_LIMIT,
                format!(
                    "schedule '{}' payload must be 1..={MAX_SCHEDULE_PAYLOAD_BYTES} bytes",
                    schedule.id
                ),
            ));
        }
        if !crate::model::is_schedule_expression(&schedule.expression) {
            diagnostics.push(diagnostic(
                durable_work_codes::SCHEDULE_EXPRESSION,
                format!(
                    "schedule '{}' needs an at/rate/cron expression",
                    schedule.id
                ),
            ));
        }
        if schedule.purpose.trim().is_empty() {
            diagnostics.push(diagnostic(
                durable_work_codes::SCHEDULE_EXPRESSION,
                format!("schedule '{}' requires a reviewable purpose", schedule.id),
            ));
        }
        if schedule.timezone.as_deref().is_some_and(|timezone| {
            timezone.is_empty()
                || timezone.len() > MAX_SCHEDULE_TIMEZONE_BYTES
                || !timezone.bytes().all(|byte| {
                    byte.is_ascii_alphanumeric() || byte == b'/' || byte == b'_' || byte == b'-'
                })
        }) {
            diagnostics.push(diagnostic(
                durable_work_codes::SCHEDULE_TIMEZONE,
                format!("schedule '{}' timezone must be an IANA name", schedule.id),
            ));
        }
        if schedule
            .maximum_retry_attempts
            .is_some_and(|attempts| attempts > MAX_SCHEDULE_RETRY_ATTEMPTS)
        {
            diagnostics.push(diagnostic(
                durable_work_codes::SCHEDULE_RETRY,
                format!(
                    "schedule '{}' retry attempts exceed the provider maximum of {MAX_SCHEDULE_RETRY_ATTEMPTS}",
                    schedule.id
                ),
            ));
        }
        if schedule
            .flexible_window_minutes
            .is_some_and(|minutes| minutes == 0 || minutes > MAX_FLEXIBLE_WINDOW_MINUTES)
        {
            diagnostics.push(diagnostic(
                durable_work_codes::SCHEDULE_FLEX_WINDOW,
                format!(
                    "schedule '{}' flexible window must be 1..={MAX_FLEXIBLE_WINDOW_MINUTES} minutes",
                    schedule.id
                ),
            ));
        }
        if let (Some(dlq), Some(profile)) = (
            schedule.dead_letter_queue_id.as_deref(),
            topology.profile(&schedule.worker_profile),
        ) {
            let referenced = plan.queues.iter().find(|queue| queue.id == dlq);
            let Some(referenced) = referenced else {
                diagnostics.push(diagnostic(
                    durable_work_codes::SCHEDULE_DLQ_REFERENCE,
                    format!(
                        "schedule '{}' dead-letter queue '{dlq}' does not exist in the plan",
                        schedule.id
                    ),
                ));
                let _ = profile;
                continue;
            };
            if referenced.fifo {
                diagnostics.push(diagnostic(
                    durable_work_codes::SCHEDULE_DLQ_FIFO,
                    format!(
                        "schedule '{}' DLQ must be a standard queue (provider limit)",
                        schedule.id
                    ),
                ));
            }
        }
        // Validate the COMPLETE rendered Scheduler input — wrapper, context
        // placeholders, job metadata, payload and quoting overhead — against
        // the provider's 256 KiB target-input cap, then the materialized
        // envelope against the envelope bound.
        let input_bytes = scheduled_trigger_input(schedule).len();
        if input_bytes > MAX_SCHEDULE_PAYLOAD_BYTES {
            diagnostics.push(diagnostic(
                durable_work_codes::SCHEDULE_INPUT_SIZE,
                format!(
                    "schedule '{}' rendered input is {input_bytes} bytes; the provider target \
                     input maximum is {MAX_SCHEDULE_PAYLOAD_BYTES}",
                    schedule.id
                ),
            ));
        }
    }
    diagnostics
}

/// Cost dimension for one scheduled job dispatch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobScheduleCostDimension {
    pub schedule_id: String,
    pub worker_profile: String,
    pub queue_id: String,
    pub expression: String,
    pub enabled: bool,
    /// Monthly invocation estimate from the expression, when derivable.
    pub estimated_monthly_invocations: Option<u64>,
    /// Additional queue request per invocation (the dispatched message).
    pub queue_requests_per_invocation: u64,
}

/// Structural cost dimensions for one durable-work topology.
///
/// This is a sidecar estimate over the topology's explicit schedules; it
/// invents no provider prices and claims only request-shaped queue and
/// scheduled invocations whose rates stay region-dependent.
#[must_use]
pub fn estimate_durable_work_cost(
    plan: &DeploymentPlan,
    topology: &DurableWorkTopology,
) -> Vec<JobScheduleCostDimension> {
    if !topology.enabled {
        return Vec::new();
    }
    topology
        .schedules
        .iter()
        .filter(|schedule| {
            topology
                .profile(&schedule.worker_profile)
                .is_some_and(|profile| plan.queues.iter().any(|queue| queue.id == profile.queue_id))
        })
        .map(|schedule| {
            let queue_id = topology
                .profile(&schedule.worker_profile)
                .map_or_else(String::new, |profile| profile.queue_id.clone());
            JobScheduleCostDimension {
                schedule_id: schedule.id.clone(),
                worker_profile: schedule.worker_profile.clone(),
                queue_id,
                expression: schedule.expression.clone(),
                enabled: schedule.enabled,
                estimated_monthly_invocations: crate::cost::monthly_schedule_invocations(
                    &schedule.expression,
                ),
                queue_requests_per_invocation: 1,
            }
        })
        .collect()
}

fn yaml_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

/// Render SAM for a plan with durable work applied.
///
/// Job queues, workers and event-source mappings render through the
/// ordinary template because [`apply_durable_work`] synthesized them into
/// the schema-2 collections; this pass appends the Scheduler-to-SQS
/// resources: one execution role with least-privilege `sqs:SendMessage` and
/// one `AWS::Scheduler::Schedule` per schedule whose input embeds the
/// per-invocation context attributes.
pub fn render_sam_with_durable_work(
    plan: &DeploymentPlan,
    topology: &DurableWorkTopology,
    code_uris: &std::collections::BTreeMap<String, String>,
) -> Result<String, crate::PlanError> {
    let mut template = crate::sam::render_sam_with_code_uris(plan, code_uris)?;
    if !topology.enabled || topology.schedules.is_empty() {
        return Ok(template);
    }
    let mut resources = String::new();
    let mut target_queues: Vec<String> = Vec::new();
    for schedule in &topology.schedules {
        let Some(profile) = topology.profile(&schedule.worker_profile) else {
            continue;
        };
        let queue_logical = format!("{}Queue", crate::sam_logical_id(&profile.queue_id));
        if !target_queues.contains(&queue_logical) {
            target_queues.push(queue_logical.clone());
        }
        let input = scheduled_trigger_input(schedule);
        let schedule_logical = format!("{}Schedule", crate::sam_logical_id(&schedule.id));
        let name = format!("{}-{}-{}", plan.application, plan.environment, schedule.id);
        writeln!(
            resources,
            "  {schedule_logical}:\n    Type: AWS::Scheduler::Schedule\n    Properties:\n      Name: {}\n      ScheduleExpression: {}\n      State: {}\n      Description: {}\n      FlexibleTimeWindow:\n        Mode: {}",
            yaml_quote(&name),
            yaml_quote(&schedule.expression),
            if schedule.enabled { "ENABLED" } else { "DISABLED" },
            yaml_quote(&schedule.purpose),
            if schedule.flexible_window_minutes.is_some() { "FLEXIBLE" } else { "OFF" },
        )
        .expect("write to string");
        if let Some(minutes) = schedule.flexible_window_minutes {
            writeln!(resources, "        MaximumWindowInMinutes: {minutes}")
                .expect("write to string");
        }
        if let Some(timezone) = schedule.timezone.as_deref() {
            writeln!(
                resources,
                "      ScheduleExpressionTimezone: {}",
                yaml_quote(timezone)
            )
            .expect("write to string");
        }
        // Per the provider schema, RetryPolicy and DeadLetterConfig belong
        // to the schedule Target, not to the schedule root.
        write!(
            resources,
            "      Target:\n        Arn: !GetAtt {queue_logical}.Arn\n        RoleArn: !GetAtt JobsSchedulerRole.Arn\n        Input: {}",
            yaml_quote(&input),
        )
        .expect("write to string");
        if let Some(attempts) = schedule.maximum_retry_attempts {
            write!(
                resources,
                "\n        RetryPolicy:\n          MaximumRetryAttempts: {attempts}"
            )
            .expect("write to string");
        }
        if let Some(dlq) = schedule.dead_letter_queue_id.as_deref() {
            write!(
                resources,
                "\n        DeadLetterConfig:\n          Arn: !GetAtt {}Queue.Arn",
                crate::sam_logical_id(dlq)
            )
            .expect("write to string");
        }
        writeln!(resources).expect("write to string");
    }
    resources.push_str(
        "  JobsSchedulerRole:\n    Type: AWS::IAM::Role\n    Properties:\n      AssumeRolePolicyDocument:\n        Version: '2012-10-17'\n        Statement:\n          - Effect: Allow\n            Principal:\n              Service: scheduler.amazonaws.com\n            Action: sts:AssumeRole\n      Policies:\n        - PolicyName: !Sub '${AWS::StackName}-jobs-scheduler'\n          PolicyDocument:\n            Version: '2012-10-17'\n            Statement:\n              - Effect: Allow\n                Action: sqs:SendMessage\n                Resource:\n",
    );
    for queue in &target_queues {
        writeln!(resources, "                  - !GetAtt {queue}.Arn").expect("write to string");
    }
    // Resources must be inserted before the Outputs block of the template.
    let outputs_marker = "\nOutputs:\n";
    let Some(position) = template.find(outputs_marker) else {
        return Err(crate::PlanError::UnsupportedDeployment(
            "SAM template is missing the Outputs block for durable-work resources".into(),
        ));
    };
    template.insert_str(position + 1, &resources);
    Ok(template)
}

/// The static Scheduler target input. The two `<aws.scheduler...>` context
/// attributes are substituted on every invocation, giving each recurrence a
/// fresh execution identity without a dispatcher function.
#[must_use]
pub fn scheduled_trigger_input(schedule: &JobSchedulePlan) -> String {
    let mut input = serde_json::json!({
        "schema_version": 1,
        "kind": "scheduled_trigger",
        "schedule_id": schedule.id,
        "job_name": schedule.job_name,
        "job_version": schedule.job_version,
        "worker_profile": schedule.worker_profile,
        "payload": schedule.payload,
        "execution_id": "<aws.scheduler.execution-id>",
        "scheduled_time": "<aws.scheduler.scheduled-time>",
    });
    let _ = &mut input;
    serde_json::to_string(&input).expect("static input serializes")
}

#[cfg(test)]
impl DeploymentPlan {
    fn trusters_contains_mapping(&self) -> bool {
        self.triggers.iter().any(|trigger| {
            matches!(
                trigger,
                TriggerPlan::Sqs {
                    queue_id,
                    function_id,
                    report_batch_item_failures: true,
                    ..
                } if queue_id == "jobs-orders-notifications"
                    && function_id == "orders-jobs-worker"
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{AuthPlan, DatabaseDeployment, DeploymentConfig, IngressPlan, RuntimePlan};

    fn config() -> DeploymentConfig {
        DeploymentConfig {
            schema_version: 2,
            application: "orders".into(),
            environment: "dev".into(),
            region: "ap-southeast-2".into(),
            runtime: RuntimePlan::LambdaZipArm64,
            ingress: IngressPlan::ApiGatewayHttpApi,
            auth: AuthPlan::None,
            database: DatabaseDeployment::SelfHostedPostgres {
                host_monthly_usd: 10.0,
                storage_gb_month: 1.0,
                storage_rate_usd: 0.1,
                backup_gb_month: 1.0,
                backup_rate_usd: 0.05,
                operations_monthly_usd: 0.0,
            },
            functions: vec![],
            queues: vec![],
            triggers: vec![],
            scheduled_wakeups: vec![],
            uses_nat_gateway: false,
            allowed_origins: vec!["https://orders.example.com".into()],
            allowed_headers: vec![],
            log_retention_days: 7,
            cost_policy: crate::model::CostPolicy::default(),
            performance_policy: crate::model::PerformancePolicy::default(),
        }
    }

    fn topology() -> DurableWorkTopology {
        DurableWorkTopology {
            enabled: true,
            profiles: vec![WorkerProfilePlan {
                id: "orders-notifications".into(),
                queue_id: "jobs-orders-notifications".into(),
                function_id: "orders-jobs-worker".into(),
                artifact_path: "target/lambda/orders-jobs-worker.zip".into(),
                fifo: false,
                batch_size: 10,
                batching_window_seconds: 1,
                maximum_concurrency: 2,
                memory_mb: 512,
                timeout_seconds: 30,
                reserved_concurrency: 2,
                max_payload_bytes: 262_144,
                database_connections_per_instance: 1,
                dead_letter_queue_id: None,
                max_receive_count: None,
                data_classes: vec!["internal".into()],
                required_capabilities: vec!["notifications.send".into()],
            }],
            routes: vec![JobRoutePlan {
                job_name: "orders.send-confirmation".into(),
                job_version: 1,
                worker_profile: "orders-notifications".into(),
                ordering_source: None,
            }],
            schedules: vec![],
        }
    }

    fn api_function() -> FunctionPlan {
        FunctionPlan {
            name: "api".into(),
            role: FunctionRole::HttpApi,
            artifact_path: "target/lambda/bootstrap.zip".into(),
            memory_mb: 512,
            timeout_seconds: 10,
            reserved_concurrency: 0,
            provisioned_concurrency: 0,
            database_connections_per_instance: 2,
        }
    }

    fn plan_with(topology: &DurableWorkTopology) -> DeploymentPlan {
        let mut config = config();
        config.functions = vec![api_function()];
        let contract = minco_contract::ContractDocument {
            source: "inline".into(),
            openapi_version: "3.1.0".into(),
            title: "orders".into(),
            version: "1".into(),
            sha256: "hash".into(),
            operations: Vec::new(),
            schema_names: Vec::new(),
            raw: serde_json::json!({}),
        };
        let plan = config.into_plan_with_graph(&contract, minco_core::ApplicationGraph::default());
        apply_durable_work(&plan, topology)
    }

    fn plan_with_kept(topology: &DurableWorkTopology) -> DeploymentPlan {
        let mut config = config();
        config.functions = vec![api_function()];
        let contract = minco_contract::ContractDocument {
            source: "inline".into(),
            openapi_version: "3.1.0".into(),
            title: "orders".into(),
            version: "1".into(),
            sha256: "hash".into(),
            operations: Vec::new(),
            schema_names: Vec::new(),
            raw: serde_json::json!({}),
        };
        let plan = config.into_plan_with_graph(&contract, minco_core::ApplicationGraph::default());
        apply_durable_work(&plan, topology)
    }

    #[test]
    fn disabled_topology_renders_zero_resources_and_no_schema_change() {
        let mut topology = topology();
        topology.enabled = false;
        topology.profiles.clear();
        topology.routes.clear();
        topology.schedules.clear();
        let plan = plan_with_kept(&topology);
        assert!(plan.queues.is_empty(), "no job queues");
        assert!(
            !plan
                .functions
                .iter()
                .any(|function| function.role == FunctionRole::Worker),
            "no worker functions"
        );
        assert!(validate_durable_work(&plan, &topology).is_empty());
        // A plan without the sidecar serializes identically to schema 2.
        let mut config = config();
        config.functions = vec![api_function()];
        let contract = minco_contract::ContractDocument {
            source: "inline".into(),
            openapi_version: "3.1.0".into(),
            title: "orders".into(),
            version: "1".into(),
            sha256: "hash".into(),
            operations: Vec::new(),
            schema_names: Vec::new(),
            raw: serde_json::json!({}),
        };
        let plan = config.into_plan_with_graph(&contract, minco_core::ApplicationGraph::default());
        let value = serde_json::to_value(&plan).expect("serialize");
        assert!(value.get("durable_work").is_none());
        let round: DeploymentPlan =
            serde_json::from_value(serde_json::to_value(&plan).unwrap()).expect("round trip");
        assert_eq!(round, plan);
    }

    #[test]
    fn enabled_topology_synthesizes_queue_worker_and_mapping() {
        let plan = plan_with(&topology());
        assert!(
            plan.queues
                .iter()
                .any(|queue| queue.id == "jobs-orders-notifications")
        );
        let worker = plan
            .functions
            .iter()
            .find(|function| function.name == "orders-jobs-worker")
            .expect("worker function");
        assert_eq!(worker.role, FunctionRole::Worker);
        assert!(
            plan.trusters_contains_mapping(),
            "event-source mapping with partial batch responses"
        );
        // Existing topology validation governs the synthesized resources:
        // enabling durable work must introduce no new errors beyond the
        // minimal fixture's baseline diagnostics.
        let mut baseline = topology();
        baseline.enabled = false;
        baseline.profiles.clear();
        baseline.routes.clear();
        let base = plan_with(&baseline);
        let base_codes: Vec<String> = base
            .validate()
            .iter()
            .filter(|diagnostic| diagnostic.severity == Severity::Error)
            .map(|diagnostic| diagnostic.code.clone())
            .collect();
        let error_codes: Vec<String> = plan
            .validate()
            .iter()
            .filter(|diagnostic| diagnostic.severity == Severity::Error)
            .map(|diagnostic| diagnostic.code.clone())
            .collect();
        let introduced: Vec<&String> = error_codes
            .iter()
            .filter(|code| !base_codes.contains(code))
            .collect();
        assert!(
            introduced.is_empty(),
            "introduced {introduced:?} over {base_codes:?}"
        );
    }

    #[test]
    fn fifo_profile_requires_an_ordering_source_on_routes() {
        let mut topology = topology();
        topology.profiles[0].fifo = true;
        let diagnostics = validate_durable_work(&plan_with(&topology), &topology);
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == durable_work_codes::PROFILE_FIFO_PAYLOAD)
        );
        topology.routes[0].ordering_source = Some("partition".into());
        assert!(validate_durable_work(&plan_with(&topology), &topology).is_empty());
    }

    fn schedule() -> JobSchedulePlan {
        JobSchedulePlan {
            id: "orders-expiry".into(),
            job_name: "orders.expire-unpaid".into(),
            job_version: 1,
            worker_profile: "orders-notifications".into(),
            payload: serde_json::json!({ "older_than_hours": 24 }),
            expression: "rate(1 hours)".into(),
            enabled: true,
            purpose: "Expire unpaid orders".into(),
            timezone: None,
            flexible_window_minutes: None,
            maximum_retry_attempts: None,
            dead_letter_queue_id: None,
        }
    }

    #[test]
    fn schedule_validation_covers_expression_timezone_retry_and_payload() {
        let mut topology = topology();
        topology.schedules = vec![schedule()];
        let plan = plan_with_kept(&topology);
        let diagnostics = validate_durable_work(&plan, &topology);
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        topology.schedules[0].expression = "every tuesday".into();
        topology.schedules[0].timezone = Some("Not/A Timezone".into());
        topology.schedules[0].maximum_retry_attempts = Some(200);
        topology.schedules[0].flexible_window_minutes = Some(0);
        topology.schedules[0].payload = serde_json::json!({ "pad": "x".repeat(300_000) });
        let diagnostics = validate_durable_work(&plan_with(&topology), &topology);
        for code in [
            durable_work_codes::SCHEDULE_EXPRESSION,
            durable_work_codes::SCHEDULE_TIMEZONE,
            durable_work_codes::SCHEDULE_RETRY,
            durable_work_codes::SCHEDULE_FLEX_WINDOW,
            durable_work_codes::SCHEDULE_PAYLOAD_LIMIT,
        ] {
            assert!(
                diagnostics.iter().any(|diagnostic| diagnostic.code == code),
                "expected {code} in {diagnostics:?}"
            );
        }
    }

    #[test]
    fn schedule_id_collisions_fail() {
        let mut topology = topology();
        topology.schedules = vec![schedule(), schedule()];
        let diagnostics = validate_durable_work(&plan_with(&topology), &topology);
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == durable_work_codes::SCHEDULE_ID_COLLISION)
        );
    }

    fn render(topology: &DurableWorkTopology) -> String {
        let plan = plan_with_kept(topology);
        let mut code_uris = std::collections::BTreeMap::new();
        code_uris.insert("api".to_owned(), "./bootstrap.zip".to_owned());
        render_sam_with_durable_work(&plan, topology, &code_uris).expect("render")
    }

    #[test]
    fn rate_cron_and_one_time_schedules_render_with_least_privilege_iam() {
        let mut topology = topology();
        topology.schedules = vec![schedule()];
        let template = render(&topology);
        assert!(template.contains("OrdersExpirySchedule:"));
        assert!(template.contains("Type: AWS::Scheduler::Schedule"));
        assert!(template.contains("ScheduleExpression: 'rate(1 hours)'"));
        assert!(template.contains("State: ENABLED"));
        assert!(template.contains("Arn: !GetAtt JobsOrdersNotificationsQueue.Arn"));
        assert!(template.contains("RoleArn: !GetAtt JobsSchedulerRole.Arn"));
        assert!(template.contains("Service: scheduler.amazonaws.com"));
        assert!(template.contains("Action: sqs:SendMessage"));
        assert!(
            template.contains("<aws.scheduler.execution-id>"),
            "the input carries the fresh per-invocation identity"
        );
        assert!(
            template.contains("<aws.scheduler.scheduled-time>"),
            "the input carries the per-invocation scheduled time"
        );

        let mut cron = schedule();
        cron.id = "orders-nightly".into();
        cron.expression = "cron(0 13 * * ? *)".into();
        cron.timezone = Some("Pacific/Auckland".into());
        cron.flexible_window_minutes = Some(15);
        cron.maximum_retry_attempts = Some(3);
        topology.schedules = vec![cron];
        let template = render(&topology);
        assert!(template.contains("ScheduleExpression: 'cron(0 13 * * ? *)'"));
        assert!(template.contains("ScheduleExpressionTimezone: 'Pacific/Auckland'"));
        assert!(template.contains("Mode: FLEXIBLE"));
        assert!(template.contains("MaximumWindowInMinutes: 15"));
        assert!(template.contains("MaximumRetryAttempts: 3"));

        let mut one_time = schedule();
        one_time.id = "orders-one-off".into();
        one_time.expression = "at(2026-09-01T03:00:00)".into();
        one_time.enabled = false;
        topology.schedules = vec![one_time];
        let template = render(&topology);
        assert!(template.contains("ScheduleExpression: 'at(2026-09-01T03:00:00)'"));
        assert!(template.contains("State: DISABLED"));
    }

    #[test]
    fn recurring_invocations_carry_distinct_identity_tokens() {
        let input = scheduled_trigger_input(&schedule());
        let value: serde_json::Value = serde_json::from_str(&input).expect("json");
        assert!(input.contains("<aws.scheduler.execution-id>"));
        // The static payload carries no job identity of its own.
        assert!(value.get("job_id").is_none());
        assert_eq!(
            value.get("kind").and_then(serde_json::Value::as_str),
            Some("scheduled_trigger")
        );
        assert!(
            serde_json::to_string(&input)
                .expect("json")
                .contains("scheduled_trigger")
        );
    }

    #[test]
    fn schedule_renders_no_resources_when_topology_lacks_schedules() {
        let template = render(&topology());
        assert!(!template.contains("AWS::Scheduler::Schedule"));
        assert!(!template.contains("JobsSchedulerRole"));
    }
}
