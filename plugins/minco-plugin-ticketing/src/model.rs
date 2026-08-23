use chrono::{DateTime, Utc};
use minco_interaction::{
    AttachmentKind, AttachmentMetadata, SupportResourceReference, TransitionRule,
    transition_allowed,
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, fmt, str::FromStr};
use uuid::Uuid;

macro_rules! ticket_id {
    ($name:ident) => {
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(pub Uuid);

        impl $name {
            #[must_use]
            pub fn new() -> Self {
                Self(Uuid::now_v7())
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(formatter)
            }
        }

        impl FromStr for $name {
            type Err = uuid::Error;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                value.parse().map(Self)
            }
        }
    };
}

ticket_id!(TicketId);
ticket_id!(TicketMessageId);
ticket_id!(TicketAttachmentId);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TicketPriority {
    Low,
    #[default]
    Normal,
    High,
    Urgent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TicketChannel {
    Portal,
    Email,
    Api,
    Voice,
    Internal,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TicketStatus {
    New,
    Open,
    PendingRequester,
    PendingInternal,
    OnHold,
    Resolved,
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TicketClockState {
    Open,
    Paused,
    Stopped,
}

impl TicketStatus {
    #[must_use]
    pub const fn clock_state(self) -> TicketClockState {
        match self {
            Self::New | Self::Open | Self::PendingInternal => TicketClockState::Open,
            Self::PendingRequester | Self::OnHold => TicketClockState::Paused,
            Self::Resolved | Self::Closed => TicketClockState::Stopped,
        }
    }

    #[must_use]
    pub fn can_transition_to(self, target: Self) -> bool {
        use TicketStatus::{
            Closed, New, OnHold, Open, PendingInternal, PendingRequester, Resolved,
        };
        const RULES: &[TransitionRule<TicketStatus>] = &[
            TransitionRule::new(New, Open),
            TransitionRule::new(New, PendingRequester),
            TransitionRule::new(New, PendingInternal),
            TransitionRule::new(New, OnHold),
            TransitionRule::new(New, Resolved),
            TransitionRule::new(Open, PendingRequester),
            TransitionRule::new(Open, PendingInternal),
            TransitionRule::new(Open, OnHold),
            TransitionRule::new(Open, Resolved),
            TransitionRule::new(PendingRequester, Open),
            TransitionRule::new(PendingRequester, PendingInternal),
            TransitionRule::new(PendingRequester, OnHold),
            TransitionRule::new(PendingRequester, Resolved),
            TransitionRule::new(PendingInternal, Open),
            TransitionRule::new(PendingInternal, PendingRequester),
            TransitionRule::new(PendingInternal, OnHold),
            TransitionRule::new(PendingInternal, Resolved),
            TransitionRule::new(OnHold, Open),
            TransitionRule::new(OnHold, PendingRequester),
            TransitionRule::new(OnHold, PendingInternal),
            TransitionRule::new(OnHold, Resolved),
            TransitionRule::new(Resolved, Open),
            TransitionRule::new(Resolved, Closed),
            TransitionRule::new(Closed, Open),
        ];
        transition_allowed(&self, &target, RULES)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TicketMessageKind {
    PublicReply,
    InternalNote,
    SystemEvent,
    VoiceTranscript,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TicketMessageDirection {
    Inbound,
    Outbound,
    Internal,
    System,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TicketRequester {
    pub subject: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TicketSourceReference {
    pub provider: String,
    pub scope: String,
    pub external_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TicketMessage {
    pub id: TicketMessageId,
    pub kind: TicketMessageKind,
    pub direction: TicketMessageDirection,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author_subject: Option<String>,
    pub body: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TicketAttachment {
    pub id: TicketAttachmentId,
    pub kind: AttachmentKind,
    pub object_key: String,
    pub file_name: String,
    pub content_type: String,
    pub size_bytes: u64,
    pub sha256: String,
    pub created_at: DateTime<Utc>,
}

impl From<AttachmentMetadata> for TicketAttachment {
    fn from(value: AttachmentMetadata) -> Self {
        Self {
            id: TicketAttachmentId(value.id),
            kind: value.kind,
            object_key: value.object_key.as_str().to_owned(),
            file_name: value.file_name,
            content_type: value.content_type,
            size_bytes: value.size_bytes,
            sha256: value.sha256,
            created_at: value.created_at,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ticket {
    pub id: TicketId,
    pub project_id: String,
    pub display_reference: String,
    pub subject: String,
    pub description: String,
    pub requester: TicketRequester,
    pub channel: TicketChannel,
    pub priority: TicketPriority,
    pub status: TicketStatus,
    pub clock_state: TicketClockState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub queue_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assignee_subject: Option<String>,
    #[serde(default)]
    pub followers: BTreeSet<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(default)]
    pub tags: BTreeSet<String>,
    #[serde(default)]
    pub source_references: Vec<TicketSourceReference>,
    #[serde(default)]
    pub resource_references: Vec<SupportResourceReference>,
    #[serde(default)]
    pub messages: Vec<TicketMessage>,
    #[serde(default)]
    pub attachments: Vec<TicketAttachment>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_public_response_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub waiting_since: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub closed_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolution: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub close_reason: Option<String>,
    pub revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateTicketInput {
    pub project_id: String,
    pub subject: String,
    pub description: String,
    pub requester: TicketRequester,
    pub channel: TicketChannel,
    #[serde(default)]
    pub priority: TicketPriority,
    #[serde(default)]
    pub resource_references: Vec<SupportResourceReference>,
}

impl Ticket {
    pub fn create(
        input: CreateTicketInput,
        display_reference: impl Into<String>,
        now: DateTime<Utc>,
    ) -> Result<Self, TicketValidationError> {
        validate_text("project_id", &input.project_id, 100)?;
        validate_text("subject", &input.subject, 300)?;
        validate_text("description", &input.description, 20_000)?;
        validate_text("requester.subject", &input.requester.subject, 300)?;
        validate_optional_text(
            "requester.display_name",
            input.requester.display_name.as_deref(),
            300,
        )?;
        validate_optional_text("requester.email", input.requester.email.as_deref(), 320)?;
        let display_reference = display_reference.into();
        validate_text("display_reference", &display_reference, 100)?;
        if input.resource_references.len() > 32 {
            return Err(TicketValidationError::InvalidField {
                field: "resource_references",
                detail: "must not contain more than 32 values".into(),
            });
        }
        let id = TicketId::new();
        let mut ticket = Self {
            id,
            project_id: input.project_id,
            display_reference,
            subject: input.subject,
            description: input.description.clone(),
            requester: input.requester,
            channel: input.channel,
            priority: input.priority,
            status: TicketStatus::New,
            clock_state: TicketClockState::Open,
            queue_id: None,
            assignee_subject: None,
            followers: BTreeSet::new(),
            category: None,
            tags: BTreeSet::new(),
            source_references: Vec::new(),
            resource_references: input.resource_references,
            messages: Vec::new(),
            attachments: Vec::new(),
            created_at: now,
            updated_at: now,
            first_public_response_at: None,
            waiting_since: None,
            resolved_at: None,
            closed_at: None,
            resolution: None,
            close_reason: None,
            revision: 0,
        };
        ticket.messages.push(TicketMessage {
            id: TicketMessageId::new(),
            kind: TicketMessageKind::PublicReply,
            direction: TicketMessageDirection::Inbound,
            author_subject: Some(ticket.requester.subject.clone()),
            body: input.description,
            created_at: now,
        });
        Ok(ticket)
    }

    pub fn reply_as_requester(
        &mut self,
        body: impl Into<String>,
        now: DateTime<Utc>,
    ) -> Result<(), TicketValidationError> {
        let body = body.into();
        validate_text("body", &body, 20_000)?;
        if matches!(
            self.status,
            TicketStatus::PendingRequester | TicketStatus::Resolved | TicketStatus::Closed
        ) {
            self.apply_status(TicketStatus::Open, None, None, now)?;
        }
        self.messages.push(TicketMessage {
            id: TicketMessageId::new(),
            kind: TicketMessageKind::PublicReply,
            direction: TicketMessageDirection::Inbound,
            author_subject: Some(self.requester.subject.clone()),
            body,
            created_at: now,
        });
        self.touch(now);
        Ok(())
    }

    pub fn reply_as_agent(
        &mut self,
        actor_subject: impl Into<String>,
        body: impl Into<String>,
        now: DateTime<Utc>,
    ) -> Result<(), TicketValidationError> {
        let actor_subject = actor_subject.into();
        let body = body.into();
        validate_text("actor_subject", &actor_subject, 300)?;
        validate_text("body", &body, 20_000)?;
        self.first_public_response_at.get_or_insert(now);
        if self.status == TicketStatus::New {
            self.apply_status(TicketStatus::Open, None, None, now)?;
        }
        self.messages.push(TicketMessage {
            id: TicketMessageId::new(),
            kind: TicketMessageKind::PublicReply,
            direction: TicketMessageDirection::Outbound,
            author_subject: Some(actor_subject),
            body,
            created_at: now,
        });
        self.touch(now);
        Ok(())
    }

    pub fn add_internal_note(
        &mut self,
        actor_subject: impl Into<String>,
        body: impl Into<String>,
        now: DateTime<Utc>,
    ) -> Result<(), TicketValidationError> {
        let actor_subject = actor_subject.into();
        let body = body.into();
        validate_text("actor_subject", &actor_subject, 300)?;
        validate_text("body", &body, 20_000)?;
        self.messages.push(TicketMessage {
            id: TicketMessageId::new(),
            kind: TicketMessageKind::InternalNote,
            direction: TicketMessageDirection::Internal,
            author_subject: Some(actor_subject),
            body,
            created_at: now,
        });
        self.touch(now);
        Ok(())
    }

    pub fn change_status(
        &mut self,
        target: TicketStatus,
        resolution: Option<String>,
        close_reason: Option<String>,
        now: DateTime<Utc>,
    ) -> Result<(), TicketValidationError> {
        self.apply_status(target, resolution, close_reason, now)?;
        self.messages.push(TicketMessage {
            id: TicketMessageId::new(),
            kind: TicketMessageKind::SystemEvent,
            direction: TicketMessageDirection::System,
            author_subject: None,
            body: format!("status changed to {target:?}").to_ascii_lowercase(),
            created_at: now,
        });
        self.touch(now);
        Ok(())
    }

    fn apply_status(
        &mut self,
        target: TicketStatus,
        resolution: Option<String>,
        close_reason: Option<String>,
        now: DateTime<Utc>,
    ) -> Result<(), TicketValidationError> {
        if !self.status.can_transition_to(target) {
            return Err(TicketValidationError::InvalidTransition {
                current: self.status,
                target,
            });
        }
        if let Some(value) = resolution.as_deref() {
            validate_text("resolution", value, 20_000)?;
        }
        if let Some(value) = close_reason.as_deref() {
            validate_text("close_reason", value, 500)?;
        }
        if target == TicketStatus::Resolved
            && resolution.as_ref().or(self.resolution.as_ref()).is_none()
        {
            return Err(TicketValidationError::ResolutionRequired);
        }
        if target == TicketStatus::Closed {
            if resolution.as_ref().or(self.resolution.as_ref()).is_none() {
                return Err(TicketValidationError::ResolutionRequired);
            }
            if close_reason.is_none() {
                return Err(TicketValidationError::CloseReasonRequired);
            }
        }
        self.status = target;
        self.clock_state = target.clock_state();
        self.waiting_since = matches!(
            target,
            TicketStatus::PendingRequester | TicketStatus::OnHold
        )
        .then_some(now);
        if target == TicketStatus::Resolved {
            self.resolved_at = Some(now);
            self.resolution = resolution.clone().or_else(|| self.resolution.clone());
        } else if !matches!(target, TicketStatus::Closed) {
            self.resolved_at = None;
            if target == TicketStatus::Open {
                self.closed_at = None;
                self.close_reason = None;
            }
        }
        if target == TicketStatus::Closed {
            self.closed_at = Some(now);
            self.resolution = resolution.or_else(|| self.resolution.clone());
            self.close_reason = close_reason;
        }
        Ok(())
    }

    pub fn assign(
        &mut self,
        subject: Option<String>,
        now: DateTime<Utc>,
    ) -> Result<(), TicketValidationError> {
        validate_optional_text("assignee_subject", subject.as_deref(), 300)?;
        self.assignee_subject = subject;
        self.touch(now);
        Ok(())
    }

    pub fn transfer_queue(
        &mut self,
        queue_id: String,
        now: DateTime<Utc>,
    ) -> Result<(), TicketValidationError> {
        validate_text("queue_id", &queue_id, 200)?;
        self.queue_id = Some(queue_id);
        self.touch(now);
        Ok(())
    }

    pub const fn change_priority(&mut self, priority: TicketPriority, now: DateTime<Utc>) {
        self.priority = priority;
        self.touch(now);
    }

    pub fn add_attachment(&mut self, attachment: TicketAttachment, now: DateTime<Utc>) {
        self.attachments.push(attachment);
        self.touch(now);
    }

    const fn touch(&mut self, now: DateTime<Utc>) {
        self.updated_at = now;
        self.revision = self.revision.saturating_add(1);
    }

    #[must_use]
    pub fn requester_projection(&self) -> RequesterTicket {
        RequesterTicket {
            id: self.id,
            project_id: self.project_id.clone(),
            display_reference: self.display_reference.clone(),
            subject: self.subject.clone(),
            description: self.description.clone(),
            requester: self.requester.clone(),
            channel: self.channel.clone(),
            priority: self.priority,
            status: self.status,
            messages: self
                .messages
                .iter()
                .filter(|message| message.kind != TicketMessageKind::InternalNote)
                .cloned()
                .collect(),
            attachments: self
                .attachments
                .iter()
                .map(RequesterTicketAttachment::from)
                .collect(),
            created_at: self.created_at,
            updated_at: self.updated_at,
            revision: self.revision,
        }
    }

    #[must_use]
    pub fn export_ai_context(&self) -> TicketAiContext {
        TicketAiContext {
            schema_version: 1,
            project_id: self.project_id.clone(),
            ticket_id: self.id,
            display_reference: self.display_reference.clone(),
            subject: self.subject.clone(),
            description: self.description.clone(),
            status: self.status,
            priority: self.priority,
            resource_references: self.resource_references.clone(),
            messages: self.messages.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequesterTicketAttachment {
    pub id: TicketAttachmentId,
    pub kind: AttachmentKind,
    pub file_name: String,
    pub content_type: String,
    pub size_bytes: u64,
    pub created_at: DateTime<Utc>,
}

impl From<&TicketAttachment> for RequesterTicketAttachment {
    fn from(value: &TicketAttachment) -> Self {
        Self {
            id: value.id,
            kind: value.kind,
            file_name: value.file_name.clone(),
            content_type: value.content_type.clone(),
            size_bytes: value.size_bytes,
            created_at: value.created_at,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequesterTicket {
    pub id: TicketId,
    pub project_id: String,
    pub display_reference: String,
    pub subject: String,
    pub description: String,
    pub requester: TicketRequester,
    pub channel: TicketChannel,
    pub priority: TicketPriority,
    pub status: TicketStatus,
    pub messages: Vec<TicketMessage>,
    pub attachments: Vec<RequesterTicketAttachment>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub revision: u64,
}

/// Compact agent-facing ticket summary. Deliberately excludes descriptions,
/// message bodies, object keys, digests, audit, AI context and provider data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TicketSummary {
    pub id: TicketId,
    pub project_id: String,
    pub display_reference: String,
    pub subject: String,
    pub requester_subject: String,
    pub status: TicketStatus,
    pub clock_state: TicketClockState,
    pub priority: TicketPriority,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub queue_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assignee_subject: Option<String>,
    pub message_count: usize,
    pub attachment_count: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_activity_at: Option<DateTime<Utc>>,
    pub needs_attention: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub revision: u64,
}

impl Ticket {
    #[must_use]
    pub const fn needs_attention(&self) -> bool {
        matches!(
            self.status,
            TicketStatus::New | TicketStatus::PendingInternal
        )
    }

    #[must_use]
    pub fn agent_summary(&self) -> TicketSummary {
        TicketSummary {
            id: self.id,
            project_id: self.project_id.clone(),
            display_reference: self.display_reference.clone(),
            subject: self.subject.clone(),
            requester_subject: self.requester.subject.clone(),
            status: self.status,
            clock_state: self.clock_state,
            priority: self.priority,
            queue_id: self.queue_id.clone(),
            assignee_subject: self.assignee_subject.clone(),
            message_count: self.messages.len(),
            attachment_count: self.attachments.len(),
            last_activity_at: self.messages.last().map(|message| message.created_at),
            needs_attention: self.needs_attention(),
            created_at: self.created_at,
            updated_at: self.updated_at,
            revision: self.revision,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TicketAiContext {
    pub schema_version: u32,
    pub project_id: String,
    pub ticket_id: TicketId,
    pub display_reference: String,
    pub subject: String,
    pub description: String,
    pub status: TicketStatus,
    pub priority: TicketPriority,
    pub resource_references: Vec<SupportResourceReference>,
    pub messages: Vec<TicketMessage>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TicketFromHandoffInput {
    pub subject: String,
    pub description: String,
    pub channel: TicketChannel,
    pub priority: TicketPriority,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum TicketValidationError {
    #[error("invalid {field}: {detail}")]
    InvalidField { field: &'static str, detail: String },
    #[error("ticket cannot transition from {current:?} to {target:?}")]
    InvalidTransition {
        current: TicketStatus,
        target: TicketStatus,
    },
    #[error("a resolution is required")]
    ResolutionRequired,
    #[error("a close reason is required")]
    CloseReasonRequired,
}

fn validate_text(
    field: &'static str,
    value: &str,
    maximum: usize,
) -> Result<(), TicketValidationError> {
    if value.trim().is_empty()
        || value.chars().count() > maximum
        || value.chars().any(char::is_control)
    {
        Err(TicketValidationError::InvalidField {
            field,
            detail: format!("must contain 1-{maximum} visible characters"),
        })
    } else {
        Ok(())
    }
}

fn validate_optional_text(
    field: &'static str,
    value: Option<&str>,
    maximum: usize,
) -> Result<(), TicketValidationError> {
    value.map_or(Ok(()), |value| validate_text(field, value, maximum))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeDelta;

    fn ticket() -> Ticket {
        Ticket::create(
            CreateTicketInput {
                project_id: "project-a".into(),
                subject: "Need help".into(),
                description: "It broke".into(),
                requester: TicketRequester {
                    subject: "user-1".into(),
                    display_name: None,
                    email: None,
                },
                channel: TicketChannel::Portal,
                priority: TicketPriority::Normal,
                resource_references: Vec::new(),
            },
            "SUP-1",
            Utc::now(),
        )
        .unwrap()
    }

    #[test]
    fn clock_mapping_keeps_pending_internal_open() {
        assert_eq!(
            TicketStatus::PendingInternal.clock_state(),
            TicketClockState::Open
        );
        assert_eq!(
            TicketStatus::PendingRequester.clock_state(),
            TicketClockState::Paused
        );
        assert_eq!(
            TicketStatus::Closed.clock_state(),
            TicketClockState::Stopped
        );
    }

    #[test]
    fn requester_reply_reopens_and_internal_note_stays_private() {
        let mut ticket = ticket();
        let now = ticket.created_at + TimeDelta::minutes(1);
        ticket
            .change_status(TicketStatus::PendingRequester, None, None, now)
            .unwrap();
        ticket.add_internal_note("agent", "private", now).unwrap();
        assert_eq!(ticket.status, TicketStatus::PendingRequester);
        ticket.reply_as_requester("more detail", now).unwrap();
        assert_eq!(ticket.status, TicketStatus::Open);
        assert!(
            !serde_json::to_string(&ticket.requester_projection())
                .unwrap()
                .contains("private")
        );
    }

    #[test]
    fn first_response_and_resolution_timestamps_are_stable() {
        let mut ticket = ticket();
        let first = ticket.created_at + TimeDelta::minutes(1);
        ticket.reply_as_agent("agent", "hello", first).unwrap();
        ticket
            .reply_as_agent("agent", "again", first + TimeDelta::minutes(1))
            .unwrap();
        assert_eq!(ticket.first_public_response_at, Some(first));
        let resolved = first + TimeDelta::minutes(2);
        ticket
            .change_status(TicketStatus::Resolved, Some("fixed".into()), None, resolved)
            .unwrap();
        assert_eq!(ticket.resolved_at, Some(resolved));
        assert_eq!(
            ticket.change_status(TicketStatus::Closed, None, None, resolved),
            Err(TicketValidationError::CloseReasonRequired)
        );
        ticket
            .change_status(
                TicketStatus::Closed,
                None,
                Some("confirmed".into()),
                resolved,
            )
            .unwrap();
        ticket.reply_as_requester("regressed", resolved).unwrap();
        assert_eq!(ticket.status, TicketStatus::Open);
        assert!(ticket.closed_at.is_none());
    }
}
