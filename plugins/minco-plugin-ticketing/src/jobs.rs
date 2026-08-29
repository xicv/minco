//! Optional Ticketing-to-Jobs bridge (ADR-0054, ADR-0055, ADR-0063).
//!
//! Two typed commands ship: `ticketing.deliver-public-notification` v1
//! (email delivery through the observable mail path with reconciliation
//! and ambiguity recovery, in-app through the notifications port) and
//! `ticketing.process-inbound-email` v1 (verified raw-object ingress).
//! The bridge owns no queue, lease, retry or scheduling machinery — all of
//! that is the released jobs plugin (ADR-0048).

#[cfg(feature = "sqlite")]
use crate::TicketStoreError;
use crate::{
    OutboundDeliveryEvidence, OutboundEvidenceKind, SendIntent, SendIntentState, TicketId,
    TicketMessageId, TicketingService, TicketingStoreService,
};
#[cfg(feature = "sqlite")]
use async_trait::async_trait;
use chrono::{TimeDelta, Utc};
use minco_plugin_jobs::{
    Job, JobEnvelope, JobError, JobExecutionFailure, JobHandlerRegistry, JobOptions, RetryPolicy,
    pending_record,
};
use minco_plugin_notifications::{
    MailAddress, MailError, MailRetryAdvice, MailService, Notification, NotificationChannel,
    NotificationService,
};
#[cfg(test)]
use minco_plugin_object_storage::ObjectStore as _;
use minco_plugin_object_storage::ObjectStoreService;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use uuid::Uuid;

/// Deferred command: notify the requester about one public message.
/// The payload carries bounded identifiers only — never message bodies,
/// addresses or credentials (ADR-0054).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DeliverPublicNotification {
    pub project_id: String,
    pub ticket_id: TicketId,
    pub message_id: TicketMessageId,
}

impl Job for DeliverPublicNotification {
    const NAME: &'static str = "ticketing.deliver-public-notification";
    const VERSION: u16 = 1;
}

/// Deferred command: process one inbound raw email for a known ticket
/// (ADR-0055). The raw MIME stays authoritative in object storage; this
/// payload carries bounded identities and digests only.
///
/// Ingress is revision-free — the store reloads the authoritative ticket
/// inside its transaction, so retries always converge (review finding 7).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ProcessInboundEmail {
    pub project_id: String,
    pub provider: String,
    pub mailbox_scope: String,
    pub external_id: String,
    pub content_sha256: String,
    pub raw_object_key: String,
    /// `Some` for a threaded reply to a resolved ticket; `None` for a
    /// verified first-contact email (review finding 6).
    pub ticket_id: Option<TicketId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub internet_message_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub in_reply_to: Option<String>,
    #[serde(default)]
    pub references: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
}

impl Job for ProcessInboundEmail {
    const NAME: &'static str = "ticketing.process-inbound-email";
    const VERSION: u16 = 1;
}

/// One RFC 8601 method-result: a mechanism verdict WITH its own
/// properties (exact-head review R23/P0-4).
///
/// A DKIM pass and its `header.i` identity belong to the same method
/// result — cross-result assembly lets a victim-domain identity
/// validate an attacker-domain pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticationMethodResult {
    pub method: String,
    pub result: String,
    /// This method result's `ptype.prop=value` properties.
    pub properties: Vec<(String, String)>,
}

impl AuthenticationMethodResult {
    fn identity_domain(&self, keys: &[&str]) -> Option<String> {
        self.properties
            .iter()
            .find(|(key, _)| keys.contains(&key.as_str()))
            .map(|(_, value)| property_domain(value).to_owned())
    }
}

/// One structurally parsed Authentication-Results header (RFC 8601).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedAuthResults {
    pub authserv_id: String,
    pub methods: Vec<AuthenticationMethodResult>,
}

/// Strips RFC 8601 CFWS comments (balanced parentheses) from one token,
/// keeping the grammar bounded: at most 8 nesting levels and 998 bytes.
fn strip_comments(token: &str) -> String {
    if !token.contains('(') {
        return token.trim().to_owned();
    }
    let mut output = String::with_capacity(token.len());
    let mut depth: u8 = 0;
    for character in token.chars().take(998) {
        match character {
            '(' => {
                if depth < 8 {
                    depth += 1;
                }
            }
            ')' if depth > 0 => depth -= 1,
            _ if depth == 0 => output.push(character),
            _ => {}
        }
    }
    output.trim().to_owned()
}

const AUTH_MECHANISMS: [&str; 5] = ["spf", "dkim", "dmarc", "dkim-atps", "spf2"];

/// RFC 8601 structural parse preserving method-result association.
///
/// Comments are stripped first; each ';'-clause is tokenized on
/// whitespace; the FIRST k=v token whose key is a known mechanism
/// starts a method result, and all subsequent property tokens belong to
/// THAT result until the next mechanism token. SES property-only
/// clauses (`envelope-from=...`) attach to the preceding method result
/// when one exists in the same clause chain.
pub fn parse_authentication_results(value: &str) -> Option<ParsedAuthResults> {
    let value = value.trim();
    if value.is_empty() || value.len() > 2_048 {
        return None;
    }
    let stripped = strip_comments(value);
    let (authserv_id, rest) = stripped.split_once(';')?;
    let authserv_id = authserv_id.trim();
    if authserv_id.is_empty()
        || !authserv_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'.' || byte == b'-')
    {
        return None;
    }
    // The current method result persists ACROSS clauses: AWS SES emits
    // `spf=pass (...) client-ip=...; envelope-from="..."` — the
    // envelope-from property belongs to the preceding SPF result even
    // though it sits in its own clause (exact-head review R23).
    let mut methods: Vec<AuthenticationMethodResult> = Vec::new();
    let mut current: Option<AuthenticationMethodResult> = None;
    for part in rest.split(';').flat_map(str::split_whitespace) {
        let Some((key, value)) = part.split_once('=') else {
            continue;
        };
        let key = key.trim().to_ascii_lowercase();
        let value = value.trim().to_ascii_lowercase();
        if key.is_empty() || value.is_empty() {
            continue;
        }
        if AUTH_MECHANISMS.contains(&key.as_str()) {
            if let Some(method) = current.take() {
                methods.push(method);
            }
            current = Some(AuthenticationMethodResult {
                method: key,
                result: value,
                properties: Vec::new(),
            });
        } else if let Some(method) = current.as_mut() {
            method.properties.push((key, value));
        }
    }
    if let Some(method) = current.take() {
        methods.push(method);
    }
    Some(ParsedAuthResults {
        authserv_id: authserv_id.to_owned(),
        methods,
    })
}

/// Extracts the domain from a mailbox- or domain-shaped property value.
fn property_domain(value: &str) -> &str {
    let trimmed = if value.len() >= 2 && value.starts_with('"') && value.ends_with('"') {
        &value[1..value.len() - 1]
    } else {
        value
    };
    match trimmed.rsplit_once('@') {
        Some((_, domain)) if !domain.is_empty() => domain,
        _ => trimmed,
    }
}

/// The sender-trust decision for one inbound message (exact-head
/// reviews R6 and R15).
///
/// Every Authentication-Results header is parsed INDEPENDENTLY: a
/// malformed header from an untrusted authserv-id is ignored rather
/// than poisoning trusted provider evidence. Only the configured
/// authserv-id is trusted; two trusted headers are ambiguous; explicit
/// mechanism failures and SES spam/virus failures quarantine under
/// every policy; strict policies demand their aligned pass using the
/// real provider properties (SPF: `envelope-from`/`smtp.mailfrom`;
/// DKIM: `header.i`/`header.d`; DMARC: `header.from` per RFC 7489).
pub fn evaluate_inbound_trust(
    headers: &[String],
    spam_verdict: Option<&str>,
    virus_verdict: Option<&str>,
    from_domain: Option<&str>,
    expected_authserv_id: &str,
    policy: crate::InboundAuthPolicy,
) -> Result<(), &'static str> {
    // SES spam/virus verdicts are provider evidence: FAIL and the
    // indeterminate GRAY/PROCESSING_FAILED outcomes quarantine under
    // every policy (exact-head review R23).
    for verdict in [spam_verdict, virus_verdict].into_iter().flatten() {
        let normalised = verdict.trim().to_ascii_uppercase();
        if normalised == "FAIL" || normalised == "GRAY" || normalised == "PROCESSING_FAILED" {
            return Err("ticketing.inbound_sender_unverified");
        }
    }
    let mut trusted: Vec<ParsedAuthResults> = Vec::new();
    for value in headers {
        let Some(parsed) = parse_authentication_results(value) else {
            let claimed = strip_comments(value)
                .split(';')
                .next()
                .unwrap_or_default()
                .split_whitespace()
                .next()
                .unwrap_or_default()
                .to_ascii_lowercase();
            if claimed.eq_ignore_ascii_case(expected_authserv_id) {
                return Err("ticketing.inbound_sender_unverified");
            }
            continue;
        };
        if parsed
            .authserv_id
            .eq_ignore_ascii_case(expected_authserv_id)
        {
            trusted.push(parsed);
        }
    }
    if trusted.len() > 1 {
        return Err("ticketing.inbound_sender_unverified");
    }
    let quarantine_results = [
        "fail",
        "hardfail",
        "permerror",
        "temperror",
        "processing_failed",
        "gray",
    ];
    for results in &trusted {
        for method in &results.methods {
            if quarantine_results.contains(&method.result.as_str()) {
                return Err("ticketing.inbound_sender_unverified");
            }
        }
    }
    let required = match policy {
        crate::InboundAuthPolicy::LocalTrusted => None,
        crate::InboundAuthPolicy::RequireAlignedSpf => Some("spf"),
        crate::InboundAuthPolicy::RequireAlignedDkim => Some("dkim"),
        crate::InboundAuthPolicy::RequireDmarc => Some("dmarc"),
    };
    let Some(required) = required else {
        return Ok(());
    };
    let Some(results) = trusted.first() else {
        return Err("ticketing.inbound_sender_unverified");
    };
    // Alignment within ONE method result (exact-head review R23/P0-4):
    // the pass verdict and the identity property must come from the
    // same method result, never assembled across results.
    let identity_keys: &[&str] = match required {
        "spf" => &["envelope-from", "smtp.mailfrom", "mailfrom"],
        "dkim" => &["header.i", "header.d"],
        // DMARC's registry property is header.from (RFC 7489) — no
        // header.d fallback.
        _ => &["header.from"],
    };
    let from_domain = from_domain.map(str::trim).map(str::to_ascii_lowercase);
    let mut aligned_pass = false;
    for method in &results.methods {
        if method.method != required || method.result != "pass" {
            continue;
        }
        let Some(identity) = method.identity_domain(identity_keys) else {
            continue;
        };
        if identity.is_empty() {
            continue;
        }
        if let Some(from) = from_domain.as_deref()
            && !from.is_empty()
            && (identity == from || identity.ends_with(&format!(".{from}")))
        {
            aligned_pass = true;
        }
    }
    if aligned_pass {
        Ok(())
    } else {
        Err("ticketing.inbound_sender_unverified")
    }
}

/// Deferred command: run private development automation for one ticket
/// (ADR-0070).
///
/// The handler assembles a deterministic proposal from ticket context —
/// model output is a proposal, never authority — and stores it awaiting
/// human review. The payload carries bounded identifiers only.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RunDevelopmentAutomation {
    pub project_id: String,
    pub ticket_id: TicketId,
    pub requested_by: String,
    /// Freshness binding (exact-head review R8): the ticket revision and
    /// a context digest captured at submission. The handler refuses to
    /// store a proposal when the authoritative ticket moved past them.
    #[serde(default)]
    pub bound_revision: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bound_context_digest: Option<String>,
    /// Unique run identity: distinct legitimate requests by the same
    /// person are never deduplicated away.
    #[serde(default = "uuid::Uuid::new_v4")]
    pub run_id: Uuid,
}

impl Job for RunDevelopmentAutomation {
    const NAME: &'static str = "ticketing.run-development-automation";
    const VERSION: u16 = 1;
}

/// Envelope policy for the automation command (ADR-0070): dedupe by
/// ticket and requester, serialize per ticket, partition by project,
/// bounded retry, one-hour deadline.
pub const TICKETING_AUTOMATION_PROFILE: &str = "ticketing-development";

pub fn development_automation_envelope(
    payload: &RunDevelopmentAutomation,
    correlation_id: Uuid,
    now: chrono::DateTime<Utc>,
) -> Result<JobEnvelope, JobError> {
    // Exact-head review R8: a dedicated worker profile, and a dedupe
    // identity bound to the run (revision + context digest) so a later
    // legitimate request by the same person on a changed ticket is
    // never treated as a duplicate of an earlier one; the unique run id
    // rides the payload.
    let context = payload.bound_context_digest.as_deref().unwrap_or("none");
    let envelope = JobEnvelope::for_job(payload, TICKETING_AUTOMATION_PROFILE, correlation_id)?
        .with(
            JobOptions::default()
                .with_dedupe_key(format!(
                    "automation:{}:{}:{}:{}:{}",
                    payload.ticket_id,
                    payload.requested_by,
                    payload.bound_revision,
                    context,
                    payload.run_id
                ))
                .with_overlap_key(format!("ticket:{}", payload.ticket_id))
                .with_partition(payload.project_id.clone())
                .with_retry(RetryPolicy::exponential(5, 5, 900))
                .with_deadline(now + TimeDelta::seconds(3600))
                .with_causation(correlation_id),
        );
    Ok(envelope)
}

pub const TICKETING_MAIL_PROFILE: &str = "ticketing-mail";
pub const NOTIFICATION_DEADLINE_SECONDS: i64 = 3600;
pub const INBOUND_EMAIL_DEADLINE_SECONDS: i64 = 6 * 3600;

fn digest_of(parts: &[&str]) -> String {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update((part.len() as u64).to_le_bytes());
        hasher.update(part.as_bytes());
    }
    hex::encode(hasher.finalize())
}

/// Envelope policy for the inbound command (ADR-0055): dedupe by the
/// provider-scoped external identity, serialize per mailbox, partition by
/// project, bounded exponential retry, six-hour deadline.
pub fn inbound_email_envelope(
    payload: &ProcessInboundEmail,
    correlation_id: Uuid,
    now: chrono::DateTime<Utc>,
) -> Result<JobEnvelope, JobError> {
    let envelope = JobEnvelope::for_job(payload, TICKETING_MAIL_PROFILE, correlation_id)?.with(
        JobOptions::default()
            .with_dedupe_key(format!(
                "mail:{}",
                digest_of(&[
                    &payload.provider,
                    &payload.mailbox_scope,
                    &payload.external_id,
                ])
            ))
            .with_overlap_key(format!(
                "mailbox:{}",
                digest_of(&[&payload.provider, &payload.mailbox_scope])
            ))
            .with_partition(payload.project_id.clone())
            .with_retry(RetryPolicy::exponential(5, 10, 900))
            .with_deadline(now + TimeDelta::seconds(INBOUND_EMAIL_DEADLINE_SECONDS))
            .with_causation(correlation_id),
    );
    Ok(envelope)
}

/// Envelope policy for the notification command (ADR-0054).
///
/// Dedupe by ticket and message, serialize per ticket, partition by
/// project, bounded exponential retry, and a deadline so a stale
/// acknowledgement never sends.
pub fn public_notification_envelope(
    payload: &DeliverPublicNotification,
    correlation_id: Uuid,
    now: chrono::DateTime<Utc>,
) -> Result<JobEnvelope, JobError> {
    let envelope = JobEnvelope::for_job(payload, TICKETING_MAIL_PROFILE, correlation_id)?.with(
        JobOptions::default()
            .with_dedupe_key(format!(
                "notification:{}:{}",
                payload.ticket_id, payload.message_id
            ))
            .with_overlap_key(format!("ticket:{}", payload.ticket_id))
            .with_partition(payload.project_id.clone())
            .with_retry(RetryPolicy::exponential(5, 5, 900))
            .with_deadline(now + TimeDelta::seconds(NOTIFICATION_DEADLINE_SECONDS))
            .with_causation(correlation_id),
    );
    Ok(envelope)
}

/// Explicit composition-root dependencies for the ticketing handlers.
/// The worker identity holds only `ticketing.ingest` (ADR-0055): a job
/// worker never bypasses ticketing authorization.
pub struct TicketingJobsDeps {
    pub service: TicketingService,
    pub notifications: Arc<NotificationService>,
    /// Observable mail path for email-channel public replies (ADR-0063).
    /// When absent, email notifications fail closed as a permanent
    /// configuration defect instead of silently downgrading the channel.
    pub mail: Option<Arc<MailService>>,
    pub objects: Arc<ObjectStoreService>,
    pub worker: minco_plugin_identity::Identity,
}

impl std::fmt::Debug for TicketingJobsDeps {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Redacted: service state and worker permissions are not debug data.
        formatter
            .debug_struct("TicketingJobsDeps")
            .field("service", &"[REDACTED]")
            .field("notifications", &"[REDACTED]")
            .field("objects", &"[REDACTED]")
            .field("worker", &"[REDACTED]")
            .finish()
    }
}

/// Register the ticketing handlers on the composition's registry. Static
/// and explicit: the composition root calls this before building
/// `JobsServices`; no runtime scanning, no plugin retro-fit.
pub fn register_ticketing_jobs(
    registry: &JobHandlerRegistry,
    store: &TicketingStoreService,
    deps: TicketingJobsDeps,
) -> Result<(), JobError> {
    let notification_store = store.clone();
    let notification_sink = deps.notifications.clone();
    let notification_mail = deps.mail.clone();
    registry.register_typed::<DeliverPublicNotification, _, _>(move |command, _context| {
        let store = notification_store.clone();
        let notifications = notification_sink.clone();
        let mail = notification_mail.clone();
        async move {
            deliver_public_notification(&store, &notifications, mail.as_ref(), &command).await
        }
    })?;
    let automation_service = deps.service.clone();
    let automation_store = store.clone();
    registry.register_typed::<RunDevelopmentAutomation, _, _>(move |command, _context| {
        let service = automation_service.clone();
        let store = automation_store.clone();
        async move { run_development_automation(&service, &store, &command).await }
    })?;
    let inbound_service = deps.service.clone();
    let inbound_objects = deps.objects.clone();
    let inbound_worker = deps.worker;
    registry.register_typed::<ProcessInboundEmail, _, _>(move |command, context| {
        let service = inbound_service.clone();
        let objects = inbound_objects.clone();
        let worker = inbound_worker.clone();
        async move { process_inbound_email(&service, &objects, &worker, &command, context).await }
    })
}

async fn process_inbound_email(
    service: &TicketingService,
    objects: &ObjectStoreService,
    worker: &minco_plugin_identity::Identity,
    command: &ProcessInboundEmail,
    context: minco_plugin_jobs::JobContext,
) -> Result<(), JobExecutionFailure> {
    let key = minco_plugin_object_storage::ObjectKey::parse(command.raw_object_key.clone())
        .map_err(|_| JobExecutionFailure::permanent("ticketing.inbound_object_missing"))?;
    let stored = objects
        .0
        .get(&key)
        .await
        .map_err(|_| JobExecutionFailure::retryable("ticketing.inbound_store_unavailable"))?
        .ok_or_else(|| JobExecutionFailure::permanent("ticketing.inbound_object_missing"))?;
    let actual_digest = crate::external_content_sha256(&stored.bytes);
    if !actual_digest.eq_ignore_ascii_case(&command.content_sha256) {
        // Unverified content is never ingested (ADR-0055).
        return Err(JobExecutionFailure::permanent(
            "ticketing.inbound_digest_mismatch",
        ));
    }
    let message = mail_parser::MessageParser::default()
        .parse(&stored.bytes)
        .ok_or_else(|| JobExecutionFailure::permanent("ticketing.inbound_mime_invalid"))?;
    let body = message
        .body_text(0)
        .ok_or_else(|| JobExecutionFailure::permanent("ticketing.inbound_body_missing"))?
        // Ticket bodies are single-paragraph by domain contract; v1 email
        // ingestion flattens line breaks to spaces. The raw MIME stays
        // authoritative in object storage (ADR-0055).
        .replace(['\r', '\n', '\t'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    // Sender trust (review finding 6): the From address and any explicit
    // authentication failures decide participation; the raw MIME stays in
    // object storage as the quarantine record for refused mail.
    let sender = message
        .from()
        .and_then(|address| address.first())
        .and_then(|entry| entry.address())
        .map(str::to_owned);
    // Every Authentication-Results header participates; the configured
    // authserv-id decides which are trusted (exact-head review R6).
    let auth_headers: Vec<String> = message
        .headers_raw()
        .filter(|(name, _)| name.eq_ignore_ascii_case("Authentication-Results"))
        .map(|(_, value)| value.trim().to_owned())
        .collect();
    let header_text = |name: &str| -> Option<String> {
        message
            .headers_raw()
            .filter(|(header, _)| header.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.trim().to_owned())
            .next()
    };
    let from_domain = sender
        .as_deref()
        .and_then(|address| address.rsplit_once('@'))
        .map(|(_, domain)| domain.to_ascii_lowercase());
    let config = service.config();
    evaluate_inbound_trust(
        &auth_headers,
        header_text("X-SES-Spam-Verdict").as_deref(),
        header_text("X-SES-Virus-Verdict").as_deref(),
        from_domain.as_deref(),
        &config.inbound_authserv_id,
        config.inbound_auth_policy,
    )
    .map_err(JobExecutionFailure::permanent)?;
    match command.ticket_id {
        // Verified first contact: the sender becomes the requester of a
        // new ticket and the message identity registers atomically.
        None => {
            let sender = sender.ok_or_else(|| {
                JobExecutionFailure::permanent("ticketing.inbound_sender_unverified")
            })?;
            let subject = command
                .subject
                .clone()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| "Email support request".into());
            match service
                .create_ticket_from_inbound_email(
                    worker,
                    crate::ExternalMessageIdentity {
                        project_id: command.project_id.clone(),
                        provider: command.provider.clone(),
                        mailbox_scope: command.mailbox_scope.clone(),
                        external_id: command.external_id.clone(),
                        content_sha256: command.content_sha256.to_ascii_lowercase(),
                        raw_message_object_key: Some(command.raw_object_key.clone()),
                        internet_message_id: command.internet_message_id.clone(),
                        in_reply_to: command.in_reply_to.clone(),
                        references: command.references.clone(),
                    },
                    subject,
                    body,
                    sender,
                    context.correlation_id,
                    Utc::now(),
                )
                .await
            {
                Ok(_) => Ok(()),
                Err(error @ crate::TicketingServiceError::PermissionDenied(_)) => {
                    let _ = error;
                    Err(JobExecutionFailure::permanent(
                        "ticketing.ingest_unauthorized",
                    ))
                }
                Err(crate::TicketingServiceError::Store(
                    crate::TicketStoreError::ExternalIdentityConflict,
                )) => Err(JobExecutionFailure::permanent(
                    "ticketing.inbound_identity_conflict",
                )),
                Err(crate::TicketingServiceError::Configuration(_)) => Err(
                    JobExecutionFailure::permanent("ticketing.inbound_first_contact_disabled"),
                ),
                Err(other) => {
                    tracing::warn!(error = %other, "first-contact email intake failed; retrying");
                    Err(JobExecutionFailure::retryable(
                        "ticketing.inbound_store_unavailable",
                    ))
                }
            }
        }
        Some(ticket_id) => {
            // Threaded reply: the sender must be the ticket's requester
            // participant; a stranger who learned the thread identity is
            // quarantined, never appended.
            let participants = service
                .ticket_requester_email(&command.project_id, ticket_id)
                .await
                .map_err(|_| {
                    JobExecutionFailure::retryable("ticketing.inbound_store_unavailable")
                })?;
            let sender = sender.as_deref().map(str::to_ascii_lowercase);
            let allowed = participants
                .as_deref()
                .is_some_and(|expected| sender.as_deref() == Some(&expected.to_ascii_lowercase()));
            if !allowed {
                return Err(JobExecutionFailure::permanent(
                    "ticketing.inbound_sender_unverified",
                ));
            }
            let identity = crate::ExternalMessageIdentity {
                project_id: command.project_id.clone(),
                provider: command.provider.clone(),
                mailbox_scope: command.mailbox_scope.clone(),
                external_id: command.external_id.clone(),
                content_sha256: command.content_sha256.to_ascii_lowercase(),
                raw_message_object_key: Some(command.raw_object_key.clone()),
                internet_message_id: command.internet_message_id.clone(),
                in_reply_to: command.in_reply_to.clone(),
                references: command.references.clone(),
            };
            match service
                .ingest_external_message(
                    worker,
                    identity,
                    ticket_id,
                    body,
                    context.correlation_id,
                    Utc::now(),
                )
                .await
            {
                Ok(_) => Ok(()),
                Err(error @ crate::TicketingServiceError::PermissionDenied(_)) => {
                    let _ = error;
                    Err(JobExecutionFailure::permanent(
                        "ticketing.ingest_unauthorized",
                    ))
                }
                Err(
                    crate::TicketingServiceError::StaleRevision { .. }
                    | crate::TicketingServiceError::Store(crate::TicketStoreError::StaleRevision {
                        ..
                    }),
                ) => Err(JobExecutionFailure::retryable(
                    "ticketing.inbound_revision_stale",
                )),
                Err(crate::TicketingServiceError::Store(
                    crate::TicketStoreError::ExternalIdentityConflict,
                )) => Err(JobExecutionFailure::permanent(
                    "ticketing.inbound_identity_conflict",
                )),
                Err(
                    crate::TicketingServiceError::Validation(_)
                    | crate::TicketingServiceError::InvalidExternalIdentity
                    | crate::TicketingServiceError::InvalidContentDigest
                    | crate::TicketingServiceError::Store(crate::TicketStoreError::Validation(_)),
                ) => Err(JobExecutionFailure::permanent("ticketing.inbound_invalid")),
                Err(other) => {
                    // Store and infrastructure failures are retryable; the exact
                    // cause stays in worker logs, never in the failure code.
                    tracing::warn!(error = %other, "inbound email ingestion failed; retrying");
                    Err(JobExecutionFailure::retryable(
                        "ticketing.inbound_store_unavailable",
                    ))
                }
            }
        }
    }
}

/// Executes the automation command (ADR-0070): profile-gated, exclusion-
/// checked, proposal-shaped. Nothing here holds authority.
async fn run_development_automation(
    service: &TicketingService,
    store: &TicketingStoreService,
    command: &RunDevelopmentAutomation,
) -> Result<(), JobExecutionFailure> {
    let config = service.config();
    if config.automation.profile == crate::AutomationProfile::Off {
        // Automation is opt-in; a queued command after the profile was
        // turned off fails closed, not silently.
        return Err(JobExecutionFailure::permanent(
            "ticketing.automation_disabled",
        ));
    }
    let ticket = store
        .get(&command.project_id, command.ticket_id)
        .await
        .map_err(|_| JobExecutionFailure::retryable("ticketing.automation_store_unavailable"))?
        .ok_or_else(|| JobExecutionFailure::permanent("ticketing.automation_target_missing"))?;
    // Freshness (exact-head review R8): a proposal must provably come
    // from the submitted ticket context. A ticket that moved past the
    // bound revision is stale work — classify it superseded instead of
    // storing a proposal over changed reality.
    if ticket.revision != command.bound_revision {
        return Err(JobExecutionFailure::permanent(
            "ticketing.automation_superseded",
        ));
    }
    // The context digest proves the proposal derives from the submitted
    // ticket content, not merely the same revision number (exact-head
    // review R18).
    if let Some(bound) = command.bound_context_digest.as_deref() {
        use sha2::Digest as _;
        let mut hasher = sha2::Sha256::new();
        hasher.update(ticket.subject.as_bytes());
        hasher.update(ticket.description.as_bytes());
        let authoritative = hex::encode(hasher.finalize());
        if authoritative != bound {
            return Err(JobExecutionFailure::permanent(
                "ticketing.automation_superseded",
            ));
        }
    }
    // Deterministic local model (ADR-0070): a proposal assembled from
    // ticket context. No external calls, no hidden execution.
    let requested_actions = match config.automation.profile {
        crate::AutomationProfile::Assist => vec!["summarize".to_owned()],
        crate::AutomationProfile::Supervised | crate::AutomationProfile::Autonomous => {
            vec!["summarize".to_owned(), "draft.reply".to_owned()]
        }
        crate::AutomationProfile::Off => unreachable!("checked above"),
    };
    crate::validate_automation_actions(&requested_actions)
        .map_err(|_| JobExecutionFailure::permanent("ticketing.automation_action_excluded"))?;
    let summary = format!(
        "Automation proposal for {} ({}): {} typed form answer(s), {} knowledge link(s).",
        ticket.display_reference,
        serde_json::to_string(&ticket.ticket_type).unwrap_or_default(),
        ticket.form_answers.len(),
        ticket.knowledge_links.len(),
    );
    let proposal = crate::AutomationProposal::new(
        command.ticket_id,
        summary,
        requested_actions,
        &command.requested_by,
        Utc::now(),
    );
    store
        .insert_automation_proposal(&command.project_id, proposal)
        .await
        .map_err(|_| JobExecutionFailure::retryable("ticketing.automation_store_unavailable"))?;
    Ok(())
}

async fn deliver_public_notification(
    store: &TicketingStoreService,
    notifications: &NotificationService,
    mail: Option<&Arc<MailService>>,
    command: &DeliverPublicNotification,
) -> Result<(), JobExecutionFailure> {
    let ticket = store
        .get(&command.project_id, command.ticket_id)
        .await
        .map_err(|_| JobExecutionFailure::permanent("ticketing.notification_target_missing"))?
        .ok_or_else(|| JobExecutionFailure::permanent("ticketing.notification_target_missing"))?;
    let message = ticket
        .messages
        .iter()
        .find(|message| message.id == command.message_id)
        .ok_or_else(|| JobExecutionFailure::permanent("ticketing.notification_target_missing"))?;
    if message.kind != crate::TicketMessageKind::PublicReply {
        // Only public messages are notifiable; anything else is a
        // permanent refusal, not an infrastructure retry.
        return Err(JobExecutionFailure::permanent(
            "ticketing.notification_target_missing",
        ));
    }
    if ticket.requester.email.is_none() {
        // In-app channel: no outbound email, no delivery evidence —
        // the notifications port is the whole story (ADR-0054).
        return notifications
            .send(Notification {
                id: Uuid::now_v7(),
                topic: "ticketing.public-notification".into(),
                channel: NotificationChannel::InApp,
                recipient: ticket.requester.subject.clone(),
                title: format!("{} — {}", ticket.display_reference, ticket.subject),
                body: message.body.clone(),
                link: None,
                metadata: std::collections::BTreeMap::default(),
                created_at: Utc::now(),
            })
            .await
            .map_err(|_| JobExecutionFailure::retryable("ticketing.notification_send_failed"));
    }
    let Some(mail) = mail else {
        // Email channel without a configured mail service is a permanent
        // configuration defect; silently downgrading the channel would
        // hide mail the requester was promised.
        return Err(JobExecutionFailure::permanent(
            "ticketing.notification_mail_unconfigured",
        ));
    };
    deliver_public_reply_by_mail(store, mail, command, &ticket, message).await
}

/// Email-channel delivery with reconciliation and ambiguity recovery
/// (ADR-0063). Provider acceptance is recorded as evidence and never
/// claimed as delivery. Every send of one message carries the same stable
/// RFC Message-ID, so provider-side dedupe can bound duplicates and email
/// replies thread back to the originating ticket; an ambiguous transport
/// result fails closed for explicit reconciliation instead of blindly
/// resending (review finding 8).
async fn deliver_public_reply_by_mail(
    store: &TicketingStoreService,
    mail: &MailService,
    command: &DeliverPublicNotification,
    ticket: &crate::Ticket,
    message: &crate::TicketMessage,
) -> Result<(), JobExecutionFailure> {
    let email =
        ticket.requester.email.as_deref().ok_or_else(|| {
            JobExecutionFailure::permanent("ticketing.notification_target_missing")
        })?;
    let message_id = message.id;
    // Reconcile before any send: a recorded acceptance suppresses the
    // resend, and an unresolved ambiguous outcome fails closed — the
    // outcome must be reconciled explicitly, never guessed by resending.
    let evidence = store
        .outbound_evidence(&command.project_id, command.ticket_id, message_id)
        .await
        .map_err(|_| JobExecutionFailure::retryable("ticketing.evidence_unavailable"))?;
    if evidence
        .iter()
        .any(|row| row.kind == OutboundEvidenceKind::Accepted)
    {
        return Ok(());
    }
    // An ambiguous outcome stays unresolved until a later authoritative
    // row (acceptance or a reconciled verdict) closes it; evidence rows
    // are append-only, so resolution is positional.
    let last_ambiguous = evidence
        .iter()
        .rposition(|row| row.kind == OutboundEvidenceKind::Ambiguous);
    let last_resolved = evidence.iter().rposition(|row| {
        matches!(
            row.kind,
            OutboundEvidenceKind::Accepted | OutboundEvidenceKind::PermanentFailure
        )
    });
    let unresolved = match (last_ambiguous, last_resolved) {
        (Some(_), None) => true,
        (Some(ambiguous_at), Some(resolved_at)) => resolved_at < ambiguous_at,
        _ => false,
    };
    if unresolved {
        return Err(JobExecutionFailure::permanent(
            "ticketing.notification_reconciliation_required",
        ));
    }
    let address = MailAddress::new(email)
        .map_err(|_| JobExecutionFailure::permanent("ticketing.notification_recipient_invalid"))?;
    // One message carries one mail identity forever: the deterministic
    // message id drives the rendered RFC Message-ID
    // (<id@from-domain>), so retries, ambiguous redrives and reconciled
    // resends are all dedupeable by identity.
    let stable_mail_id = Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        format!(
            "ticketing:public-reply:{}:{}:{}",
            command.project_id, command.ticket_id, message_id
        )
        .as_bytes(),
    );
    let stable_message_id = format!("<{stable_mail_id}@outbound.ticketing.invalid>");
    // The send intent commits BEFORE provider contact (exact-head review
    // R4): sent → done; sending/recovery → reconciliation only;
    // pending → the one identity-stable resend.
    let logical_send_id = stable_mail_id.to_string();
    let mut current_attempt: Option<Uuid> = None;
    let claimed = store
        .claim_send_intent(SendIntent {
            logical_send_id: logical_send_id.clone(),
            project_id: command.project_id.clone(),
            ticket_id: command.ticket_id,
            message_id,
            state: SendIntentState::Sending,
            provider_message_id: None,
            updated_at: Utc::now(),
            created_at: Utc::now(),
            attempt_id: None,
            attempt_sequence: 0,
            lease_expires_at: None,
        })
        .await
        .map_err(|_| JobExecutionFailure::retryable("ticketing.evidence_unavailable"))?;
    if let Some(existing) = claimed {
        match existing.state {
            SendIntentState::Sent => return Ok(()),
            SendIntentState::PendingSend => {
                // A reconciled authoritative no-send: the fenced
                // pending_send -> sending claim must SUCCEED before any
                // provider contact (exact-head review R13). A failed
                // claim makes zero provider calls.
                let Some(attempt) = store
                    .claim_send_attempt(
                        &existing.logical_send_id,
                        SendIntentState::PendingSend,
                        Utc::now(),
                    )
                    .await
                    .map_err(|_| {
                        JobExecutionFailure::retryable("ticketing.evidence_unavailable")
                    })?
                else {
                    return Err(JobExecutionFailure::permanent(
                        "ticketing.notification_reconciliation_required",
                    ));
                };
                current_attempt = Some(attempt);
            }
            SendIntentState::Sending | SendIntentState::RecoveryRequired => {
                return Err(JobExecutionFailure::permanent(
                    "ticketing.notification_reconciliation_required",
                ));
            }
            SendIntentState::FailedNoSend => {
                return Err(JobExecutionFailure::permanent(
                    "ticketing.notification_permanent",
                ));
            }
        }
    }
    // The initial claim created the intent in sending WITHOUT an
    // attempt identity; claim one now so every subsequent fenced
    // transition validates against it (exact-head review R22/P0-3).
    if current_attempt.is_none() {
        current_attempt = store
            .claim_send_attempt(&logical_send_id, SendIntentState::Sending, Utc::now())
            .await
            .map_err(|_| JobExecutionFailure::retryable("ticketing.evidence_unavailable"))?;
    }
    let outbound_body = message.body.clone();
    let mail_message = minco_plugin_notifications::MailMessage::builder(
        "ticketing.public-notification",
        format!("{} — {}", ticket.display_reference, ticket.subject),
    )
    .id(stable_mail_id)
    .to(address)
    .text(outbound_body.clone())
    .tag("minco-send-id", logical_send_id.clone())
    .build()
    .map_err(|_| JobExecutionFailure::permanent("ticketing.notification_message_invalid"))?;
    let now = Utc::now();
    match mail.send(mail_message).await {
        Ok(receipt) => {
            // Provider acceptance resolves the intent with the provider's
            // own message identity: SES overwrites caller Message-IDs, so
            // the provider id is the correlation truth and the logical id
            // remains the stable tag (exact-head review R4).
            let attempt = current_attempt
                .ok_or_else(|| JobExecutionFailure::retryable("ticketing.evidence_unavailable"))?;
            let fenced = store
                .resolve_send_intent_fenced(
                    &logical_send_id,
                    attempt,
                    SendIntentState::Sent,
                    Some(receipt.provider_message_id.clone()),
                    Utc::now(),
                )
                .await
                .map_err(|_| JobExecutionFailure::retryable("ticketing.evidence_unavailable"))?;
            if !fenced {
                // The attempt fence was lost — a newer worker owns this
                // send. This worker's provider acceptance is recorded in
                // evidence but the state machine is the other worker's.
                tracing::warn!(
                    "send attempt fence lost after provider acceptance; \
                     evidence still recorded"
                );
            }
            store
                .append_outbound_evidence(OutboundDeliveryEvidence {
                    project_id: command.project_id.clone(),
                    ticket_id: command.ticket_id,
                    message_id,
                    kind: OutboundEvidenceKind::Accepted,
                    provider: receipt.transport.clone(),
                    provider_message_id: receipt.provider_message_id.clone(),
                    feedback: None,
                    failure_kind: None,
                    recorded_at: now,
                })
                .await
                .map_err(|_| JobExecutionFailure::retryable("ticketing.evidence_unavailable"))?;
            // Register the outbound threading identity so an emailed reply
            // referencing our Message-ID resolves to this ticket
            // (ADR-0058 resolution, review finding 8). Registration is
            // idempotent; a failure here is retriable and never
            // re-triggers a send because acceptance is already recorded.
            store
                .register_outbound_identity(
                    &command.project_id,
                    crate::ExternalMessageIdentity {
                        project_id: command.project_id.clone(),
                        provider: receipt.transport.clone(),
                        mailbox_scope: "outbound".into(),
                        external_id: if receipt.provider_message_id.is_empty() {
                            stable_message_id.clone()
                        } else {
                            receipt.provider_message_id.clone()
                        },
                        content_sha256: crate::external_content_sha256(outbound_body.as_bytes()),
                        raw_message_object_key: None,
                        internet_message_id: Some(stable_message_id),
                        in_reply_to: None,
                        references: Vec::new(),
                    },
                    command.ticket_id,
                )
                .await
                .map_err(|_| JobExecutionFailure::retryable("ticketing.evidence_unavailable"))?;
            Ok(())
        }
        Err(error) => match error.retry_advice() {
            MailRetryAdvice::SafeAfterBackoff => {
                // The provider explicitly reported no side effect: the
                // intent returns to pending_send so the next attempt can
                // retry (exact-head review R13). A persistence failure
                // surfaces as retryable — never a silent wedge.
                if let Some(attempt) = current_attempt {
                    let fenced = store
                        .resolve_send_intent_fenced(
                            &logical_send_id,
                            attempt,
                            SendIntentState::PendingSend,
                            None,
                            Utc::now(),
                        )
                        .await
                        .map_err(|_| {
                            JobExecutionFailure::retryable("ticketing.evidence_unavailable")
                        })?;
                    if !fenced {
                        tracing::warn!("safe backoff recorded on a lost send fence");
                    }
                }
                Err(JobExecutionFailure::retryable(
                    "ticketing.notification_transport_retryable",
                ))
            }
            MailRetryAdvice::ReconcileBeforeRetry => {
                // Ambiguous result: the intent holds in recovery and the
                // evidence records the fact; later attempts fail closed
                // until explicit reconciliation — never a blind retry
                // (exact-head review R4).
                if let Some(attempt) = current_attempt
                    && !store
                        .resolve_send_intent_fenced(
                            &logical_send_id,
                            attempt,
                            SendIntentState::RecoveryRequired,
                            None,
                            Utc::now(),
                        )
                        .await
                        .map_err(|_| {
                            JobExecutionFailure::retryable("ticketing.evidence_unavailable")
                        })?
                {
                    // Fence lost: the newer owner decides the state; this
                    // worker still records its evidence.
                    tracing::warn!("ambiguity recorded on a lost send fence");
                }
                store
                    .append_outbound_evidence(OutboundDeliveryEvidence {
                        project_id: command.project_id.clone(),
                        ticket_id: command.ticket_id,
                        message_id,
                        kind: OutboundEvidenceKind::Ambiguous,
                        provider: error.transport.clone(),
                        provider_message_id: String::new(),
                        feedback: None,
                        failure_kind: Some(mail_failure_kind_name(&error)),
                        recorded_at: now,
                    })
                    .await
                    .map_err(|_| {
                        JobExecutionFailure::retryable("ticketing.evidence_unavailable")
                    })?;
                Err(JobExecutionFailure::retryable(
                    "ticketing.notification_ambiguous",
                ))
            }
            MailRetryAdvice::Never => {
                if let Some(attempt) = current_attempt
                    && !store
                        .resolve_send_intent_fenced(
                            &logical_send_id,
                            attempt,
                            SendIntentState::FailedNoSend,
                            None,
                            Utc::now(),
                        )
                        .await
                        .map_err(|_| {
                            JobExecutionFailure::retryable("ticketing.evidence_unavailable")
                        })?
                {
                    tracing::warn!("permanent failure recorded on a lost send fence");
                }
                store
                    .append_outbound_evidence(OutboundDeliveryEvidence {
                        project_id: command.project_id.clone(),
                        ticket_id: command.ticket_id,
                        message_id,
                        kind: OutboundEvidenceKind::PermanentFailure,
                        provider: error.transport.clone(),
                        provider_message_id: String::new(),
                        feedback: None,
                        failure_kind: Some(mail_failure_kind_name(&error)),
                        recorded_at: now,
                    })
                    .await
                    .map_err(|_| {
                        JobExecutionFailure::retryable("ticketing.evidence_unavailable")
                    })?;
                Err(JobExecutionFailure::permanent(
                    "ticketing.notification_permanent",
                ))
            }
        },
    }
}

fn mail_failure_kind_name(error: &MailError) -> String {
    format!("{:?}", error.kind).to_lowercase()
}

/// Pattern A enqueue port (ADR-0054).
///
/// The `sqlite` profile commits these records in the same SQL transaction
/// as the ticket mutation. The composition root adapts the released
/// `SqliteJobStore::enqueue_in` to this port; adapters implement ports
/// owned by the application layer.
#[cfg(feature = "sqlite")]
#[async_trait]
pub trait TicketingJobEnqueue: Send + Sync + std::fmt::Debug {
    async fn enqueue_in(
        &self,
        transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        record: minco_plugin_jobs::JobRecord,
    ) -> Result<(), TicketStoreError>;
}

/// Bound on job records attached to one ticketing mutation.
pub const MAX_JOB_RECORDS_PER_MUTATION: usize = 8;

/// Builds the notification job record for a public agent reply, or `None`
/// when notification is not enabled. Kept allocation-free of message
/// content: identifiers only.
pub fn notification_record_for_reply(
    project_id: &str,
    ticket_id: TicketId,
    message_id: TicketMessageId,
    correlation_id: Uuid,
    now: chrono::DateTime<Utc>,
) -> Result<Option<minco_plugin_jobs::JobRecord>, JobError> {
    let envelope = public_notification_envelope(
        &DeliverPublicNotification {
            project_id: project_id.to_owned(),
            ticket_id,
            message_id,
        },
        correlation_id,
        now,
    )?;
    Ok(Some(pending_record(envelope)))
}

#[cfg(all(test, feature = "jobs"))]
mod tests {
    use super::*;
    use crate::{
        CreateTicketInput, MemoryTicketingStore, TicketChannel, TicketPriority, TicketRequester,
        TicketingStore, TicketingStoreService,
    };
    use minco_plugin_notifications::MemoryNotificationSink;
    use std::collections::{BTreeMap, BTreeSet};

    fn ticket(now: chrono::DateTime<Utc>) -> crate::Ticket {
        crate::Ticket::create(
            CreateTicketInput {
                project_id: "project-a".into(),
                subject: "Help".into(),
                description: "It broke".into(),
                requester: TicketRequester {
                    subject: "user-1".into(),
                    display_name: None,
                    email: Some("user-1@example.test".into()),
                },
                channel: TicketChannel::Portal,
                ticket_type: crate::TicketType::default(),
                form_answers: Vec::new(),
                priority: TicketPriority::Normal,
                resource_references: Vec::new(),
            },
            "TKT-JOB",
            now,
        )
        .unwrap()
    }

    fn worker_identity() -> minco_plugin_identity::Identity {
        minco_plugin_identity::Identity {
            subject: "ticketing-mail-worker".into(),
            permissions: BTreeSet::from(["ticketing.ingest".into()]),
            scopes: BTreeSet::new(),
            claims: BTreeMap::new(),
        }
    }

    fn notification_deps(
        notifications: Arc<NotificationService>,
        mail: Option<Arc<MailService>>,
    ) -> TicketingJobsDeps {
        let store = Arc::new(MemoryTicketingStore::default());
        let service = crate::TicketingService::new(
            TicketingStoreService::new(store),
            crate::TicketingConfig {
                project_id: "project-a".into(),
                ..crate::TicketingConfig::default()
            },
        )
        .unwrap();
        let objects = Arc::new(ObjectStoreService::new(Arc::new(
            minco_plugin_object_storage::MemoryObjectStore::default(),
        )));
        TicketingJobsDeps {
            service,
            notifications,
            mail,
            objects,
            worker: worker_identity(),
        }
    }

    #[test]
    fn notification_envelope_carries_the_contract_policies() {
        let now = Utc::now();
        let payload = DeliverPublicNotification {
            project_id: "project-a".into(),
            ticket_id: TicketId::new(),
            message_id: TicketMessageId::new(),
        };
        let envelope = public_notification_envelope(&payload, Uuid::now_v7(), now).unwrap();
        assert_eq!(envelope.job_name, DeliverPublicNotification::NAME);
        assert_eq!(envelope.worker_profile, TICKETING_MAIL_PROFILE);
        assert!(
            envelope
                .dedupe_key
                .as_deref()
                .is_some_and(|key| key.starts_with("notification:"))
        );
        assert!(
            envelope
                .overlap_key
                .as_deref()
                .is_some_and(|key| key.starts_with("ticket:"))
        );
        assert_eq!(envelope.partition.as_deref(), Some("project-a"));
        assert!(envelope.deadline.is_some());
        // The payload carries identifiers only: no message bodies.
        let encoded = serde_json::to_string(&payload).unwrap();
        assert!(!encoded.contains("body"));
    }

    #[test]
    fn envelope_debug_never_reveals_payload_content() {
        let now = Utc::now();
        let payload = DeliverPublicNotification {
            project_id: "project-a".into(),
            ticket_id: TicketId::new(),
            message_id: TicketMessageId::new(),
        };
        let envelope = public_notification_envelope(&payload, Uuid::now_v7(), now).unwrap();
        let debug = format!("{envelope:?}");
        assert!(!debug.contains("project-a"));
    }

    #[tokio::test]
    async fn development_automation_executes_durable_and_stores_a_proposal() {
        let now = Utc::now();
        let mut ticket = ticket(now);
        ticket.requester.email = None;
        let store = Arc::new(MemoryTicketingStore::default());
        let registry = Arc::new(JobHandlerRegistry::new());
        let (jobs, _jobs_store, _dispatcher) =
            minco_plugin_jobs::JobsServices::memory(registry.clone());
        let service = std::sync::Arc::new(
            crate::TicketingService::new(
                TicketingStoreService::new(store.clone()),
                crate::TicketingConfig {
                    project_id: "project-a".into(),
                    automation: crate::AutomationConfig {
                        profile: crate::AutomationProfile::Supervised,
                        review: crate::AutomationReview::Always,
                    },
                    ..crate::TicketingConfig::default()
                },
            )
            .unwrap()
            .with_portal_services(crate::TicketingPortalServices {
                jobs: Some(Arc::new(jobs.clone())),
                ..crate::TicketingPortalServices::default()
            }),
        );
        let intent = crate::TicketActivityIntent::new(
            "project-a",
            ticket.id,
            "created",
            Uuid::now_v7(),
            serde_json::json!({}),
            now,
        );
        store.create(ticket.clone(), intent).await.unwrap();
        let notifications = Arc::new(NotificationService::new(Arc::new(
            MemoryNotificationSink::default(),
        )));
        let objects = Arc::new(ObjectStoreService::new(Arc::new(
            minco_plugin_object_storage::MemoryObjectStore::default(),
        )));
        register_ticketing_jobs(
            &registry,
            &TicketingStoreService::new(store.clone()),
            TicketingJobsDeps {
                service: TicketingService::clone(&service),
                notifications,
                mail: None,
                objects,
                worker: worker_identity(),
            },
        )
        .unwrap();
        // Execute through the registered handler via the inline path.
        let envelope = development_automation_envelope(
            &RunDevelopmentAutomation {
                project_id: "project-a".into(),
                ticket_id: ticket.id,
                requested_by: "agent-1".into(),
                bound_revision: ticket.revision,
                bound_context_digest: None,
                run_id: Uuid::new_v4(),
            },
            Uuid::now_v7(),
            now,
        )
        .unwrap();
        assert_eq!(
            service.config().automation.profile,
            crate::AutomationProfile::Supervised
        );
        jobs.submit_inline(envelope).await.unwrap();
        let proposals = store
            .list_automation_proposals("project-a", ticket.id)
            .await
            .unwrap();
        assert_eq!(proposals.len(), 1);
        assert_eq!(
            proposals[0].state,
            crate::AutomationProposalState::AwaitingReview
        );
        assert!(proposals[0].summary.contains("TKT-JOB"));
    }

    #[tokio::test]
    async fn in_app_reply_goes_through_the_notifications_port_unchanged() {
        let now = Utc::now();
        let mut ticket = ticket(now);
        ticket.requester.email = None;
        let message = ticket
            .reply_as_agent_message("agent-1", "Your fix is live.", now)
            .unwrap();
        let store = Arc::new(MemoryTicketingStore::default());
        let intent = crate::TicketActivityIntent::new(
            "project-a",
            ticket.id,
            "created",
            Uuid::now_v7(),
            serde_json::json!({}),
            now,
        );
        store.create(ticket.clone(), intent).await.unwrap();

        let sink = Arc::new(MemoryNotificationSink::default());
        let notifications = Arc::new(NotificationService::new(sink.clone()));
        let registry = Arc::new(JobHandlerRegistry::new());
        register_ticketing_jobs(
            &registry,
            &TicketingStoreService::new(store.clone()),
            notification_deps(notifications, None),
        )
        .unwrap();

        let services = minco_plugin_jobs::JobsServices::memory(registry).0;
        let correlation = Uuid::now_v7();
        let envelope = public_notification_envelope(
            &DeliverPublicNotification {
                project_id: "project-a".into(),
                ticket_id: ticket.id,
                message_id: message.id,
            },
            correlation,
            now,
        )
        .unwrap();
        services.submit_inline(envelope).await.unwrap();
        let sent = sink.all().await;
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].channel, NotificationChannel::InApp);
        assert_eq!(sent[0].recipient, "user-1");
        assert_eq!(sent[0].body, "Your fix is live.");
        // In-app delivery records no outbound evidence: nothing was mailed.
        assert!(store.all_outbound_evidence().await.is_empty());
    }

    /// Scriptable mail transport: pops one scripted result per attempt and
    /// records every submitted message.
    type ScriptedResult = Result<minco_plugin_notifications::MailReceipt, MailError>;

    struct ScriptedMailTransport {
        script: tokio::sync::Mutex<Vec<ScriptedResult>>,
        submitted: tokio::sync::Mutex<Vec<minco_plugin_notifications::MailMessage>>,
    }

    impl ScriptedMailTransport {
        fn new(script: Vec<ScriptedResult>) -> Arc<Self> {
            Arc::new(Self {
                script: tokio::sync::Mutex::new(script),
                submitted: tokio::sync::Mutex::new(Vec::new()),
            })
        }

        async fn submit_count(&self) -> usize {
            self.submitted.lock().await.len()
        }
    }

    impl std::fmt::Debug for ScriptedMailTransport {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.debug_struct("ScriptedMailTransport").finish()
        }
    }

    #[async_trait::async_trait]
    impl minco_plugin_notifications::MailTransport for ScriptedMailTransport {
        fn name(&self) -> &'static str {
            "scripted"
        }

        async fn send(
            &self,
            message: &minco_plugin_notifications::MailMessage,
            attempt: u32,
        ) -> ScriptedResult {
            self.submitted.lock().await.push(message.clone());
            self.script.lock().await.pop().unwrap_or_else(|| {
                Ok(minco_plugin_notifications::MailReceipt {
                    message_id: message.id,
                    transport: "scripted".into(),
                    provider_message_id: format!("provider-{attempt}"),
                    accepted_at: Utc::now(),
                    attempt,
                })
            })
        }
    }

    fn mail_error(kind: minco_plugin_notifications::MailErrorKind) -> MailError {
        MailError::new(kind, "scripted", "scripted failure")
    }

    async fn mail_setup(
        script: Vec<ScriptedResult>,
    ) -> (
        minco_plugin_jobs::JobsServices,
        Arc<ScriptedMailTransport>,
        Arc<MemoryTicketingStore>,
        crate::Ticket,
        crate::TicketMessage,
    ) {
        let now = Utc::now();
        let mut ticket = ticket(now);
        let message = ticket
            .reply_as_agent_message("agent-1", "Your fix is live.", now)
            .unwrap();
        let store = Arc::new(MemoryTicketingStore::default());
        let intent = crate::TicketActivityIntent::new(
            "project-a",
            ticket.id,
            "created",
            Uuid::now_v7(),
            serde_json::json!({}),
            now,
        );
        store.create(ticket.clone(), intent).await.unwrap();
        let transport = ScriptedMailTransport::new(script);
        let mail = Arc::new(
            MailService::single(
                transport.clone(),
                Arc::new(minco_plugin_notifications::NoopMailObserver),
            )
            .unwrap(),
        );
        let sink = Arc::new(MemoryNotificationSink::default());
        let notifications = Arc::new(NotificationService::new(sink));
        let registry = Arc::new(JobHandlerRegistry::new());
        register_ticketing_jobs(
            &registry,
            &TicketingStoreService::new(store.clone()),
            notification_deps(notifications, Some(mail)),
        )
        .unwrap();
        (
            minco_plugin_jobs::JobsServices::memory(registry).0,
            transport,
            store,
            ticket,
            message,
        )
    }

    fn deliver_envelope(
        ticket: &crate::Ticket,
        message: &crate::TicketMessage,
    ) -> minco_plugin_jobs::JobEnvelope {
        public_notification_envelope(
            &DeliverPublicNotification {
                project_id: "project-a".into(),
                ticket_id: ticket.id,
                message_id: message.id,
            },
            Uuid::now_v7(),
            Utc::now(),
        )
        .unwrap()
    }

    #[tokio::test]
    async fn email_reply_records_acceptance_and_never_claims_delivery() {
        let (services, transport, store, ticket, message) = mail_setup(vec![]).await;
        services
            .submit_inline(deliver_envelope(&ticket, &message))
            .await
            .unwrap();
        assert_eq!(transport.submit_count().await, 1);
        let evidence = store.all_outbound_evidence().await;
        assert_eq!(evidence.len(), 1);
        assert_eq!(evidence[0].kind, OutboundEvidenceKind::Accepted);
        assert_eq!(evidence[0].provider, "scripted");
        assert_eq!(evidence[0].provider_message_id, "provider-1");
        assert_eq!(evidence[0].message_id, message.id);
    }

    #[tokio::test]
    async fn redelivery_after_acceptance_never_resends() {
        let (services, transport, store, ticket, message) = mail_setup(vec![]).await;
        services
            .submit_inline(deliver_envelope(&ticket, &message))
            .await
            .unwrap();
        // Redelivery (at-least-once) reconciles against recorded
        // acceptance and suppresses the duplicate send.
        services
            .submit_inline(deliver_envelope(&ticket, &message))
            .await
            .unwrap();
        assert_eq!(transport.submit_count().await, 1);
        assert_eq!(store.all_outbound_evidence().await.len(), 1);
    }

    #[tokio::test]
    async fn safe_backoff_returns_to_pending_and_the_next_attempt_retries() {
        // Exact-head review R13: a provider-confirmed no-side-effect
        // failure must return the intent to pending_send so the retry
        // actually retries — the old code left it in sending and the
        // next attempt wedged permanently on reconciliation_required.
        let (services, transport, store, ticket, message) = mail_setup(vec![Err(mail_error(
            minco_plugin_notifications::MailErrorKind::Throttled,
        ))])
        .await;
        let first = services
            .submit_inline(deliver_envelope(&ticket, &message))
            .await
            .unwrap_err();
        assert_eq!(first.code(), "ticketing.notification_transport_retryable");
        let logical_send_id = Uuid::new_v5(
            &Uuid::NAMESPACE_URL,
            format!(
                "ticketing:public-reply:project-a:{}:{}",
                ticket.id, message.id
            )
            .as_bytes(),
        )
        .to_string();
        let intent = TicketingStoreService::new(store)
            .send_intent(&logical_send_id)
            .await
            .unwrap()
            .expect("intent exists");
        assert_eq!(intent.state, crate::SendIntentState::PendingSend);
        // The retry now genuinely contacts the provider again.
        services
            .submit_inline(deliver_envelope(&ticket, &message))
            .await
            .unwrap();
        assert_eq!(transport.submit_count().await, 2);
    }

    #[tokio::test]
    async fn failed_pending_send_claim_makes_zero_provider_calls() {
        // Exact-head review R13: when the fenced pending_send->sending
        // claim loses (another worker holds it), the attempt must not
        // contact the provider at all.
        let (services, transport, store, ticket, message) = mail_setup(Vec::new()).await;
        let logical_send_id = Uuid::new_v5(
            &Uuid::NAMESPACE_URL,
            format!(
                "ticketing:public-reply:project-a:{}:{}",
                ticket.id, message.id
            )
            .as_bytes(),
        )
        .to_string();
        // Simulate a prior attempt that resolved to pending (reconciled
        // no-send) and then another worker already claimed sending.
        store
            .put_send_intent_for_tests(
                &logical_send_id,
                ticket.id,
                message.id,
                crate::SendIntentState::Sending,
            )
            .await;
        let failure = services
            .submit_inline(deliver_envelope(&ticket, &message))
            .await
            .unwrap_err();
        assert_eq!(
            failure.code(),
            "ticketing.notification_reconciliation_required"
        );
        assert!(
            failure.is_permanent(),
            "a lost claim is reconciliation, not a resend"
        );
        assert_eq!(
            transport.submit_count().await,
            0,
            "a lost fenced claim makes zero provider calls"
        );
    }

    #[tokio::test]
    async fn stale_worker_cannot_overwrite_a_newer_attempt_state() {
        // Exact-head review R22/P0-3: a worker whose attempt fence was
        // lost (a newer worker claimed the send) cannot resolve the
        // state — its transition returns false and never converts to
        // success.
        let (_services, transport, store, ticket, message) = mail_setup(Vec::new()).await;
        let logical_send_id = Uuid::new_v5(
            &Uuid::NAMESPACE_URL,
            format!(
                "ticketing:public-reply:project-a:{}:{}",
                ticket.id, message.id
            )
            .as_bytes(),
        )
        .to_string();
        // Seed the intent in pending_send (a reconciled no-send).
        store
            .put_send_intent_for_tests(
                &logical_send_id,
                ticket.id,
                message.id,
                SendIntentState::PendingSend,
            )
            .await;
        let store_service = || TicketingStoreService::new(store.clone());
        // Worker A claims the attempt.
        let attempt_a = store_service()
            .claim_send_attempt(&logical_send_id, SendIntentState::PendingSend, Utc::now())
            .await
            .unwrap()
            .expect("worker A claims");
        // Reconciliation resets to pending; worker B claims a NEW attempt.
        store_service()
            .resolve_send_intent(
                &logical_send_id,
                SendIntentState::PendingSend,
                None,
                Utc::now(),
            )
            .await
            .unwrap();
        let attempt_b = store_service()
            .claim_send_attempt(&logical_send_id, SendIntentState::PendingSend, Utc::now())
            .await
            .unwrap()
            .expect("worker B claims");
        assert_ne!(attempt_a, attempt_b);
        // Worker A's fenced transition must fail — it cannot overwrite
        // worker B's state (exact-head review R22/P0-3).
        let fenced = store_service()
            .resolve_send_intent_fenced(
                &logical_send_id,
                attempt_a,
                SendIntentState::Sent,
                Some("stale".into()),
                Utc::now(),
            )
            .await
            .unwrap();
        assert!(!fenced, "a stale worker can never write state");
        // Worker B's transition succeeds.
        let fenced_b = store_service()
            .resolve_send_intent_fenced(
                &logical_send_id,
                attempt_b,
                SendIntentState::Sent,
                Some("fresh".into()),
                Utc::now(),
            )
            .await
            .unwrap();
        assert!(fenced_b, "the current attempt owner writes state");
        assert_eq!(transport.submit_count().await, 0, "no provider contact");
    }

    #[tokio::test]
    async fn resolved_send_intent_prevents_resend_after_evidence_failure() {
        // Exact-head review R4: the provider accepted the mail and the
        // intent resolved, but the evidence write failed. The retry must
        // NOT contact the provider again.
        let (services, transport, store, ticket, message) = mail_setup(Vec::new()).await;
        // Simulate the crashed prior attempt: intent resolved as sent,
        // evidence missing.
        let logical_send_id = Uuid::new_v5(
            &Uuid::NAMESPACE_URL,
            format!(
                "ticketing:public-reply:project-a:{}:{}",
                ticket.id, message.id
            )
            .as_bytes(),
        )
        .to_string();
        store
            .put_send_intent_for_tests(
                &logical_send_id,
                ticket.id,
                message.id,
                crate::SendIntentState::Sent,
            )
            .await;
        services
            .submit_inline(deliver_envelope(&ticket, &message))
            .await
            .unwrap();
        assert_eq!(
            transport.submit_count().await,
            0,
            "a resolved send intent must suppress the resend"
        );
    }

    #[tokio::test]
    async fn outbound_threading_identity_resolves_replies_to_the_ticket() {
        // One successful mail delivery registers the stable outbound
        // identity; an emailed reply referencing the rendered
        // <id@from-domain> resolves back to the ticket (review finding 8).
        let (services, transport, store, ticket, message) = mail_setup(Vec::new()).await;
        services
            .submit_inline(deliver_envelope(&ticket, &message))
            .await
            .unwrap();
        assert_eq!(transport.submit_count().await, 1);
        let sent = transport.submitted.lock().await.clone();
        let rendered = format!("<{}@mail.example.test>", sent[0].id);
        let resolved = TicketingStoreService::new(store)
            .find_ticket_by_message_identity("project-a", "scripted", &rendered)
            .await
            .unwrap()
            .expect("the reply must resolve to the originating ticket");
        assert_eq!(resolved.0, ticket.id);
    }

    #[tokio::test]
    async fn ambiguous_result_records_evidence_and_resends_only_after_reconciliation() {
        // First attempt: the transport result is ambiguous (was the mail
        // accepted or not?). Evidence records the ambiguity; the job fails
        // retryably — never a blind resend.
        let (services, transport, store, ticket, message) = mail_setup(vec![Err(mail_error(
            minco_plugin_notifications::MailErrorKind::Ambiguous,
        ))])
        .await;
        let failure = services
            .submit_inline(deliver_envelope(&ticket, &message))
            .await
            .unwrap_err();
        assert_eq!(failure.code(), "ticketing.notification_ambiguous");
        assert!(!failure.is_permanent());
        let evidence = store.all_outbound_evidence().await;
        assert_eq!(evidence.len(), 1);
        assert_eq!(evidence[0].kind, OutboundEvidenceKind::Ambiguous);
        assert_eq!(evidence[0].failure_kind.as_deref(), Some("ambiguous"));
        assert_eq!(transport.submit_count().await, 1);

        // A blind retry is refused: an ambiguous outcome demands explicit
        // reconciliation (review finding 8) — never a guessed resend.
        let refused = services
            .submit_inline(deliver_envelope(&ticket, &message))
            .await
            .unwrap_err();
        assert_eq!(
            refused.code(),
            "ticketing.notification_reconciliation_required"
        );
        assert!(refused.is_permanent());
        assert_eq!(transport.submit_count().await, 1, "no blind resend");

        // The authoritative reconciliation: the operator confirmed the
        // provider never received the message.
        let service = crate::TicketingService::new(
            TicketingStoreService::new(store.clone()),
            crate::TicketingConfig {
                project_id: "project-a".into(),
                ..crate::TicketingConfig::default()
            },
        )
        .unwrap();
        service
            .reconcile_outbound_delivery(
                &worker_identity(),
                "project-a",
                ticket.id,
                message.id,
                false,
                "scripted",
                "provider-0",
                Utc::now(),
            )
            .await
            .unwrap();

        // The redrive resends under the same stable mail identity and
        // records acceptance; further redrives stay suppressed.
        services
            .submit_inline(deliver_envelope(&ticket, &message))
            .await
            .unwrap();
        assert_eq!(transport.submit_count().await, 2);
        let submitted = transport.submitted.lock().await.clone();
        assert_eq!(
            submitted[0].id, submitted[1].id,
            "the mail identity must be stable across sends"
        );
        services
            .submit_inline(deliver_envelope(&ticket, &message))
            .await
            .unwrap();
        assert_eq!(transport.submit_count().await, 2);
        let kinds = store
            .all_outbound_evidence()
            .await
            .into_iter()
            .map(|row| row.kind)
            .collect::<Vec<_>>();
        assert_eq!(
            kinds,
            vec![
                OutboundEvidenceKind::Ambiguous,
                OutboundEvidenceKind::PermanentFailure,
                OutboundEvidenceKind::Accepted
            ]
        );
    }

    #[tokio::test]
    async fn permanent_transport_failure_records_evidence_and_fails_permanently() {
        let (services, _transport, store, ticket, message) = mail_setup(vec![Err(mail_error(
            minco_plugin_notifications::MailErrorKind::Rejected,
        ))])
        .await;
        let failure = services
            .submit_inline(deliver_envelope(&ticket, &message))
            .await
            .unwrap_err();
        assert_eq!(failure.code(), "ticketing.notification_permanent");
        assert!(failure.is_permanent());
        let evidence = store.all_outbound_evidence().await;
        assert_eq!(evidence.len(), 1);
        assert_eq!(evidence[0].kind, OutboundEvidenceKind::PermanentFailure);
        assert_eq!(evidence[0].failure_kind.as_deref(), Some("rejected"));
    }

    #[tokio::test]
    async fn retryable_transport_failure_records_no_evidence() {
        // Transient attempts are the mail observer's story, not decision
        // evidence: nothing is recorded until a terminal fact exists.
        let (services, _transport, store, ticket, message) = mail_setup(vec![Err(mail_error(
            minco_plugin_notifications::MailErrorKind::Throttled,
        ))])
        .await;
        let failure = services
            .submit_inline(deliver_envelope(&ticket, &message))
            .await
            .unwrap_err();
        assert_eq!(failure.code(), "ticketing.notification_transport_retryable");
        assert!(!failure.is_permanent());
        assert!(store.all_outbound_evidence().await.is_empty());
    }

    #[tokio::test]
    async fn email_channel_without_mail_service_fails_closed() {
        let now = Utc::now();
        let mut ticket = ticket(now);
        let message = ticket
            .reply_as_agent_message("agent-1", "Your fix is live.", now)
            .unwrap();
        let store = Arc::new(MemoryTicketingStore::default());
        let intent = crate::TicketActivityIntent::new(
            "project-a",
            ticket.id,
            "created",
            Uuid::now_v7(),
            serde_json::json!({}),
            now,
        );
        store.create(ticket.clone(), intent).await.unwrap();
        let sink = Arc::new(MemoryNotificationSink::default());
        let registry = Arc::new(JobHandlerRegistry::new());
        register_ticketing_jobs(
            &registry,
            &TicketingStoreService::new(store.clone()),
            notification_deps(Arc::new(NotificationService::new(sink.clone())), None),
        )
        .unwrap();
        let services = minco_plugin_jobs::JobsServices::memory(registry).0;
        let failure = services
            .submit_inline(deliver_envelope(&ticket, &message))
            .await
            .unwrap_err();
        assert_eq!(failure.code(), "ticketing.notification_mail_unconfigured");
        assert!(failure.is_permanent());
        assert!(sink.all().await.is_empty());
        assert!(store.all_outbound_evidence().await.is_empty());
    }

    #[tokio::test]
    async fn missing_target_is_permanent_and_nothing_is_sent() {
        let store = Arc::new(MemoryTicketingStore::default());
        let sink = Arc::new(MemoryNotificationSink::default());
        let notifications = Arc::new(NotificationService::new(sink.clone()));
        let registry = Arc::new(JobHandlerRegistry::new());
        register_ticketing_jobs(
            &registry,
            &TicketingStoreService::new(store),
            notification_deps(notifications, None),
        )
        .unwrap();
        let services = minco_plugin_jobs::JobsServices::memory(registry).0;
        let now = Utc::now();
        let envelope = public_notification_envelope(
            &DeliverPublicNotification {
                project_id: "project-a".into(),
                ticket_id: TicketId::new(),
                message_id: TicketMessageId::new(),
            },
            Uuid::now_v7(),
            now,
        )
        .unwrap();
        let failure = services.submit_inline(envelope).await.unwrap_err();
        assert_eq!(failure.code(), "ticketing.notification_target_missing");
        assert!(failure.is_permanent());
        assert!(sink.all().await.is_empty());
    }

    const RAW_EMAIL: &str = "From: user-1@example.test\r\n\
        To: support@example.test\r\n\
        Subject: Re: Help\r\n\
        MIME-Version: 1.0\r\n\
        Content-Type: text/plain; charset=utf-8\r\n\
        \r\n\
        Reply from the mailbox.\r\n";

    async fn inbound_setup() -> (
        minco_plugin_jobs::JobsServices,
        Arc<minco_plugin_object_storage::MemoryObjectStore>,
        Arc<MemoryTicketingStore>,
        crate::TicketId,
        u64,
        String,
    ) {
        let memory = Arc::new(MemoryTicketingStore::default());
        let config = crate::TicketingConfig {
            project_id: "project-a".into(),
            ..crate::TicketingConfig::default()
        };
        let service =
            crate::TicketingService::new(TicketingStoreService::new(memory.clone()), config)
                .unwrap();
        let created = service
            .create_ticket(
                &minco_plugin_identity::Identity {
                    subject: "user-1".into(),
                    permissions: BTreeSet::from(["ticketing.create".into()]),
                    scopes: BTreeSet::new(),
                    claims: BTreeMap::new(),
                },
                crate::CreateTicketInput {
                    project_id: "project-a".into(),
                    subject: "Help".into(),
                    description: "It broke".into(),
                    requester: crate::TicketRequester {
                        subject: "user-1".into(),
                        display_name: None,
                        email: Some("user-1@example.test".into()),
                    },
                    channel: crate::TicketChannel::Email,
                    ticket_type: crate::TicketType::default(),
                    form_answers: Vec::new(),
                    priority: crate::TicketPriority::Normal,
                    resource_references: Vec::new(),
                },
                Uuid::now_v7(),
                Utc::now(),
            )
            .await
            .unwrap()
            .ticket;
        let objects = Arc::new(minco_plugin_object_storage::MemoryObjectStore::default());
        let digest = crate::external_content_sha256(RAW_EMAIL.as_bytes());
        objects
            .put(minco_plugin_object_storage::PutObject {
                key: minco_plugin_object_storage::ObjectKey::parse("mail/project-a/message-1")
                    .unwrap(),
                bytes: RAW_EMAIL.as_bytes().to_vec(),
                content_type: "message/rfc822".into(),
                attributes: BTreeMap::new(),
            })
            .await
            .unwrap();
        let registry = Arc::new(JobHandlerRegistry::new());
        register_ticketing_jobs(
            &registry,
            &TicketingStoreService::new(memory.clone()),
            TicketingJobsDeps {
                service: service.clone(),
                notifications: Arc::new(NotificationService::new(Arc::new(
                    MemoryNotificationSink::default(),
                ))),
                mail: None,
                objects: Arc::new(ObjectStoreService::new(objects.clone())),
                worker: worker_identity(),
            },
        )
        .unwrap();
        (
            minco_plugin_jobs::JobsServices::memory(registry).0,
            objects,
            memory,
            created.id,
            created.revision,
            digest,
        )
    }

    fn inbound_command(ticket_id: TicketId, digest: &str) -> ProcessInboundEmail {
        ProcessInboundEmail {
            project_id: "project-a".into(),
            provider: "ses".into(),
            mailbox_scope: "support@example.test".into(),
            external_id: "message-1".into(),
            content_sha256: digest.into(),
            raw_object_key: "mail/project-a/message-1".into(),
            ticket_id: Some(ticket_id),
            internet_message_id: Some("<message-1@example.test>".into()),
            in_reply_to: None,
            references: Vec::new(),
            subject: Some("Re: Help".into()),
        }
    }

    #[tokio::test]
    async fn inbound_email_is_verified_parsed_and_ingested_idempotently() {
        let (services, _objects, memory, ticket_id, revision, digest) = inbound_setup().await;
        let correlation = Uuid::now_v7();
        let envelope = inbound_email_envelope(
            &inbound_command(ticket_id, &digest),
            correlation,
            Utc::now(),
        )
        .unwrap();
        services.submit_inline(envelope).await.unwrap();
        let ticket = memory.get("project-a", ticket_id).await.unwrap().unwrap();
        assert!(
            ticket
                .messages
                .iter()
                .any(|message| message.body.contains("Reply from the mailbox."))
        );
        assert_eq!(ticket.revision, revision + 1);

        // Same external identity replays without a second message.
        let envelope = inbound_email_envelope(
            &inbound_command(ticket_id, &digest),
            correlation,
            Utc::now(),
        )
        .unwrap();
        services.submit_inline(envelope).await.unwrap();
        let ticket = memory.get("project-a", ticket_id).await.unwrap().unwrap();
        assert_eq!(
            ticket
                .messages
                .iter()
                .filter(|message| message.body.contains("Reply from the mailbox."))
                .count(),
            1
        );
    }

    #[tokio::test]
    async fn digest_mismatch_is_permanent_and_nothing_is_ingested() {
        let (services, _objects, _memory, ticket_id, _revision, _digest) = inbound_setup().await;
        let envelope = inbound_email_envelope(
            &inbound_command(ticket_id, &"f".repeat(64)),
            Uuid::now_v7(),
            Utc::now(),
        )
        .unwrap();
        let failure = services.submit_inline(envelope).await.unwrap_err();
        assert_eq!(failure.code(), "ticketing.inbound_digest_mismatch");
        assert!(failure.is_permanent());
    }

    #[tokio::test]
    async fn missing_object_is_permanent() {
        let (services, objects, _memory, ticket_id, _revision, digest) = inbound_setup().await;
        objects
            .delete(
                &minco_plugin_object_storage::ObjectKey::parse("mail/project-a/message-1").unwrap(),
            )
            .await
            .unwrap();
        let envelope = inbound_email_envelope(
            &inbound_command(ticket_id, &digest),
            Uuid::now_v7(),
            Utc::now(),
        )
        .unwrap();
        let failure = services.submit_inline(envelope).await.unwrap_err();
        assert_eq!(failure.code(), "ticketing.inbound_object_missing");
        assert!(failure.is_permanent());
    }

    #[tokio::test]
    async fn unparseable_mime_is_permanent() {
        let (services, objects, _memory, ticket_id, _revision, _digest) = inbound_setup().await;
        let garbage = b"\x00\x01\x02 not mime at all \xff\xfe".to_vec();
        objects
            .put(minco_plugin_object_storage::PutObject {
                key: minco_plugin_object_storage::ObjectKey::parse("mail/project-a/message-1")
                    .unwrap(),
                bytes: garbage.clone(),
                content_type: "application/octet-stream".into(),
                attributes: BTreeMap::new(),
            })
            .await
            .unwrap();
        let digest = crate::external_content_sha256(&garbage);
        let envelope = inbound_email_envelope(
            &inbound_command(ticket_id, &digest),
            Uuid::now_v7(),
            Utc::now(),
        )
        .unwrap();
        let failure = services.submit_inline(envelope).await.unwrap_err();
        assert!(failure.is_permanent(), "unexpected code {}", failure.code());
    }

    #[test]
    fn structural_auth_parsing_rejects_forgery_and_ambiguity() {
        use super::evaluate_inbound_trust;
        // A trusted SPF pass with aligned mailfrom passes strict SPF.
        assert!(
            evaluate_inbound_trust(
                &["amazonses.com; spf=pass smtp.mailfrom=example.test".into()],
                Some("PASS"),
                Some("PASS"),
                Some("example.test"),
                "amazonses.com",
                crate::InboundAuthPolicy::RequireAlignedSpf,
            )
            .is_ok()
        );
        // A foreign authserv-id is attacker-forged: never trusted, so a
        // strict policy quarantines despite the 'pass'.
        assert!(
            evaluate_inbound_trust(
                &["attacker.example; spf=pass smtp.mailfrom=example.test".into()],
                None,
                None,
                Some("example.test"),
                "amazonses.com",
                crate::InboundAuthPolicy::RequireAlignedSpf,
            )
            .is_err()
        );
        // A forged foreign header plus one trusted header: only the
        // trusted one counts.
        assert!(
            evaluate_inbound_trust(
                &[
                    "attacker.example; spf=fail".into(),
                    "amazonses.com; spf=pass smtp.mailfrom=example.test".into(),
                ],
                None,
                None,
                Some("example.test"),
                "amazonses.com",
                crate::InboundAuthPolicy::RequireAlignedSpf,
            )
            .is_ok()
        );
        // Two TRUSTED headers are ambiguous evidence.
        assert!(
            evaluate_inbound_trust(
                &[
                    "amazonses.com; spf=pass smtp.mailfrom=example.test".into(),
                    "amazonses.com; spf=pass smtp.mailfrom=example.test".into(),
                ],
                None,
                None,
                Some("example.test"),
                "amazonses.com",
                crate::InboundAuthPolicy::RequireAlignedSpf,
            )
            .is_err()
        );
        // Misaligned authenticated domain fails a strict policy.
        assert!(
            evaluate_inbound_trust(
                &["amazonses.com; dkim=pass (2048-bit) header.d=other.example".into()],
                None,
                None,
                Some("example.test"),
                "amazonses.com",
                crate::InboundAuthPolicy::RequireAlignedDkim,
            )
            .is_err()
        );
        // Aligned subdomain passes.
        assert!(
            evaluate_inbound_trust(
                &["amazonses.com; dkim=pass header.d=mail.example.test".into()],
                None,
                None,
                Some("example.test"),
                "amazonses.com",
                crate::InboundAuthPolicy::RequireAlignedDkim,
            )
            .is_ok()
        );
        // GRAY and PROCESSING_FAILED quarantine; missing evidence fails
        // strict policies but passes LocalTrusted.
        assert!(
            evaluate_inbound_trust(
                &["amazonses.com; dmarc=gray".into()],
                None,
                None,
                Some("example.test"),
                "amazonses.com",
                crate::InboundAuthPolicy::LocalTrusted,
            )
            .is_err()
        );
        assert!(
            evaluate_inbound_trust(
                &[],
                None,
                None,
                Some("example.test"),
                "amazonses.com",
                crate::InboundAuthPolicy::LocalTrusted,
            )
            .is_ok()
        );
        assert!(
            evaluate_inbound_trust(
                &[],
                None,
                None,
                Some("example.test"),
                "amazonses.com",
                crate::InboundAuthPolicy::RequireDmarc,
            )
            .is_err()
        );
        // SES spam verdict FAIL quarantines under every policy.
        assert!(
            evaluate_inbound_trust(
                &[],
                Some("FAIL"),
                None,
                Some("example.test"),
                "amazonses.com",
                crate::InboundAuthPolicy::LocalTrusted,
            )
            .is_err()
        );
        // A malformed header from an UNTRUSTED authserv-id is ignored,
        // never allowed to poison trusted provider evidence.
        assert!(
            evaluate_inbound_trust(
                &["no-semicolon-value".into()],
                None,
                None,
                Some("example.test"),
                "amazonses.com",
                crate::InboundAuthPolicy::LocalTrusted,
            )
            .is_ok()
        );
        // A malformed header CLAIMING the trusted authserv-id cannot be
        // verified and quarantines.
        assert!(
            evaluate_inbound_trust(
                &["amazonses.com no clause separator".into()],
                None,
                None,
                Some("example.test"),
                "amazonses.com",
                crate::InboundAuthPolicy::LocalTrusted,
            )
            .is_err()
        );
    }

    #[test]
    fn automation_dedupe_identity_includes_the_run_id() {
        // Exact-head review R18: two explicit runs by the same requester
        // on the same revision/context are distinct jobs — the unique
        // run id rides the dedupe identity, so the second run is never
        // swallowed as a duplicate of the first.
        let base = RunDevelopmentAutomation {
            project_id: "project-a".into(),
            ticket_id: TicketId::new(),
            requested_by: "agent-1".into(),
            bound_revision: 3,
            bound_context_digest: Some("a".repeat(64)),
            run_id: Uuid::new_v4(),
        };
        let mut second = base.clone();
        second.run_id = Uuid::new_v4();
        let first = development_automation_envelope(&base, Uuid::now_v7(), Utc::now()).unwrap();
        let second_envelope =
            development_automation_envelope(&second, Uuid::now_v7(), Utc::now()).unwrap();
        assert_ne!(
            first.dedupe_key, second_envelope.dedupe_key,
            "an explicit second run is a distinct job"
        );
        let replay = development_automation_envelope(&base, Uuid::now_v7(), Utc::now()).unwrap();
        assert_eq!(
            first.dedupe_key, replay.dedupe_key,
            "an identical submission still dedupes"
        );
    }

    #[tokio::test]
    async fn stale_automation_run_is_superseded_not_proposed() {
        // Exact-head review R8: the handler compares the bound revision
        // with the authoritative ticket; stale work classifies as
        // superseded instead of storing a proposal over changed reality.
        // Compose a service with automation enabled and the handler
        // registered around one seeded ticket.
        let memory = Arc::new(MemoryTicketingStore::default());
        let now = Utc::now();
        let ticket = ticket(now);
        let intent = crate::TicketActivityIntent::new(
            "project-a",
            ticket.id,
            "created",
            Uuid::now_v7(),
            serde_json::json!({}),
            now,
        );
        TicketingStoreService::new(memory.clone())
            .create(ticket.clone(), intent)
            .await
            .unwrap();
        let service = Arc::new(
            TicketingService::new(
                TicketingStoreService::new(memory.clone()),
                crate::TicketingConfig {
                    project_id: "project-a".into(),
                    automation: crate::AutomationConfig {
                        profile: crate::AutomationProfile::Assist,
                        ..crate::AutomationConfig::default()
                    },
                    ..crate::TicketingConfig::default()
                },
            )
            .unwrap(),
        );
        let registry = Arc::new(minco_plugin_jobs::JobHandlerRegistry::new());
        register_ticketing_jobs(
            &registry,
            &TicketingStoreService::new(memory.clone()),
            TicketingJobsDeps {
                service: TicketingService::clone(&service),
                notifications: Arc::new(NotificationService::new(Arc::new(
                    MemoryNotificationSink::default(),
                ))),
                mail: None,
                objects: Arc::new(minco_plugin_object_storage::ObjectStoreService::new(
                    Arc::new(minco_plugin_object_storage::MemoryObjectStore::default()),
                )),
                worker: worker_identity(),
            },
        )
        .unwrap();
        let services = minco_plugin_jobs::JobsServices::memory(registry).0;
        let envelope = development_automation_envelope(
            &RunDevelopmentAutomation {
                project_id: "project-a".into(),
                ticket_id: ticket.id,
                requested_by: "agent-1".into(),
                bound_revision: ticket.revision + 5,
                bound_context_digest: None,
                run_id: Uuid::new_v4(),
            },
            Uuid::now_v7(),
            Utc::now(),
        )
        .unwrap();
        let failure = services.submit_inline(envelope).await.unwrap_err();
        assert_eq!(failure.code(), "ticketing.automation_superseded");
        assert!(failure.is_permanent());
        assert!(
            TicketingStoreService::new(memory)
                .list_automation_proposals("project-a", ticket.id)
                .await
                .unwrap()
                .is_empty(),
            "no proposal is stored for stale work"
        );
    }

    #[test]
    fn real_aws_ses_authentication_results_fixture_passes_strict_policies() {
        // Exact-head review R15: the byte-accurate AWS SES receiving
        // header shape — comments with spaces, envelope-from,
        // header.i, header.from — parses and aligns.
        let ses_header = concat!(
            "amazonses.com; ",
            "spf=pass (spfCheck: domain of alice@example.com designates 10.0.0.1 as permitted sender) ",
            "client-ip=10.0.0.1; ",
            "envelope-from=\"alice@example.com\"; ",
            "dkim=pass (2048-bit key) header.i=@example.com; ",
            "dmarc=pass (p=quarantine dis=none) header.from=example.com"
        );
        // SPF alignment via envelope-from (mailbox -> domain).
        assert!(
            super::evaluate_inbound_trust(
                &[ses_header.into()],
                Some("PASS"),
                Some("PASS"),
                Some("example.com"),
                "amazonses.com",
                crate::InboundAuthPolicy::RequireAlignedSpf,
            )
            .is_ok()
        );
        // DKIM alignment via header.i.
        assert!(
            super::evaluate_inbound_trust(
                &[ses_header.into()],
                None,
                None,
                Some("example.com"),
                "amazonses.com",
                crate::InboundAuthPolicy::RequireAlignedDkim,
            )
            .is_ok()
        );
        // DMARC alignment via header.from (RFC 7489).
        assert!(
            super::evaluate_inbound_trust(
                &[ses_header.into()],
                None,
                None,
                Some("example.com"),
                "amazonses.com",
                crate::InboundAuthPolicy::RequireDmarc,
            )
            .is_ok()
        );
        // A misaligned From domain fails every strict policy.
        assert!(
            super::evaluate_inbound_trust(
                &[ses_header.into()],
                None,
                None,
                Some("other.example"),
                "amazonses.com",
                crate::InboundAuthPolicy::RequireDmarc,
            )
            .is_err()
        );
        // Malformed UNTRUSTED junk next to the trusted header is
        // ignored; the trusted evidence still decides.
        assert!(
            super::evaluate_inbound_trust(
                &["garbage header (no equals)".into(), ses_header.into(),],
                None,
                None,
                Some("example.com"),
                "amazonses.com",
                crate::InboundAuthPolicy::RequireDmarc,
            )
            .is_ok()
        );
    }

    #[test]
    fn cross_result_assembly_cannot_validate_a_mismatched_dkim_pass() {
        // Exact-head review R23/P0-4: AWS SES emits MULTIPLE DKIM
        // results in one header. A dkim=none for the victim domain and
        // dkim=pass for the attacker domain must NOT be combined into
        // "a pass with the victim's header.i".
        let header = concat!(
            "amazonses.com; ",
            "dkim=none (no signatures) header.i=@victim.example; ",
            "dkim=pass (2048-bit key) header.i=@attacker.example"
        );
        // The victim From domain: only the attacker's DKIM passed, and
        // ITS header.i is attacker.example — not aligned.
        assert!(
            super::evaluate_inbound_trust(
                &[header.into()],
                None,
                None,
                Some("victim.example"),
                "amazonses.com",
                crate::InboundAuthPolicy::RequireAlignedDkim,
            )
            .is_err()
        );
        // The attacker From domain: their pass IS aligned — this is the
        // correct accept, but for the attacker's mail, not the victim's.
        assert!(
            super::evaluate_inbound_trust(
                &[header.into()],
                None,
                None,
                Some("attacker.example"),
                "amazonses.com",
                crate::InboundAuthPolicy::RequireAlignedDkim,
            )
            .is_ok()
        );
    }

    #[test]
    fn dmarc_requires_header_from_not_header_d() {
        // Exact-head review R23/P0-4: DMARC's registry property is
        // header.from (RFC 7489); a header.d fallback must not exist.
        let header_only_d = "amazonses.com; dmarc=pass header.d=attacker.example";
        assert!(
            super::evaluate_inbound_trust(
                &[header_only_d.into()],
                None,
                None,
                Some("attacker.example"),
                "amazonses.com",
                crate::InboundAuthPolicy::RequireDmarc,
            )
            .is_err(),
            "header.d is not a DMARC identity — no fallback"
        );
        let header_from = "amazonses.com; dmarc=pass header.from=attacker.example";
        assert!(
            super::evaluate_inbound_trust(
                &[header_from.into()],
                None,
                None,
                Some("attacker.example"),
                "amazonses.com",
                crate::InboundAuthPolicy::RequireDmarc,
            )
            .is_ok()
        );
    }

    #[test]
    fn gray_and_processing_failed_spam_verdicts_quarantine() {
        // Exact-head review R23/P0-4: indeterminate SES scan outcomes
        // must not silently pass under any policy.
        for verdict in ["GRAY", "PROCESSING_FAILED"] {
            assert!(
                super::evaluate_inbound_trust(
                    &[],
                    Some(verdict),
                    None,
                    Some("example.com"),
                    "amazonses.com",
                    crate::InboundAuthPolicy::LocalTrusted,
                )
                .is_err(),
                "spam verdict {verdict} must quarantine"
            );
            assert!(
                super::evaluate_inbound_trust(
                    &[],
                    None,
                    Some(verdict),
                    Some("example.com"),
                    "amazonses.com",
                    crate::InboundAuthPolicy::LocalTrusted,
                )
                .is_err(),
                "virus verdict {verdict} must quarantine"
            );
        }
    }

    #[test]
    fn inbound_envelope_carries_the_contract_policies() {
        let payload = inbound_command(TicketId::new(), &"a".repeat(64));
        let envelope = inbound_email_envelope(&payload, Uuid::now_v7(), Utc::now()).unwrap();
        assert_eq!(envelope.job_name, ProcessInboundEmail::NAME);
        assert_eq!(envelope.worker_profile, TICKETING_MAIL_PROFILE);
        assert!(
            envelope
                .dedupe_key
                .as_deref()
                .is_some_and(|k| k.starts_with("mail:"))
        );
        assert!(
            envelope
                .overlap_key
                .as_deref()
                .is_some_and(|k| k.starts_with("mailbox:"))
        );
        assert_eq!(envelope.partition.as_deref(), Some("project-a"));
        assert!(envelope.deadline.is_some());
        let encoded = serde_json::to_string(&payload).unwrap();
        assert!(!encoded.contains("Reply from the mailbox"));
    }

    const STRANGER_EMAIL: &str = "From: stranger@example.test\r\n\
        To: support@example.test\r\n\
        Subject: Re: Help\r\n\
        \r\n\
        Let me into this thread.\r\n";

    const FIRST_CONTACT_EMAIL: &str = "From: new-person@example.test\r\n\
        To: support@example.test\r\n\
        Subject: The dashboard will not load\r\n\
        \r\n\
        It shows a blank page since this morning.\r\n";

    #[tokio::test]
    async fn stranger_reply_is_quarantined_never_appended() {
        // Review finding 6: knowing the thread identity is not enough —
        // the From address must be the ticket's requester participant.
        let (services, objects, memory, ticket_id, _revision, _digest) = inbound_setup().await;
        let stranger_digest = crate::external_content_sha256(STRANGER_EMAIL.as_bytes());
        objects
            .put(minco_plugin_object_storage::PutObject {
                key: minco_plugin_object_storage::ObjectKey::parse("mail/project-a/message-1")
                    .unwrap(),
                bytes: STRANGER_EMAIL.as_bytes().to_vec(),
                content_type: "message/rfc822".into(),
                attributes: BTreeMap::new(),
            })
            .await
            .unwrap();
        let envelope = inbound_email_envelope(
            &ProcessInboundEmail {
                content_sha256: stranger_digest,
                ..inbound_command_shape(ticket_id)
            },
            Uuid::now_v7(),
            Utc::now(),
        )
        .unwrap();
        let failure = services.submit_inline(envelope).await.unwrap_err();
        assert_eq!(failure.code(), "ticketing.inbound_sender_unverified");
        assert!(failure.is_permanent());
        let ticket = memory.get("project-a", ticket_id).await.unwrap().unwrap();
        assert!(
            !ticket
                .messages
                .iter()
                .any(|message| message.body.contains("Let me into this thread.")),
            "a quarantined stranger reply must never append"
        );
    }

    #[tokio::test]
    async fn failed_authentication_verdict_is_quarantined() {
        let (services, objects, _memory, ticket_id, _revision, _digest) = inbound_setup().await;
        let failed = concat!(
            "From: user-1@example.test\r\nTo: support@example.test\r\n",
            "Subject: Re: Help\r\nAuthentication-Results: amazonses.com; spf=fail; dkim=pass\r\n",
            "\r\nReply from the mailbox.\r\n"
        );
        objects
            .put(minco_plugin_object_storage::PutObject {
                key: minco_plugin_object_storage::ObjectKey::parse("mail/project-a/message-1")
                    .unwrap(),
                bytes: failed.as_bytes().to_vec(),
                content_type: "message/rfc822".into(),
                attributes: BTreeMap::new(),
            })
            .await
            .unwrap();
        let digest = crate::external_content_sha256(failed.as_bytes());
        let envelope = inbound_email_envelope(
            &ProcessInboundEmail {
                content_sha256: digest,
                ..inbound_command_shape(ticket_id)
            },
            Uuid::now_v7(),
            Utc::now(),
        )
        .unwrap();
        let failure = services.submit_inline(envelope).await.unwrap_err();
        assert_eq!(failure.code(), "ticketing.inbound_sender_unverified");
        assert!(failure.is_permanent());
    }

    #[tokio::test]
    async fn first_contact_email_creates_a_ticket_for_the_verified_sender() {
        // Review finding 6: unthreaded mail from a sender with no failed
        // verdicts creates a ticket instead of being rejected.
        let (services, objects, memory, ticket_id, _revision, _digest) = inbound_setup().await;
        let digest = crate::external_content_sha256(FIRST_CONTACT_EMAIL.as_bytes());
        objects
            .put(minco_plugin_object_storage::PutObject {
                key: minco_plugin_object_storage::ObjectKey::parse("mail/project-a/message-1")
                    .unwrap(),
                bytes: FIRST_CONTACT_EMAIL.as_bytes().to_vec(),
                content_type: "message/rfc822".into(),
                attributes: BTreeMap::new(),
            })
            .await
            .unwrap();
        let envelope = inbound_email_envelope(
            &ProcessInboundEmail {
                content_sha256: digest,
                ticket_id: None,
                subject: Some("The dashboard will not load".into()),
                ..inbound_command_shape(ticket_id)
            },
            Uuid::now_v7(),
            Utc::now(),
        )
        .unwrap();
        services.submit_inline(envelope).await.unwrap();
        let created = memory
            .list(crate::TicketListFilter {
                project_id: "project-a".into(),
                limit: 10,
                ..Default::default()
            })
            .await
            .unwrap()
            .into_iter()
            .find(|ticket| ticket.requester.email.as_deref() == Some("new-person@example.test"))
            .expect("the first-contact ticket exists");
        assert_eq!(created.requester.subject, "new-person@example.test");
        assert_eq!(created.subject, "The dashboard will not load");
        assert_eq!(
            created.description,
            "It shows a blank page since this morning."
        );
        assert_eq!(
            serde_json::to_string(&created.channel).unwrap(),
            "\"email\""
        );
    }

    fn inbound_command_shape(ticket_id: TicketId) -> ProcessInboundEmail {
        ProcessInboundEmail {
            project_id: "project-a".into(),
            provider: "ses".into(),
            mailbox_scope: "support@example.test".into(),
            external_id: "message-1".into(),
            content_sha256: "0".repeat(64),
            raw_object_key: "mail/project-a/message-1".into(),
            ticket_id: Some(ticket_id),
            internet_message_id: Some("<message-1@example.test>".into()),
            in_reply_to: None,
            references: Vec::new(),
            subject: Some("Re: Help".into()),
        }
    }

    #[tokio::test]
    async fn inbound_append_converges_after_concurrent_revision_movement() {
        let (services, _objects, memory, ticket_id, revision, digest) = inbound_setup().await;
        // Move the ticket forward after the command was frozen — the old
        // behavior retried an immutable stale revision forever (review
        // finding 7); revision-free ingress must converge on the append.
        let mut moved = memory.get("project-a", ticket_id).await.unwrap().unwrap();
        moved.change_priority(crate::TicketPriority::High, Utc::now());
        let intent = crate::TicketActivityIntent::new(
            "project-a",
            moved.id,
            "changed",
            Uuid::now_v7(),
            serde_json::json!({}),
            Utc::now(),
        );
        memory.save(moved, revision, intent).await.unwrap();
        let envelope = inbound_email_envelope(
            &inbound_command(ticket_id, &digest),
            Uuid::now_v7(),
            Utc::now(),
        )
        .unwrap();
        services.submit_inline(envelope).await.unwrap();
        let ticket = memory.get("project-a", ticket_id).await.unwrap().unwrap();
        assert!(
            ticket
                .messages
                .iter()
                .any(|message| message.body.contains("Reply from the mailbox.")),
            "the reply must land on the moved ticket"
        );
        assert_eq!(ticket.revision, revision + 2);
    }
}
