//! Inbound mail binding sidecar (ADR-0065).
//!
//! Renders the explicit wake chain `SES receiving -> private S3 raw MIME ->
//! S3 ObjectCreated notification -> SQS wake -> worker` into the deployment
//! plan: one synthesized wake queue per binding, one SQS event-source
//! trigger for the bound worker, and SAM resources for the raw bucket
//! (with lifecycle retention and SES write policy), the bucket-to-queue
//! notification, the queue policy, and the SES receipt rule. Nothing is
//! deployed from this crate; no provider contact happens here.

use crate::{
    DeploymentPlan, InboundMailBinding, PlanDiagnostic, QueuePlan, Severity, TriggerPlan,
    sam_logical_id,
};
use std::collections::BTreeSet;
use std::fmt::Write as _;

/// Stable validation codes for the inbound-mail sidecar.
pub mod inbound_mail_codes {
    pub const DISABLED_ZERO_BINDINGS: &str = "MINCO-MAIL-001";
    pub const DUPLICATE_BINDING_ID: &str = "MINCO-MAIL-002";
    pub const QUEUE_SHARED: &str = "MINCO-MAIL-003";
    pub const UNKNOWN_WORKER_FUNCTION: &str = "MINCO-MAIL-004";
    pub const WORKER_NOT_WORKER_ROLE: &str = "MINCO-MAIL-005";
    pub const QUEUE_MISSING: &str = "MINCO-MAIL-006";
    pub const TRIGGER_MISSING: &str = "MINCO-MAIL-007";
    pub const INVALID_MAILBOX_SCOPE: &str = "MINCO-MAIL-008";
    pub const INVALID_BUCKET_NAME: &str = "MINCO-MAIL-009";
    pub const INVALID_KEY_PREFIX: &str = "MINCO-MAIL-010";
    pub const INVALID_RETENTION: &str = "MINCO-MAIL-011";
    pub const INVALID_IDENTIFIER: &str = "MINCO-MAIL-012";
    pub const WORKER_TRIGGER_BOUND_ELSEWHERE: &str = "MINCO-MAIL-013";
    /// An existing same-ID queue does not match the exact expected wake
    /// shape (exact-head review 5072859042).
    pub const QUEUE_SHAPE_MISMATCH: &str = "MINCO-MAIL-014";
    /// The wake queue is FIFO; S3 direct event notifications cannot
    /// target FIFO SQS queues (exact-head review 5072859042).
    pub const QUEUE_IS_FIFO: &str = "MINCO-MAIL-015";
    /// An existing same-ID trigger does not match the exact expected
    /// mapping shape (exact-head review 5072859042).
    pub const TRIGGER_SHAPE_MISMATCH: &str = "MINCO-MAIL-016";
    /// Another trigger also consumes the wake queue — competing
    /// consumers on one queue steal messages, they do not fan out
    /// (exact-head review 5072859042).
    pub const QUEUE_SECOND_CONSUMER: &str = "MINCO-MAIL-017";
    /// Binding ids collapse to the same `CloudFormation` logical id
    /// after `sam_logical_id` normalization (exact-head review
    /// 5072859042).
    pub const LOGICAL_ID_COLLISION: &str = "MINCO-MAIL-018";
    /// Two bindings route the same mailbox (exact-head review
    /// 5083559431 P1): SES evaluates every matching recipient rule, so
    /// duplicate recipients are an accidental mail fan-out.
    pub const DUPLICATE_MAILBOX_SCOPE: &str = "MINCO-MAIL-019";
    /// Two bindings own one physical bucket name (exact-head review
    /// 5083559431 P1): the provider cannot create two buckets with a
    /// single name.
    pub const DUPLICATE_BUCKET_NAME: &str = "MINCO-MAIL-020";
}

/// Wake queue defaults: SES notifications are small and single-object;
/// visibility must exceed the worker timeout, and four days of queue
/// retention covers operator recovery windows.
pub const WAKE_VISIBILITY_TIMEOUT_SECONDS: u32 = 300;
pub const WAKE_RETENTION_SECONDS: u32 = 345_600;
/// Wake notifications redeliver this many times before the dead-letter
/// queue takes over (review finding 5).
pub const WAKE_MAX_RECEIVE_COUNT: u32 = 5;
/// Raw MIME retention bounds (days).
pub const MIN_RETENTION_DAYS: u32 = 1;
pub const MAX_RETENTION_DAYS: u32 = 3_650;

/// Explicit inbound-mail topology input.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct InboundMailTopology {
    pub enabled: bool,
    #[serde(default)]
    pub bindings: Vec<InboundMailBinding>,
}

impl InboundMailTopology {
    #[must_use]
    pub fn binding(&self, id: &str) -> Option<&InboundMailBinding> {
        self.bindings.iter().find(|binding| binding.id == id)
    }
}

/// The wake queue's visibility timeout must cover the bound worker's
/// timeout six-fold plus the batching window — the same rule
/// `DeploymentPlan` validation enforces (exact-head review 5064401898).
fn expected_wake_visibility(
    functions: &[crate::FunctionPlan],
    binding: &InboundMailBinding,
) -> u32 {
    functions
        .iter()
        .find(|function| function.name == binding.worker_function_id)
        .map_or(WAKE_VISIBILITY_TIMEOUT_SECONDS, |function| {
            function
                .timeout_seconds
                .saturating_mul(6)
                .saturating_add(binding.batching_window_seconds)
        })
        .max(WAKE_VISIBILITY_TIMEOUT_SECONDS)
}

/// The exact wake-queue shape one binding owns (exact-head review
/// 5072859042): apply creates it when absent and validate compares any
/// existing same-ID queue against it field by field — the sidecar never
/// silently adopts a same-ID resource with a different shape.
fn expected_wake_queue(
    functions: &[crate::FunctionPlan],
    binding: &InboundMailBinding,
) -> QueuePlan {
    QueuePlan {
        id: binding.queue_id.clone(),
        fifo: false,
        visibility_timeout_seconds: expected_wake_visibility(functions, binding),
        retention_seconds: WAKE_RETENTION_SECONDS,
        dead_letter_queue_id: Some(format!("{}-dlq", binding.queue_id)),
        max_receive_count: Some(WAKE_MAX_RECEIVE_COUNT),
    }
}

/// The exact wake-DLQ shape paired with the wake queue.
fn expected_wake_dlq(functions: &[crate::FunctionPlan], binding: &InboundMailBinding) -> QueuePlan {
    QueuePlan {
        id: format!("{}-dlq", binding.queue_id),
        fifo: false,
        visibility_timeout_seconds: expected_wake_visibility(functions, binding),
        retention_seconds: WAKE_RETENTION_SECONDS,
        dead_letter_queue_id: None,
        max_receive_count: None,
    }
}

fn wake_trigger_id(binding: &InboundMailBinding) -> String {
    format!("{}-mail", binding.id)
}

/// The exact wake-trigger shape one binding owns (exact-head review
/// 5072859042).
fn expected_wake_trigger(binding: &InboundMailBinding) -> TriggerPlan {
    TriggerPlan::Sqs {
        id: wake_trigger_id(binding),
        function_id: binding.worker_function_id.clone(),
        queue_id: binding.queue_id.clone(),
        batch_size: binding.batch_size,
        batching_window_seconds: binding.batching_window_seconds,
        report_batch_item_failures: true,
        maximum_concurrency: binding.maximum_concurrency,
    }
}

/// Synthesize wake queues and worker triggers into a copy of the plan.
///
/// The topology stays an explicit sidecar (exact-head review
/// 5060065907): it is never stored on `DeploymentPlan`, whose public
/// field set is a published compatibility boundary, so `apply` only
/// projects into the EXISTING queues/triggers/function collections,
/// mirroring the durable-work sidecar. Base plans without inbound mail
/// stay unchanged.
///
/// Resource ownership follows the exact-shape contract (exact-head
/// review 5072859042): an absent expected resource is created; an
/// existing resource is reused ONLY when it is semantically identical
/// (apply never overwrites an existing same-ID resource, so a mismatch
/// survives for `validate_inbound_mail` to reject — it is never
/// silently adopted).
#[must_use]
pub fn apply_inbound_mail(plan: &DeploymentPlan, topology: &InboundMailTopology) -> DeploymentPlan {
    let mut next = plan.clone();
    if !topology.enabled {
        return next;
    }
    for binding in &topology.bindings {
        // Every wake queue carries a dead-letter queue (review finding 5):
        // exhausted notifications must be inspectable, not silently lost.
        let dlq = expected_wake_dlq(&next.functions, binding);
        if !next.queues.iter().any(|queue| queue.id == dlq.id) {
            next.queues.push(dlq);
        }
        let wake = expected_wake_queue(&next.functions, binding);
        if !next.queues.iter().any(|queue| queue.id == wake.id) {
            next.queues.push(wake);
        }
        let trigger = expected_wake_trigger(binding);
        let trigger_id = wake_trigger_id(binding);
        if !next.triggers.iter().any(|trigger| match trigger {
            TriggerPlan::Sqs { id, .. } => id == &trigger_id,
            _ => false,
        }) {
            next.triggers.push(trigger);
        }
    }
    // Synthesized queues and triggers change the derived local-service
    // list and IAM intents; recompute both exactly as plan construction
    // does so the ordinary validators accept the result (exact-head
    // review 5064401898 — shared with the durable-work sidecar).
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

/// Validate the inbound-mail sidecar against the applied plan.
#[must_use]
pub fn validate_inbound_mail(
    plan: &DeploymentPlan,
    topology: &InboundMailTopology,
) -> Vec<PlanDiagnostic> {
    let mut diagnostics = Vec::new();
    if !topology.enabled {
        if !topology.bindings.is_empty() {
            diagnostics.push(diagnostic(
                inbound_mail_codes::DISABLED_ZERO_BINDINGS,
                "a disabled inbound-mail topology must declare no bindings".into(),
            ));
        }
        return diagnostics;
    }
    let mut binding_ids = BTreeSet::new();
    let mut queue_ids = BTreeSet::new();
    let mut mailbox_scopes = BTreeSet::new();
    let mut bucket_names = BTreeSet::new();
    for binding in &topology.bindings {
        if !valid_identifier(&binding.id) {
            diagnostics.push(diagnostic(
                inbound_mail_codes::INVALID_IDENTIFIER,
                format!("binding id must be [a-z0-9-]: {}", binding.id),
            ));
        }
        if !binding_ids.insert(binding.id.clone()) {
            diagnostics.push(diagnostic(
                inbound_mail_codes::DUPLICATE_BINDING_ID,
                format!("duplicate inbound-mail binding id: {}", binding.id),
            ));
        }
        // Physical ingress ownership (exact-head review 5083559431
        // P1): one mailbox routes to exactly one binding — SES evaluates
        // every matching recipient rule, so duplicate recipients would
        // silently fan one mail into multiple buckets/wakes/projects —
        // and one physical bucket belongs to exactly one binding
        // (CloudFormation cannot create two buckets with one name).
        // Shared-mailbox fan-out needs an explicit future model, never
        // an accidental second SES rule.
        if !mailbox_scopes.insert(binding.mailbox_scope.trim().to_ascii_lowercase()) {
            diagnostics.push(diagnostic(
                inbound_mail_codes::DUPLICATE_MAILBOX_SCOPE,
                format!(
                    "mailbox scope is already routed by another binding: {}",
                    binding.mailbox_scope
                ),
            ));
        }
        if !bucket_names.insert(binding.bucket_name.trim().to_ascii_lowercase()) {
            diagnostics.push(diagnostic(
                inbound_mail_codes::DUPLICATE_BUCKET_NAME,
                format!(
                    "physical raw-mail bucket is already owned by another binding: {}",
                    binding.bucket_name
                ),
            ));
        }
        if !queue_ids.insert(binding.queue_id.clone()) {
            diagnostics.push(diagnostic(
                inbound_mail_codes::QUEUE_SHARED,
                format!(
                    "each inbound-mail binding needs its own wake queue: {}",
                    binding.queue_id
                ),
            ));
        }
        if binding.mailbox_scope.trim().is_empty()
            || binding.mailbox_scope.chars().count() > 320
            || !binding.mailbox_scope.contains('@')
        {
            diagnostics.push(diagnostic(
                inbound_mail_codes::INVALID_MAILBOX_SCOPE,
                format!("mailbox scope must be a bounded address: {}", binding.id),
            ));
        }
        if !valid_bucket_name(&binding.bucket_name) {
            diagnostics.push(diagnostic(
                inbound_mail_codes::INVALID_BUCKET_NAME,
                format!("bucket name is not S3-valid: {}", binding.bucket_name),
            ));
        }
        if binding.key_prefix.is_empty()
            || binding.key_prefix.len() > 64
            || !binding.key_prefix.ends_with('/')
        {
            diagnostics.push(diagnostic(
                inbound_mail_codes::INVALID_KEY_PREFIX,
                format!(
                    "key prefix must be bounded and end with '/': {}",
                    binding.id
                ),
            ));
        }
        if !(MIN_RETENTION_DAYS..=MAX_RETENTION_DAYS).contains(&binding.retention_days) {
            diagnostics.push(diagnostic(
                inbound_mail_codes::INVALID_RETENTION,
                format!(
                    "retention must be {MIN_RETENTION_DAYS}..={MAX_RETENTION_DAYS} days: {}",
                    binding.id
                ),
            ));
        }
        let Some(function) = plan
            .functions
            .iter()
            .find(|function| function.name == binding.worker_function_id)
        else {
            diagnostics.push(diagnostic(
                inbound_mail_codes::UNKNOWN_WORKER_FUNCTION,
                format!(
                    "inbound-mail worker function does not exist: {}",
                    binding.worker_function_id
                ),
            ));
            continue;
        };
        if !matches!(function.role, crate::FunctionRole::Worker) {
            diagnostics.push(diagnostic(
                inbound_mail_codes::WORKER_NOT_WORKER_ROLE,
                format!(
                    "inbound-mail consumers must have the worker role: {}",
                    binding.worker_function_id
                ),
            ));
        }
        if !plan.queues.iter().any(|queue| queue.id == binding.queue_id) {
            diagnostics.push(diagnostic(
                inbound_mail_codes::QUEUE_MISSING,
                format!("wake queue was not synthesized: {}", binding.queue_id),
            ));
        } else if let Some(existing) = plan
            .queues
            .iter()
            .find(|queue| queue.id == binding.queue_id)
        {
            // Exact-shape ownership (exact-head review 5072859042): a
            // same-ID queue is this binding's wake queue only when it
            // matches the expected shape field by field — a base-plan
            // or durable-work queue that merely shares the ID is a
            // collision, not something to adopt.
            let expected = expected_wake_queue(&plan.functions, binding);
            if existing.fifo {
                diagnostics.push(diagnostic(
                    inbound_mail_codes::QUEUE_IS_FIFO,
                    format!(
                        "wake queue is FIFO; S3 direct event notifications cannot target \
                         FIFO SQS queues: {}",
                        binding.queue_id
                    ),
                ));
            }
            if existing.dead_letter_queue_id != expected.dead_letter_queue_id
                || existing.visibility_timeout_seconds != expected.visibility_timeout_seconds
                || existing.retention_seconds != expected.retention_seconds
                || existing.max_receive_count != expected.max_receive_count
            {
                diagnostics.push(diagnostic(
                    inbound_mail_codes::QUEUE_SHAPE_MISMATCH,
                    format!(
                        "existing queue does not match the exact wake shape \
                         (fifo {}, visibility {}, retention {}, dlq {:?}, max_receive {:?}); \
                         expected (visibility {}, retention {}, dlq {:?}, max_receive {:?}): {}",
                        existing.fifo,
                        existing.visibility_timeout_seconds,
                        existing.retention_seconds,
                        existing.dead_letter_queue_id,
                        existing.max_receive_count,
                        expected.visibility_timeout_seconds,
                        expected.retention_seconds,
                        expected.dead_letter_queue_id,
                        expected.max_receive_count,
                        binding.queue_id
                    ),
                ));
            }
        }
        // The paired DLQ must also match its expected shape when present
        // under the same ID.
        let expected_dlq = expected_wake_dlq(&plan.functions, binding);
        if let Some(existing) = plan.queues.iter().find(|queue| queue.id == expected_dlq.id)
            && (existing.fifo
                || existing.dead_letter_queue_id != expected_dlq.dead_letter_queue_id
                || existing.visibility_timeout_seconds != expected_dlq.visibility_timeout_seconds
                || existing.retention_seconds != expected_dlq.retention_seconds
                || existing.max_receive_count != expected_dlq.max_receive_count)
        {
            diagnostics.push(diagnostic(
                inbound_mail_codes::QUEUE_SHAPE_MISMATCH,
                format!(
                    "existing dead-letter queue does not match the exact wake shape: {}",
                    expected_dlq.id
                ),
            ));
        }
        let trigger_id = wake_trigger_id(binding);
        match plan
            .triggers
            .iter()
            .find(|trigger| matches!(trigger, TriggerPlan::Sqs { id, .. } if id == &trigger_id))
        {
            None => {
                diagnostics.push(diagnostic(
                    inbound_mail_codes::TRIGGER_MISSING,
                    format!("wake trigger was not synthesized: {trigger_id}"),
                ));
            }
            Some(TriggerPlan::Sqs {
                id: _,
                function_id,
                queue_id,
                batch_size,
                batching_window_seconds,
                report_batch_item_failures,
                maximum_concurrency,
            }) => {
                // Exact-shape ownership (exact-head review 5072859042):
                // the same-ID trigger must be THIS binding's mapping —
                // function, queue, batching and failure reporting all
                // compared — or it is a collision.
                if *function_id != binding.worker_function_id
                    || *queue_id != binding.queue_id
                    || *batch_size != binding.batch_size
                    || *batching_window_seconds != binding.batching_window_seconds
                    || !*report_batch_item_failures
                    || *maximum_concurrency != binding.maximum_concurrency
                {
                    diagnostics.push(diagnostic(
                        inbound_mail_codes::TRIGGER_SHAPE_MISMATCH,
                        format!(
                            "existing trigger does not match the exact wake mapping \
                             (function {function_id}, queue {queue_id}, batch {batch_size}, \
                             window {batching_window_seconds}, partial-batch {report_batch_item_failures}, \
                             concurrency {maximum_concurrency}): {trigger_id}"
                        ),
                    ));
                }
            }
            Some(_) => unreachable!("the find matched an Sqs variant"),
        }
        // Competing consumers steal messages, they do not fan out
        // (exact-head review 5072859042): a second trigger polling the
        // wake queue means mail wakes can land in the wrong handler.
        let second_consumer = plan
            .triggers
            .iter()
            .filter(|trigger| {
                matches!(trigger, TriggerPlan::Sqs { id, queue_id, .. }
                    if queue_id == &binding.queue_id && id != &trigger_id)
            })
            .count();
        if second_consumer > 0 {
            diagnostics.push(diagnostic(
                inbound_mail_codes::QUEUE_SECOND_CONSUMER,
                format!(
                    "another trigger also consumes the wake queue; competing consumers \
                     steal mail wakes instead of fanning out: {}",
                    binding.queue_id
                ),
            ));
        }
        // A worker function bound to another trigger source would mix wake
        // semantics; one worker consumes one wake discipline.
        let bound_elsewhere = plan.triggers.iter().any(|trigger| match trigger {
            TriggerPlan::Sqs {
                id, function_id, ..
            } => function_id == &binding.worker_function_id && id != &trigger_id,
            _ => false,
        });
        if bound_elsewhere {
            diagnostics.push(diagnostic(
                inbound_mail_codes::WORKER_TRIGGER_BOUND_ELSEWHERE,
                format!(
                    "inbound-mail worker is already bound to another SQS trigger: {}",
                    binding.worker_function_id
                ),
            ));
        }
    }
    // Provider logical-ID collisions (exact-head review 5072859042):
    // distinct binding ids that normalize to the same CloudFormation
    // logical id would render two provider chains onto one resource.
    let mut logical_ids = BTreeSet::new();
    for binding in &topology.bindings {
        let logical = crate::sam_logical_id(&binding.id);
        if !logical_ids.insert(logical.clone()) {
            diagnostics.push(diagnostic(
                inbound_mail_codes::LOGICAL_ID_COLLISION,
                format!("binding ids collapse to the same CloudFormation logical id: {logical}"),
            ));
        }
    }
    diagnostics
}

fn valid_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn valid_bucket_name(value: &str) -> bool {
    (3..=63).contains(&value.len())
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-' || byte == b'.'
        })
        && !value.starts_with('.')
        && !value.ends_with('.')
}

/// Builds the stable SES receipt-rule-set name (exact-head review
/// 5072859042): `{application}-{environment}-inbound-mail-{digest}`.
///
/// The digest covers the ORDER-INDEPENDENT binding set (sorted
/// mailbox/queue/worker/bucket identities), so reordering bindings
/// never changes the provider deployment identity while any actual
/// topology change does. The whole name is bounded to SES's 64-character
/// `RuleSetName` limit; activation of the rule set remains an explicit
/// operator step.
fn receipt_rule_set_name(plan: &DeploymentPlan, topology: &InboundMailTopology) -> String {
    use sha2::{Digest as _, Sha256};
    use std::fmt::Write as _;

    // Canonical length-framed digest input (exact-head review
    // 5083559431 P0-4): the FULL untruncated deployment identity —
    // application, environment, region and the sorted binding set — so
    // visible-prefix truncation can never make two deployments share a
    // rule-set name.
    let mut bindings = topology
        .bindings
        .iter()
        .map(|binding| {
            format!(
                "{}|{}|{}|{}|{}",
                binding.id,
                binding.mailbox_scope,
                binding.queue_id,
                binding.worker_function_id,
                binding.bucket_name
            )
        })
        .collect::<Vec<_>>();
    bindings.sort();
    let mut framed = String::new();
    for part in [
        plan.application.as_str(),
        plan.environment.as_str(),
        plan.region.as_str(),
        bindings.join("\n").as_str(),
    ] {
        let _ = write!(framed, "{:016x}", part.len());
        framed.push_str(part);
    }
    let digest = Sha256::digest(framed.as_bytes());
    let short = hex::encode(&digest[..6]);

    let sanitize = |value: &str| {
        let cleaned: String = value
            .chars()
            .map(|character| {
                if character.is_ascii_alphanumeric() {
                    character.to_ascii_lowercase()
                } else {
                    '-'
                }
            })
            .collect();
        cleaned.trim_matches('-').to_owned()
    };
    let application = sanitize(&plan.application);
    let environment = sanitize(&plan.environment);
    // Budget counts EVERY separator (exact-head review 5083559431
    // P0-4): the app/env '-' plus the fixed "-inbound-mail-{digest}"
    // suffix — the round-8 budget omitted the separator and could emit
    // a 65-character name.
    let suffix = format!("-inbound-mail-{short}");
    let prefix_budget = 64 - suffix.len() - 1;
    let application_budget = prefix_budget / 2;
    let environment_budget = prefix_budget - application_budget;
    // An empty sanitized part (punctuation-only input) falls back to a
    // stable placeholder so the name never starts with a separator.
    let truncate = |value: String, budget: usize| {
        if value.is_empty() {
            "app".to_owned()
        } else {
            value[..value.len().min(budget)].to_owned()
        }
    };
    let application = truncate(application, application_budget);
    let environment = truncate(environment, environment_budget);
    format!("{application}-{environment}{suffix}")
}

/// Cost assumptions one binding adds, stated explicitly instead of priced.
///
/// Per accepted mail the chain performs one S3 PUT, one S3 notification
/// delivery, one SQS send and (per wake) one S3 GET plus queue receives.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct InboundMailCostAssumptions {
    pub binding_id: String,
    pub s3_puts_per_mail: u64,
    pub s3_gets_per_wake: u64,
    pub sqs_sends_per_mail: u64,
    pub sqs_receives_per_wake_attempt: u64,
    pub raw_storage_gb_per_10k_mails: f64,
    pub retention_days: u32,
}

#[must_use]
pub fn estimate_inbound_mail_cost(
    topology: &InboundMailTopology,
) -> Vec<InboundMailCostAssumptions> {
    if !topology.enabled {
        return Vec::new();
    }
    topology
        .bindings
        .iter()
        .map(|binding| InboundMailCostAssumptions {
            binding_id: binding.id.clone(),
            s3_puts_per_mail: 1,
            s3_gets_per_wake: 1,
            sqs_sends_per_mail: 1,
            sqs_receives_per_wake_attempt: 1,
            // A typical raw MIME occupies ~100 KiB including envelope
            // overhead; stated so reviewers can correct it per workload.
            raw_storage_gb_per_10k_mails: 1.0,
            retention_days: binding.retention_days,
        })
        .collect()
}

fn yaml_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

/// Render SAM for a plan with inbound mail applied.
///
/// Queues, triggers and worker event-source mappings render through the
/// ordinary template (the sidecar synthesized them); this pass appends the
/// provider-side chain per binding: the raw MIME bucket with lifecycle
/// retention and an SES-write-only bucket policy, the bucket-to-queue
/// notification with a prefix filter, the S3-to-SQS queue policy, the SES
/// receipt rule set and the receiving rule itself (content scanning
/// disabled — the raw MIME is authoritative, ADR-0055).
pub fn render_sam_with_inbound_mail(
    plan: &DeploymentPlan,
    topology: &InboundMailTopology,
    code_uris: &std::collections::BTreeMap<String, String>,
) -> Result<String, crate::PlanError> {
    // Fail closed BEFORE any binding reaches the base renderer
    // (exact-head review 5064401898): a disabled topology carrying
    // bindings is internally inconsistent — the worker IAM
    // environment would reference raw-mail buckets whose provider
    // resources are never rendered. Rendering it is refused rather
    // than half-produced.
    if !topology.enabled && !topology.bindings.is_empty() {
        return Err(crate::PlanError::UnsupportedDeployment(
            "inbound mail topology is disabled but carries bindings".into(),
        ));
    }
    if !topology.enabled || topology.bindings.is_empty() {
        return crate::sam::render_sam_with_code_uris(plan, code_uris);
    }
    // Exact-shape ownership (exact-head review 5072859042): the
    // renderer refuses an applied plan whose same-ID resources do not
    // match the binding's exact wake shapes, carry competing consumers
    // or collapse logical ids — rendering would silently adopt a
    // collision the validator already flagged.
    let diagnostics = validate_inbound_mail(plan, topology);
    if let Some(first) = diagnostics.first() {
        return Err(crate::PlanError::UnsupportedDeployment(format!(
            "inbound mail sidecar validation failed: {} — {}",
            first.code, first.message
        )));
    }
    // The base render receives the sidecar's bindings explicitly so the
    // worker IAM environment scopes the mail buckets (the plan itself
    // no longer carries the topology — exact-head review 5060065907).
    let mut template = crate::sam::render_sam_template(plan, code_uris, &topology.bindings)?;
    let mut resources = String::new();
    // One shared, explicitly named receipt rule set (review finding 5);
    // activation semantics belong to the operator applying the change
    // set. The name is a stable deployment identity (exact-head review
    // 5072859042): derived from the application, the environment and a
    // bounded digest of the ORDER-INDEPENDENT binding set — never from
    // whichever binding happens to be first — so reordering or removing
    // one binding does not silently replace the provider rule set, and
    // two applications never collide on the same name.
    let shared_rule_set_name = receipt_rule_set_name(plan, topology);
    write!(
        resources,
        "  InboundMailReceiptRuleSet:\n    Type: AWS::SES::ReceiptRuleSet\n    Properties:\n      RuleSetName: {}\n",
        yaml_quote(&shared_rule_set_name),
    )
    .expect("write to String");
    for binding in &topology.bindings {
        let logical = sam_logical_id(&binding.id);
        let bucket_logical = format!("{logical}RawMailBucket");
        let queue_logical = format!("{}Queue", sam_logical_id(&binding.queue_id));
        // The full mailbox address scopes the SES recipient (review
        // finding 5): a bare local part would capture every domain's
        // mail for that local part.
        let mailbox_recipient = binding.mailbox_scope.trim().to_ascii_lowercase();
        let queue_policy_logical = format!("{logical}MailQueuePolicy");
        // Clean-create dependency graph (exact-head review 5083559431
        // P0-1/P0-2): S3 validates the bucket notification's destination
        // queue AND its permission at notification-apply time, so the
        // queue policy must exist BEFORE the bucket. A policy that
        // !GetAtt's the bucket ARN would invert that order and could
        // only be resolved circularly — instead the SourceArn is built
        // from the EXPLICIT configured bucket name (`BucketName` is a
        // concrete value, never a !Ref), so the policy depends only on
        // the queue, and the bucket DependsOn the policy:
        //   Queue -> QueuePolicy -> Bucket(+Notification) -> BucketPolicy
        //   ReceiptRuleSet -> ReceiptRule (depends on BucketPolicy and
        //   the queue policy; !Ref on the rule set for a real
        //   dependency — identical literal strings create none).
        write!(
            resources,
            "  {bucket_logical}:\n    Type: AWS::S3::Bucket\n    DependsOn: [{queue_policy_logical}]\n    Properties:\n      BucketName: {bucket_name}\n      PublicAccessBlockConfiguration:\n        BlockPublicAcls: true\n        BlockPublicPolicy: true\n        IgnorePublicAcls: true\n        RestrictPublicBuckets: true\n      NotificationConfiguration:\n        QueueConfigurations:\n          - Event: s3:ObjectCreated:*\n            Queue: !GetAtt {queue_logical}.Arn\n            Filter:\n              S3Key:\n                Rules:\n                  - Name: prefix\n                    Value: {key_prefix_a}\n      LifecycleConfiguration:\n        Rules:\n          - Id: expire-raw-mail\n            Status: Enabled\n            Prefix: {key_prefix_b}\n            ExpirationInDays: {retention_days}\n",
            bucket_logical = bucket_logical,
            queue_policy_logical = queue_policy_logical,
            bucket_name = yaml_quote(&binding.bucket_name),
            key_prefix_a = yaml_quote(&binding.key_prefix),
            key_prefix_b = yaml_quote(&binding.key_prefix),
            retention_days = binding.retention_days,
        )
        .expect("write to String");
        // The queue policy references only the queue and the explicit
        // bucket name — never the bucket resource — so it can be
        // created before the bucket exists (exact-head review
        // 5083559431 P0-1).
        write!(
            resources,
            "  {queue_policy_logical}:\n    Type: AWS::SQS::QueuePolicy\n    Properties:\n      Queues:\n        - !Ref {queue_logical}\n      PolicyDocument:\n        Version: '2012-10-17'\n        Statement:\n          - Sid: AllowBucketWakeSend\n            Effect: Allow\n            Principal:\n              Service: s3.amazonaws.com\n            Action: sqs:SendMessage\n            Resource: !GetAtt {queue_logical}.Arn\n            Condition:\n              ArnLike:\n                aws:SourceArn: !Sub 'arn:${{AWS::Partition}}:s3:::{bucket_name_sub}'\n              StringEquals:\n                aws:SourceAccount: !Ref AWS::AccountId\n",
            queue_policy_logical = queue_policy_logical,
            queue_logical = queue_logical,
            bucket_name_sub = binding.bucket_name.replace('\'', "''"),
        )
        .expect("write to String");
        // Exact-head review R9/R14: the SES write grant is scoped to the
        // configured key prefix and the exact receipt-rule ARN. The whole
        // !Sub substitution is built first and quoted exactly once —
        // nesting yaml_quote inside an already-quoted scalar produced
        // invalid YAML.
        let rule_name = format!("{}-inbound-mail", binding.id);
        let resource_sub = format!("${{{bucket_logical}.Arn}}/{}*", binding.key_prefix);
        let source_arn_sub = format!(
            "arn:aws:ses:${{AWS::Region}}:${{AWS::AccountId}}:receipt-rule-set/{shared_rule_set_name}:receipt-rule/{rule_name}"
        );
        write!(
            resources,
            "  {bucket_logical}Policy:\n    Type: AWS::S3::BucketPolicy\n    Properties:\n      Bucket: !Ref {bucket_logical}\n      PolicyDocument:\n        Version: '2012-10-17'\n        Statement:\n          - Sid: AllowSeSInboundWrite\n            Effect: Allow\n            Principal:\n              Service: ses.amazonaws.com\n            Action: s3:PutObject\n            Resource: !Sub {resource}\n            Condition:\n              StringEquals:\n                aws:SourceAccount: !Sub '${{AWS::AccountId}}'\n              ArnLike:\n                aws:SourceArn: !Sub {source_arn}\n",
            bucket_logical = bucket_logical,
            resource = yaml_quote(&resource_sub),
            source_arn = yaml_quote(&source_arn_sub),
        )
        .expect("write to String");
        // One shared receipt rule set (review finding 5): every binding
        // adds a rule to the single activated set instead of competing
        // rule sets; content scanning stays enabled. Ordering
        // (exact-head review 5083559431 P0-2): the rule set is
        // referenced with !Ref (a real CloudFormation dependency — the
        // Ref returns the rule-set name; identical literal strings
        // create none) and the rule waits for the SES-write bucket
        // policy and the wake queue policy, so the bucket, its write
        // grant and the rule set all exist before the enabled rule.
        // Rule-set activation remains an explicit operator step.
        let rule_name = format!("{}-inbound-mail", binding.id);
        let bucket_policy_logical = format!("{bucket_logical}Policy");
        write!(
            resources,
            "  {logical}ReceiptRule:\n    Type: AWS::SES::ReceiptRule\n    DependsOn: [{bucket_policy_logical}, {queue_policy_logical}]\n    Properties:\n      RuleSetName: !Ref InboundMailReceiptRuleSet\n      Rule:\n        Name: {rule_name_value}\n        Enabled: true\n        ScanEnabled: true\n        TlsPolicy: Require\n        Recipients:\n          - {recipient}\n        Actions:\n          - S3Action:\n              BucketName: !Ref {bucket_logical}\n              ObjectKeyPrefix: {key_prefix_value}\n",
            bucket_policy_logical = bucket_policy_logical,
            queue_policy_logical = queue_policy_logical,
            rule_name_value = yaml_quote(&rule_name),
            recipient = yaml_quote(&mailbox_recipient),
            key_prefix_value = yaml_quote(&binding.key_prefix),
        )
        .expect("write to String");
    }
    // Resources must be inserted before the Outputs block of the template.
    let outputs_marker = "\nOutputs:\n";
    let Some(position) = template.find(outputs_marker) else {
        return Err(crate::PlanError::UnsupportedDeployment(
            "SAM template is missing the Outputs block for inbound-mail resources".into(),
        ));
    };
    template.insert_str(position + 1, &resources);
    Ok(template)
}
