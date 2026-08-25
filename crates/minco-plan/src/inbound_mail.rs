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
}

/// Wake queue defaults: SES notifications are small and single-object;
/// visibility must exceed the worker timeout, and four days of queue
/// retention covers operator recovery windows.
pub const WAKE_VISIBILITY_TIMEOUT_SECONDS: u32 = 300;
pub const WAKE_RETENTION_SECONDS: u32 = 345_600;
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

/// Synthesize wake queues, worker triggers and the binding list into a
/// copy of the plan. Base plans without inbound mail stay unchanged.
#[must_use]
pub fn apply_inbound_mail(plan: &DeploymentPlan, topology: &InboundMailTopology) -> DeploymentPlan {
    let mut next = plan.clone();
    if !topology.enabled {
        next.inbound_mail = Vec::new();
        return next;
    }
    for binding in &topology.bindings {
        if !next.queues.iter().any(|queue| queue.id == binding.queue_id) {
            next.queues.push(QueuePlan {
                id: binding.queue_id.clone(),
                fifo: false,
                visibility_timeout_seconds: WAKE_VISIBILITY_TIMEOUT_SECONDS,
                retention_seconds: WAKE_RETENTION_SECONDS,
                dead_letter_queue_id: None,
                max_receive_count: None,
            });
        }
        let trigger_id = format!("{}-mail", binding.id);
        if !next.triggers.iter().any(|trigger| match trigger {
            TriggerPlan::Sqs { id, .. } => id == &trigger_id,
            _ => false,
        }) {
            next.triggers.push(TriggerPlan::Sqs {
                id: trigger_id,
                function_id: binding.worker_function_id.clone(),
                queue_id: binding.queue_id.clone(),
                batch_size: binding.batch_size,
                batching_window_seconds: binding.batching_window_seconds,
                report_batch_item_failures: true,
                maximum_concurrency: binding.maximum_concurrency,
            });
        }
    }
    next.inbound_mail.clone_from(&topology.bindings);
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
        }
        let trigger_id = format!("{}-mail", binding.id);
        if !plan.triggers.iter().any(|trigger| match trigger {
            TriggerPlan::Sqs { id, queue_id, .. } => {
                id == &trigger_id && queue_id == &binding.queue_id
            }
            _ => false,
        }) {
            diagnostics.push(diagnostic(
                inbound_mail_codes::TRIGGER_MISSING,
                format!("wake trigger was not synthesized: {trigger_id}"),
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
    let mut template = crate::sam::render_sam_with_code_uris(plan, code_uris)?;
    if !topology.enabled || topology.bindings.is_empty() {
        return Ok(template);
    }
    let mut resources = String::new();
    for binding in &topology.bindings {
        let logical = sam_logical_id(&binding.id);
        let bucket_logical = format!("{logical}RawMailBucket");
        let queue_logical = format!("{}Queue", sam_logical_id(&binding.queue_id));
        let mailbox_local = binding
            .mailbox_scope
            .split('@')
            .next()
            .unwrap_or_default()
            .to_owned();
        write!(
            resources,
            "  {bucket_logical}:\n    Type: AWS::S3::Bucket\n    Properties:\n      BucketName: {}\n      PublicAccessBlockConfiguration:\n        BlockPublicAcls: true\n        BlockPublicPolicy: true\n        IgnorePublicAcls: true\n        RestrictPublicBuckets: true\n      NotificationConfiguration:\n        QueueConfigurations:\n          - Event: s3:ObjectCreated:*\n            Queue: !GetAtt {queue_logical}.Arn\n            Filter:\n              S3Key:\n                Rules:\n                  - Name: prefix\n                    Value: {}\n      LifecycleConfiguration:\n        Rules:\n          - Id: expire-raw-mail\n            Status: Enabled\n            Prefix: {}\n            ExpirationInDays: {}\n",
            yaml_quote(&binding.bucket_name),
            yaml_quote(&binding.key_prefix),
            yaml_quote(&binding.key_prefix),
            binding.retention_days,
        )
        .expect("write to String");
        write!(
            resources,
            "  {bucket_logical}Policy:\n    Type: AWS::S3::BucketPolicy\n    Properties:\n      Bucket: !Ref {bucket_logical}\n      PolicyDocument:\n        Version: '2012-10-17'\n        Statement:\n          - Sid: AllowSeSInboundWrite\n            Effect: Allow\n            Principal:\n              Service: ses.amazonaws.com\n            Action: s3:PutObject\n            Resource:\n              - !GetAtt {bucket_logical}.Arn\n              - !Sub '${{{bucket_logical}.Arn}}/*'\n",
        )
        .expect("write to String");
        write!(
            resources,
            "  {logical}MailQueuePolicy:\n    Type: AWS::SQS::QueuePolicy\n    Properties:\n      Queues:\n        - !Ref {queue_logical}\n      PolicyDocument:\n        Version: '2012-10-17'\n        Statement:\n          - Sid: AllowBucketWakeSend\n            Effect: Allow\n            Principal:\n              Service: s3.amazonaws.com\n            Action: sqs:SendMessage\n            Resource: !GetAtt {queue_logical}.Arn\n            Condition:\n              ArnLike:\n                aws:SourceArn: !GetAtt {bucket_logical}.Arn\n",
        )
        .expect("write to String");
        write!(
            resources,
            "  {logical}ReceiptRuleSet:\n    Type: AWS::SES::ReceiptRuleSet\n",
        )
        .expect("write to String");
        write!(
            resources,
            "  {logical}ReceiptRule:\n    Type: AWS::SES::ReceiptRule\n    Properties:\n      RuleSetName: !Ref {logical}ReceiptRuleSet\n      Rule:\n        Name: {}\n        Enabled: true\n        ScanEnabled: false\n        Recipients:\n          - {}\n        Actions:\n          - S3Action:\n              BucketName: !Ref {bucket_logical}\n              ObjectKeyPrefix: {}\n",
            yaml_quote(&format!("{}-inbound-mail", binding.id)),
            yaml_quote(&mailbox_local),
            yaml_quote(&binding.key_prefix),
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
